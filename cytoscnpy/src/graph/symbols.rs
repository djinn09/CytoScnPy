//! Global Symbol Table for Semantic Analysis
//!
//! Maps Fully Qualified Names (FQNs) to their definitions across the project.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// Type of symbol definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolType {
    /// A standalone function.
    Function,
    /// A method within a class.
    Method,
    /// A class definition.
    Class,
    /// A variable assignment.
    Variable,
    /// An import statement.
    Import,
    /// A module definition.
    Module,
    /// Unknown symbol type.
    Unknown,
}

/// Information about a symbol (function, class, variable).
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolInfo {
    /// Fully qualified name (e.g., "my_pkg.utils.process")
    pub fqn: String,
    /// Path to the file containing the definition
    pub file_path: PathBuf,
    /// Line number (1-indexed)
    pub line: usize,
    /// Type of symbol (function, class, method, etc.)
    pub def_type: SymbolType,
    /// Parameters (if function/method)
    pub params: Vec<String>,
    /// Module path defining this symbol
    pub module_path: String,
    /// Whether the symbol is exported/public
    pub is_exported: bool,
    /// Whether the symbol is an entry point (e.g., main block)
    pub is_entry_point: bool,
    /// Start byte offset in the file
    pub start_byte: usize,
    /// End byte offset in the file
    pub end_byte: usize,
    /// Decorators applied to the symbol
    pub decorators: Vec<String>,
    /// Base classes if it is a class
    pub base_classes: Vec<String>,
}

/// Information about an import statement.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportInfo {
    /// The name of the module being imported (e.g. "json", "os.path").
    pub module: String,
    /// The specific name imported from the module (if any).
    pub name: String,
    /// Optional alias (as X).
    pub alias: Option<String>,
    /// Line number where the import occurs.
    pub line: usize,
}

/// Represents an entry point in the code.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryPoint {
    /// Path to the file containing this entry point.
    pub file_path: PathBuf,
    /// Line number where the entry point starts.
    pub line: usize,
    /// The type of entry point.
    pub kind: EntryPointType,
}

/// Represents the kind of an entry point in the code.
#[derive(Debug, Clone, PartialEq)]
pub enum EntryPointType {
    /// The main execution block of the program (e.g., if __name__ == "__main__").
    MainBlock,
    /// A framework-defined route or handler (e.g., web server endpoint).
    FrameworkRoute(String), // e.g., "FastAPI: /users"
    /// A standalone script execution.
    Script,
}

/// Stub for external library symbols.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryStub {
    /// Name of the external library (e.g., "requests").
    pub name: String,
    /// Version string if available.
    pub version: Option<String>,
}

/// Represents a symbol that is unreachable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnreachableSymbol {
    /// Fully qualified name of the unreachable symbol.
    pub fqn: String,

    /// Path to the file containing the definition.
    pub file_path: PathBuf,

    /// Line number (1-indexed).
    pub line: usize,

    /// Type of the symbol.
    pub def_type: SymbolType,

    /// The reason why it is considered unreachable.
    pub reason: UnreachableReason,
}

/// Reason why a symbol is unreachable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnreachableReason {
    /// Code that is never executed.
    DeadCode,
    /// Symbol is shadowed by another definition.
    Shadowed,
    /// Unreachable due to constant condition (e.g., if false).
    ConditionalConstant(bool),
}

/// Errors related to semantic analysis.
#[derive(Debug, thiserror::Error)]
pub enum SemanticError {
    /// Symbol with the given FQN was not found.
    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),
    /// Symbol definition collides with an existing one.
    #[error("Duplicate symbol definition: {0}")]
    DuplicateSymbol(String),
    /// Circular dependency detected in imports.
    #[error("Import cycle detected: {0:?}")]
    ImportCycle(Vec<String>),
}

/// Thread-safe global symbol table.
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    /// Map from FQN to SymbolInfo
    symbols: Arc<DashMap<String, SymbolInfo>>,
}

impl SymbolTable {
    /// Creates a new empty symbol table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a symbol into the table.
    pub fn insert(&self, fqn: String, info: SymbolInfo) {
        self.symbols.insert(fqn, info);
    }

    /// Retrieves a symbol by its FQN.
    pub fn get(&self, fqn: &str) -> Option<SymbolInfo> {
        self.symbols.get(fqn).map(|r| r.value().clone())
    }

