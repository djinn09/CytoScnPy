use crate::analyzer::semantic::graph::SemanticGraph;
use crate::analyzer::types::FixSuggestion;
use petgraph::graph::NodeIndex;
use std::sync::Arc;

/// Adapter for generating auto-fixes in both syntactic and semantic modes.
pub struct AutoFixAdapter;

impl AutoFixAdapter {
    /// Generates fixes for unreachable symbols found in the SemanticGraph.
    ///
    /// # Arguments
    /// * `graph` - The semantic graph containing reachability info (implicit in node state or external set).
    /// * `unreachable_nodes` - List of nodes computed as unreachable.
    pub fn generate_semantic_fixes(
        graph: &Arc<SemanticGraph>,
        unreachable_nodes: &[NodeIndex],
    ) -> Vec<FixSuggestion> {
        let mut fixes = Vec::new();
        let raw_graph = graph.graph.read().unwrap();

        for &node_idx in unreachable_nodes {
            if let Some(node) = raw_graph.node_weight(node_idx) {
                // Verify we can safely delete this
                // e.g., We might want to keep public APIs unless configured otherwise.
                // For now, generate a deletion suggestion.

                // Construct fix
                let fix = FixSuggestion::deletion(node.start_byte, node.end_byte);
                fixes.push(fix);
            }
        }
        fixes
    }

    /// Passthrough for syntactic fixes (unused definitions).
    pub fn generate_syntactic_fixes(
        unused_defs: &[crate::visitor::Definition],
    ) -> Vec<FixSuggestion> {
        unused_defs
            .iter()
            .map(|def| FixSuggestion::deletion(def.start_byte, def.end_byte))
            .collect()
    }
}
