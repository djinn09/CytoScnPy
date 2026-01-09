pub mod adapters;
pub mod graph;
pub mod impact;
pub mod imports;
pub mod reachability;

use crate::graph::symbols::{SymbolInfo, SymbolTable, SymbolType, UnreachableSymbol};
use crate::utils::LineIndex;
use crate::visitor::CytoScnPyVisitor;
use graph::{EdgeType, SemanticGraph};
use impact::{ImpactAnalyzer, ImpactJson, ImpactResult};
use imports::ImportResolver;
use rayon::prelude::*;
use reachability::ReachabilityAnalyzer;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_parser::parse_module;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// Configuration for semantic analysis.
#[derive(Debug, Clone)]
pub struct SemanticConfig {
    pub project_root: PathBuf,
    pub include_tests: bool,
    pub exclude_folders: Vec<String>,
    pub enable_taint: bool,
    pub enable_fix: bool,
}

/// Orchestrator for the semantic analysis pipeline.
pub struct SemanticAnalyzer {
    config: SemanticConfig,
    symbol_table: Arc<SymbolTable>,
    import_resolver: Arc<ImportResolver>,
    graph: Arc<SemanticGraph>,
}

impl SemanticAnalyzer {
    pub fn new(config: SemanticConfig) -> Self {
        let symbol_table = Arc::new(SymbolTable::new());
        let import_resolver = Arc::new(ImportResolver::new(
            symbol_table.clone(),
            config.project_root.clone(),
        ));
        let graph = Arc::new(SemanticGraph::new());

        Self {
            config,
            symbol_table,
            import_resolver,
            graph,
        }
    }

    /// Executed the full semantic analysis pipeline.
    pub fn analyze(&self, paths: &[PathBuf]) -> Result<SemanticResult, Box<dyn std::error::Error>> {
        let start_time = Instant::now();

        // 0. File Collection
        let t0 = Instant::now();
        let files = self.collect_files(paths);
        let t_collect = t0.elapsed();

        // Stage 1: Indexing (Build Global Symbol Table)
        let t1 = Instant::now();
        self.stage_index(&files);
        let t_index = t1.elapsed();

        // Populate Graph Nodes from Symbol Table
        self.populate_graph_nodes(); // Part of indexing technically

        // Stage 2: Linking (Resolve Imports)
        let t2 = Instant::now();
        self.stage_imports();
        let t_imports = t2.elapsed();

        // Stage 3: Graph Building (Call Graph & Dependencies)
        let t3 = Instant::now();
        self.stage_graph(&files);
        self.graph.finish_building();
        let t_graph = t3.elapsed();

        // Stage 4: Analysis (Reachability)
        let t4 = Instant::now();
        let reachable = {
            let analyzer = ReachabilityAnalyzer::new(&self.graph);
            analyzer.compute_reachable()
        };
        let t_reach = t4.elapsed();

        let duration = start_time.elapsed();

        // Convert unreachable nodes back to symbols for result
        let mut unreachable_symbols = Vec::new();
        // Naive iteration for now - optimize later
        // We know total nodes - reachable nodes = unreachable nodes
        // But we need their FQNs and reasons.
        // For now, let's just return empty until we implement granular unreachable reason mapping in ReachabilityAnalyzer
        // Or we can iterate all symbols and check if they are in reachable set.

        for symbol in self.symbol_table.iter() {
            if let Some(node) = self.graph.get_node_by_fqn(symbol.key()) {
                if !reachable.reachable_nodes.contains(&node) {
                    unreachable_symbols.push(UnreachableSymbol {
                        fqn: symbol.key().clone(),
                        file_path: symbol.value().file_path.clone(),
                        line: symbol.value().line,
                        def_type: symbol.value().def_type.clone(),
                        reason: crate::graph::symbols::UnreachableReason::DeadCode,
                    });
                }
            }
        }

        // Run Taint Analysis
        let mut taint_findings = Vec::new();
        let t_taint_start = Instant::now();
        if self.config.enable_taint {
            let taint_config = crate::taint::analyzer::TaintConfig::all_levels();
            let mut adapter =
                crate::analyzer::semantic::adapters::taint::TaintAnalysisAdapter::new(taint_config);

            let mut file_inputs = Vec::with_capacity(files.len());
            for path in &files {
                if let Ok(content) = std::fs::read_to_string(path) {
                    file_inputs.push((path.clone(), content));
                }
            }

            taint_findings = adapter.analyze(&file_inputs, Some(&self.graph));
        }
        let t_taint = t_taint_start.elapsed();

        // Run AutoFix
        let mut fixes = Vec::new();
        let t_fix_start = Instant::now();
        if self.config.enable_fix {
            // Find unreachable nodes
            let graph_reader = self.graph.graph.read().unwrap();
            let all_nodes: Vec<_> = graph_reader.node_indices().collect();
            let unreachable_indices: Vec<_> = all_nodes
                .into_iter()
                .filter(|idx| !reachable.reachable_nodes.contains(idx))
                .collect();

            // Drop lock before calling adapter (though adapter takes &Arc<SemanticGraph> and locks internally,
            // taking indices beforehand is fine, but we must NOT hold read lock if adapter needs write lock -
            // adapter only needs read lock).
            drop(graph_reader);

            fixes = crate::analyzer::semantic::adapters::autofix::AutoFixAdapter::generate_semantic_fixes(&self.graph, &unreachable_indices);
        }
        let t_fix = t_fix_start.elapsed();

        // Print Profiling Stats (if verbose logic existed or just always for this phase)
        // Ideally we check a flag. For now, we use a crude check or just print.
        // Let's assume we print if duration > 1000ms OR arbitrarily always since we are in "Tuning" mode task.
        // Actually, let's checking `parsing logic` from entry_point could be passed down, but config doesn't have it.
        // We'll print to stderr so it doesn't break JSON output (unless JSON is on stdout).
        eprintln!("--- Semantic Phase Profiling ---");
        eprintln!("Collection: {:?}", t_collect);
        eprintln!("Indexing:   {:?}", t_index);
        eprintln!("Imports:    {:?}", t_imports);
        eprintln!("GraphBuild: {:?}", t_graph);
        eprintln!("Reachabil:  {:?}", t_reach);
        if self.config.enable_taint {
            eprintln!("Taint:      {:?}", t_taint);
        }
        if self.config.enable_fix {
            eprintln!("AutoFix:    {:?}", t_fix);
        }
        eprintln!("Total:      {:?}", duration);
        eprintln!("--------------------------------");

        Ok(SemanticResult {
            total_files: files.len(),
            total_symbols: self.symbol_table.len(),
            reachable_symbols: reachable.reachable_nodes.len(),
            unreachable_symbols,
            taint_findings,
            fixes,
            duration_ms: duration.as_millis() as u64,
        })
    }

