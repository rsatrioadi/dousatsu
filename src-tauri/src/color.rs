use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use tauri::State;

use crate::AppState;

#[derive(Debug, Serialize)]
pub struct GradientResult {
    pub map: HashMap<String, f64>, // node_id -> normalized [0,1]
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Serialize)]
pub struct CategoricalResult {
    pub map: HashMap<String, String>, // node_id -> category id
    pub categories: Vec<CategoryInfo>,
}

#[derive(Debug, Serialize)]
pub struct CategoryInfo {
    pub id: String,
    pub name: String,
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
pub fn compute_coloring_categorical(
    state: State<AppState>,
) -> Result<CategoricalResult, String> {
    let guard = state.elements.lock().map_err(|e| format!("state lock: {e}"))?;
    let elements = guard.as_ref().ok_or("no graph loaded")?;

    let by_id: HashMap<&str, &crate::graph::Node> =
        elements.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut map: HashMap<String, String> = HashMap::new();
    let mut category_ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for e in &elements.edges {
        if e.label != "implements" {
            continue;
        }
        let Some(target) = by_id.get(e.target.as_str()).copied() else {
            continue;
        };
        if target.labels.first().map(|s| s.as_str()) != Some("Category") {
            continue;
        }
        // earlier-wins: don't overwrite existing
        map.entry(e.source.clone()).or_insert_with(|| {
            category_ids.insert(e.target.clone());
            e.target.clone()
        });
        category_ids.insert(e.target.clone());
    }

    let categories: Vec<CategoryInfo> = category_ids
        .into_iter()
        .map(|id| {
            let name = by_id
                .get(id.as_str())
                .and_then(|n| n.properties.get("simpleName"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| id.clone());
            CategoryInfo { id, name }
        })
        .collect();

    Ok(CategoricalResult { map, categories })
}
