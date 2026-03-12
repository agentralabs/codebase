//! Cross-file symbol resolution.
//!
//! Builds a symbol table from raw code units and resolves references:
//! local names, imported symbols, and external library references.

use std::collections::{HashMap, HashSet};

use crate::parse::{RawCodeUnit, RawReference, ReferenceKind};
use crate::types::{AcbResult, CodeUnitType, Language};

/// A hierarchical symbol table for name resolution.
#[derive(Debug)]
pub struct SymbolTable {
    /// Qualified name → temp_id mapping.
    symbol_map: HashMap<String, u64>,
    /// Simple name → vec of temp_ids (handles overloading/shadowing).
    name_map: HashMap<String, Vec<u64>>,
    /// File path → vec of temp_ids.
    file_map: HashMap<String, Vec<u64>>,
    /// temp_id → qualified_name.
    id_to_qname: HashMap<u64, String>,
    /// Import target name → unit that imports it.
    import_targets: HashMap<String, Vec<u64>>,
}

impl SymbolTable {
    /// Create an empty symbol table.
    pub fn new() -> Self {
        Self {
            symbol_map: HashMap::new(),
            name_map: HashMap::new(),
            file_map: HashMap::new(),
            id_to_qname: HashMap::new(),
            import_targets: HashMap::new(),
        }
    }

    /// Build symbol table from raw units.
    pub fn build(units: &[RawCodeUnit]) -> AcbResult<Self> {
        let mut table = Self::new();

        for unit in units {
            // Register by qualified name
            table
                .symbol_map
                .insert(unit.qualified_name.clone(), unit.temp_id);
            table
                .id_to_qname
                .insert(unit.temp_id, unit.qualified_name.clone());

            // Register by simple name
            table
                .name_map
                .entry(unit.name.clone())
                .or_default()
                .push(unit.temp_id);

            // Register by file
            let file_key = unit.file_path.to_string_lossy().to_string();
            table
                .file_map
                .entry(file_key)
                .or_default()
                .push(unit.temp_id);

            // Track import targets
            if unit.unit_type == CodeUnitType::Import {
                for ref_info in &unit.references {
                    if ref_info.kind == ReferenceKind::Import {
                        table
                            .import_targets
                            .entry(ref_info.name.clone())
                            .or_default()
                            .push(unit.temp_id);
                    }
                }
            }
        }

        Ok(table)
    }

    /// Look up a unit by qualified name.
    pub fn lookup_qualified(&self, qname: &str) -> Option<u64> {
        self.symbol_map.get(qname).copied()
    }