    /// Computes the impact of changing a specific symbol.
    pub fn compute_impact(&self, symbol_fqn: &str) -> Option<ImpactResult> {
        let node = self.graph.get_node_by_fqn(symbol_fqn)?;
        let analyzer = ImpactAnalyzer::new(&self.graph);
        Some(analyzer.compute_impact(node))
    }

    /// Formats the impact result as a tree string.
    pub fn format_impact(&self, symbol_fqn: &str, result: &ImpactResult) -> String {
        if let Some(node) = self.graph.get_node_by_fqn(symbol_fqn) {
            let analyzer = ImpactAnalyzer::new(&self.graph);
            analyzer.format_impact_tree(node, result)
        } else {
            "Symbol found during analysis but not found during formatting.".to_string()
        }
    }

    /// Converts ImpactResult to serializable JSON.
    pub fn get_impact_json(&self, result: &ImpactResult) -> ImpactJson {
        let analyzer = ImpactAnalyzer::new(&self.graph);
        analyzer.to_json(result)
    }

    fn collect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        let mut all_files = Vec::new();
        let include_folders: Vec<String> = Vec::new();

        for path in paths {
            if path.is_file() {
                if path.extension().is_some_and(|e| e == "py") {
                    all_files.push(path.clone());
                }
            } else if path.is_dir() {
                let (files, _) = crate::utils::collect_python_files_gitignore(
                    path,
                    &self.config.exclude_folders,
                    &include_folders,
                    false,
                    false,
                );
                all_files.extend(files);
            }
        }
        all_files
    }

    fn stage_index(&self, files: &[PathBuf]) {
        files.par_iter().for_each(|file_path| {
            if let Ok(source) = std::fs::read_to_string(file_path) {
                if let Ok(parsed) = parse_module(&source) {
                    let line_index = LineIndex::new(&source);
                    let module_name = self.derive_module_name(file_path);

                    let mut visitor =
                        CytoScnPyVisitor::new(file_path.clone(), module_name.clone(), &line_index);

                    for stmt in parsed.into_syntax().body {
                        visitor.visit_stmt(&stmt);
                    }

                    // Populate SymbolTable
                    for def in visitor.definitions {
                        // Convert Definition to SymbolInfo
                        let symbol_type = match def.def_type.as_str() {
                            "function" => SymbolType::Function,
                            "method" => SymbolType::Method,
                            "class" => SymbolType::Class,
                            "variable" => SymbolType::Variable,
                            "import" => SymbolType::Import,
                            _ => SymbolType::Unknown,
                        };

                        let symbol = SymbolInfo {
                            fqn: def.full_name.clone(),
                            file_path: def.file.as_ref().clone(),
                            line: def.line,
                            def_type: symbol_type,
                            params: vec![],
                            module_path: module_name.clone(),
                            is_exported: def.is_exported,
                            is_entry_point: def.is_entry_point,
                            decorators: def.decorators,
                            base_classes: def.base_classes.into_vec(),
                            start_byte: def.start_byte,
                            end_byte: def.end_byte,
                        };

                        self.symbol_table.insert(def.full_name, symbol);
                    }
                }
            }
        });
    }

    fn populate_graph_nodes(&self) {
        for entry in self.symbol_table.iter() {
            self.graph.add_node(entry.value().clone());
        }
    }

    fn stage_imports(&self) {
        // Iterate over all symbols. careful with concurrent access if we were mutating symbol table
        // strictly, but here we are mutating import_resolver state (DashMap)

        self.symbol_table.iter().par_bridge().for_each(|entry| {
            let symbol = entry.value();
            if symbol.def_type == SymbolType::Import {
                // Heuristic: SymbolInfo FQN for import is usually "module.imported_name"
                // But CytoScnPyVisitor might store it differently.
                // Assuming fqn is correct unique identifier.
                // We need to look at specific ImportInfo if we stored it?
                // SymbolInfo currently doesn't store "what" is imported, only "that" something is imported at FQN.
                // LIMITATION: Visitor needs to store Import details in SymbolInfo or separate table.

                // For now, let's assume we can resolve just by checking the name?
                // Actually, without knowing "from X import Y", we can't fully resolve.
                // Visitor enhancement (previous ticket) might have missed adding ImportInfo to SymbolTable?
                // Or we re-parse? No, Stage 1 visitor should have handled it.
                // Let's check SymbolInfo definition... it has no fields for import Details.
                // We will defer precise import resolution to re-parsing/AST traversal in Stage 3 if needed,
                // OR we rely on resolve_import calls during AST traversal in Stage 3.

                // Optimized approach: Do mostly nothing here unless we upgrade SymbolTable.
                // BUT, we can implement wildcard expansion if we had data.

                // For this implementation, we will perform resolution lazily in Stage 3
                // as we see Call nodes or Name nodes.
                // So this stage might be reserved for FUTURE bulk resolution.
            }
        });
    }

    fn stage_graph(&self, files: &[PathBuf]) {
        let graph_ref = &self.graph;
        let resolver_ref = &self.import_resolver;

        files.par_iter().for_each(|file_path| {
            if let Ok(source) = std::fs::read_to_string(file_path) {
                if let Ok(parsed) = parse_module(&source) {
                    let module_name = self.derive_module_name(file_path);
                    build_edges_recursive(
                        &parsed.into_syntax().body,
                        &module_name,
                        graph_ref,
                        resolver_ref,
                        &module_name,
                    );
                }
            }
        });
    }

    fn derive_module_name(&self, path: &Path) -> String {
        if let Ok(rel) = path.strip_prefix(&self.config.project_root) {
            let components: Vec<&str> = rel
                .components()
                .filter_map(|c| c.as_os_str().to_str())
                .collect();

            let mut parts = Vec::new();
            for (i, part) in components.iter().enumerate() {
                if i == components.len() - 1 {
                    if let Some(stem) = Path::new(part).file_stem() {
                        let s = stem.to_string_lossy();
                        if s != "__init__" {
                            parts.push(s.to_string());
                        }
                    }
                } else {
                    parts.push(part.to_string());
                }
            }
            return parts.join(".");
        }

        path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }
}

