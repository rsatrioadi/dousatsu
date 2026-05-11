use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tauri::State;

use crate::graph::{Elements, Node};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct GradientResult {
    pub map: HashMap<String, f64>, // node_id -> normalized [0,1]
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Serialize)]
pub struct CategoricalResult {
    pub map: HashMap<String, String>, // node_id -> bucket id
    pub categories: Vec<CategoryInfo>, // buckets (top-level under the dimension)
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

/// Buckets of a dimension = :Category nodes that directly `composes` the dimension.
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

/// For a starting category `c`, follow outgoing `refines` edges (Category -> Category)
/// until we land on a member of `buckets`. Returns None if no such ancestor is reached.
/// Cycle-safe via a visited set and depth cap.
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

    dimensions
        .into_iter()
        .map(|dim| {
            let mut buckets = buckets_of_dimension(elements, &by_id, &dim.id);
            buckets.sort_by(|a, b| simple_name(a).cmp(&simple_name(b)).then(a.id.cmp(&b.id)));
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

    // Walk `measures` edges that target metric_id; pick numeric `property` from edge.properties.
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
pub fn compute_coloring_by_dimension(
    state: State<AppState>,
    dimension_id: String,
) -> Result<CategoricalResult, String> {
    let guard = state.elements.lock().map_err(|e| format!("state lock: {e}"))?;
    let elements = guard.as_ref().ok_or("no graph loaded")?;

    let by_id: HashMap<&str, &Node> =
        elements.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Resolve dimension and its buckets.
    let dim = by_id
        .get(dimension_id.as_str())
        .copied()
        .ok_or_else(|| format!("unknown dimension: {dimension_id}"))?;
    if primary_label(dim) != "Dimension" {
        return Err(format!("node {dimension_id} is not a :Dimension"));
    }

    let bucket_nodes = buckets_of_dimension(elements, &by_id, &dimension_id);
    let bucket_ids: HashSet<&str> = bucket_nodes.iter().map(|n| n.id.as_str()).collect();

    // refines: Category -> Category (single outgoing per source; earlier-wins on conflict).
    let mut refines_out: HashMap<&str, &str> = HashMap::new();
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
        refines_out.entry(s.id.as_str()).or_insert(t.id.as_str());
    }

    // For each node, accumulate weight per bucket via implements edges.
    let mut weights: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut first_seen_idx: HashMap<(String, String), usize> = HashMap::new();
    for (idx, e) in elements.edges.iter().enumerate() {
        if e.label != "implements" {
            continue;
        }
        let Some(target) = by_id.get(e.target.as_str()).copied() else {
            continue;
        };
        if primary_label(target) != "Category" {
            continue;
        }
        let Some(bucket) = bucket_of(target.id.as_str(), &bucket_ids, &refines_out) else {
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
        let bucket_owned = bucket.to_string();
        let entry = weights
            .entry(e.source.clone())
            .or_default()
            .entry(bucket_owned.clone())
            .or_insert(0.0);
        *entry += w;
        first_seen_idx
            .entry((e.source.clone(), bucket_owned))
            .or_insert(idx);
    }

    // Pick max-weight bucket per node; tie-break by earliest first-seen.
    let mut map: HashMap<String, String> = HashMap::new();
    for (node_id, bucket_weights) in weights {
        let chosen = bucket_weights
            .into_iter()
            .max_by(|a, b| {
                a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| {
                    let ai = first_seen_idx
                        .get(&(node_id.clone(), a.0.clone()))
                        .copied()
                        .unwrap_or(usize::MAX);
                    let bi = first_seen_idx
                        .get(&(node_id.clone(), b.0.clone()))
                        .copied()
                        .unwrap_or(usize::MAX);
                    // earlier index wins on ties → reverse so it's the "max"
                    bi.cmp(&ai)
                })
            })
            .map(|(b, _)| b);
        if let Some(b) = chosen {
            map.insert(node_id, b);
        }
    }

    let mut buckets_sorted = bucket_nodes;
    buckets_sorted.sort_by(|a, b| simple_name(a).cmp(&simple_name(b)).then(a.id.cmp(&b.id)));
    let categories: Vec<CategoryInfo> = buckets_sorted
        .into_iter()
        .map(|n| CategoryInfo {
            id: n.id.clone(),
            name: simple_name(n),
        })
        .collect();

    Ok(CategoricalResult { map, categories })
}
