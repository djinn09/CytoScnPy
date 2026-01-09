use cytoscnpy::analyzer::semantic::adapters::autofix::AutoFixAdapter;
use cytoscnpy::analyzer::semantic::adapters::metrics::MetricsAdapter;
use cytoscnpy::analyzer::semantic::adapters::taint::TaintAnalysisAdapter;
use cytoscnpy::analyzer::semantic::graph::SemanticGraph;
use cytoscnpy::taint::analyzer::TaintConfig;
use cytoscnpy::visitor::Definition;
use smallvec::SmallVec;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_metrics_adapter() {
    let graph = Arc::new(SemanticGraph::new());
    // Add some nodes/edges for testing
    let _ = graph.add_node(tests::create_test_symbol("A"));
    let node_b = graph.add_node(tests::create_test_symbol("B"));
    graph.add_call(node_b, node_b); // Self loop/cycle

    let metrics = MetricsAdapter::calculate_semantic_metrics(&graph);

    // We added 2 nodes.
    // 1 Edge (B->B)

    // Complexity = Edges / Nodes = 1 / 2 = 0.5
    assert!((metrics.average_complexity - 0.5).abs() < f64::EPSILON);
    assert_eq!(metrics.cyclic_dependencies, 1);
}

#[test]
fn test_taint_adapter_instantiation() {
    let config = TaintConfig::default();
    let mut adapter = TaintAnalysisAdapter::new(config);
    // Smoke test analyze method
    let results = adapter.analyze(&[], None);
    assert!(results.is_empty());
}

#[test]
fn test_autofix_adapter_syntactic() {
    let def = Definition {
        name: "unused".to_string(),
        full_name: "mod.unused".to_string(),
        simple_name: "unused".to_string(),
        def_type: "function".to_string(),
        file: Arc::new(PathBuf::from("test.py")),
        line: 10,
        end_line: 12,
        start_byte: 100,
        end_byte: 150,
        confidence: 100,
        references: 0,
        is_exported: false,
        in_init: false,
        base_classes: SmallVec::new(),
        is_type_checking: false,
        cell_number: None,
        is_self_referential: false,
        message: None,
        fix: None,
        decorators: vec![],
        is_entry_point: false,
    };

    let fixes = AutoFixAdapter::generate_syntactic_fixes(&[def]);
    assert_eq!(fixes.len(), 1);
    assert_eq!(fixes[0].start_byte, 100);
    assert_eq!(fixes[0].end_byte, 150);
}

// Mock helper needs to be available or we redefine locally
mod tests {
    use cytoscnpy::graph::symbols::{SymbolInfo, SymbolType};
    use std::path::PathBuf;

    pub fn create_test_symbol(name: &str) -> SymbolInfo {
        SymbolInfo {
            fqn: name.to_string(),
            file_path: PathBuf::from("mock.py"),
            line: 1,
            def_type: SymbolType::Function,
            start_byte: 0,
            end_byte: 10,
            params: vec![],
            module_path: "mock".to_string(),
            is_exported: false,
            is_entry_point: false,
            decorators: vec![],
            base_classes: vec![],
        }
    }
}
