use leptos::*;
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::Deserialize;
use std::collections::HashMap;

// ── Data model (what the AI produces) ────────────────────────────────────────

#[derive(Deserialize, Clone, Debug)]
pub struct WorkflowGraph {
    pub nodes: Vec<WfNode>,
    pub edges: Vec<WfEdge>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct WfNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub kind: NodeKind,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Start,
    #[default]
    Process,
    Decision,
    End,
}

#[derive(Deserialize, Clone, Debug)]
pub struct WfEdge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
}

// ── Layout ────────────────────────────────────────────────────────────────────

const NODE_W: f32 = 140.0;
const NODE_H: f32 = 44.0;
const ROW_GAP: f32 = 90.0;   // vertical spacing between ranks
const COL_GAP: f32 = 32.0;   // horizontal gap between nodes on same rank

struct LayoutNode {
    id: String,
    label: String,
    kind: NodeKind,
    x: f32,
    y: f32,
}

struct LayoutEdge {
    x1: f32, y1: f32,
    cx1: f32, cy1: f32,
    cx2: f32, cy2: f32,
    x2: f32, y2: f32,
    label: Option<String>,
}

struct Layout {
    nodes: Vec<LayoutNode>,
    edges: Vec<LayoutEdge>,
    width: f32,
    height: f32,
}

fn compute_layout(graph: &WorkflowGraph) -> Layout {
    if graph.nodes.is_empty() {
        return Layout { nodes: vec![], edges: vec![], width: 0.0, height: 0.0 };
    }

    // Build petgraph DiGraph
    let mut pg: DiGraph<String, ()> = DiGraph::new();
    let mut id_to_idx: HashMap<String, NodeIndex> = HashMap::new();
    for n in &graph.nodes {
        let idx = pg.add_node(n.id.clone());
        id_to_idx.insert(n.id.clone(), idx);
    }
    for e in &graph.edges {
        if let (Some(&a), Some(&b)) = (id_to_idx.get(&e.from), id_to_idx.get(&e.to)) {
            pg.add_edge(a, b, ());
        }
    }

    // Rank = longest path from any source (BFS rank assignment)
    let topo = toposort(&pg, None).unwrap_or_else(|_| {
        pg.node_indices().collect()
    });

    let mut rank: HashMap<NodeIndex, usize> = HashMap::new();
    for idx in &topo {
        let r = pg.neighbors_directed(*idx, petgraph::Direction::Incoming)
            .filter_map(|p| rank.get(&p).copied())
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        rank.insert(*idx, r);
    }

    // Group by rank
    let max_rank = rank.values().copied().max().unwrap_or(0);
    let mut by_rank: Vec<Vec<NodeIndex>> = vec![vec![]; max_rank + 1];
    for idx in pg.node_indices() {
        by_rank[rank[&idx]].push(idx);
    }

    // Compute positions — nodes centered per rank
    let max_cols = by_rank.iter().map(|r| r.len()).max().unwrap_or(1);
    let canvas_w = max_cols as f32 * (NODE_W + COL_GAP) - COL_GAP + 40.0;

    let mut positions: HashMap<NodeIndex, (f32, f32)> = HashMap::new();
    for (r, row) in by_rank.iter().enumerate() {
        let row_total = row.len() as f32 * (NODE_W + COL_GAP) - COL_GAP;
        let row_start_x = (canvas_w - row_total) / 2.0;
        let y = 20.0 + r as f32 * (NODE_H + ROW_GAP);
        for (col, idx) in row.iter().enumerate() {
            let x = row_start_x + col as f32 * (NODE_W + COL_GAP);
            positions.insert(*idx, (x, y));
        }
    }

    // Build LayoutNodes
    let node_map: HashMap<String, &WfNode> = graph.nodes.iter().map(|n| (n.id.clone(), n)).collect();
    let layout_nodes: Vec<LayoutNode> = pg.node_indices().map(|idx| {
        let id = &pg[idx];
        let (x, y) = positions[&idx];
        let wfn = node_map[id.as_str()];
        LayoutNode { id: id.clone(), label: wfn.label.clone(), kind: wfn.kind.clone(), x, y }
    }).collect();

    // Build LayoutEdges with cubic bezier
    let pos_by_id: HashMap<String, (f32, f32)> = layout_nodes.iter()
        .map(|n| (n.id.clone(), (n.x, n.y)))
        .collect();

    let layout_edges: Vec<LayoutEdge> = graph.edges.iter().filter_map(|e| {
        let &(x1, y1) = pos_by_id.get(&e.from)?;
        let &(x2, y2) = pos_by_id.get(&e.to)?;
        let cx1 = x1 + NODE_W / 2.0;
        let cy1 = y1 + NODE_H;
        let cx2 = x2 + NODE_W / 2.0;
        let cy2 = y2;
        let ctrl_y = (cy1 + cy2) / 2.0;
        Some(LayoutEdge {
            x1: cx1, y1: cy1,
            cx1, cy1: ctrl_y,
            cx2, cy2: ctrl_y,
            x2: cx2, y2: cy2,
            label: e.label.clone(),
        })
    }).collect();

    let canvas_h = 20.0 + (max_rank + 1) as f32 * (NODE_H + ROW_GAP) - ROW_GAP + 20.0;

    Layout { nodes: layout_nodes, edges: layout_edges, width: canvas_w, height: canvas_h }
}

