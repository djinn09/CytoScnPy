use crate::graph::symbols::{LibraryStub, SymbolTable};
use dashmap::DashMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Resolves import statements to Fully Qualified Names (FQNs).
#[derive(Debug)]
pub struct ImportResolver {
    /// Cache for absolute imports: import_name -> FQN
    /// Key is just the import name (e.g., "json", "pkg.utils"), no allocation needed for lookup.
    absolute_cache: DashMap<String, String>,

    /// Cache for relative imports: (source_module, import_name) -> Option<FQN>
    /// Stores Option to support negative caching (remembering failed resolutions).
    relative_cache: DashMap<(String, String), Option<String>>,

    /// Tracks wildcard imports for conservative analysis
    /// Map: source_module -> Vec<imported_module_fqn>
    wildcard_imports: DashMap<String, Vec<String>>,

    /// Reference to global symbol table
    symbol_table: Arc<SymbolTable>,

    /// Project root for path calculations
    #[allow(dead_code)] // May be used for more advanced resolution later
    project_root: PathBuf,

    /// External library stubs for framework detection
    external_stubs: HashMap<String, LibraryStub>,
}

impl ImportResolver {
    /// Creates a new ImportResolver.
    pub fn new(symbol_table: Arc<SymbolTable>, project_root: PathBuf) -> Self {
        let mut resolver = Self {
            absolute_cache: DashMap::new(),
            relative_cache: DashMap::new(),
            wildcard_imports: DashMap::new(),
            symbol_table,
            project_root,
            external_stubs: HashMap::new(),
        };
        resolver.initialize_stubs();
        resolver
    }

    fn initialize_stubs(&mut self) {
        let common_libs = vec![
            "os",
            "sys",
            "json",
            "math",
            "re",
            "collections",
            "itertools",
            "functools",
            "flask",
            "django",
            "fastapi",
            "pandas",
            "numpy",
            "scipy",
            "sklearn",
            "requests",
            "httpx",
            "sqlalchemy",
            "pydantic",
            "pytest",
            "unittest",
        ];

        for lib in common_libs {
            self.external_stubs.insert(
                lib.to_string(),
                LibraryStub {
                    name: lib.to_string(),
                    version: None,
                },
            );
        }
    }

    /// Resolves a single import to its FQN.
    ///
    /// # Arguments
    /// * `source_module` - The FQN of the module containing the import (e.g. "pkg.sub")
    /// * `import_name` - The name string as it appears in the import statement (e.g. ".sibling", "json", "pkg.utils")
    pub fn resolve_import(&self, source_module: &str, import_name: &str) -> Option<String> {
        if import_name.starts_with('.') {
            // Relative Import - Use relative cache with negative caching
            let cache_key = (source_module.to_string(), import_name.to_string());

            if let Some(cached) = self.relative_cache.get(&cache_key) {
                return cached.value().clone();
            }

            let resolved = self.resolve_relative(source_module, import_name);

            // Cache the result (Some or None)
            self.relative_cache.insert(cache_key, resolved.clone());
            resolved
        } else {
            // Absolute Import - Use absolute cache (key is just import_name)
            if let Some(fqn) = self.absolute_cache.get(import_name) {
                return Some(fqn.value().clone());
            }

            let resolved = self.resolve_absolute(import_name);

            if let Some(ref fqn) = resolved {
                self.absolute_cache
                    .insert(import_name.to_string(), fqn.clone());
            }
            // Note: We don't verify strictness here, so we generally accept absolute imports
            // even if they aren't explicitly in the symbol table (e.g. external libs).
            // Thus, resolve_absolute currently always returns Some(import_name),
            // but we keep the structure for potential future validation logic.

            resolved
        }
    }

    fn resolve_relative(&self, source_module: &str, import_name: &str) -> Option<String> {
        let dot_count = import_name.chars().take_while(|c| *c == '.').count();
        let parts: Vec<&str> = source_module.split('.').collect();

        // dot_count = 1: from . import x (current package)
        // dot_count = 2: from .. import x (parent package)

        if dot_count > parts.len() {
            return None; // Too many dots
        }

        let parent_len = parts.len().saturating_sub(dot_count);
        let parent_parts = &parts[0..parent_len];

        let suffix = &import_name[dot_count..];

        let mut new_parts = parent_parts.to_vec();
        if !suffix.is_empty() {
            new_parts.push(suffix);
        }

        let candidate = new_parts.join(".");

        if !candidate.is_empty() {
            return Some(candidate);
        }
        None
    }

    fn resolve_absolute(&self, import_name: &str) -> Option<String> {
        let root_module = import_name.split('.').next().unwrap_or(import_name);

        // 1. Check external stubs
        if self.external_stubs.contains_key(root_module) {
            return Some(import_name.to_string());
        }

        // 2. Check internal symbol table for exact match
        if self.symbol_table.get(import_name).is_some() {
            return Some(import_name.to_string());
        }

        // 3. Fallback: Assume it is a valid FQN if it looks like one
        Some(import_name.to_string())
    }

