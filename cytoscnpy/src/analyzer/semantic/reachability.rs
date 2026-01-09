use crate::analyzer::semantic::graph::SemanticGraph;
use petgraph::graph::NodeIndex;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug)]
pub struct ReachabilityResult {
    pub reachable_nodes: HashSet<NodeIndex>,
    /// Map of Node -> Predecessor Node (The node that led to discovery).
    /// Used to reconstruct one possible path from an Entry Point.
    pub predecessors: HashMap<NodeIndex, NodeIndex>,
}

pub struct ReachabilityAnalyzer<'a> {
    graph: &'a SemanticGraph,
}

impl<'a> ReachabilityAnalyzer<'a> {
    pub fn new(graph: &'a SemanticGraph) -> Self {
        Self { graph }
    }

    /// Computes the set of reachable nodes from the registered entry points.
    /// Returns a result containing the reachable set and predecessor map for tracing.
    pub fn compute_reachable(&self) -> ReachabilityResult {
        let graph_read = self.graph.graph.read().unwrap();
        let mut reachable = HashSet::new();
        let mut predecessors = HashMap::new();
        let mut queue = VecDeque::new();

        // Initialize queue with all entry points
        // Sort by confidence or just process all?
        // Process all.
        let entry_points = self.graph.entry_points.read().unwrap();
        for (entry_node, _confidence) in entry_points.iter() {
            if !reachable.contains(entry_node) {
                reachable.insert(*entry_node);
                queue.push_back(*entry_node);
                // Pred of entry point is None (or self?) let's not insert pred for entry roots
            }
        }

        while let Some(current) = queue.pop_front() {
            // Outgoing edges: current -> neighbor (Calls, Imports, etc.)
            // If current calls neighbor, neighbor is reachable.
            for neighbor in graph_read.neighbors(current) {
                if !reachable.contains(&neighbor) {
                    reachable.insert(neighbor);
                    predecessors.insert(neighbor, current);
                    queue.push_back(neighbor);
                }
            }
        }

        ReachabilityResult {
            reachable_nodes: reachable,
            predecessors,
        }
    }

    /// Identifies symbols that are NOT reachable.
    pub fn get_unreachable_nodes(&self, result: &ReachabilityResult) -> Vec<NodeIndex> {
        let graph_read = self.graph.graph.read().unwrap();
        graph_read
            .node_indices()
            .filter(|idx| !result.reachable_nodes.contains(idx))
            .collect()
    }

    /// Generates a call chain (trace) from an entry point to the target node.
    /// Returns a list of nodes [Entry, A, B, ..., Target].
    /// Returns None if node is unreachable or no path found in the provided result/map.
    pub fn get_trace(
        &self,
        target: NodeIndex,
        result: &ReachabilityResult,
    ) -> Option<Vec<NodeIndex>> {
        if !result.reachable_nodes.contains(&target) {
            return None;
        }

        let mut path = Vec::new();
        let mut curr = target;

        path.push(curr);

        // Backtrack using predecessors
        while let Some(&pred) = result.predecessors.get(&curr) {
            // Detect cycles in predecessor map just in case (should happen in BFS tree)
            if pred == curr {
                break;
            } // Self-loop or root
            curr = pred;
            path.push(curr);
        }

        // Check if the last node is truly an entry point (or we stopped at a root)
        // If predecessor map doesn't have entry, it means it's a root.

        path.reverse();
        Some(path)
    }
}
