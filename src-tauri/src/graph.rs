use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use tauri::State;

use crate::AppState;

// -----------------------------------------------------------------------------
// Domain types
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub labels: Vec<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: String,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Elements {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

// -----------------------------------------------------------------------------
// JSON parsing (lenient — accepts any of the SABO-like shapes)
// -----------------------------------------------------------------------------

fn parse_elements(json: &str) -> Result<Elements, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    let elements = v
        .get("elements")
        .ok_or_else(|| "missing top-level `elements`".to_string())?;
    let raw_nodes = elements
        .get("nodes")
        .and_then(|n| n.as_array())
        .ok_or_else(|| "missing `elements.nodes`".to_string())?;
    let raw_edges = elements
        .get("edges")
        .and_then(|n| n.as_array())
        .ok_or_else(|| "missing `elements.edges`".to_string())?;

    let mut nodes = Vec::with_capacity(raw_nodes.len());
    for (i, n) in raw_nodes.iter().enumerate() {
        let data = n.get("data").unwrap_or(n);
        let id = data
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("node[{i}] missing id"))?
            .to_string();
        let labels = match data.get("labels") {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            Some(Value::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        };
        let properties = match data.get("properties") {
            Some(Value::Object(map)) => map
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<BTreeMap<_, _>>(),
            _ => BTreeMap::new(),
        };
        nodes.push(Node {
            id,
            labels,
            properties,
        });
    }

    let mut edges = Vec::with_capacity(raw_edges.len());
    for (i, e) in raw_edges.iter().enumerate() {
        let data = e.get("data").unwrap_or(e);
        let id = data
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("e{i}"));
        let source = data
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("edge[{i}] missing source"))?
            .to_string();
        let target = data
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("edge[{i}] missing target"))?
            .to_string();
        let label = data
            .get("label")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("edge[{i}] missing label"))?
            .to_string();
        let properties = match data.get("properties") {
            Some(Value::Object(map)) => map
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<BTreeMap<_, _>>(),
            _ => BTreeMap::new(),
        };
        edges.push(Edge {
            id,
            source,
            target,
            label,
            properties,
        });
    }

    Ok(Elements { nodes, edges })
}

// -----------------------------------------------------------------------------
// Summary
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct EdgeTypeStat {
    pub label: String,
    pub count: usize,
    pub source_target_pairs: Vec<(String, String, usize)>,
}

#[derive(Debug, Serialize)]
pub struct GraphSummary {
    pub node_count: usize,
    pub edge_count: usize,
    pub node_labels: Vec<(String, usize)>,
    pub edge_labels: Vec<EdgeTypeStat>,
    pub metric_nodes: Vec<MetricRef>,
    pub measure_properties: Vec<String>,
    pub has_implements: bool,
}

#[derive(Debug, Serialize)]
pub struct MetricRef {
    pub id: String,
    pub name: String,
}

fn primary_label(n: &Node) -> &str {
    n.labels.first().map(|s| s.as_str()).unwrap_or("")
}

