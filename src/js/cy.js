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

const isDark = () => window.matchMedia("(prefers-color-scheme: dark)").matches;

// Per-node-type fill (used when coloring mode is "none")
const TYPE_COLORS_LIGHT = {
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

const TYPE_COLORS_DARK = {
  Folder:    "#3a4d2e",
  File:      "#424d2c",
  Type:      "#5c4b1a",
  Operation: "#1e3a5c",
  Variable:  "#5c2e1e",
  Category:  "#2e3a5c",
  Metric:    "#3a3d2e",
  Scope:     "#4d3a2e",
  Project:   "#2e4d3a",
};

function getThemeColor(type) {
  const colors = isDark() ? TYPE_COLORS_DARK : TYPE_COLORS_LIGHT;
  return colors[type] || (isDark() ? "#2a2a2a" : "#dce8f5");
}

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

export function refreshStyle(cy) {
  cy.style(baseStyle());
}

function baseStyle() {
  const dark = isDark();
  return [
    {
      selector: "node",
      style: {
        label: "data(name)",
        "font-family": "-apple-system, BlinkMacSystemFont, Helvetica Neue, sans-serif",
        "font-size": "11px",
        color: dark ? "#e0e0e0" : "#2b2b2b",
        "text-valign": "center",
        "text-halign": "center",
        "text-outline-color": dark ? "#1e1e1e" : "#ffffff",
        "text-outline-width": 1.5,
        "text-outline-opacity": 0.8,
        "background-fill": "linear-gradient",
        "background-gradient-direction": "to-right",
        "background-gradient-stop-colors": "data(_stop_colors)",
        "background-gradient-stop-positions": "data(_stop_positions)",
        shape: "data(_shape)",
        "border-width": 1,
        "border-color": dark ? "#4a85be" : "#5a87b8",
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
        "border-color": dark ? "#555" : "#9aa3ad",
        "border-style": "dashed",
        "text-valign": "top",
        "text-halign": "center",
        "text-margin-y": -4,
        "font-size": "10.5px",
        color: dark ? "#aaa" : "#555",
        padding: "16px",
        shape: "round-rectangle",
      },
    },
    {
      selector: "node[_role = 'focused']",
      style: {
        "background-opacity": dark ? 0.4 : 0.7,
        "border-width": 2.5,
        "border-color": "#3a6da6",
        "font-weight": "bold",
        "font-size": "12px",
        color: dark ? "#87b3dd" : "#1d3a5c",
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
        opacity: dark ? 0.7 : 0.85,
      },
    },
    {
      selector: "edge:selected",
      style: { width: 2.4, opacity: 1 },
    },
  ];
}

/// Solid fill, expressed as a 1-stop gradient so the cytoscape style can stay uniform.
function solidStops(color) {
  return { colors: `${color} ${color}`, positions: "0% 100%" };
}

/// Hard-stop multi-segment gradient: each stop repeats at the segment boundary
/// so adjacent colours don't blend.
function sharpStops(segments) {
  // segments: [{ color, fraction }] with fractions summing to ~1.
  const colors = [];
  const positions = [];
  let acc = 0;
  for (const seg of segments) {
    const start = acc * 100;
    acc += seg.fraction;
    const end = acc * 100;
    colors.push(seg.color, seg.color);
    positions.push(`${start.toFixed(3)}%`, `${end.toFixed(3)}%`);
  }
  return { colors: colors.join(" "), positions: positions.join(" ") };
}

function stopsFor(node, coloring) {
  const lbl = node.label;

  if (coloring && coloring.kind === "dimension") {
    const stops = coloring.stopsByNode[node.id];
    const dimId = coloring.dimensionByNode[node.id];
    if (stops && stops.length && dimId) {
      const palette = coloring.palettes[dimId] || {};
      const segs = stops
        .map((s) => ({
          color: palette[s.category_id] || getThemeColor(lbl),
          fraction: s.fraction,
        }))
        .filter((s) => s.fraction > 0);
      if (segs.length === 1) return solidStops(segs[0].color);
      if (segs.length > 1) return sharpStops(segs);
    }
    return solidStops(getThemeColor(lbl));
  }

  if (coloring && coloring.kind === "gradient" && node.id in coloring.byNode) {
    const t = coloring.byNode[node.id];
    const c = isDark() ? gradientDarkToBlue(t) : gradientWhiteToBlue(t);
    return solidStops(c);
  }
  if (coloring && coloring.kind === "gradient") {
    return solidStops(isDark() ? "#333" : "#dcdcdc");
  }

  return solidStops(getThemeColor(lbl));
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

function gradientDarkToBlue(t) {
  // t in [0,1]: dark grey (#1e1e1e) -> blue (#4a85be)
  const clamp = (x) => Math.max(0, Math.min(1, x));
  t = clamp(t);
  const r = Math.round(30 + (74 - 30) * t);
  const g = Math.round(30 + (133 - 30) * t);
  const b = Math.round(30 + (190 - 30) * t);
  return `rgb(${r},${g},${b})`;
}

// Run an elk layout over the current cytoscape contents.
export function runLayout(cy, opts = {}) {
  if (cy.elements().length === 0) return;
  const layout = cy.layout({
    name: "elk",
    animate: true,
    animationDuration: 300,
    fit: true,
    padding: 30,
    elk: {
      algorithm: opts.algorithm || "box",
    },
  });
  layout.run();
}

// Paint a focused view's worth of elements into cytoscape and run layout.
export function paintFocusedView(cy, view, coloring, layoutOpts = {}) {
  cy.elements().remove();

  const nodes = view.nodes.map((n) => {
    const s = stopsFor(n, coloring);
    return {
      group: "nodes",
      data: {
        id: n.id,
        name: n.name,
        _label: n.label,
        _role: n.role,
        _stop_colors: s.colors,
        _stop_positions: s.positions,
        _shape: COMPOUND_ROLES.has(n.role) ? "round-rectangle" : shapeFor(n),
        parent: n.parent || undefined,
      },
    };
  });

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

  runLayout(cy, layoutOpts);
}
