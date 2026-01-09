use cytoscnpy::analyzer::semantic::graph::SemanticGraph;
use cytoscnpy::analyzer::semantic::impact::ImpactAnalyzer;
use cytoscnpy::analyzer::semantic::reachability::ReachabilityAnalyzer;
use std::sync::Arc;

// Helper to add nodes
mod tests {
    use cytoscnpy::graph::symbols::{SymbolInfo, SymbolType};
    use std::path::PathBuf;

    pub fn create_test_symbol(name: &str) -> SymbolInfo {
        SymbolInfo {
            fqn: name.to_string(),
            file_path: PathBuf::from("mock.py"),
            line: 1,
            def_type: SymbolType::Function,
            params: vec![],
            module_path: "mock".to_string(),
            is_exported: false,
            is_entry_point: false,
            decorators: vec![],
            base_classes: vec![],
            start_byte: 0,
            end_byte: 10,
        }
    }
}

#[test]
fn test_reachability_trace() {
    let mut graph = SemanticGraph::new();

    // A -> B -> C
    // A is entry point

    let node_a = graph.add_node(tests::create_test_symbol("A"));
    let node_b = graph.add_node(tests::create_test_symbol("B"));
    let node_c = graph.add_node(tests::create_test_symbol("C"));
    let node_d = graph.add_node(tests::create_test_symbol("D")); // Unreachable

    graph.add_call(node_a, node_b);
    graph.add_call(node_b, node_c);

    graph.mark_entry_point(node_a, 100);

    let analyzer = ReachabilityAnalyzer::new(&graph);
    let result = analyzer.compute_reachable();

    assert!(result.reachable_nodes.contains(&node_a));
    assert!(result.reachable_nodes.contains(&node_b));
    assert!(result.reachable_nodes.contains(&node_c));
    assert!(!result.reachable_nodes.contains(&node_d));

    // Check trace A -> C
    let trace = analyzer.get_trace(node_c, &result).unwrap();
    // Path should be [A, B, C]
    assert_eq!(trace.len(), 3);
    assert_eq!(trace[0], node_a);
    assert_eq!(trace[1], node_b);
    assert_eq!(trace[2], node_c);
}

#[test]
fn test_impact_analysis() {
    let graph = SemanticGraph::new();

    // X -> Y -> Z
    // If Z changes, who is impacted? Only Z (no incoming) - Wait, impact usually means "Who calls me?"
    // If Z changes, nobody calls Z, so nobody else breaks. (Assuming Z is leaf)
    // If Y changes, X calls Y, so X is impacted.

    let node_x = graph.add_node(tests::create_test_symbol("X"));
    let node_y = graph.add_node(tests::create_test_symbol("Y"));
    let node_z = graph.add_node(tests::create_test_symbol("Z"));

    graph.add_call(node_x, node_y);
    graph.add_call(node_y, node_z);

    let analyzer = ImpactAnalyzer::new(&graph);

    // Impact of Z: Y calls Z, so Y is impacted. X calls Y, so X is impacted (transitive).
    // Incoming edges: Y->Z (Y depends on Z). X->Y (X depends on Y).
    // Reverse BFS from Z: Neighbors(Incoming) -> Y. From Y -> Neighbors(Incoming) -> X.
    // Impact Set: {Z, Y, X}

    let result_z = analyzer.compute_impact(node_z);
    assert!(result_z.impacted_nodes.contains(&node_x));
    assert!(result_z.impacted_nodes.contains(&node_y));
    assert!(result_z.impacted_nodes.contains(&node_z));

    // Impact of Y: X calls Y. Z does NOT call Y.
    // Impact Set: {Y, X}
    let result_y = analyzer.compute_impact(node_y);
    assert!(result_y.impacted_nodes.contains(&node_x));
    assert!(result_y.impacted_nodes.contains(&node_y));
    assert!(!result_y.impacted_nodes.contains(&node_z));
}