fn summarize(elements: &Elements) -> GraphSummary {
    let mut node_labels: HashMap<String, usize> = HashMap::new();
    let mut by_id: HashMap<&str, &Node> = HashMap::with_capacity(elements.nodes.len());
    for n in &elements.nodes {
        let lbl = primary_label(n);
        if !lbl.is_empty() {
            *node_labels.entry(lbl.to_string()).or_insert(0) += 1;
        }
        by_id.insert(&n.id, n);
    }

    let mut edge_labels: HashMap<String, HashMap<(String, String), usize>> = HashMap::new();
    let mut has_implements = false;
    let mut measure_props: BTreeSet<String> = BTreeSet::new();
    for e in &elements.edges {
        let s_lbl = by_id
            .get(e.source.as_str())
            .map(|n| primary_label(n))
            .unwrap_or("");
        let t_lbl = by_id
            .get(e.target.as_str())
            .map(|n| primary_label(n))
            .unwrap_or("");
        *edge_labels
            .entry(e.label.clone())
            .or_default()
            .entry((s_lbl.to_string(), t_lbl.to_string()))
            .or_insert(0) += 1;
        if e.label == "implements" {
            has_implements = true;
        }
        if e.label == "measures" {
            for (k, v) in &e.properties {
                if v.is_number() {
                    measure_props.insert(k.clone());
                }
            }
        }
    }

    let mut node_labels: Vec<_> = node_labels.into_iter().collect();
    node_labels.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut edge_label_stats: Vec<EdgeTypeStat> = edge_labels
        .into_iter()
        .map(|(label, pairs)| {
            let count: usize = pairs.values().sum();
            let mut pairs: Vec<(String, String, usize)> =
                pairs.into_iter().map(|((s, t), c)| (s, t, c)).collect();
            pairs.sort_by(|a, b| b.2.cmp(&a.2));
            EdgeTypeStat {
                label,
                count,
                source_target_pairs: pairs,
            }
        })
        .collect();
    edge_label_stats.sort_by(|a, b| b.count.cmp(&a.count).then(a.label.cmp(&b.label)));

    let metric_nodes: Vec<MetricRef> = elements
        .nodes
        .iter()
        .filter(|n| primary_label(n) == "Metric")
        .map(|n| {
            let name = n
                .properties
                .get("simpleName")
                .and_then(|v| v.as_str())
                .unwrap_or(&n.id)
                .to_string();
            MetricRef {
                id: n.id.clone(),
                name,
            }
        })
        .collect();

    GraphSummary {
        node_count: elements.nodes.len(),
        edge_count: elements.edges.len(),
        node_labels,
        edge_labels: edge_label_stats,
        metric_nodes,
        measure_properties: measure_props.into_iter().collect(),
        has_implements,
    }
}

