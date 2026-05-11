use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use tauri::State;

use crate::graph::{Elements, Node};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct GradientResult {
    pub map: HashMap<String, f64>, // node_id -> normalized [0,1]
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct CategoryInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct DimensionInfo {
    pub id: String,
    pub name: String,
    pub buckets: Vec<CategoryInfo>,
    /// Primary node labels for which at least one node has an implements edge
    /// resolving (via the refines chain) to a bucket of this dimension.
    pub applies_to: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Stop {
    pub category_id: String,
    pub fraction: f64,
}

#[derive(Debug, Serialize)]
pub struct DimensionSlice {
    pub id: String,
    pub name: String,
    pub categories: Vec<CategoryInfo>,
}

#[derive(Debug, Serialize)]
pub struct LevelColoringResult {
    /// node_id -> stops summing to 1. Empty entries are omitted.
    pub stops_by_node: HashMap<String, Vec<Stop>>,
    /// node_id -> dimension id that governs it (so the frontend picks the right palette).
    pub dimension_by_node: HashMap<String, String>,
    /// Per-dimension info for palette + legend (only dimensions actually used).
    pub dimensions: Vec<DimensionSlice>,
}

fn primary_label(n: &Node) -> &str {
    n.labels.first().map(|s| s.as_str()).unwrap_or("")
}

fn simple_name(n: &Node) -> String {
    n.properties
        .get("simpleName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| n.id.clone())
}

/// Direct composes-children of a :Dimension that are themselves :Category nodes.
fn buckets_of_dimension<'a>(
    elements: &'a Elements,
    by_id: &HashMap<&'a str, &'a Node>,
    dimension_id: &str,
) -> Vec<&'a Node> {
    let mut out: Vec<&Node> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for e in &elements.edges {
        if e.label != "composes" || e.target != dimension_id {
            continue;
        }
        let Some(src) = by_id.get(e.source.as_str()).copied() else {
            continue;
        };
        if primary_label(src) != "Category" {
            continue;
        }
        if seen.insert(src.id.as_str()) {
            out.push(src);
        }
    }
    out
}

/// Map every :Category in the graph to its outgoing `refines` target (also a :Category).
fn refines_map<'a>(elements: &'a Elements, by_id: &HashMap<&'a str, &'a Node>) -> HashMap<&'a str, &'a str> {
    let mut out: HashMap<&str, &str> = HashMap::new();
    for e in &elements.edges {
        if e.label != "refines" {
            continue;
        }
        let (Some(s), Some(t)) = (
            by_id.get(e.source.as_str()).copied(),
            by_id.get(e.target.as_str()).copied(),
        ) else {
            continue;
        };
        if primary_label(s) != "Category" || primary_label(t) != "Category" {
            continue;
        }
        out.entry(s.id.as_str()).or_insert(t.id.as_str());
    }
    out
}

/// Walk outgoing `refines` from `start` until we hit a bucket of the dimension.
fn bucket_of<'a>(
    start: &'a str,
    buckets: &HashSet<&str>,
    refines_out: &HashMap<&'a str, &'a str>,
) -> Option<&'a str> {
    if buckets.contains(start) {
        return Some(start);
    }
    let mut cur = start;
    let mut visited: HashSet<&str> = HashSet::new();
    visited.insert(cur);
    for _ in 0..256 {
        let next = match refines_out.get(cur) {
            Some(n) => *n,
            None => return None,
        };
        if buckets.contains(next) {
            return Some(next);
        }
        if !visited.insert(next) {
            return None;
        }
        cur = next;
    }
    None
}