    /// Resolves `from module import *` by finding all exported symbols in `module`.
    pub fn expand_wildcard(&self, source_module: &str, target_module: &str) -> Vec<String> {
        // Resolve target module FQN
        let target_fqn = self
            .resolve_import(source_module, target_module)
            .unwrap_or_else(|| target_module.to_string());

        let mut expanded = Vec::new();

        // Inefficient scan, but works for now.
        // TODO: optimize SymbolTable with hierarchical index.
        for entry in self.symbol_table.iter() {
            let sym = entry.value();
            if sym.module_path == target_fqn && sym.is_exported {
                expanded.push(sym.fqn.clone());
            }
        }

        self.wildcard_imports
            .insert(source_module.to_string(), expanded.clone());
        expanded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::symbols::{SymbolInfo, SymbolType};
    use std::path::PathBuf;

    #[test]
    fn test_resolve_existing_symbol() {
        let table = Arc::new(SymbolTable::new());
        let fqn = "pkg.mod.Class".to_string();
        table.insert(
            fqn.clone(),
            SymbolInfo {
                fqn: fqn.clone(),
                file_path: PathBuf::from("src/mod.rs"),
                line: 1,
                def_type: SymbolType::Class,
                params: vec![],
                module_path: "pkg.mod".to_string(),
                is_exported: true,
                is_entry_point: false,
                decorators: vec![],
                base_classes: vec![],
                start_byte: 0,
                end_byte: 0,
            },
        );

        let resolver = ImportResolver::new(table, PathBuf::from("/"));
        assert_eq!(
            resolver.resolve_import("pkg.other", "pkg.mod.Class"),
            Some(fqn)
        );
    }

    #[test]
    fn test_resolve_relative() {
        let table = Arc::new(SymbolTable::new());
        let resolver = ImportResolver::new(table, PathBuf::from("/"));

        // src: pkg.sub.mod, import: .sibling -> pkg.sub.sibling
        assert_eq!(
            resolver.resolve_import("pkg.sub.mod", ".sibling"),
            Some("pkg.sub.sibling".to_string())
        );

        // src: pkg.sub.mod, import: ..parent -> pkg.parent
        assert_eq!(
            resolver.resolve_import("pkg.sub.mod", "..parent"),
            Some("pkg.parent".to_string())
        );
    }

    #[test]
    fn test_external_stub() {
        let table = Arc::new(SymbolTable::new());
        let resolver = ImportResolver::new(table, PathBuf::from("/"));

        assert_eq!(
            resolver.resolve_import("pkg.mod", "json.dumps"),
            Some("json.dumps".to_string())
        );
        assert_eq!(
            resolver.resolve_import("pkg.mod", "fastapi.FastAPI"),
            Some("fastapi.FastAPI".to_string())
        );
    }

    #[test]
    fn test_wildcard_expansion() {
        let table = Arc::new(SymbolTable::new());

        // Define module symbols
        let mod_fqn = "pkg.utils";
        let sym1 = "pkg.utils.helper";
        let sym2 = "pkg.utils.secret";

        table.insert(
            sym1.to_string(),
            SymbolInfo {
                fqn: sym1.to_string(),
                file_path: PathBuf::from("src/utils.rs"),
                line: 1,
                def_type: SymbolType::Function,
                params: vec![],
                module_path: mod_fqn.to_string(),
                is_exported: true,
                is_entry_point: false,
                decorators: vec![],
                base_classes: vec![],
                start_byte: 0,
                end_byte: 0,
            },
        );

        // Not exported
        table.insert(
            sym2.to_string(),
            SymbolInfo {
                fqn: sym2.to_string(),
                file_path: PathBuf::from("src/utils.rs"),
                line: 2,
                def_type: SymbolType::Function,
                params: vec![],
                module_path: mod_fqn.to_string(),
                is_exported: false,
                is_entry_point: false,
                decorators: vec![],
                base_classes: vec![],
                start_byte: 0,
                end_byte: 0,
            },
        );

        let resolver = ImportResolver::new(table, PathBuf::from("/"));
        let expanded = resolver.expand_wildcard("pkg.main", "pkg.utils");

        assert!(expanded.contains(&sym1.to_string()));
        assert!(!expanded.contains(&sym2.to_string()));
    }

    #[test]
    fn test_cache_hits_and_negative_caching() {
        let table = Arc::new(SymbolTable::new());
        let resolver = ImportResolver::new(table, PathBuf::from("/"));

        // 1. Test Absolute Cache
        // Initial call should be a miss (internal logic), then cached.
        let fqn = "os.path";
        resolver.resolve_import("any.module", fqn);

        // Verify it's in absolute cache
        assert!(resolver.absolute_cache.contains_key(fqn));

        // 2. Test Relative Cache
        let source = "pkg.sub";
        let target = ".sibling";

        resolver.resolve_import(source, target);
        assert!(resolver
            .relative_cache
            .contains_key(&(source.to_string(), target.to_string())));

        // 3. Test Negative Caching (Relative)
        // This relative import is invalid (too many dots)
        let invalid_target = "......parent";
        let result = resolver.resolve_import(source, invalid_target);
        assert!(result.is_none());

        // Should still be cached as None
        let cache_key = (source.to_string(), invalid_target.to_string());
        assert!(resolver.relative_cache.contains_key(&cache_key));
        assert!(resolver.relative_cache.get(&cache_key).unwrap().is_none());
    }
}
