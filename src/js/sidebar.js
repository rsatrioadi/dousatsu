// Sidebar UI: hierarchy schema builder, dependency toggles, coloring config.
import { DEP_COLORS } from "./cy.js";

const RECOGNISED_DEPS = [
  "requires",
  "specializes",
  "returns",
  "instantiates",
  "typed",
  "uses",
  "invokes",
];

export class Sidebar {
  constructor(opts) {
    this.opts = opts; // { onBuild }
    this.summary = null;
    this.schemaLinks = []; // [{source, edge, target}]
    this.enabledDeps = new Set();
    this.coloring = { mode: "none", metricId: null, property: null };

    this._wireDisclosures();
    this._wireColorRadios();
    this._wireBuildBtn();
    this._wireAddLinkBtn();

    document.getElementById("metric-select").addEventListener("change", (e) => {
      this.coloring.metricId = e.target.value || null;
      this._refreshMetricProps();
    });
    document.getElementById("metric-prop-select").addEventListener("change", (e) => {
      this.coloring.property = e.target.value || null;
    });
  }

  _wireDisclosures() {
    document.querySelectorAll(".panel-header").forEach((h) => {
      h.addEventListener("click", () => {
        h.parentElement.classList.toggle("collapsed");
      });
    });
  }

  _wireColorRadios() {
    document.querySelectorAll('input[name="color-mode"]').forEach((r) => {
      r.addEventListener("change", () => {
        this.coloring.mode = r.value;
        document.getElementById("gradient-config").hidden = r.value !== "gradient";
      });
    });
  }

  _wireBuildBtn() {
    document.getElementById("build-btn").addEventListener("click", () => {
      if (!this.summary) return;
      this.opts.onBuild({
        schema: { links: [...this.schemaLinks] },
        enabledDeps: [...this.enabledDeps],
        coloring: { ...this.coloring },
      });
    });
  }

  _wireAddLinkBtn() {
    document.getElementById("add-link-btn").addEventListener("click", () => {
      if (!this.summary) return;
      this.schemaLinks.push(this._defaultLink());
      this._renderHierarchy();
    });
  }

  _defaultLink() {
    if (!this.summary) return { source: "", edge: "", target: "" };
    // Try to seed with the most populous (source, edge, target) triple
    const top = this.summary.edge_labels[0];
    if (top && top.source_target_pairs.length) {
      const [s, t] = top.source_target_pairs[0];
      return { source: s, edge: top.label, target: t };
    }
    return { source: "", edge: "", target: "" };
  }

  setSummary(summary) {
    this.summary = summary;
    document.getElementById("schema-hint").style.display = "none";
    document.getElementById("add-link-btn").disabled = false;
    document.getElementById("build-btn").disabled = false;
    document.getElementById("reset-view-btn").disabled = false;

    // Seed default schema: every "containment-like" edge that exists.
    this.schemaLinks = [];
    document.getElementById("hierarchy-list").innerHTML = "";
    this._seedDefaultSchema();
    this._renderHierarchy();

    this._renderDepsList();
    this._renderColoringConfig();
  }

  _seedDefaultSchema() {
    // Heuristic: pick all (sLbl, edge, tLbl) triples whose edge label is
    // a structural one (contains/declares/encapsulates) — common SABO containment.
    const structural = new Set([
      "contains",
      "declares",
      "encapsulates",
      "includes",
      "encloses",
      "composes",
    ]);
    for (const stat of this.summary.edge_labels) {
      if (!structural.has(stat.label)) continue;
      for (const [s, t, _c] of stat.source_target_pairs) {
        if (!s || !t) continue;
        this.schemaLinks.push({ source: s, edge: stat.label, target: t });
      }
    }
  }

  _renderHierarchy() {
    const list = document.getElementById("hierarchy-list");
    list.innerHTML = "";
    this.schemaLinks.forEach((link, idx) => {
      list.appendChild(this._linkRow(link, idx));
    });
  }

