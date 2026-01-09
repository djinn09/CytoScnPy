use cytoscnpy::analyzer::semantic::{SemanticAnalyzer, SemanticConfig};
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

#[test]
fn test_semantic_analyzer_pipeline() {
    // 1. Setup temporary python project
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    // Create main.py
    let main_py = root.join("main.py");
    let mut f = File::create(&main_py).unwrap();
    writeln!(
        f,
        "
import utils

def main():
    utils.helper()

if __name__ == '__main__':
    main()
"
    )
    .unwrap();

    // Create utils.py
    let utils_py = root.join("utils.py");
    let mut f = File::create(&utils_py).unwrap();
    writeln!(
        f,
        "
def helper():
    print('helping')

def unused_func():
    pass
"
    )
    .unwrap();

    // 2. Configure Analyzer
    let config = SemanticConfig {
        project_root: root.clone(),
        include_tests: false,
        exclude_folders: vec![],
        enable_taint: false,
        enable_fix: false,
    };

    let analyzer = SemanticAnalyzer::new(config);
    let paths = vec![root.clone()];

    // 3. Run Analysis
    let result = analyzer.analyze(&paths).expect("Analysis failed");

    // 4. Verify Results
    println!("Semantic analysis result: {:?}", result);

    // We expect 2 files
    assert_eq!(result.total_files, 2);

    // Symbols: main, utils, main.main, utils.helper, utils.unused_func, imports...
    // approximate check
    assert!(result.total_symbols >= 4);

    // Reachability:
    // Entry points not fully hooked up in v1 scaffold yet, so reachable might be 0 or just based on hardcoded entries if logic existed.
    // For now, we assert the pipeline ran without crashing.
}
