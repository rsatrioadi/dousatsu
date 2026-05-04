# Dousatsu (洞察)

Dousatsu is a powerful, interactive graph exploration and visualization tool built with **Tauri**, **Rust**, and **Cytoscape.js**. It is designed to provide deep insights into complex hierarchical structures and dependency networks, such as software architectures, organizational charts, or knowledge graphs.

![Dousatsu Screenshot Placeholder](https://via.placeholder.com/800x450.png?text=Dousatsu+Graph+Explorer)

## How to Use

Dousatsu operates on a "Focused View" principle. Instead of showing the entire graph at once (which can be overwhelming), it focuses on a specific node and its immediate hierarchical context.

### 1. Load a Graph
Click **Open File…** and select a JSON file. You can find a sample in `samples/example.json`.

### 2. Define Your Hierarchy (Panel A)
Dousatsu needs to know how nodes are nested. In the **Hierarchy schema** panel:
- Add links that define parent-child relationships (e.g., `Folder --contains--> File`).
- The tool will automatically build a tree based on these rules.
- Click **Build** to apply the schema.

### 3. Navigate the Hierarchy
- **Double-click** a node to drill down into it.
- Use the **Breadcrumbs** at the top to navigate back up.
- Click **Reset View** to return to the root(s).

### 4. Visualize Dependencies (Panel B)
Select which edge labels should be treated as dependencies (e.g., `invokes`, `uses`).
- **Lifting**: If a dependency exists between two deep descendants of different visible nodes, Dousatsu "lifts" that edge to the current level, showing you that a relationship exists between the high-level components.

### 5. Analyze with Color (Panel C)
- **Categorical**: Colors nodes based on the "implements" edge. Useful for seeing which components belong to which architectural layer or category.
- **Gradient**: Select a metric node (e.g., `LCOM`, `Size`) and a property. Dousatsu will calculate a color gradient based on the values in the graph, highlighting hotspots or outliers.

## Input Format

Dousatsu expects a JSON file containing graph elements. The structure generally follows the Cytoscape JSON format:

```json
{
  "elements": {
    "nodes": [
      {
        "data": {
          "id": "node-1",
          "labels": ["Package"],
          "properties": { "simpleName": "MyPackage" }
        }
      }
    ],
    "edges": [
      {
        "data": {
          "id": "edge-1",
          "source": "parent-id",
          "target": "child-id",
          "label": "contains"
        }
      }
    ]
  }
}
```

### Special Labels
- **Metric**: Nodes with this label are treated as metrics for gradient coloring.
- **measures**: Edges with this label connect nodes to `Metric` nodes and should contain numerical values in their properties.

## Tech Stack

- **Frontend**: Vite, Vanilla JavaScript.
- **Graph Engine**: [Cytoscape.js](https://js.cytoscape.org/) + [ELK](https://github.com/cytoscape/cytoscape.js-elk).
- **Backend**: [Rust](https://www.rust-lang.org/) + [Tauri v2](https://tauri.app/).

## Development

```bash
pnpm install
pnpm tauri dev
```

## License

MIT