// Helper function to traverse AST and build edges
fn build_edges_recursive(
    stmts: &[Stmt],
    current_fqn: &str,
    graph: &SemanticGraph,
    resolver: &ImportResolver,
    module_name: &str,
) {
    for stmt in stmts {
        match stmt {
            Stmt::FunctionDef(f) => {
                let func_name = &f.name;
                let new_fqn = if current_fqn.is_empty() {
                    format!("{}", func_name)
                } else {
                    format!("{}.{}", current_fqn, func_name)
                };

                build_edges_recursive(&f.body, &new_fqn, graph, resolver, module_name);
            }
            Stmt::ClassDef(c) => {
                let class_name = &c.name;
                let new_fqn = if current_fqn.is_empty() {
                    format!("{}", class_name)
                } else {
                    format!("{}.{}", current_fqn, class_name)
                };
                build_edges_recursive(&c.body, &new_fqn, graph, resolver, module_name);
            }
            Stmt::Expr(e) => {
                find_calls_in_expr(&e.value, current_fqn, graph, resolver, module_name);
            }
            Stmt::Assign(a) => {
                find_calls_in_expr(&a.value, current_fqn, graph, resolver, module_name);
            }
            Stmt::Return(r) => {
                if let Some(val) = &r.value {
                    find_calls_in_expr(val, current_fqn, graph, resolver, module_name);
                }
            }
            // Import handling handled by visitor in Stage 1, but we could add import edges here?
            _ => {}
        }
    }
}

