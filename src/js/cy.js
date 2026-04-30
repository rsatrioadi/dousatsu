// Cytoscape rendering: setup, styling, layout, focused-view paint.
import cytoscape from "cytoscape";
import elk from "cytoscape-elk";

cytoscape.use(elk);

// Hand-picked colours for the well-known SABO dependency labels. Any other
// label gets a deterministic hash-derived hue via depColor().
const KNOWN_DEP_COLORS = {
  requires:     "#7c5cb3",
  specializes:  "#d97a3b",
  returns:      "#3aaf85",
  instantiates: "#c0a23a",
  typed:        "#b04a6a",
  uses:         "#9c5dbf",
  invokes:      "#c25577",
};

export function depColor(label) {
  if (KNOWN_DEP_COLORS[label]) return KNOWN_DEP_COLORS[label];
  let h = 0;
  for (let i = 0; i < label.length; i++) {
    h = (Math.imul(h, 31) + label.charCodeAt(i)) | 0;
  }
  const hue = ((h % 360) + 360) % 360;
  return `hsl(${hue}, 50%, 45%)`;
}

// Per-node-type fill (used when coloring mode is "none")
const TYPE_COLORS = {
  Folder:    "#cfe2bd",
  File:      "#dbe6c4",
  Type:      "#f0d8a0",
  Operation: "#dce8f5",
  Variable:  "#f1c8b7",
  Category:  "#cfd7e8",
  Metric:    "#cfd0b0",
  Scope:     "#e3d7bf",
  Project:   "#bfd6c2",
};

// Cytoscape shape per primary label
const SHAPE = {
  Folder:    "round-rectangle",
  Package:   "round-rectangle",
  File:      "cut-rectangle",
  Type:      "ellipse",
  Operation: "diamond",
  Variable:  "barrel",
  Category:  "hexagon",
  Metric:    "star",
  Scope:     "round-rectangle",
  Project:   "round-rectangle",
};

const COMPOUND_ROLES = new Set(["focused", "focused_parent", "neighbour_parent"]);

export function createCy(container) {
  const cy = cytoscape({
    container,
    boxSelectionEnabled: false,
    selectionType: "single",
    style: baseStyle(),
  });
  return cy;
}

function baseStyle() {
  return [
    {
      selector: "node",
      style: {
        label: "data(name)",
        "font-family": "-apple-system, BlinkMacSystemFont, Helvetica Neue, sans-serif",
        "font-size": "11px",
        color: "#2b2b2b",
        "text-valign": "center",
        "text-halign": "center",
        "text-outline-color": "#ffffff",
        "text-outline-width": 1.5,
        "text-outline-opacity": 0.7,
        "background-color": "data(_fill)",
        shape: "data(_shape)",
        "border-width": 1,
        "border-color": "#5a87b8",
        "border-opacity": 1,
        width: "label",
        height: "label",
        padding: "8px",
      },
    },
    // Compound parents (focused / parents-of-context)
    {
      selector: "node[_role = 'focused_parent'], node[_role = 'neighbour_parent']",
      style: {
        "background-opacity": 0.45,
        "border-color": "#9aa3ad",
        "border-style": "dashed",
        "text-valign": "top",
        "text-halign": "center",
        "text-margin-y": -4,
        "font-size": "10.5px",
        color: "#555",
        padding: "16px",
        shape: "round-rectangle",
      },
    },
    {
      selector: "node[_role = 'focused']",
      style: {
        "background-opacity": 0.7,
        "border-width": 2.5,
        "border-color": "#3a6da6",
        "font-weight": "bold",
        "font-size": "12px",
        color: "#1d3a5c",
        "text-valign": "top",
        "text-halign": "center",
        "text-margin-y": -4,
        padding: "20px",
        shape: "round-rectangle",
      },
    },
    {
      selector: "node:selected",
      style: {
        "border-color": "#f0c040",
        "border-width": 3,
      },
    },
    // Edge styles, one per dependency type via _color attribute on the edge
    {
      selector: "edge",
      style: {
        "curve-style": "bezier",
        width: 1.4,
        "line-color": "data(_color)",
        "target-arrow-color": "data(_color)",
        "target-arrow-shape": "triangle",
        "arrow-scale": 1,
        opacity: 0.85,
      },
    },
    {
      selector: "edge:selected",
      style: { width: 2.4, opacity: 1 },
    },
  ];
}

function fillFor(node, coloring) {
  const lbl = node.label;
  if (coloring && coloring.kind === "categorical" && coloring.byNode[node.id]) {
    return coloring.palette[coloring.byNode[node.id]] || TYPE_COLORS[lbl] || "#dce8f5";
  }
  if (coloring && coloring.kind === "gradient" && node.id in coloring.byNode) {
    const t = coloring.byNode[node.id];
    return gradientWhiteToBlue(t);
  }
  if (coloring && coloring.kind === "gradient") {
    return "#dcdcdc"; // unmeasured
  }
  return TYPE_COLORS[lbl] || "#dce8f5";
}

function shapeFor(node) {
  return SHAPE[node.label] || "rectangle";
}

function gradientWhiteToBlue(t) {
  // t in [0,1]: white (#ffffff) -> blue (#4a85be)
  const clamp = (x) => Math.max(0, Math.min(1, x));
  t = clamp(t);
  const r = Math.round(255 + (74 - 255) * t);
  const g = Math.round(255 + (133 - 255) * t);
  const b = Math.round(255 + (190 - 255) * t);
  return `rgb(${r},${g},${b})`;
}

// Run an elk layout over the current cytoscape contents.
export function runLayout(cy, opts = {}) {
  if (cy.elements().length === 0) return;
  const layout = cy.layout({
    name: "elk",
    animate: opts.animate ?? "end",
    animationDuration: opts.animationDuration ?? 300,
    fit: opts.fit ?? true,
    padding: 30,
    elk: {
      algorithm: opts.algorithm || "layered",
    },
  });
  layout.run();
}

// Paint a focused view's worth of elements into cytoscape and run layout.
export function paintFocusedView(cy, view, coloring, layoutOpts = {}) {
  cy.elements().remove();

  const nodes = view.nodes.map((n) => ({
      group: "nodes",
      data: {
        id: n.id,
        name: n.name,
        _label: n.label,
        _role: n.role,
        _fill: fillFor(n, coloring),
        _shape: COMPOUND_ROLES.has(n.role) ? "round-rectangle" : shapeFor(n),
      parent: n.parent || undefined,
      },
  }));

  const edges = view.edges.map((e) => ({
    group: "edges",
    data: {
      id: e.id,
      source: e.source,
      target: e.target,
      _label: e.label,
      _color: depColor(e.label),
    },
  }));

  cy.add(nodes);
  cy.add(edges);

  // Initial paint: full randomised layout, no animation (instantly settled).
  runLayout(cy, layoutOpts);
}
