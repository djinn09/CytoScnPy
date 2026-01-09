use crate::analyzer::semantic::graph::{EdgeType, SemanticGraph};
use petgraph::graph::NodeIndex;
use petgraph::Direction;
use std::collections::{HashMap, HashSet};

/// Result of an impact analysis.
#[derive(Debug, Clone)]
pub struct ImpactResult {
    /// The set of nodes impacted by the change.
    pub impacted_nodes: HashSet<NodeIndex>,
    /// A mapping of node -> nodes that directly depend on it (reverse adjacency list subset).
    /// Used to reconstruct the impact tree.
    pub dependency_tree: HashMap<NodeIndex, Vec<NodeIndex>>,
}

pub struct ImpactAnalyzer<'a> {
    graph: &'a SemanticGraph,
}

impl<'a> ImpactAnalyzer<'a> {
    pub fn new(graph: &'a SemanticGraph) -> Self {
        Self { graph }
    }

    /// Computes which nodes are impacted if `target_node` changes.
    ///
    /// Impact propagates backwards along edges:
    /// If A calls B, and B changes, A is impacted.
    /// So we traverse Incoming edges from B.
    pub fn compute_impact(&self, target_node: NodeIndex) -> ImpactResult {
        let graph_read = self.graph.graph.read().unwrap();
        let mut impacted_nodes = HashSet::new();
        let mut dependency_tree = HashMap::new();

        // Check if node exists
        if graph_read.node_weight(target_node).is_none() {
            return ImpactResult {
                impacted_nodes,
                dependency_tree,
            };
        }

        // BFS in Incoming direction (Reverse BFS)
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(target_node);
        impacted_nodes.insert(target_node); // The node itself is the root of the impact

        while let Some(current) = queue.pop_front() {
            // Find all nodes that point TO current
            let mut dependents = Vec::new();

            // neighbors_directed(Incoming) gives nodes n where n -> current
            for neighbor in graph_read.neighbors_directed(current, Direction::Incoming) {
                if !impacted_nodes.contains(&neighbor) {
                    impacted_nodes.insert(neighbor);
                    queue.push_back(neighbor);
                }

                // Record the relationship for the tree (neighbor depends on current)
                dependents.push(neighbor);
            }

            if !dependents.is_empty() {
                dependency_tree.insert(current, dependents);
            }
        }

        ImpactResult {
            impacted_nodes,
            dependency_tree,
        }
    }

    /// Generates a textual representation of the impact tree (ASCII tree).
    pub fn format_impact_tree(&self, root: NodeIndex, result: &ImpactResult) -> String {
        let graph = self.graph.graph.read().unwrap();
        let mut output = String::new();

        if let Some(symbol) = graph.node_weight(root) {
            output.push_str(&format!("Impact Analysis for {}:\n", symbol.fqn));
            self.format_tree_recursive(&*graph, root, &result.dependency_tree, 0, &mut output);
        }

        output
    }

    fn format_tree_recursive(
        &self,
        graph: &petgraph::graph::Graph<crate::graph::symbols::SymbolInfo, EdgeType>,
        current: NodeIndex,
        tree: &HashMap<NodeIndex, Vec<NodeIndex>>,
        depth: usize,
        output: &mut String,
    ) {
        if let Some(children) = tree.get(&current) {
            for &child in children {
                let indent = "  ".repeat(depth + 1);
                if let Some(child_sym) = graph.node_weight(child) {
                    output.push_str(&format!("{}- {}\n", indent, child_sym.fqn));
                    if depth < 20 {
                        self.format_tree_recursive(graph, child, tree, depth + 1, output);
                    }
                }
            }
        }
    }

    /// Converts ImpactResult to a serializable JSON-friendly struct.
    pub fn to_json(&self, result: &ImpactResult) -> ImpactJson {
        let graph = self.graph.graph.read().unwrap();
        let impacted_fqns: Vec<String> = result
            .impacted_nodes
            .iter()
            .filter_map(|&idx| graph.node_weight(idx).map(|s| s.fqn.clone()))
            .collect();

        let mut dependency_tree_map = HashMap::new();
        for (parent, children) in &result.dependency_tree {
            if let Some(parent_sym) = graph.node_weight(*parent) {
                let child_fqns: Vec<String> = children
                    .iter()
                    .filter_map(|&c| graph.node_weight(c).map(|s| s.fqn.clone()))
                    .collect();
                dependency_tree_map.insert(parent_sym.fqn.clone(), child_fqns);
            }
        }

        ImpactJson {
            impacted_symbols: impacted_fqns,
            dependency_tree: dependency_tree_map,
        }
    }
}

/// JSON-serializable representation of impact analysis.
#[derive(Debug, serde::Serialize)]
pub struct ImpactJson {
    pub impacted_symbols: Vec<String>,
    pub dependency_tree: HashMap<String, Vec<String>>,
}
