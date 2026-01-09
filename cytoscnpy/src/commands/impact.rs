use crate::analyzer::semantic::{SemanticAnalyzer, SemanticConfig};
use std::io::Write;
use std::path::PathBuf;

pub fn run_impact<W: Write>(
    paths: &[PathBuf],
    root: &PathBuf,
    symbol_fqn: &str,
    json: bool,
    _depth: Option<usize>, // TODO: Implement depth limit in ImpactAnalyzer
    config: crate::config::Config,
    verbose: bool,
    writer: &mut W,
) -> anyhow::Result<()> {
    if verbose {
        eprintln!("[VERBOSE] Starting Impact Analysis for symbol: {symbol_fqn}");
    }

    // 1. Configure Semantic Analyzer
    let semantic_config = SemanticConfig {
        project_root: root.clone(),
        include_tests: config.cytoscnpy.include_tests.unwrap_or(false),
        exclude_folders: config.cytoscnpy.exclude_folders.clone().unwrap_or_default(),
        enable_taint: false,
        enable_fix: false,
    };

    let analyzer = SemanticAnalyzer::new(semantic_config);

    // 2. Index and Build Graph
    // Use analyze() to build the graph.
    let mut analyzer = analyzer;
    let _analysis_result = analyzer
        .analyze(paths)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // 3. Compute Impact
    let impact_result = analyzer.compute_impact(symbol_fqn);

    if let Some(result) = impact_result {
        if json {
            // Serialize result using helper
            let json_obj = analyzer.get_impact_json(&result);
            writeln!(writer, "{}", serde_json::to_string_pretty(&json_obj)?)?;
        } else {
            writeln!(writer, "Impact Analysis for '{}':", symbol_fqn)?;
            writeln!(writer, "--------------------------------------------------")?;
            if result.impacted_nodes.is_empty() {
                writeln!(writer, "No impacted symbols found.")?;
            } else {
                writeln!(
                    writer,
                    "Found {} impacted symbols.",
                    result.impacted_nodes.len()
                )?;
                writeln!(writer, "\nDependency Tree:")?;

                let tree_str = analyzer.format_impact(symbol_fqn, &result);
                writeln!(writer, "{}", tree_str)?;
            }
        }
    } else {
        if json {
            writeln!(writer, "{{ \"error\": \"Symbol not found\" }}")?;
        } else {
            eprintln!("Error: Symbol '{}' not found in the graph.", symbol_fqn);
            return Err(anyhow::anyhow!("Symbol not found"));
        }
    }

    Ok(())
}
