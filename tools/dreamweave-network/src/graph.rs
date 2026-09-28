//! Layered dependency maps as static SVG.
//!
//! The graphs on this site are small and structural: a project, what it needs, what needs it.
//! A layered drawing (dependents above their dependencies) reads well at that size, and drawing
//! it here keeps the build free of a headless browser and the pages free of a graph framework.
//! Every graph is supplemental. The tables beside it are the interface that has to be right.
//!
//! Layout: longest-path layering from the nodes nothing points at, then a few barycenter sweeps
//! to untangle each layer, then straight arithmetic. Deterministic: the same graph always draws
//! the same bytes.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Focus,
    Project,
    Foundation,
    External,
    Capability,
}

impl NodeKind {
    fn class(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Project => "project",
            Self::Foundation => "foundation",
            Self::External => "external",
            Self::Capability => "capability",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub href: Option<String>,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// `requires`, `recommends`, `conflicts`, `compatible`, `replaces` or `provides`.
    pub kind: String,
}

#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

const NODE_WIDTH: usize = 196;
const NODE_HEIGHT: usize = 48;
const COLUMN_GAP: usize = 26;
const ROW_GAP: usize = 70;
const MARGIN: usize = 16;
const LABEL_CHARACTERS: usize = 24;

impl Graph {
    pub fn add_node(&mut self, node: Node) {
        if !self.nodes.iter().any(|existing| existing.id == node.id) {
            self.nodes.push(node);
        }
    }