  _linkRow(link, idx) {
    const li = document.createElement("li");
    li.draggable = true;
    li.dataset.idx = String(idx);

    const handle = document.createElement("span");
    handle.className = "drag-handle";
    handle.textContent = "⋮⋮";
    li.appendChild(handle);

    const sourceSel = this._nodeLabelSelect(link.source);
    const edgeSel = this._edgeLabelSelect(link.edge);
    const targetSel = this._nodeLabelSelect(link.target);

    sourceSel.addEventListener("change", () => {
      this.schemaLinks[idx].source = sourceSel.value;
    });
    edgeSel.addEventListener("change", () => {
      this.schemaLinks[idx].edge = edgeSel.value;
    });
    targetSel.addEventListener("change", () => {
      this.schemaLinks[idx].target = targetSel.value;
    });

    const arrow = document.createElement("span");
    arrow.className = "arrow";
    arrow.textContent = "→";

    li.appendChild(sourceSel);
    li.appendChild(arrow);
    li.appendChild(edgeSel);
    li.appendChild(arrow.cloneNode(true));
    li.appendChild(targetSel);

    const rm = document.createElement("button");
    rm.className = "remove-btn";
    rm.title = "Remove";
    rm.textContent = "×";
    rm.addEventListener("click", () => {
      this.schemaLinks.splice(idx, 1);
      this._renderHierarchy();
    });
    li.appendChild(rm);

    // Drag-and-drop reordering
    li.addEventListener("dragstart", (e) => {
      li.classList.add("dragging");
      e.dataTransfer.setData("text/plain", String(idx));
      e.dataTransfer.effectAllowed = "move";
    });
    li.addEventListener("dragend", () => li.classList.remove("dragging"));
    li.addEventListener("dragover", (e) => e.preventDefault());
    li.addEventListener("drop", (e) => {
      e.preventDefault();
      const fromIdx = Number(e.dataTransfer.getData("text/plain"));
      const toIdx = Number(li.dataset.idx);
      if (Number.isNaN(fromIdx) || Number.isNaN(toIdx) || fromIdx === toIdx) return;
      const [item] = this.schemaLinks.splice(fromIdx, 1);
      this.schemaLinks.splice(toIdx, 0, item);
      this._renderHierarchy();
    });

    return li;
  }

  _nodeLabelSelect(value) {
    const sel = document.createElement("select");
    const labels = this.summary?.node_labels.map(([l]) => l) || [];
    for (const l of labels) {
      const opt = document.createElement("option");
      opt.value = l;
      opt.textContent = l;
      if (l === value) opt.selected = true;
      sel.appendChild(opt);
    }
    return sel;
  }

  _edgeLabelSelect(value) {
    const sel = document.createElement("select");
    const labels = this.summary?.edge_labels.map((s) => s.label) || [];
    for (const l of labels) {
      const opt = document.createElement("option");
      opt.value = l;
      opt.textContent = l;
      if (l === value) opt.selected = true;
      sel.appendChild(opt);
    }
    return sel;
  }

  _renderDepsList() {
    const list = document.getElementById("deps-list");
    list.innerHTML = "";
    const present = new Map(
      (this.summary.edge_labels || []).map((s) => [s.label, s.count])
    );
    this.enabledDeps = new Set();
    for (const lbl of RECOGNISED_DEPS) {
      const count = present.get(lbl) || 0;
      const row = document.createElement("label");
      row.className = "dep-row" + (count === 0 ? " disabled-dep" : "");
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = count > 0;
      cb.disabled = count === 0;
      if (cb.checked) this.enabledDeps.add(lbl);
      cb.addEventListener("change", () => {
        if (cb.checked) this.enabledDeps.add(lbl);
        else this.enabledDeps.delete(lbl);
      });
      const swatch = document.createElement("span");
      swatch.className = "swatch";
      swatch.style.background = DEP_COLORS[lbl] || "#888";
      const name = document.createElement("span");
      name.textContent = lbl;
      const cnt = document.createElement("span");
      cnt.className = "count";
      cnt.textContent = count > 0 ? `${count}` : "—";
      row.appendChild(cb);
      row.appendChild(swatch);
      row.appendChild(name);
      row.appendChild(cnt);
      list.appendChild(row);
    }
  }

  _renderColoringConfig() {
    const ms = document.getElementById("metric-select");
    ms.innerHTML = "";
    const blank = document.createElement("option");
    blank.value = "";
    blank.textContent = "— select metric —";
    ms.appendChild(blank);
    for (const m of this.summary.metric_nodes || []) {
      const opt = document.createElement("option");
      opt.value = m.id;
      opt.textContent = m.name;
      ms.appendChild(opt);
    }
    this._refreshMetricProps();
  }

  _refreshMetricProps() {
    const ps = document.getElementById("metric-prop-select");
    ps.innerHTML = "";
    const blank = document.createElement("option");
    blank.value = "";
    blank.textContent = "— select property —";
    ps.appendChild(blank);
    for (const p of this.summary?.measure_properties || []) {
      const opt = document.createElement("option");
      opt.value = p;
      opt.textContent = p;
      ps.appendChild(opt);
    }
  }
}
