use crate::analyzer::semantic::graph::{EdgeType, SemanticGraph};
use crate::taint::analyzer::{TaintAnalyzer, TaintConfig};
use crate::taint::types::TaintFinding;

use std::path::PathBuf;
use std::sync::Arc;

/// Adapter for running taint analysis with optional semantic graph enhancement.
pub struct TaintAnalysisAdapter {
    analyzer: TaintAnalyzer,
}

impl TaintAnalysisAdapter {
    /// Creates a new adapter with the given config.
    pub fn new(config: TaintConfig) -> Self {
        Self {
            analyzer: TaintAnalyzer::new(config),
        }
    }

    /// Runs taint analysis.
    ///
    /// If `graph` is provided, it populates the analyzer's cross-file context
    /// with resolved imports from the semantic graph.
    pub fn analyze(
        &mut self,
        files: &[(PathBuf, String)],
        graph: Option<&Arc<SemanticGraph>>,
    ) -> Vec<TaintFinding> {
        if let Some(semantic_graph) = graph {
            self.enrich_with_graph(semantic_graph);
        }

        self.analyzer.analyze_project(files)
    }

    fn enrich_with_graph(&mut self, graph: &Arc<SemanticGraph>) {
        // Here we inject knowledge from SemanticGraph into TaintAnalyzer.
        // We traverse the graph edges of type 'Imports' and register them.
        let raw_graph = graph.graph.read().unwrap();

        for edge_idx in raw_graph.edge_indices() {
            if let Some((source_idx, target_idx)) = raw_graph.edge_endpoints(edge_idx) {
                if let Some(edge_weight) = raw_graph.edge_weight(edge_idx) {
                    if *edge_weight == EdgeType::Imports {
                        // Get Source and Target nodes
                        if let (Some(source_node), Some(target_node)) = (
                            raw_graph.node_weight(source_idx),
                            raw_graph.node_weight(target_idx),
                        ) {
                            // Deconstruct import relationship
                            // Source is the *importer* (the module doing the importing)
                            // Target is the *imported* (the module being imported)
                            // Note: SemanticGraph edge direction might be "depends upon" (importer -> imported)

                            // Extract FQNs
                            // source_node.module_path is likely the importing file's module
                            // target_node.module_path is the imported module

                            let importing_module = &source_node.module_path;
                            let actual_module = &target_node.module_path;

                            // For simplicity, we assume the alias matches the simple name of the target
                            // unless we specifically stored alias info in the Edge.
                            // Currently EdgeType::Imports doesn't carry alias info.
                            // We might need to guess or rely on SymbolInfo name.
                            let alias = target_node
                                .fqn
                                .split('.')
                                .next_back()
                                .unwrap_or(&target_node.fqn);
                            let actual_name = target_node
                                .fqn
                                .split('.')
                                .next_back()
                                .unwrap_or(&target_node.fqn);

                            self.analyzer.register_import(
                                importing_module,
                                alias,
                                actual_module,
                                actual_name,
                            );
                        }
                    }
                }
            }
        }
    }
}
