use crate::analyzer::semantic::graph::SemanticGraph;
use std::sync::Arc;

/// Adapter for metrics calculation.
pub struct MetricsAdapter;

#[derive(Debug, Default)]
pub struct SemanticMetrics {
    pub average_complexity: f64,
    pub max_depth: usize,
    pub cyclic_dependencies: usize,
}

impl MetricsAdapter {
    /// Calculates metrics using the semantic graph context.
    pub fn calculate_semantic_metrics(graph: &Arc<SemanticGraph>) -> SemanticMetrics {
        let raw_graph = graph.graph.read().unwrap();
        let cycles = graph.detect_cycles();

        // Placeholder for graph-based complexity (e.g. cyclomatic complexity of call graph)
        // Here we just count nodes/edges as proxy.
        let node_count = raw_graph.node_count();
        let edge_count = raw_graph.edge_count();

        // Avoid div by zero
        let complexity = if node_count > 0 {
            edge_count as f64 / node_count as f64
        } else {
            0.0
        };

        SemanticMetrics {
            average_complexity: complexity,
            max_depth: 0, // Dfs max depth TODO
            cyclic_dependencies: cycles.len(),
        }
    }
}