    /// Look up units by simple name.
    pub fn lookup_name(&self, name: &str) -> &[u64] {
        self.name_map.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Look up units in the same file.
    pub fn units_in_file(&self, file_path: &str) -> &[u64] {
        self.file_map
            .get(file_path)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get the qualified name for a temp_id.
    pub fn qname_for_id(&self, id: u64) -> Option<&str> {
        self.id_to_qname.get(&id).map(|s| s.as_str())
    }

    /// Get all symbol entries.
    pub fn all_symbols(&self) -> &HashMap<String, u64> {
        &self.symbol_map
    }

    /// Number of symbols.
    pub fn len(&self) -> usize {
        self.symbol_map.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.symbol_map.is_empty()
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolves references from raw units to concrete targets.
pub struct Resolver {
    /// Known external libraries.
    external_libs: HashMap<String, ExternalLibrary>,
}

/// An external library with known symbols.
#[derive(Debug)]
pub struct ExternalLibrary {
    /// Library name.
    pub name: String,
    /// Language.
    pub language: Language,
    /// Known exported symbols.
    pub known_symbols: HashSet<String>,
    /// Is this a standard library?
    pub is_stdlib: bool,
}

impl Resolver {
    /// Create a new resolver with standard library knowledge.
    pub fn new() -> Self {
        let mut resolver = Self {
            external_libs: HashMap::new(),
        };
        resolver.register_python_stdlib();
        resolver.register_rust_stdlib();
        resolver.register_node_builtins();
        resolver.register_go_stdlib();
        resolver
    }

    /// Resolve all references in the raw units.
    pub fn resolve_all(
        &self,
        units: &[RawCodeUnit],
        symbol_table: &SymbolTable,
    ) -> AcbResult<Vec<ResolvedUnit>> {
        let mut resolved = Vec::with_capacity(units.len());

        for unit in units {
            let resolved_refs = self.resolve_unit_references(unit, units, symbol_table)?;
            resolved.push(ResolvedUnit {
                unit: unit.clone(),
                resolved_refs,
            });
        }

        Ok(resolved)
    }

    fn resolve_unit_references(
        &self,
        unit: &RawCodeUnit,
        all_units: &[RawCodeUnit],
        symbol_table: &SymbolTable,
    ) -> AcbResult<Vec<ResolvedReference>> {
        let mut resolved = Vec::new();

        for raw_ref in &unit.references {
            let resolution = self.resolve_reference(raw_ref, unit, all_units, symbol_table);
            resolved.push(ResolvedReference {
                raw: raw_ref.clone(),
                resolution,
            });
        }

        Ok(resolved)
    }

    fn resolve_reference(
        &self,
        raw_ref: &RawReference,
        unit: &RawCodeUnit,
        all_units: &[RawCodeUnit],
        symbol_table: &SymbolTable,
    ) -> Resolution {
        // Strategy 1: Try exact qualified name match
        if let Some(target_id) = symbol_table.lookup_qualified(&raw_ref.name) {
            if target_id != unit.temp_id {
                return Resolution::Local(target_id);
            }
        }

        // Strategy 2: Try local resolution (same file, then by simple name)
        if let Some(local_id) = self.resolve_local(raw_ref, unit, all_units, symbol_table) {
            return Resolution::Local(local_id);
        }

        // Strategy 3: Try imported symbol resolution
        if let Some(imported) = self.resolve_imported(raw_ref, unit, all_units, symbol_table) {
            return Resolution::Imported(imported);
        }

        // Strategy 4: Try external library match
        if let Some(external) = self.resolve_external(&raw_ref.name, unit.language) {
            return Resolution::External(external);
        }

        Resolution::Unresolved
    }

    fn resolve_local(
        &self,
        raw_ref: &RawReference,
        unit: &RawCodeUnit,
        _all_units: &[RawCodeUnit],
        symbol_table: &SymbolTable,
    ) -> Option<u64> {
        let name = raw_ref.name.as_str();
        let file_key = unit.file_path.to_string_lossy().to_string();
        let file_units = symbol_table.units_in_file(&file_key);

        // Look for a matching name in the same file
        for &id in file_units {
            if id == unit.temp_id {
                continue;
            }
            if let Some(qname) = symbol_table.qname_for_id(id) {
                // Match on the simple name part of the qname
                let simple = qname.rsplit('.').next().unwrap_or(qname);
                let simple2 = qname.rsplit("::").next().unwrap_or(qname);
                let simple_overload = strip_overload_suffix(simple);
                let simple2_overload = strip_overload_suffix(simple2);
                if simple == name
                    || simple2 == name
                    || qname == name
                    || simple_overload == name
                    || simple2_overload == name
                {
                    return Some(id);
                }
            }
        }

        // Also look globally by simple name
        let candidates = symbol_table.lookup_name(name);
        candidates.iter().find(|&&cid| cid != unit.temp_id).copied()
    }

    fn resolve_imported(
        &self,
        raw_ref: &RawReference,
        unit: &RawCodeUnit,
        all_units: &[RawCodeUnit],
        symbol_table: &SymbolTable,
    ) -> Option<ImportedSymbol> {
        let name = raw_ref.name.as_str();
        // Check if any import in the same file matches this name
        let file_key = unit.file_path.to_string_lossy().to_string();
        let file_unit_ids = symbol_table.units_in_file(&file_key);

        for &fid in file_unit_ids {
            // Find the unit for this ID
            if let Some(file_unit) = all_units.iter().find(|u| u.temp_id == fid) {
                if file_unit.unit_type == CodeUnitType::Import {
                    let import_name = &file_unit.name;
                    if import_matches(unit.language, raw_ref.kind, import_name, name) {
                        return Some(ImportedSymbol {
                            unit_id: fid,
                            import_path: import_name.clone(),
                        });
                    }
                }
            }
        }

        None
    }

    fn resolve_external(&self, name: &str, language: Language) -> Option<ExternalSymbol> {
        for lib in self.external_libs.values() {
            if lib.language == language && lib.known_symbols.contains(name) {
                return Some(ExternalSymbol {
                    library: lib.name.clone(),
                    symbol: name.to_string(),
                    is_stdlib: lib.is_stdlib,
                });
            }
        }
        None
    }

    fn register_python_stdlib(&mut self) {
        let symbols: HashSet<String> = [
            "print",
            "len",
            "range",
            "int",
            "str",
            "float",
            "bool",
            "list",
            "dict",
            "set",
            "tuple",
            "type",
            "isinstance",
            "issubclass",
            "hasattr",
            "getattr",
            "setattr",
            "delattr",
            "super",
            "object",
            "open",
            "input",
            "sorted",
            "reversed",
            "enumerate",
            "zip",
            "map",
            "filter",
            "any",
            "all",
            "min",
            "max",
            "sum",
            "abs",
            "round",
            "format",
            "repr",
            "id",
            "hash",
            "iter",
            "next",
            "Exception",
            "ValueError",
            "TypeError",
            "KeyError",
            "IndexError",
            "AttributeError",
            "RuntimeError",
            "StopIteration",
            "OSError",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        self.external_libs.insert(
            "python_stdlib".to_string(),
            ExternalLibrary {
                name: "python_stdlib".to_string(),
                language: Language::Python,
                known_symbols: symbols,
                is_stdlib: true,
            },
        );
    }

    fn register_rust_stdlib(&mut self) {
        let symbols: HashSet<String> = [
            "println",
            "eprintln",
            "format",
            "vec",
            "String",
            "Vec",
            "HashMap",
            "HashSet",
            "BTreeMap",
            "BTreeSet",
            "Option",
            "Result",
            "Ok",
            "Err",
            "Some",
            "None",
            "Box",
            "Rc",
            "Arc",
            "RefCell",
            "Mutex",
            "RwLock",
            "Clone",
            "Debug",
            "Display",
            "Default",
            "Iterator",
            "IntoIterator",
            "From",
            "Into",
            "TryFrom",
            "TryInto",
            "AsRef",
            "AsMut",
            "Drop",
            "Fn",
            "FnMut",
            "FnOnce",
            "Send",
            "Sync",
            "Sized",
            "Unpin",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        self.external_libs.insert(
            "rust_stdlib".to_string(),
            ExternalLibrary {
                name: "rust_stdlib".to_string(),
                language: Language::Rust,
                known_symbols: symbols,
                is_stdlib: true,
            },
        );
    }

    fn register_node_builtins(&mut self) {
        let symbols: HashSet<String> = [
            "console",
            "setTimeout",
            "setInterval",
            "clearTimeout",
            "clearInterval",
            "Promise",
            "fetch",
            "JSON",
            "Math",
            "Date",
            "RegExp",
            "Error",
            "TypeError",
            "RangeError",
            "Array",
            "Object",
            "Map",
            "Set",
            "WeakMap",
            "WeakSet",
            "Symbol",
            "Proxy",
            "Reflect",
            "require",
            "module",
            "exports",
            "process",
            "Buffer",
            "__dirname",
            "__filename",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        self.external_libs.insert(
            "node_builtins".to_string(),
            ExternalLibrary {
                name: "node_builtins".to_string(),
                language: Language::JavaScript,
                known_symbols: symbols.clone(),
                is_stdlib: true,
            },
        );

        self.external_libs.insert(
            "ts_builtins".to_string(),
            ExternalLibrary {
                name: "ts_builtins".to_string(),
                language: Language::TypeScript,
                known_symbols: symbols,
                is_stdlib: true,
            },
        );
    }

    fn register_go_stdlib(&mut self) {
        let symbols: HashSet<String> = [
            "fmt", "os", "io", "strings", "strconv", "errors", "context", "sync", "time", "net",
            "http", "json", "log", "testing", "reflect", "sort", "math", "crypto", "path",
            "filepath", "bytes", "bufio", "regexp",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        self.external_libs.insert(
            "go_stdlib".to_string(),
            ExternalLibrary {
                name: "go_stdlib".to_string(),
                language: Language::Go,
                known_symbols: symbols,
                is_stdlib: true,
            },
        );
    }
}

fn strip_overload_suffix(name: &str) -> &str {
    name.split('$').next().unwrap_or(name)
}

fn import_matches(language: Language, kind: ReferenceKind, import_name: &str, name: &str) -> bool {
    if import_name == name {
        return true;
    }
    if import_name.ends_with(&format!(".{}", name)) {
        return true;
    }
    if name.contains('.') && name.ends_with(import_name) {
        return true;
    }

    if language == Language::Java {
        return import_matches_java(kind, import_name, name);
    }

    import_name.contains(name)
        || name.contains(import_name.rsplit('/').next().unwrap_or(import_name))
}

fn import_matches_java(kind: ReferenceKind, import_name: &str, name: &str) -> bool {
    let normalized_import = import_name.trim();
    let normalized_name = name.trim();

    if normalized_import
        .rsplit('.')
        .next()
        .is_some_and(|leaf| leaf == normalized_name)
    {
        return true;
    }

    if let Some(prefix) = normalized_import.strip_suffix(".*") {
        if normalized_name.starts_with(prefix) {
            return true;
        }
        if kind != ReferenceKind::Call && !normalized_name.contains('.') {
            return true;
        }
    }

    if kind == ReferenceKind::Call
        && normalized_import
            .rsplit('.')
            .next()
            .is_some_and(|leaf| leaf == normalized_name)
    {
        return true;
    }

    false
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

/// A raw unit with its resolved references.
#[derive(Debug, Clone)]
pub struct ResolvedUnit {
    /// The original raw code unit.
    pub unit: RawCodeUnit,
    /// Resolved references.
    pub resolved_refs: Vec<ResolvedReference>,
}

/// A resolved reference.
#[derive(Debug, Clone)]
pub struct ResolvedReference {
    /// The original raw reference.
    pub raw: RawReference,
    /// Resolution result.
    pub resolution: Resolution,
}

/// Result of resolving a reference.
#[derive(Debug, Clone)]
pub enum Resolution {
    /// Resolved to a local unit by temp_id.
    Local(u64),
    /// Resolved to an imported unit.
    Imported(ImportedSymbol),
    /// Resolved to an external library.
    External(ExternalSymbol),
    /// Could not resolve.
    Unresolved,
}

/// A symbol resolved through an import.
#[derive(Debug, Clone)]
pub struct ImportedSymbol {
    /// The import unit temp_id.
    pub unit_id: u64,
    /// The import path string.
    pub import_path: String,
}

/// A symbol from an external library.
#[derive(Debug, Clone)]
pub struct ExternalSymbol {
    /// Library name.
    pub library: String,
    /// Symbol name.
    pub symbol: String,
    /// Is from standard library.
    pub is_stdlib: bool,
}
