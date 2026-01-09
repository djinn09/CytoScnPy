use crate::graph::symbols::SymbolInfo;
use dashmap::DashMap;
use petgraph::graph::{Graph, NodeIndex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// Type of relationship between symbols.
#[derive(Debug, Clone, PartialEq)]
pub enum EdgeType {
    Calls,      // Function A calls Function B
    Inherits,   // Class A inherits from Class B
    Imports,    // Module A imports Module B
    References, // Variable A references Variable B
}

/// Hybrid graph structure for semantic analysis.
pub struct SemanticGraph {
    /// Fast FQN lookups (HashMap layer)
    /// Map: FQN -> NodeIndex
    nodes: DashMap<String, NodeIndex>,

    /// Graph algorithms (petgraph layer)
    pub graph: Arc<RwLock<Graph<SymbolInfo, EdgeType>>>,

    /// Entry points for reachability with confidence scores
    pub entry_points: RwLock<Vec<(NodeIndex, u8)>>, // (node, confidence)

    /// Construction phase flag
    is_building: AtomicBool,
}

impl SemanticGraph {
    pub fn new() -> Self {
        Self {
            nodes: DashMap::new(),
            graph: Arc::new(RwLock::new(Graph::new())),
            entry_points: RwLock::new(Vec::new()),
            is_building: AtomicBool::new(true),
        }
    }

    /// Adds a node to the graph if it doesn't exist.
    pub fn add_node(&self, symbol: SymbolInfo) -> NodeIndex {
        if let Some(idx) = self.nodes.get(&symbol.fqn) {
            return *idx.value();
        }

        let mut graph = self.graph.write().unwrap();
        // Double check after lock
        if let Some(idx) = self.nodes.get(&symbol.fqn) {
            return *idx.value();
        }

        let is_entry = symbol.is_entry_point;
        let idx = graph.add_node(symbol);
        self.nodes.insert(graph[idx].fqn.clone(), idx);

        if is_entry {
            self.entry_points.write().unwrap().push((idx, 100));
        }

        idx
    }

    /// Adds an edge between two nodes.
    pub fn add_edge(&self, source: NodeIndex, target: NodeIndex, edge_type: EdgeType) {
        let mut graph = self.graph.write().unwrap();
        graph.add_edge(source, target, edge_type);
    }

    /// Adds a 'Calls' edge.
    pub fn add_call(&self, source: NodeIndex, target: NodeIndex) {
        self.add_edge(source, target, EdgeType::Calls);
    }

    /// Adds an 'Imports' edge.
    pub fn add_import(&self, source: NodeIndex, target: NodeIndex) {
        self.add_edge(source, target, EdgeType::Imports);
    }

    /// Adds an 'Inherits' edge.
    pub fn add_inheritance(&self, source: NodeIndex, target: NodeIndex) {
        self.add_edge(source, target, EdgeType::Inherits);
    }

    /// Retrieves a node index by its FQN.
    pub fn get_node_by_fqn(&self, fqn: &str) -> Option<NodeIndex> {
        self.nodes.get(fqn).map(|idx| *idx.value())
    }

    /// Detects cycles in the graph using Tarjan's SCC algorithm.
    /// Returns a list of generic cycles (strongly connected components with > 1 node, or self-loops).
    pub fn detect_cycles(&self) -> Vec<Vec<NodeIndex>> {
        let graph = self.graph.read().unwrap();
        petgraph::algo::tarjan_scc(&*graph)
            .into_iter()
            .filter(|scc| scc.len() > 1 || (scc.len() == 1 && self.is_self_loop(&graph, scc[0])))
            .collect()
    }

    fn is_self_loop(&self, graph: &Graph<SymbolInfo, EdgeType>, node: NodeIndex) -> bool {
        graph.contains_edge(node, node)
    }

    /// Marks a node as an entry point.
    pub fn mark_entry_point(&self, node: NodeIndex, confidence: u8) {
        self.entry_points.write().unwrap().push((node, confidence));
    }

    pub fn finish_building(&self) {
        self.is_building.store(false, Ordering::SeqCst);
    }

    /// Merges another SemanticGraph (subgraph) into this one.
    /// This is used for parallelized construction where subgraphs are built per module/file.
    pub fn merge(&self, other: SemanticGraph) {
        let mut graph = self.graph.write().unwrap();
        let other_graph = other.graph.read().unwrap();

        // Map old NodeIndex (from other) to new NodeIndex (in self)
        let mut index_map = std::collections::HashMap::new();

        // Merge nodes
        for node_idx in other_graph.node_indices() {
            let symbol = &other_graph[node_idx];
            // If node already exists, reuse it; otherwise add it
            // We can't use self.add_node() directly because we are holding a write lock on graph
            // and self.add_node() tries to acquire it.
            // We must check self.nodes manually.

            let new_idx = if let Some(existing_idx) = self.nodes.get(&symbol.fqn) {
                *existing_idx.value()
            } else {
                let idx = graph.add_node(symbol.clone());
                self.nodes.insert(symbol.fqn.clone(), idx);
                idx
            };
            index_map.insert(node_idx, new_idx);
        }

        // Merge edges
        for edge_idx in other_graph.edge_indices() {
            let (source_old, target_old) = other_graph.edge_endpoints(edge_idx).unwrap();
            let edge_weight = other_graph[edge_idx].clone();

            if let (Some(&source_new), Some(&target_new)) =
                (index_map.get(&source_old), index_map.get(&target_old))
            {
                graph.add_edge(source_new, target_new, edge_weight);
            }
        }

        // Merge entry points
        let mut entry_points = self.entry_points.write().unwrap();
        let other_entry_points = other.entry_points.read().unwrap();
        for (old_idx, confidence) in other_entry_points.iter() {
            if let Some(&new_idx) = index_map.get(old_idx) {
                entry_points.push((new_idx, *confidence));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::symbols::{SymbolInfo, SymbolType};
    use std::path::PathBuf;

    fn create_dummy_symbol(name: &str) -> SymbolInfo {
        SymbolInfo {
            fqn: name.to_string(),
            file_path: PathBuf::from("test.rs"),
            line: 1,
            def_type: SymbolType::Function,
            params: vec![],
            module_path: "test".to_string(),
            is_exported: true,
            is_entry_point: false,
            start_byte: 0,
            end_byte: 10,
            decorators: vec![],
            base_classes: vec![],
        }
    }

    #[test]
    fn test_add_node_and_edge() {
        let graph = SemanticGraph::new();
        let sym1 = create_dummy_symbol("A");
        let sym2 = create_dummy_symbol("B");

        let idx1 = graph.add_node(sym1);
        let idx2 = graph.add_node(sym2);

        graph.add_call(idx1, idx2);

        let inner = graph.graph.read().unwrap();
        assert!(inner.contains_edge(idx1, idx2));
    }

    #[test]
    fn test_cycle_detection() {
        let graph = SemanticGraph::new();
        let sym1 = create_dummy_symbol("A");
        let sym2 = create_dummy_symbol("B");
        let sym3 = create_dummy_symbol("C");

        let idx1 = graph.add_node(sym1);
        let idx2 = graph.add_node(sym2);
        let idx3 = graph.add_node(sym3);

        graph.add_call(idx1, idx2);
        graph.add_call(idx2, idx3);
        graph.add_call(idx3, idx1); // Cycle

        let cycles = graph.detect_cycles();
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 3);
    }

    #[test]
    fn test_merge_graphs() {
        let graph1 = SemanticGraph::new();
        let sym1 = create_dummy_symbol("A");
        let sym2 = create_dummy_symbol("B");
        let idx1 = graph1.add_node(sym1);
        let idx2 = graph1.add_node(sym2);
        graph1.add_call(idx1, idx2);

        let graph2 = SemanticGraph::new();
        let sym3 = create_dummy_symbol("C");
        let sym4 = create_dummy_symbol("D"); // Will link to B in real scenario, but here disjoint
        let idx3 = graph2.add_node(sym3);
        let idx4 = graph2.add_node(sym4);
        graph2.add_call(idx3, idx4);

        // Emulate graph2 referencing a node known to be in graph1 (via name)?
        // In this simple test, we just merge disjoint sets.

        graph1.merge(graph2);

        let inner = graph1.graph.read().unwrap();
        assert_eq!(inner.node_count(), 4);
        assert_eq!(inner.edge_count(), 2);
    }

    #[test]
    fn test_entry_points() {
        let graph = SemanticGraph::new();
        let mut sym1 = create_dummy_symbol("Main");
        sym1.is_entry_point = true;

        let idx1 = graph.add_node(sym1);

        {
            let eps = graph.entry_points.read().unwrap();
            assert_eq!(eps.len(), 1);
            assert_eq!(eps[0].0, idx1);
        }
    }
}