    pub fn add_edge(&mut self, from: &str, to: &str, kind: &str) {
        let edge = Edge {
            from: from.to_owned(),
            to: to.to_owned(),
            kind: kind.to_owned(),
        };
        if from != to && !self.edges.contains(&edge) {
            self.edges.push(edge);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    fn layers(&self) -> Vec<Vec<usize>> {
        let index: BTreeMap<&str, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(position, node)| (node.id.as_str(), position))
            .collect();
        let edges: Vec<(usize, usize)> = self
            .edges
            .iter()
            .filter_map(|edge| {
                Some((
                    *index.get(edge.from.as_str())?,
                    *index.get(edge.to.as_str())?,
                ))
            })
            .collect();
        // Longest path, bounded by the node count so a cycle cannot loop forever: a back edge
        // simply stops pushing its target down once every node has been tried.
        let mut layer = vec![0usize; self.nodes.len()];
        for _ in 0..self.nodes.len() {
            let mut moved = false;
            for &(from, to) in &edges {
                if layer[to] < layer[from] + 1 && layer[from] + 1 < self.nodes.len() {
                    layer[to] = layer[from] + 1;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        let depth = layer.iter().copied().max().unwrap_or(0) + 1;
        let mut layers: Vec<Vec<usize>> = vec![Vec::new(); depth];
        let mut order: Vec<usize> = (0..self.nodes.len()).collect();
        order.sort_by(|left, right| {
            self.nodes[*left]
                .label
                .to_lowercase()
                .cmp(&self.nodes[*right].label.to_lowercase())
                .then_with(|| self.nodes[*left].id.cmp(&self.nodes[*right].id))
        });
        for node in order {
            layers[layer[node]].push(node);
        }
        for _ in 0..4 {
            for row in 1..layers.len() {
                reorder(&mut layers, row, row - 1, &edges, true);
            }
            for row in (0..layers.len().saturating_sub(1)).rev() {
                reorder(&mut layers, row, row + 1, &edges, false);
            }
        }
        layers.retain(|row| !row.is_empty());
        layers
    }

    /// Where every node goes, and how big the picture is.
    fn place(&self, layers: &[Vec<usize>]) -> (usize, usize, Vec<(usize, usize)>) {
        let widest = layers.iter().map(Vec::len).max().unwrap_or(1);
        let width = MARGIN * 2 + widest * NODE_WIDTH + (widest - 1) * COLUMN_GAP;
        let height = MARGIN * 2 + layers.len() * NODE_HEIGHT + (layers.len() - 1) * ROW_GAP;
        let mut position = vec![(0, 0); self.nodes.len()];
        for (row, members) in layers.iter().enumerate() {
            let used = members.len() * NODE_WIDTH + (members.len() - 1) * COLUMN_GAP;
            let start = (width - used) / 2;
            for (column, node) in members.iter().enumerate() {
                position[*node] = (
                    start + column * (NODE_WIDTH + COLUMN_GAP),
                    MARGIN + row * (NODE_HEIGHT + ROW_GAP),
                );
            }
        }
        (width, height, position)
    }

    /// The SVG, or `None` for a graph without edges, which is not worth a picture.
    pub fn render(&self, title: &str, description: &str) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let layers = self.layers();
        let (width, height, position) = self.place(&layers);
        let index: BTreeMap<&str, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(number, node)| (node.id.as_str(), number))
            .collect();

        let mut svg = String::new();
        let _ = write!(
            svg,
            r#"<svg class="net-graph" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" role="img" aria-label="{}"><title>{}</title><desc>{}</desc>"#,
            escape(title),
            escape(title),
            escape(description)
        );
        let kinds: BTreeSet<&str> = self.edges.iter().map(|edge| edge.kind.as_str()).collect();
        svg.push_str("<defs>");
        for kind in &kinds {
            let _ = write!(
                svg,
                r#"<marker id="net-arrow-{kind}" class="net-graph__arrow net-graph__arrow--{kind}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z"/></marker>"#
            );
        }
        svg.push_str("</defs>");

        let mut edges = self.edges.clone();
        edges.sort();
        for edge in &edges {
            let (Some(&from), Some(&to)) =
                (index.get(edge.from.as_str()), index.get(edge.to.as_str()))
            else {
                continue;
            };
            let _ = write!(
                svg,
                r#"<path class="net-graph__edge net-graph__edge--{kind}" d="{path}" marker-end="url(#net-arrow-{kind})"><title>{from} {kind} {to}</title></path>"#,
                kind = edge.kind,
                path = edge_path(position[from], position[to]),
                from = escape(&self.nodes[from].label),
                to = escape(&self.nodes[to].label),
            );
        }
        for (number, node) in self.nodes.iter().enumerate() {
            svg.push_str(&node_markup(node, position[number]));
        }
        svg.push_str("</svg>");
        Some(svg)
    }
}

fn edge_path((from_x, from_y): (usize, usize), (to_x, to_y): (usize, usize)) -> String {
    if to_y > from_y {
        let (start_x, end_x) = (from_x + NODE_WIDTH / 2, to_x + NODE_WIDTH / 2);
        let start_y = from_y + NODE_HEIGHT;
        let end_y = to_y - 2;
        let bend = (end_y - start_y) / 2;
        format!(
            "M{start_x} {start_y}C{start_x} {} {end_x} {} {end_x} {end_y}",
            start_y + bend,
            end_y - bend
        )
    } else {
        // Same row or pointing up (a cycle): swing out to the side.
        let start_y = from_y + NODE_HEIGHT / 2;
        let end_y = to_y + NODE_HEIGHT / 2;
        let side = from_x.max(to_x) + NODE_WIDTH + COLUMN_GAP / 2;
        format!(
            "M{} {start_y}C{side} {start_y} {side} {end_y} {} {end_y}",
            from_x + NODE_WIDTH,
            to_x + NODE_WIDTH + 2
        )
    }
}

fn node_markup(node: &Node, (x, y): (usize, usize)) -> String {
    let mut markup = String::new();
    if let Some(href) = &node.href {
        let _ = write!(markup, r#"<a href="{}">"#, escape(href));
    }
    let _ = write!(
        markup,
        r#"<g class="net-graph__node net-graph__node--{class}"><title>{label}{detail}</title><rect x="{x}" y="{y}" width="{NODE_WIDTH}" height="{NODE_HEIGHT}" rx="3"/><text class="net-graph__label" x="{text_x}" y="{label_y}">{short}</text><text class="net-graph__detail" x="{text_x}" y="{detail_y}">{detail_short}</text></g>"#,
        class = node.kind.class(),
        label = escape(&node.label),
        detail = if node.detail.is_empty() {
            String::new()
        } else {
            format!(" · {}", escape(&node.detail))
        },
        text_x = x + 10,
        label_y = y + 20,
        detail_y = y + 37,
        short = escape(&truncate(&node.label, LABEL_CHARACTERS)),
        detail_short = escape(&truncate(&node.detail, LABEL_CHARACTERS + 4)),
    );
    if node.href.is_some() {
        markup.push_str("</a>");
    }
    markup
}

fn reorder(
    layers: &mut [Vec<usize>],
    row: usize,
    reference: usize,
    edges: &[(usize, usize)],
    downward: bool,
) {
    let positions: BTreeMap<usize, usize> = layers[reference]
        .iter()
        .enumerate()
        .map(|(position, node)| (*node, position))
        .collect();
    let mut keyed: Vec<(usize, usize, usize)> = layers[row]
        .iter()
        .enumerate()
        .map(|(current, node)| {
            let neighbours: Vec<usize> = edges
                .iter()
                .filter_map(|&(from, to)| {
                    let other = if downward {
                        (to == *node).then_some(from)?
                    } else {
                        (from == *node).then_some(to)?
                    };
                    positions.get(&other).copied()
                })
                .collect();
            // Barycenter scaled by 1000 to stay in integers; nodes without neighbours keep
            // their place.
            let key = if neighbours.is_empty() {
                current * 1000
            } else {
                neighbours.iter().sum::<usize>() * 1000 / neighbours.len()
            };
            (key, current, *node)
        })
        .collect();
    keyed.sort_unstable();
    layers[row] = keyed.into_iter().map(|(_, _, node)| node).collect();
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(limit - 1).collect();
    short.push('…');
    short
}

pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, label: &str) -> Node {
        Node {
            id: id.to_owned(),
            label: label.to_owned(),
            detail: "mod".to_owned(),
            href: Some(format!("/projects/{id}/")),
            kind: NodeKind::Project,
        }
    }

    #[test]
    fn dependents_sit_above_their_dependencies() {
        let mut graph = Graph::default();
        graph.add_node(node("candlelight", "Candlelight"));
        graph.add_node(node("tallow", "Tallow"));
        graph.add_node(node("lanterns", "Lanterns"));
        graph.add_edge("candlelight", "tallow", "requires");
        graph.add_edge("lanterns", "candlelight", "requires");
        let layers = graph.layers();
        let label = |node: usize| graph.nodes[node].label.as_str();
        assert_eq!(layers.len(), 3);
        assert_eq!(label(layers[0][0]), "Lanterns");
        assert_eq!(label(layers[2][0]), "Tallow");
    }

    #[test]
    fn cycles_terminate_and_still_draw() {
        let mut graph = Graph::default();
        graph.add_node(node("a", "A"));
        graph.add_node(node("b", "B"));
        graph.add_edge("a", "b", "requires");
        graph.add_edge("b", "a", "conflicts");
        assert!(graph.render("Cycle", "A and B").is_some());
    }

    #[test]
    fn labels_are_escaped_and_rendering_is_deterministic() {
        let mut graph = Graph::default();
        graph.add_node(node("x", "<script>alert(1)</script>"));
        graph.add_node(node("y", "Tallow & friends"));
        graph.add_edge("x", "y", "requires");
        let svg = graph.render("A \"graph\"", "d").unwrap();
        assert!(!svg.contains("<script>"));
        assert!(svg.contains("Tallow &amp; friends"));
        assert_eq!(svg, graph.render("A \"graph\"", "d").unwrap());
    }

    #[test]
    fn a_graph_without_edges_is_not_drawn() {
        let mut graph = Graph::default();
        graph.add_node(node("a", "A"));
        assert!(graph.render("Alone", "").is_none());
    }
}