    /// Returns the number of symbols in the table.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Returns true if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Returns an iterator over the symbols.
    pub fn iter(&self) -> dashmap::iter::Iter<String, SymbolInfo> {
        self.symbols.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get_symbol() {
        let table = SymbolTable::new();
        let fqn = "pkg.mod.func".to_string();
        let info = SymbolInfo {
            fqn: fqn.clone(),
            file_path: PathBuf::from("src/mod.rs"),
            line: 10,
            def_type: SymbolType::Function,
            params: vec!["a".to_string(), "b".to_string()],
            module_path: "pkg.mod".to_string(),
            is_exported: true,
            is_entry_point: false,
            start_byte: 0,
            end_byte: 10,
            decorators: vec![],
            base_classes: vec![],
        };

        table.insert(fqn.clone(), info.clone());

        let retrieved = table.get(&fqn);
        assert_eq!(retrieved, Some(info));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_symbol_collisions() {
        let table = SymbolTable::new();
        let fqn = "pkg.mod.func".to_string();

        // Initial insert
        let info1 = SymbolInfo {
            fqn: fqn.clone(),
            file_path: PathBuf::from("src/mod.rs"),
            line: 10,
            def_type: SymbolType::Function,
            params: vec![],
            module_path: "pkg.mod".to_string(),
            is_exported: true,
            is_entry_point: false,
            start_byte: 0,
            end_byte: 100,
            decorators: vec![],
            base_classes: vec![],
        };
        table.insert(fqn.clone(), info1);

        // Overwrite
        let info2 = SymbolInfo {
            fqn: fqn.clone(),
            file_path: PathBuf::from("src/mod.rs"),
            line: 20, // Different line
            def_type: SymbolType::Function,
            params: vec![],
            module_path: "pkg.mod".to_string(),
            is_exported: true,
            is_entry_point: false,
            start_byte: 200,
            end_byte: 300,
            decorators: vec![],
            base_classes: vec![],
        };
        table.insert(fqn.clone(), info2.clone());

        assert_eq!(table.get(&fqn), Some(info2));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_concurrent_access() {
        let table = SymbolTable::new();
        let table_clone = table.clone();

        let handle = std::thread::spawn(move || {
            let fqn = "pkg.thread.func".to_string();
            let info = SymbolInfo {
                fqn: fqn.clone(),
                file_path: PathBuf::from("src/thread.rs"),
                line: 5,
                def_type: SymbolType::Function,
                params: vec![],
                module_path: "pkg.thread".to_string(),
                is_exported: false,
                is_entry_point: false,
                start_byte: 50,
                end_byte: 150,
                decorators: vec![],
                base_classes: vec![],
            };
            table_clone.insert(fqn, info);
        });

        handle.join().unwrap();

        assert!(table.get("pkg.thread.func").is_some());
    }
    #[test]
    fn test_symbol_type_serialization() {
        let sym_type = SymbolType::Function;
        let serialized = serde_json::to_string(&sym_type).unwrap();
        let deserialized: SymbolType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(sym_type, deserialized);
    }

    #[test]
    fn test_import_info() {
        let import = ImportInfo {
            module: "pkg.mod".to_string(),
            name: "func".to_string(),
            alias: Some("f".to_string()),
            line: 5,
        };
        assert_eq!(import.module, "pkg.mod");
        assert_eq!(import.alias, Some("f".to_string()));
    }

    #[test]
    fn test_entry_point() {
        let ep = EntryPoint {
            file_path: PathBuf::from("src/main.rs"),
            line: 1,
            kind: EntryPointType::MainBlock,
        };
        assert_eq!(ep.kind, EntryPointType::MainBlock);
    }

    #[test]
    fn test_library_stub() {
        let stub = LibraryStub {
            name: "requests".to_string(),
            version: Some("2.0.0".to_string()),
        };
        assert_eq!(stub.name, "requests");
    }

    #[test]
    fn test_unreachable_symbol() {
        let dead = UnreachableSymbol {
            fqn: "dead.code".to_string(),
            file_path: PathBuf::from("test.py"),
            line: 1,
            def_type: SymbolType::Function,
            reason: UnreachableReason::DeadCode,
        };
        matches!(dead.reason, UnreachableReason::DeadCode);
    }

    #[test]
    fn test_semantic_error_display() {
        let err = SemanticError::SymbolNotFound("missing_sym".to_string());
        assert_eq!(err.to_string(), "Symbol not found: missing_sym");

        let err_dup = SemanticError::DuplicateSymbol("dup_sym".to_string());
        assert_eq!(err_dup.to_string(), "Duplicate symbol definition: dup_sym");

        let cycle = SemanticError::ImportCycle(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(cycle.to_string(), "Import cycle detected: [\"a\", \"b\"]");
    }
}