fn list_dimensions_inner(elements: &Elements) -> Vec<DimensionInfo> {
    let by_id: HashMap<&str, &Node> =
        elements.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut dimensions: Vec<&Node> = elements
        .nodes
        .iter()
        .filter(|n| primary_label(n) == "Dimension")
        .collect();
    dimensions.sort_by(|a, b| simple_name(a).cmp(&simple_name(b)).then(a.id.cmp(&b.id)));

    let refines_out = refines_map(elements, &by_id);

    dimensions
        .into_iter()
        .map(|dim| {
            let mut buckets = buckets_of_dimension(elements, &by_id, &dim.id);
            buckets.sort_by(|a, b| simple_name(a).cmp(&simple_name(b)).then(a.id.cmp(&b.id)));
            let bucket_ids: HashSet<&str> = buckets.iter().map(|n| n.id.as_str()).collect();

            // Infer applies_to from the data: primary labels of sources of
            // implements edges whose target resolves into this dimension.
            let mut applies_to: BTreeMap<String, ()> = BTreeMap::new();
            for e in &elements.edges {
                if e.label != "implements" {
                    continue;
                }
                let Some(tgt) = by_id.get(e.target.as_str()).copied() else {
                    continue;
                };
                if primary_label(tgt) != "Category" {
                    continue;
                }
                if bucket_of(tgt.id.as_str(), &bucket_ids, &refines_out).is_none() {
                    continue;
                }
                let Some(src) = by_id.get(e.source.as_str()).copied() else {
                    continue;
                };
                let lbl = primary_label(src);
                if !lbl.is_empty() {
                    applies_to.insert(lbl.to_string(), ());
                }
            }

            let buckets_info: Vec<CategoryInfo> = buckets
                .into_iter()
                .map(|n| CategoryInfo {
                    id: n.id.clone(),
                    name: simple_name(n),
                })
                .collect();

            DimensionInfo {
                id: dim.id.clone(),
                name: simple_name(dim),
                buckets: buckets_info,
                applies_to: applies_to.into_keys().collect(),
            }
        })
        .collect()
}

pub fn list_dimensions_for(elements: &Elements) -> Vec<DimensionInfo> {
    list_dimensions_inner(elements)
}

#[tauri::command]
pub fn list_dimensions(state: State<AppState>) -> Result<Vec<DimensionInfo>, String> {
    let guard = state.elements.lock().map_err(|e| format!("state lock: {e}"))?;
    let elements = guard.as_ref().ok_or("no graph loaded")?;
    Ok(list_dimensions_inner(elements))
}

#[tauri::command]
pub fn compute_coloring_gradient(
    state: State<AppState>,
    metric_id: String,
    property: String,
) -> Result<GradientResult, String> {
    let guard = state.elements.lock().map_err(|e| format!("state lock: {e}"))?;
    let elements = guard.as_ref().ok_or("no graph loaded")?;

    let mut raw: Vec<(String, f64)> = Vec::new();
    for e in &elements.edges {
        if e.label != "measures" || e.target != metric_id {
            continue;
        }
        let v = match e.properties.get(&property) {
            Some(Value::Number(n)) => n.as_f64(),
            Some(Value::String(s)) => s.parse::<f64>().ok(),
            _ => None,
        };
        if let Some(v) = v {
            if v.is_finite() {
                raw.push((e.source.clone(), v));
            }
        }
    }

    if raw.is_empty() {
        return Ok(GradientResult {
            map: HashMap::new(),
            min: 0.0,
            max: 0.0,
        });
    }

    let min = raw.iter().map(|(_, v)| *v).fold(f64::INFINITY, f64::min);
    let max = raw
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::NEG_INFINITY, f64::max);

    let span = (max - min).abs();
    let mut map = HashMap::new();
    for (id, v) in raw {
        let n = if span < f64::EPSILON {
            0.5
        } else {
            (v - min) / span
        };
        map.insert(id, n);
    }
    Ok(GradientResult { map, min, max })
}