fn find_calls_in_expr(
    expr: &Expr,
    caller_fqn: &str,
    graph: &SemanticGraph,
    resolver: &ImportResolver,
    module_name: &str,
) {
    match expr {
        Expr::Call(call) => {
            if let Expr::Name(n) = &*call.func {
                let callee_name = &n.id;

                // Use ImportResolver to find real target!
                // 1. Try resolving as import
                let resolved_fqn = resolver.resolve_import(module_name, callee_name);

                // 2. Fallback variants
                let mut possible_fqns = Vec::new();

                if let Some(fqn) = resolved_fqn {
                    possible_fqns.push(fqn);
                }

                // Also try module-local (naive) just in case resolver missed it (e.g. defined in same file)
                possible_fqns.push(format!("{}.{}", module_name, callee_name));
                possible_fqns.push(callee_name.to_string()); // builtin or absolute?

                for target_fqn in possible_fqns {
                    if let Some(target_node_idx) = graph.get_node_by_fqn(&target_fqn) {
                        if let Some(source_node_idx) = graph.get_node_by_fqn(caller_fqn) {
                            graph.add_edge(source_node_idx, target_node_idx, EdgeType::Calls);
                            break;
                        } else {
                            // If caller is unknown (e.g. top-level module code), we might want to ensure module node exists.
                            // But for now, we only track edges from defined functions/classes.
                        }
                    }
                }
            } else if let Expr::Attribute(attr) = &*call.func {
                // Handle obj.method() calls
                // This requires type inference to know what 'obj' is.
                // We don't have that yet.
                // But we can guess if 'obj' is a known module?
                if let Expr::Name(name) = &*attr.value {
                    let obj_name = &name.id;
                    if let Some(module_fqn) = resolver.resolve_import(module_name, obj_name) {
                        // e.g. json.dumps -> module_fqn = "json", attr = "dumps"
                        let target_fqn = format!("{}.{}", module_fqn, attr.attr);
                        if let Some(target_node_idx) = graph.get_node_by_fqn(&target_fqn) {
                            if let Some(source_node_idx) = graph.get_node_by_fqn(caller_fqn) {
                                graph.add_edge(source_node_idx, target_node_idx, EdgeType::Calls);
                            }
                        }
                    }
                }
            }

            // Recurse arguments
            for arg in &call.arguments.args {
                find_calls_in_expr(arg, caller_fqn, graph, resolver, module_name);
            }
        }
        _ => {}
    }
}

#[derive(Debug, serde::Serialize)]
pub struct SemanticResult {
    pub total_files: usize,
    pub total_symbols: usize,
    pub reachable_symbols: usize,
    pub unreachable_symbols: Vec<UnreachableSymbol>,
    #[serde(default)]
    pub taint_findings: Vec<crate::taint::types::TaintFinding>,
    #[serde(default)]
    pub fixes: Vec<crate::analyzer::types::FixSuggestion>,
    pub duration_ms: u64,
}