// -----------------------------------------------------------------------------
// Hierarchy
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyLink {
    pub source: String, // source node label
    pub edge: String,   // edge label
    pub target: String, // target node label
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchySchema {
    pub links: Vec<HierarchyLink>,
}

#[derive(Debug, Default, Serialize)]
pub struct HierarchyIndex {
    pub roots: Vec<String>,
    pub parent_of: HashMap<String, String>,
    pub children_of: HashMap<String, Vec<String>>,
    pub depth_of: HashMap<String, usize>,
    pub level_of: HashMap<String, usize>, // index into chain (0 = topmost source)
}

fn build_hierarchy_index(elements: &Elements, schema: &HierarchySchema) -> HierarchyIndex {
    let mut by_id: HashMap<&str, &Node> = HashMap::with_capacity(elements.nodes.len());
    for n in &elements.nodes {
        by_id.insert(&n.id, n);
    }

    let mut parent_of: HashMap<String, String> = HashMap::new();
    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    let mut level_of: HashMap<String, usize> = HashMap::new();

    // For each link in schema (in order), walk edges of that label and treat them as
    // parent→child edges where source has source-label and target has target-label.
    // Earlier links win — once a node has a parent, later links don't overwrite.
    for (idx, link) in schema.links.iter().enumerate() {
        for e in &elements.edges {
            if e.label != link.edge {
                continue;
            }
            let s = by_id.get(e.source.as_str()).copied();
            let t = by_id.get(e.target.as_str()).copied();
            let (Some(s), Some(t)) = (s, t) else { continue };
            if primary_label(s) != link.source || primary_label(t) != link.target {
                continue;
            }
            if e.source == e.target {
                continue;
            }
            if parent_of.contains_key(&e.target) {
                continue;
            }
            parent_of.insert(e.target.clone(), e.source.clone());
            children_of
                .entry(e.source.clone())
                .or_default()
                .push(e.target.clone());
            // child level = idx + 1, source level = idx (only set if not already)
            level_of.entry(e.target.clone()).or_insert(idx + 1);
            level_of.entry(e.source.clone()).or_insert(idx);
        }
    }

    // Roots: nodes whose primary label appears as source somewhere in the chain
    // and which themselves have no parent.
    let source_labels: HashSet<&str> =
        schema.links.iter().map(|l| l.source.as_str()).collect();

    let mut roots: Vec<String> = Vec::new();
    for n in &elements.nodes {
        let lbl = primary_label(n);
        if !source_labels.contains(lbl) {
            continue;
        }
        if parent_of.contains_key(&n.id) {
            continue;
        }
        // Only include if it has at least one child (otherwise just a stray node)
        // — actually, include it anyway to allow inspecting it.
        roots.push(n.id.clone());
    }
    roots.sort();

    // depth_of via BFS from roots
    let mut depth_of: HashMap<String, usize> = HashMap::new();
    let mut queue: std::collections::VecDeque<(String, usize)> = std::collections::VecDeque::new();
    for r in &roots {
        depth_of.insert(r.clone(), 0);
        queue.push_back((r.clone(), 0));
    }
    while let Some((id, d)) = queue.pop_front() {
        if let Some(children) = children_of.get(&id) {
            for c in children {
                if !depth_of.contains_key(c) {
                    depth_of.insert(c.clone(), d + 1);
                    queue.push_back((c.clone(), d + 1));
                }
            }
        }
    }

    HierarchyIndex {
        roots,
        parent_of,
        children_of,
        depth_of,
        level_of,
    }
}

// -----------------------------------------------------------------------------
// View assembly (focused-node view)
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct FocusedView {
    pub focused_id: String,
    pub nodes: Vec<NodeView>,
    pub edges: Vec<EdgeView>,
    pub breadcrumb: Vec<NodeView>,
}

#[derive(Debug, Serialize)]
pub struct NodeView {
    pub id: String,
    pub label: String,        // primary label (type)
    pub name: String,         // simpleName or id
    pub parent: Option<String>, // compound parent in the cytoscape view
    pub role: NodeRole,
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeRole {
    Focused,        // the focused node, rendered as compound
    FocusedParent,  // the focused node's hierarchy parent (compound)
    Child,          // a hierarchy child of focused (rendered inside focused)
    Neighbour,      // dependency-neighbour of a child
    NeighbourParent, // hierarchy parent of a neighbour (compound)
}

#[derive(Debug, Serialize)]
pub struct EdgeView {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: String,
}

/// Synthetic id used when the hierarchy yields more than one root. The view
/// then shows a virtual "forest" compound containing every real root, so the
/// initial canvas can present all of them at once.
pub const FOREST_ROOT_ID: &str = "::dousatsu::forest";

fn make_forest_node() -> Node {
    let mut props = BTreeMap::new();
    props.insert(
        "simpleName".to_string(),
        Value::String("Forest".to_string()),
    );
    Node {
        id: FOREST_ROOT_ID.to_string(),
        labels: vec!["Forest".to_string()],
        properties: props,
    }
}

/// Look up a node by id, returning a synthesised forest node when asked for
/// the synthetic forest id. Returns None for unknown real ids.
fn resolve_node(by_id: &HashMap<&str, &Node>, id: &str) -> Option<Node> {
    if id == FOREST_ROOT_ID {
        Some(make_forest_node())
    } else {
        by_id.get(id).copied().cloned()
    }
}

fn focused_view(
    elements: &Elements,
    schema: &HierarchySchema,
    enabled_deps: &HashSet<String>,
    focused_id: &str,
) -> Result<FocusedView, String> {
    let by_id: HashMap<&str, &Node> = elements.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let h = build_hierarchy_index(elements, schema);
    let multi_root = h.roots.len() > 1;
    let is_forest = focused_id == FOREST_ROOT_ID;

    // The focused node — real, or synthesised when looking at the forest.
    let focused_owned = resolve_node(&by_id, focused_id)
        .ok_or_else(|| format!("unknown node id: {focused_id}"))?;
    let focused = &focused_owned;

    // The focused node's hierarchy parent. Real roots get the synthetic forest
    // as their parent when the hierarchy is a forest (more than one root).
    let focused_parent = if is_forest {
        None
    } else {
        let real_parent = h.parent_of.get(focused_id).cloned();
        if real_parent.is_none() && multi_root && h.roots.iter().any(|r| r == focused_id) {
            Some(FOREST_ROOT_ID.to_string())
        } else {
            real_parent
        }
    };

    // Direct hierarchy children of the focused node. For the forest, those
    // are all real roots.
    let children: Vec<String> = if is_forest {
        h.roots.clone()
    } else {
        h.children_of.get(focused_id).cloned().unwrap_or_default()
    };
    let child_set: HashSet<&str> = children.iter().map(|s| s.as_str()).collect();

    // For each node in `focused`'s subtree, the unique child of `focused`
    // that contains it (or the node itself if it IS a child of focused).
    // Built via BFS down from each child.
    let mut subtree_to_child: HashMap<String, String> = HashMap::new();
    for c_id in &children {
        let mut stack = vec![c_id.clone()];
        while let Some(x) = stack.pop() {
            if subtree_to_child.contains_key(&x) {
                continue;
            }
            subtree_to_child.insert(x.clone(), c_id.clone());
            if let Some(grand) = h.children_of.get(&x) {
                for g in grand {
                    stack.push(g.clone());
                }
            }
        }
    }

    // Collect dependency edges. Originals (where one or both endpoints are
    // direct children of focused) follow the spec verbatim — the off-child
    // endpoint becomes a dependency-neighbour. Lifted edges are added on top:
    // an edge whose endpoints are deep descendants is also rendered between
    // their children-of-focused projections, so e.g. operation invocations
    // surface as type-type edges when focused on a file. Lifting NEVER turns
    // a deep-descendant→external edge into a child→external edge — that
    // would invent neighbours that don't exist at the original level.
    let mut neighbours: BTreeSet<String> = BTreeSet::new();
    let mut dep_edges: Vec<EdgeView> = Vec::new();
    let mut emitted: HashSet<(String, String, String)> = HashSet::new();

    for e in &elements.edges {
        if !enabled_deps.contains(&e.label) {
            continue;
        }
        let ps = subtree_to_child.get(&e.source).cloned();
        let pt = subtree_to_child.get(&e.target).cloned();
        let s_is_child = ps
            .as_deref()
            .map(|p| p == e.source.as_str())
            .unwrap_or(false);
        let t_is_child = pt
            .as_deref()
            .map(|p| p == e.target.as_str())
            .unwrap_or(false);
        let s_in_subtree = ps.is_some();
        let t_in_subtree = pt.is_some();

        let (vs, vt, lifted) = if s_is_child && t_is_child {
            // Both direct children — original sibling edge.
            (e.source.clone(), e.target.clone(), false)
        } else if s_is_child && !t_in_subtree {
            // Child → external: original. External becomes neighbour.
            (e.source.clone(), e.target.clone(), false)
        } else if t_is_child && !s_in_subtree {
            // External → child: original. External becomes neighbour.
            (e.source.clone(), e.target.clone(), false)
        } else if s_is_child && t_in_subtree {
            // Child → deeper descendant. Lift t to its child-of-focused.
            let pt_id = pt.clone().expect("t_in_subtree");
            if pt_id == e.source {
                // descends from s — keep only if it's a true self-loop.
                if e.source == e.target {
                    (e.source.clone(), e.target.clone(), false)
                } else {
                    continue;
                }
            } else {
                (e.source.clone(), pt_id, true)
            }
        } else if t_is_child && s_in_subtree {
            let ps_id = ps.clone().expect("s_in_subtree");
            if ps_id == e.target {
                if e.source == e.target {
                    (e.source.clone(), e.target.clone(), false)
                } else {
                    continue;
                }
            } else {
                (ps_id, e.target.clone(), true)
            }
        } else if s_in_subtree && t_in_subtree {
            // Both deep descendants of (potentially different) children.
            let ps_id = ps.clone().expect("s_in_subtree");
            let pt_id = pt.clone().expect("t_in_subtree");
            if ps_id == pt_id {
                continue; // intra-child
            }
            (ps_id, pt_id, true)
        } else {
            // Either both external, or one deep + one external. Skip — we
            // do NOT lift to invent a child↔external edge.
            continue;
        };

        if vs.as_str() == focused_id || vt.as_str() == focused_id {
            continue;
        }

        let key = (vs.clone(), vt.clone(), e.label.clone());
        if !emitted.insert(key) {
            continue;
        }

        let edge_id = if lifted {
            format!("lift:{}:{}->{}", e.label, vs, vt)
        } else {
            e.id.clone()
        };

        dep_edges.push(EdgeView {
            id: edge_id,
            source: vs,
            target: vt,
            label: e.label.clone(),
        });

        // Neighbours: the off-child endpoint of an *original* (un-lifted)
        // edge that touches a child of focused.
        if !s_in_subtree {
            neighbours.insert(e.source.clone());
        }
        if !t_in_subtree {
            neighbours.insert(e.target.clone());
        }
    }

    // Defensive: don't double-list things already in view.
    for c in &children {
        neighbours.remove(c);
    }
    neighbours.remove(focused_id);
    if let Some(fp) = &focused_parent {
        neighbours.remove(fp);
    }

    // For each neighbour, decide its compound parent in the cytoscape view.
    //
    // If walking up the neighbour's hierarchy ancestor chain reaches the
    // focused node's parent (`focused_parent`), render the entire chain
    // (excluding focused_parent itself) as nested NeighbourParent compounds
    // that all eventually nest under focused_parent. Otherwise, fall back to
    // the immediate hierarchy parent as a free-floating compound.
    let mut neighbour_compound_parent: HashMap<String, Option<String>> = HashMap::new();
    // chain_compounds: id -> Some(cytoscape parent) if nested, None if free-floating.
    let mut chain_compounds: BTreeMap<String, Option<String>> = BTreeMap::new();

    for n_id in &neighbours {
        let direct_parent = h.parent_of.get(n_id).cloned();

        // Walk upward looking for focused_parent. Stop at focused_id so we
        // don't accidentally cross through the focused subtree.
        let mut chain: Vec<String> = Vec::new();
        let mut hit_fp = false;
        if let Some(fp) = focused_parent.as_deref() {
            let mut cur = direct_parent.clone();
            while let Some(c) = cur {
                if c.as_str() == fp {
                    hit_fp = true;
                    break;
                }
                if c.as_str() == focused_id {
                    break;
                }
                chain.push(c.clone());
                cur = h.parent_of.get(&c).cloned();
            }
        }

        if hit_fp {
            // chain[0] is N's direct parent (when non-empty); chain.last()'s
            // hierarchy parent is focused_parent. If chain is empty, N's
            // direct parent IS focused_parent.
            let np = if chain.is_empty() {
                focused_parent.clone()
            } else {
                Some(chain[0].clone())
            };
            neighbour_compound_parent.insert(n_id.clone(), np);
            for (i, c) in chain.iter().enumerate() {
                let cp = if i + 1 < chain.len() {
                    Some(chain[i + 1].clone())
                } else {
                    focused_parent.clone()
                };
                chain_compounds.entry(c.clone()).or_insert(cp);
            }
        } else {
            // Fallback: render N's direct parent as a free-floating compound,
            // unless that parent is already in view (focused, focused_parent,
            // or a child of focused) in which case just point at it directly.
            match direct_parent {
                None => {
                    neighbour_compound_parent.insert(n_id.clone(), None);
                }
                Some(p)
                    if p.as_str() == focused_id
                        || Some(p.as_str()) == focused_parent.as_deref()
                        || child_set.contains(p.as_str()) =>
                {
                    neighbour_compound_parent.insert(n_id.clone(), Some(p));
                }
                Some(p) => {
                    chain_compounds.entry(p.clone()).or_insert(None);
                    neighbour_compound_parent.insert(n_id.clone(), Some(p));
                }
            }
        }
    }

    // Assemble node views
    let mut nodes_out: Vec<NodeView> = Vec::new();
    let to_view = |n: &Node, role: NodeRole, parent: Option<String>| NodeView {
        id: n.id.clone(),
        label: primary_label(n).to_string(),
        name: n
            .properties
            .get("simpleName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| n.id.clone()),
        parent,
        role,
        properties: n.properties.clone(),
    };

    // focused_parent compound (if any). Resolved via resolve_node so the
    // synthetic forest can also act as the parent compound for a real root
    // when the hierarchy has more than one root.
    if let Some(fp_id) = &focused_parent {
        if let Some(n) = resolve_node(&by_id, fp_id) {
            nodes_out.push(to_view(&n, NodeRole::FocusedParent, None));
        }
    }
    // focused
    nodes_out.push(to_view(
        focused,
        NodeRole::Focused,
        focused_parent.clone(),
    ));
    // children inside focused
    for c_id in &children {
        if let Some(n) = by_id.get(c_id.as_str()).copied() {
            nodes_out.push(to_view(n, NodeRole::Child, Some(focused_id.to_string())));
        }
    }
    // chain compounds (neighbour ancestors), each with its own cytoscape
    // parent so the nesting chains naturally up to focused_parent.
    for (c_id, cp) in &chain_compounds {
        if c_id.as_str() == focused_id {
            continue;
        }
        if Some(c_id.as_str()) == focused_parent.as_deref() {
            continue;
        }
        if child_set.contains(c_id.as_str()) {
            continue;
        }
        if neighbours.contains(c_id) {
            continue;
        }
        if let Some(n) = by_id.get(c_id.as_str()).copied() {
            nodes_out.push(to_view(n, NodeRole::NeighbourParent, cp.clone()));
    }
    }
    // neighbours
    for n_id in &neighbours {
        if let Some(n) = by_id.get(n_id.as_str()).copied() {
            let parent = neighbour_compound_parent
                .get(n_id)
                .cloned()
                .flatten();
            nodes_out.push(to_view(n, NodeRole::Neighbour, parent));
        }
    }

    // Breadcrumb: walk parents up from focused. When the hierarchy is a
    // forest (multi-root), the synthetic forest is appended as the topmost
    // crumb so the user can always click their way back to the all-roots view.
    let mut bc: Vec<NodeView> = Vec::new();
    let mut cur = Some(focused_id.to_string());
    while let Some(c) = cur {
        if let Some(n) = resolve_node(&by_id, c.as_str()) {
            bc.push(to_view(&n, NodeRole::Focused, None));
        }
        cur = if c == FOREST_ROOT_ID {
            None
        } else {
            let real_parent = h.parent_of.get(&c).cloned();
            if real_parent.is_none() && multi_root && h.roots.iter().any(|r| r == &c) {
                Some(FOREST_ROOT_ID.to_string())
            } else {
                real_parent
            }
        };
    }
    bc.reverse();

    Ok(FocusedView {
        focused_id: focused_id.to_string(),
        nodes: nodes_out,
        edges: dep_edges,
        breadcrumb: bc,
    })
}

// -----------------------------------------------------------------------------
// Tauri commands
// -----------------------------------------------------------------------------

#[tauri::command]
pub fn parse_graph(
    state: State<AppState>,
    json: String,
) -> Result<GraphSummary, String> {
    let elements = parse_elements(&json)?;
    let summary = summarize(&elements);
    *state
        .elements
        .lock()
        .map_err(|e| format!("state lock: {e}"))? = Some(elements);
    Ok(summary)
}

#[tauri::command]
pub fn build_hierarchy(
    state: State<AppState>,
    schema: HierarchySchema,
) -> Result<HierarchyIndex, String> {
    let guard = state.elements.lock().map_err(|e| format!("state lock: {e}"))?;
    let elements = guard.as_ref().ok_or("no graph loaded")?;
    Ok(build_hierarchy_index(elements, &schema))
}

#[derive(Debug, Serialize)]
pub struct NeighborResult {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[tauri::command]
pub fn get_neighbors(
    state: State<AppState>,
    node_ids: Vec<String>,
    edge_filters: Vec<String>,
) -> Result<NeighborResult, String> {
    let guard = state.elements.lock().map_err(|e| format!("state lock: {e}"))?;
    let elements = guard.as_ref().ok_or("no graph loaded")?;
    let target_set: HashSet<&str> = node_ids.iter().map(|s| s.as_str()).collect();
    let label_set: HashSet<&str> = edge_filters.iter().map(|s| s.as_str()).collect();
    let mut nodes_out: HashMap<String, Node> = HashMap::new();
    let mut edges_out: Vec<Edge> = Vec::new();
    let by_id: HashMap<&str, &Node> = elements.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    for e in &elements.edges {
        if !label_set.is_empty() && !label_set.contains(e.label.as_str()) {
            continue;
        }
        let touches = target_set.contains(e.source.as_str())
            || target_set.contains(e.target.as_str());
        if !touches {
            continue;
        }
        edges_out.push(e.clone());
        for id in [&e.source, &e.target] {
            if let Some(n) = by_id.get(id.as_str()).copied() {
                nodes_out.entry(id.clone()).or_insert_with(|| n.clone());
            }
        }
    }
    Ok(NeighborResult {
        nodes: nodes_out.into_values().collect(),
        edges: edges_out,
    })
}

#[tauri::command]
pub fn get_node(state: State<AppState>, id: String) -> Result<Option<Node>, String> {
    let guard = state.elements.lock().map_err(|e| format!("state lock: {e}"))?;
    let elements = guard.as_ref().ok_or("no graph loaded")?;
    Ok(elements.nodes.iter().find(|n| n.id == id).cloned())
}

#[tauri::command]
pub fn get_focused_view(
    state: State<AppState>,
    schema: HierarchySchema,
    enabled_deps: Vec<String>,
    focused_id: String,
) -> Result<FocusedView, String> {
    let guard = state.elements.lock().map_err(|e| format!("state lock: {e}"))?;
    let elements = guard.as_ref().ok_or("no graph loaded")?;
    let enabled: HashSet<String> = enabled_deps.into_iter().collect();
    focused_view(elements, &schema, &enabled, &focused_id)
}