#[tauri::command]
pub fn compute_coloring_by_levels(
    state: State<AppState>,
    level_dimensions: HashMap<String, String>,
) -> Result<LevelColoringResult, String> {
    let guard = state.elements.lock().map_err(|e| format!("state lock: {e}"))?;
    let elements = guard.as_ref().ok_or("no graph loaded")?;

    let by_id: HashMap<&str, &Node> =
        elements.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let refines_out = refines_map(elements, &by_id);

    // Cache: dimension_id -> (bucket_ids set, ordered bucket list for output)
    let mut dim_buckets: HashMap<&str, HashSet<&str>> = HashMap::new();
    let mut dim_categories: HashMap<&str, Vec<CategoryInfo>> = HashMap::new();
    let mut dim_names: HashMap<&str, String> = HashMap::new();

    for dim_id in level_dimensions.values() {
        let key = dim_id.as_str();
        if dim_buckets.contains_key(key) {
            continue;
        }
        let Some(dim_node) = by_id.get(key).copied() else {
            continue;
        };
        if primary_label(dim_node) != "Dimension" {
            continue;
        }
        let mut buckets = buckets_of_dimension(elements, &by_id, dim_id);
        buckets.sort_by(|a, b| simple_name(a).cmp(&simple_name(b)).then(a.id.cmp(&b.id)));
        let ids: HashSet<&str> = buckets.iter().map(|n| n.id.as_str()).collect();
        let cats: Vec<CategoryInfo> = buckets
            .iter()
            .map(|n| CategoryInfo {
                id: n.id.clone(),
                name: simple_name(n),
            })
            .collect();
        dim_buckets.insert(key, ids);
        dim_categories.insert(key, cats);
        dim_names.insert(key, simple_name(dim_node));
    }

    // Per node, accumulate weight per bucket (filtered by the dimension that governs the node's level).
    let mut weights: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut governed_dim: HashMap<String, String> = HashMap::new();

    for e in &elements.edges {
        if e.label != "implements" {
            continue;
        }
        let Some(src) = by_id.get(e.source.as_str()).copied() else {
            continue;
        };
        let Some(tgt) = by_id.get(e.target.as_str()).copied() else {
            continue;
        };
        if primary_label(tgt) != "Category" {
            continue;
        }
        let src_label = primary_label(src);
        let Some(dim_id) = level_dimensions.get(src_label) else {
            continue;
        };
        let Some(bucket_ids) = dim_buckets.get(dim_id.as_str()) else {
            continue;
        };
        let Some(bucket) = bucket_of(tgt.id.as_str(), bucket_ids, &refines_out) else {
            continue;
        };
        let w = match e.properties.get("weight") {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(1.0),
            Some(Value::String(s)) => s.parse::<f64>().unwrap_or(1.0),
            _ => 1.0,
        };
        if !w.is_finite() || w <= 0.0 {
            continue;
        }
        *weights
            .entry(src.id.clone())
            .or_default()
            .entry(bucket.to_string())
            .or_insert(0.0) += w;
        governed_dim
            .entry(src.id.clone())
            .or_insert_with(|| dim_id.clone());
    }

    // Normalize weights to fractions; emit stops in category-order per dimension.
    let mut stops_by_node: HashMap<String, Vec<Stop>> = HashMap::new();
    for (node_id, bucket_weights) in weights {
        let total: f64 = bucket_weights.values().sum();
        if total <= 0.0 {
            continue;
        }
        let Some(dim_id) = governed_dim.get(&node_id) else {
            continue;
        };
        let Some(cats) = dim_categories.get(dim_id.as_str()) else {
            continue;
        };
        // Order stops by the dimension's category order (stable, matches legend).
        let mut stops: Vec<Stop> = Vec::new();
        for cat in cats {
            if let Some(&w) = bucket_weights.get(&cat.id) {
                stops.push(Stop {
                    category_id: cat.id.clone(),
                    fraction: w / total,
                });
            }
        }
        if !stops.is_empty() {
            stops_by_node.insert(node_id, stops);
        }
    }

    // Build per-dimension slice list (only dimensions that actually produced stops).
    let mut used_dims: BTreeMap<String, ()> = BTreeMap::new();
    for d in governed_dim.values() {
        used_dims.insert(d.clone(), ());
    }
    let dimensions: Vec<DimensionSlice> = used_dims
        .into_keys()
        .filter_map(|d| {
            let key = d.as_str();
            let name = dim_names.get(key)?.clone();
            let categories = dim_categories.get(key)?.clone();
            Some(DimensionSlice {
                id: d.clone(),
                name,
                categories,
            })
        })
        .collect();

    Ok(LevelColoringResult {
        stops_by_node,
        dimension_by_node: governed_dim,
        dimensions,
    })
}
