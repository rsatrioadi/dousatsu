// Top-level controller: file open → parse → schema → build → render → navigate.
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { readTextFile } from "@tauri-apps/plugin-fs";

import { Sidebar } from "./sidebar.js";
import { createCy, paintFocusedView, runLayout } from "./cy.js";

const FOREST_ROOT_ID = "::dousatsu::forest";

const state = {
  schema: null,
  enabledDeps: [],
  coloring: null, // { kind, byNode, palette, min, max }
  history: [],
  focusedId: null,
  hierarchy: null,
  cy: null,
};

const sidebar = new Sidebar({ onBuild: (cfg) => onBuild(cfg) });

document.getElementById("open-file-btn").addEventListener("click", openFile);
document.getElementById("back-btn").addEventListener("click", goBack);
document.getElementById("reset-view-btn").addEventListener("click", () => {
  if (state.hierarchy && state.hierarchy.roots.length) {
    const rootId = state.hierarchy.roots.length > 1 ? FOREST_ROOT_ID : state.hierarchy.roots[0];
    focusNode(rootId, { resetHistory: true });
  }
});
document.getElementById("run-layout-btn").addEventListener("click", () => {
  if (state.cy) {
    runLayout(state.cy, { algorithm: document.getElementById("layout-algo").value });
  }
});

state.cy = createCy(document.getElementById("cy"));
window.cy = state.cy;
state.cy.on("tap", "node", (evt) => {
  const id = evt.target.id();
  if (id !== state.focusedId) focusNode(id);
});

document.getElementById("layout-algo").addEventListener("change", (e) => {
  runLayout(state.cy, { algorithm: e.target.value });
});

window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  if (state.cy) {
    import("./cy.js").then(({ refreshStyle }) => refreshStyle(state.cy));
    if (state.coloring?.dimensions) {
      const palettes = {};
      for (const dim of state.coloring.dimensions) {
        palettes[dim.id] = makeCategoricalPalette(dim.categories.map((c) => c.id));
      }
      state.coloring.palettes = palettes;
    }
    if (state.focusedId) focusNode(state.focusedId);
    renderLegend();
  }
});

// ---------- File loading ----------
async function openFile() {
  let path;
  try {
    path = await openDialog({
      multiple: false,
      filters: [{ name: "Graph JSON", extensions: ["json"] }],
    });
  } catch (e) {
    console.error("dialog error", e);
    return;
  }
  if (!path) return;
  try {
    const text = await readTextFile(path);
    document.getElementById("file-name").textContent =
      path.split(/[\\/]/).pop();
    const summary = await invoke("parse_graph", { json: text });
    sidebar.setSummary(summary);
  } catch (e) {
    console.error("parse failed", e);
    alert("Failed to parse graph: " + e);
  }
}

// ---------- Build (commit schema, compute hierarchy, paint initial view) ----------
async function onBuild(cfg) {
  state.schema = cfg.schema;
  state.enabledDeps = cfg.enabledDeps;

  try {
    state.hierarchy = await invoke("build_hierarchy", { schema: cfg.schema });
  } catch (e) {
    console.error("build_hierarchy failed", e);
    alert("Hierarchy build failed: " + e);
    return;
  }

  await computeColoring(cfg.coloring);
  renderLegend();

  if (!state.hierarchy.roots.length) {
    alert("Hierarchy yielded no roots — check schema.");
    return;
  }
  state.history = [];
  const rootId = state.hierarchy.roots.length > 1 ? FOREST_ROOT_ID : state.hierarchy.roots[0];
  await focusNode(rootId, { resetHistory: true });
}

async function computeColoring(modeCfg) {
  state.coloring = null;
  const byLabel = modeCfg.byLabel || {};
  if (Object.keys(byLabel).length === 0) return;

  const levelDimensions = {};
  const levelMetrics = {};
  for (const [label, cfg] of Object.entries(byLabel)) {
    if (cfg.type === "dimension") levelDimensions[label] = cfg.id;
    else if (cfg.type === "metric") levelMetrics[label] = { metricId: cfg.metricId, property: cfg.property };
  }

  const coloring = { kind: "mixed" };

  if (Object.keys(levelDimensions).length > 0) {
    try {
      const r = await invoke("compute_coloring_by_levels", { levelDimensions });
      const palettes = {};
      for (const dim of r.dimensions) {
        palettes[dim.id] = makeCategoricalPalette(dim.categories.map((c) => c.id));
      }
      coloring.stopsByNode = r.stops_by_node;
      coloring.dimensionByNode = r.dimension_by_node;
      coloring.dimensions = r.dimensions;
      coloring.palettes = palettes;
    } catch (e) {
      console.error("dimension color failed", e);
    }
  }

  if (Object.keys(levelMetrics).length > 0) {
    try {
      const r = await invoke("compute_coloring_gradient", { levelMetrics });
      coloring.gradientByNode = r.map;
      coloring.gradientRanges = r.ranges;
    } catch (e) {
      console.error("gradient color failed", e);
    }
  }

  state.coloring = coloring;
}

