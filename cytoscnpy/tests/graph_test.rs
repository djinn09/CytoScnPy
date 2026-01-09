use cytoscnpy::analyzer::semantic::graph::{EdgeType, SemanticGraph};
use cytoscnpy::graph::symbols::{SymbolInfo, SymbolType};
use std::path::PathBuf;
use std::sync::Arc;

fn create_mock_symbol(name: &str) -> SymbolInfo {
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

#[test]
fn test_graph_nodes_and_edges() {
    let graph = SemanticGraph::new();

    let sym_a = create_mock_symbol("A");
    let sym_b = create_mock_symbol("B");

    let idx_a = graph.add_node(sym_a.clone());
    let idx_b = graph.add_node(sym_b.clone());

    // Idempotency check
    assert_eq!(graph.add_node(sym_a), idx_a);

    graph.add_call(idx_a, idx_b);

    // Check edge existence via petgraph manually or via our simple API if exposed
    // Currently we don't have a 'has_edge' in SemanticGraph public API,
    // but the underlying graph is public (Arc<RwLock<...>>)
    let raw_graph = graph.graph.read().unwrap();
    assert!(raw_graph.contains_edge(idx_a, idx_b));
}

#[test]
fn test_cycle_detection() {
    let graph = SemanticGraph::new();

    let a = graph.add_node(create_mock_symbol("A"));
    let b = graph.add_node(create_mock_symbol("B"));
    let c = graph.add_node(create_mock_symbol("C"));

    // A -> B -> C -> A (Cycle)
    graph.add_call(a, b);
    graph.add_call(b, c);
    graph.add_call(c, a);

    let cycles = graph.detect_cycles();
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0].len(), 3);
}

#[test]
fn test_self_loop_detection() {
    let graph = SemanticGraph::new();
    let a = graph.add_node(create_mock_symbol("A"));

    graph.add_call(a, a);

    let cycles = graph.detect_cycles();
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0].len(), 1);
}
