// Sidebar UI: hierarchy schema builder, dependency toggles, coloring config.
import { depColor } from "./cy.js";

export class Sidebar {
  constructor(opts) {
    this.opts = opts; // { onBuild }
    this.summary = null;
    this.schemaLinks = []; // [{source, edge, target}]
    this.enabledDeps = new Set();
    // Labels the user has explicitly unchecked. Persists across re-renders so
    // toggling a label off doesn't get re-enabled when the dep list refreshes.
    this.depsExplicitlyExcluded = new Set();
    this.coloring = { byLabel: {} };

    this._wireDisclosures();
    this._wireBuildBtn();
    this._wireAddLinkBtn();
  }

  _wireDisclosures() {
    document.querySelectorAll(".panel-header").forEach((h) => {
      h.addEventListener("click", () => {
        h.parentElement.classList.toggle("collapsed");
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
    this.depsExplicitlyExcluded = new Set();
    this.enabledDeps = new Set();
    document.getElementById("hierarchy-list").innerHTML = "";
    this._seedDefaultSchema();
    this._renderHierarchy(); // also refreshes the deps list
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
    // Schema edges are taken out of the dep pool — refresh that list.
    this._renderDepsList();
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
      this._renderDepsList();
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
    if (!this.summary) return;

    const usedInSchema = new Set(
      this.schemaLinks.map((l) => l.edge).filter(Boolean)
    );
    const available = (this.summary.edge_labels || []).filter(
      (s) => !usedInSchema.has(s.label)
    );

    // Sync enabledDeps: drop labels that are no longer available; default-add
    // newly available labels unless the user previously unchecked them.
    for (const lbl of [...this.enabledDeps]) {
      if (usedInSchema.has(lbl)) this.enabledDeps.delete(lbl);
    }
    for (const stat of available) {
      if (!this.depsExplicitlyExcluded.has(stat.label)) {
        this.enabledDeps.add(stat.label);
      }
    }

    if (available.length === 0) {
      const hint = document.createElement("div");
      hint.className = "hint";
      hint.textContent =
        "All edge labels are claimed by the hierarchy schema.";
      list.appendChild(hint);
      return;
    }

    for (const stat of available) {
      const row = document.createElement("label");
      row.className = "dep-row";
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = this.enabledDeps.has(stat.label);
      cb.addEventListener("change", () => {
        if (cb.checked) {
          this.enabledDeps.add(stat.label);
          this.depsExplicitlyExcluded.delete(stat.label);
        } else {
          this.enabledDeps.delete(stat.label);
          this.depsExplicitlyExcluded.add(stat.label);
        }
      });
      const swatch = document.createElement("span");
      swatch.className = "swatch";
      swatch.style.background = depColor(stat.label);
      const name = document.createElement("span");
      name.textContent = stat.label;
      const cnt = document.createElement("span");
      cnt.className = "count";
      cnt.textContent = `${stat.count}`;
      row.appendChild(cb);
      row.appendChild(swatch);
      row.appendChild(name);
      row.appendChild(cnt);
      list.appendChild(row);
    }
  }

  _renderColoringConfig() {
    this._renderColorLevels();
  }

  _renderColorLevels() {
    const PREFERRED_ORDER = [
      "Project", "Scope", "Container", "Folder", "Package", "File",
      "Type", "Structure", "Constructor", "Operation", "Variable", "Primitive",
    ];

    const host = document.getElementById("color-levels");
    const hint = document.getElementById("color-hint");
    host.innerHTML = "";
    this.coloring.byLabel = {};

    const dimensions = this.summary.dimensions || [];
    const measurePairs = this.summary.measure_pairs || {};
    const presentLabels = new Set((this.summary.node_labels || []).map(([l]) => l));

    // Build per-label option lists.
    const labelOptions = new Map(); // label -> { dims, metricPairs }
    for (const d of dimensions) {
      for (const l of d.applies_to || []) {
        if (!presentLabels.has(l)) continue;
        if (!labelOptions.has(l)) labelOptions.set(l, { dims: [], metricPairs: [] });
        labelOptions.get(l).dims.push(d);
      }
    }
    for (const [label, pairs] of Object.entries(measurePairs)) {
      if (!presentLabels.has(label)) continue;
      if (!labelOptions.has(label)) labelOptions.set(label, { dims: [], metricPairs: [] });
      labelOptions.get(label).metricPairs.push(...pairs);
    }

    if (labelOptions.size === 0) {
      hint.hidden = false;
      return;
    }
    hint.hidden = true;

    const ordered = [
      ...PREFERRED_ORDER.filter((l) => labelOptions.has(l)),
      ...[...labelOptions.keys()].filter((l) => !PREFERRED_ORDER.includes(l)).sort(),
    ];

    for (const label of ordered) {
      const { dims, metricPairs } = labelOptions.get(label);

      const row = document.createElement("label");
      row.className = "field";
      const tag = document.createElement("span");
      tag.textContent = label;
      const sel = document.createElement("select");

      const blankOpt = document.createElement("option");
      blankOpt.value = "";
      blankOpt.textContent = "—";
      sel.appendChild(blankOpt);

      for (const d of dims) {
        const opt = document.createElement("option");
        opt.value = JSON.stringify({ type: "dimension", id: d.id });
        opt.textContent = d.name;
        sel.appendChild(opt);
      }

      for (const p of metricPairs) {
        const opt = document.createElement("option");
        opt.value = JSON.stringify({ type: "metric", metricId: p.metricId, property: p.property });
        opt.textContent = `${p.metricName} – ${p.property}`;
        sel.appendChild(opt);
      }

      sel.addEventListener("change", (e) => {
        const v = e.target.value;
        if (v) this.coloring.byLabel[label] = JSON.parse(v);
        else delete this.coloring.byLabel[label];
      });

      row.appendChild(tag);
      row.appendChild(sel);
      host.appendChild(row);
    }
  }
}