// ── SVG renderer ──────────────────────────────────────────────────────────────

#[component]
pub fn WorkflowCanvas(graph: WorkflowGraph) -> impl IntoView {
    let layout = compute_layout(&graph);
    let w = layout.width;
    let h = layout.height;

    let vbox = format!("0 0 {w:.0} {h:.0}");

    view! {
        <svg
            viewBox=vbox
            style="width:100%;height:auto;overflow:visible"
            xmlns="http://www.w3.org/2000/svg"
        >
            <defs>
                <marker
                    id="arrow"
                    markerWidth="8" markerHeight="8"
                    refX="6" refY="3"
                    orient="auto"
                    markerUnits="strokeWidth"
                >
                    <path d="M0,0 L0,6 L8,3 z" fill="#dc2626" opacity="0.7"/>
                </marker>
            </defs>

            // Edges
            {layout.edges.into_iter().map(|e| {
                let d = format!(
                    "M {:.1},{:.1} C {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
                    e.x1, e.y1, e.cx1, e.cy1, e.cx2, e.cy2, e.x2, e.y2
                );
                let mid_x = (e.x1 + e.x2) / 2.0;
                let mid_y = (e.y1 + e.y2) / 2.0;
                view! {
                    <g>
                        <path
                            d=d
                            fill="none"
                            stroke="#dc2626"
                            stroke-width="1.5"
                            stroke-opacity="0.6"
                            marker-end="url(#arrow)"
                        />
                        {e.label.map(|lbl| view! {
                            <text
                                x=mid_x y=mid_y
                                text-anchor="middle"
                                dominant-baseline="middle"
                                font-size="10"
                                fill="#9ca3af"
                                class="select-none"
                            >{lbl}</text>
                        })}
                    </g>
                }
            }).collect_view()}

            // Nodes
            {layout.nodes.into_iter().map(|n| {
                let (fill, stroke, text_fill) = node_colors(&n.kind);
                let rx = if n.kind == NodeKind::Decision { NODE_W / 2.0 } else { 8.0 };
                view! {
                    <g>
                        <rect
                            x=n.x y=n.y
                            width=NODE_W height=NODE_H
                            rx=rx ry=rx
                            fill=fill
                            stroke=stroke
                            stroke-width="1.5"
                        />
                        <text
                            x=n.x + NODE_W / 2.0
                            y=n.y + NODE_H / 2.0
                            text-anchor="middle"
                            dominant-baseline="middle"
                            font-size="12"
                            font-family="Ubuntu, sans-serif"
                            fill=text_fill
                            class="select-none"
                        >{n.label}</text>
                    </g>
                }
            }).collect_view()}
        </svg>
    }
}

fn node_colors(kind: &NodeKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        NodeKind::Start    => ("#dc2626", "#b91c1c", "#ffffff"),
        NodeKind::End      => ("#111827", "#374151", "#f9fafb"),
        NodeKind::Decision => ("#fef3c7", "#d97706", "#92400e"),
        NodeKind::Process  => ("var(--bg-secondary,#f5f5f5)", "#e5e7eb", "var(--text-primary,#111)"),
    }
}