function makeCategoricalPalette(ids) {
  // Distinct hues, soft saturation, fixed lightness — categorical-friendly.
  // NB: no spaces inside hsl(...) so the value is safe to use inside cytoscape's
  // space-separated background-gradient-stop-colors list.
  const out = {};
  const n = Math.max(1, ids.length);
  const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const l = dark ? 40 : 78;
  const s = dark ? 45 : 55;
  for (let i = 0; i < ids.length; i++) {
    const h = Math.round((360 * i) / n);
    out[ids[i]] = `hsl(${h},${s}%,${l}%)`;
  }
  return out;
}

// ---------- Focus navigation ----------
async function focusNode(id, opts = {}) {
  const view = await invoke("get_focused_view", {
    schema: state.schema,
    enabledDeps: state.enabledDeps,
    focusedId: id,
  }).catch((e) => {
    console.error("get_focused_view failed", e);
    alert("View load failed: " + e);
    return null;
  });
  if (!view) return;

  if (!opts.resetHistory && state.focusedId && state.focusedId !== id) {
    state.history.push(state.focusedId);
    if (state.history.length > 50) state.history.shift();
  }
  if (opts.resetHistory) state.history = [];

  state.focusedId = id;
  try {
    paintFocusedView(state.cy, view, state.coloring, { algorithm: document.getElementById("layout-algo").value });
  } catch (e) {
    console.error("paintFocusedView failed", e);
  }
  renderBreadcrumb(view.breadcrumb);
  document.getElementById("back-btn").disabled = state.history.length === 0;
}

async function goBack() {
  if (!state.history.length) return;
  const prev = state.history.pop();
  // Don't push the current id back onto history — we are stepping back.
  state.focusedId = null;
  await focusNode(prev);
}

function renderBreadcrumb(crumbs) {
  const bc = document.getElementById("breadcrumb");
  bc.innerHTML = "";
  crumbs.forEach((c, i) => {
    if (i > 0) {
      const sep = document.createElement("span");
      sep.className = "sep";
      sep.textContent = "›";
      bc.appendChild(sep);
    }
    const el = document.createElement("span");
    el.className = "crumb" + (i === crumbs.length - 1 ? " current" : "");
    el.textContent = c.name;
    el.addEventListener("click", () => {
      if (c.id !== state.focusedId) focusNode(c.id);
    });
    bc.appendChild(el);
  });
}

// ---------- Legend ----------
function renderLegend() {
  const root = document.getElementById("legend");
  root.innerHTML = "";
  if (!state.coloring) return;

  const coloring = state.coloring;

  for (const dim of coloring.dimensions || []) {
    const title = document.createElement("div");
    title.className = "legend-title";
    title.textContent = dim.name;
    root.appendChild(title);
    const palette = coloring.palettes?.[dim.id] || {};
    for (const cat of dim.categories) {
      const row = document.createElement("div");
      row.className = "legend-row";
      const sw = document.createElement("span");
      sw.className = "legend-swatch";
      sw.style.background = palette[cat.id] || "#ccc";
      const name = document.createElement("span");
      name.textContent = cat.name;
      row.appendChild(sw);
      row.appendChild(name);
      root.appendChild(row);
    }
  }

  for (const [label, range] of Object.entries(coloring.gradientRanges || {})) {
    const title = document.createElement("div");
    title.className = "legend-title";
    title.textContent = label;
    root.appendChild(title);
    const bar = document.createElement("div");
    bar.className = "legend-bar";
    root.appendChild(bar);
    const row = document.createElement("div");
    row.className = "legend-row";
    row.style.justifyContent = "space-between";
    row.style.width = "100%";
    const lo = document.createElement("span");
    lo.textContent = fmt(range[0]);
    const hi = document.createElement("span");
    hi.textContent = fmt(range[1]);
    row.appendChild(lo);
    row.appendChild(hi);
    root.appendChild(row);
  }
}

function fmt(x) {
  if (typeof x !== "number" || !isFinite(x)) return "—";
  if (Math.abs(x) >= 1000 || Math.abs(x) < 0.01) return x.toExponential(1);
  return Number.isInteger(x) ? String(x) : x.toFixed(2);
}

// Stash for debugging
window.__dousatsu__ = state;
