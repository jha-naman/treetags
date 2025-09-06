use std::collections::{HashMap, HashSet};

/// Configuration for which tag kinds to generate
#[derive(Debug, Clone)]
pub struct TagKindConfig {
    pub enabled_kinds: HashSet<String>,
    // Cache for optimization - whether we need to traverse certain node types
    pub needs_traversal_cache: HashMap<String, bool>,
}

impl TagKindConfig {
    /// Create a new configuration with all kinds enabled by default for Rust
    pub fn new_rust() -> Self {
        let mut enabled_kinds = HashSet::new();
        // Add all possible tag kinds
        enabled_kinds.insert("n".to_string()); // module
        enabled_kinds.insert("s".to_string()); // struct
        enabled_kinds.insert("g".to_string()); // enum
        enabled_kinds.insert("u".to_string()); // union
        enabled_kinds.insert("i".to_string()); // trait/interface
        enabled_kinds.insert("c".to_string()); // implementation
        enabled_kinds.insert("f".to_string()); // function
        enabled_kinds.insert("P".to_string()); // method/procedure
        enabled_kinds.insert("m".to_string()); // method signature
        enabled_kinds.insert("e".to_string()); // enum variant
        enabled_kinds.insert("T".to_string()); // associated type
        enabled_kinds.insert("C".to_string()); // constant
        enabled_kinds.insert("v".to_string()); // variable/static
        enabled_kinds.insert("t".to_string()); // type alias
        enabled_kinds.insert("M".to_string()); // macro

        let mut config = Self {
            enabled_kinds,
            needs_traversal_cache: HashMap::new(),
        };
        config.rebuild_rust_traversal_cache();
        config
    }

    /// Create a new configuration with all kinds enabled by default for Go
    pub fn new_go() -> Self {
        let mut enabled_kinds = HashSet::new();
        // Add all possible Go tag kinds
        enabled_kinds.insert("p".to_string()); // package
        enabled_kinds.insert("f".to_string()); // function
        enabled_kinds.insert("c".to_string()); // constant
        enabled_kinds.insert("t".to_string()); // type
        enabled_kinds.insert("v".to_string()); // variable
        enabled_kinds.insert("s".to_string()); // struct
        enabled_kinds.insert("i".to_string()); // interface
        enabled_kinds.insert("m".to_string()); // struct member
        enabled_kinds.insert("M".to_string()); // struct anonymous member
        enabled_kinds.insert("n".to_string()); // interface method specification
        enabled_kinds.insert("P".to_string()); // imported package
        enabled_kinds.insert("a".to_string()); // type alias

        let mut config = Self {
            enabled_kinds,
            needs_traversal_cache: HashMap::new(),
        };
        config.rebuild_go_traversal_cache();
        config
    }

    /// Create a configuration from a kinds string (e.g. "f,s,c" or "fsc")
    pub fn from_string(
        kinds_str: &str,
        kind_mapping: &[(&[&str], &str)],
        rebuild_cache_fn: impl FnOnce(&mut Self),
    ) -> Self {
        let mut enabled_kinds = HashSet::new();

        let full_kind_map: HashMap<&str, &str> = kind_mapping
            .iter()
            .flat_map(|(aliases, canonical)| aliases.iter().map(move |alias| (*alias, *canonical)))
            .collect();

        if kinds_str.contains(',') {
            for kind in kinds_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                if let Some(canonical) = full_kind_map.get(kind) {
                    enabled_kinds.insert((*canonical).to_string());
                } else {
                    eprintln!("Warning: Unknown tag kind: {}", kind);
                }
            }
        } else {
            for kind_char in kinds_str.chars().filter(|c| !c.is_whitespace()) {
                let kind_str = &kind_char.to_string();
                if let Some(canonical) = full_kind_map.get(kind_str.as_str()) {
                    enabled_kinds.insert((*canonical).to_string());
                } else {
                    eprintln!("Warning: Unknown tag kind: {}", kind_char);
                }
            }
        }

        let mut config = Self {
            enabled_kinds,
            needs_traversal_cache: HashMap::new(),
        };
        rebuild_cache_fn(&mut config);
        config
    }

    /// Create a configuration from a kinds string for Rust (e.g., "nsf" or "n,s,f")
    pub fn from_rust_kinds_string(kinds_str: &str) -> Self {
        const RUST_KIND_MAPPING: &[(&[&str], &str)] = &[
            (&["n", "module"], "n"),
            (&["s", "struct"], "s"),
            (&["g", "enum"], "g"),
            (&["u", "union"], "u"),
            (&["i", "trait", "interface"], "i"),
            (&["c", "impl", "implementation"], "c"),
            (&["f", "function"], "f"),
            (&["P", "method", "procedure"], "P"),
            (&["m", "field"], "m"),
            (&["e", "enumerator", "variant"], "e"),
            (&["T", "typedef", "associated_type"], "T"),
            (&["C", "constant"], "C"),
            (&["v", "variable", "static"], "v"),
            (&["t", "type", "alias"], "t"),
            (&["M", "macro"], "M"),
        ];
        Self::from_string(kinds_str, RUST_KIND_MAPPING, |config| {
            config.rebuild_rust_traversal_cache()
        })
    }

    /// Create a configuration from a kinds string for Go (e.g., "pfc" or "p,f,c")
    pub fn from_go_kinds_string(kinds_str: &str) -> Self {
        const GO_KIND_MAPPING: &[(&[&str], &str)] = &[
            (&["p", "package"], "p"),
            (&["f", "function"], "f"),
            (&["c", "constant"], "c"),
            (&["t", "type"], "t"),
            (&["v", "variable"], "v"),
            (&["s", "struct"], "s"),
            (&["i", "interface"], "i"),
            (&["m", "member"], "m"),
            (&["M", "anonymous"], "M"),
            (&["n", "method"], "n"),
            (&["P", "import"], "P"),
            (&["a", "alias"], "a"),
        ];
        Self::from_string(kinds_str, GO_KIND_MAPPING, |config| {
            config.rebuild_go_traversal_cache()
        })
    }

    /// Check if a tag kind is enabled
    pub fn is_kind_enabled(&self, kind: &str) -> bool {
        self.enabled_kinds.contains(kind)
    }

    /// Check if we need to traverse into a specific node type for optimization
    pub fn needs_traversal(&self, node_kind: &str) -> bool {
        self.needs_traversal_cache
            .get(node_kind)
            .copied()
            .unwrap_or(true)
    }

    /// Rebuild the traversal optimization cache for Rust
    fn rebuild_rust_traversal_cache(&mut self) {
        self.needs_traversal_cache.clear();

        // Define what child tags each node type can contain
        // Only traverse if we need the parent tag OR any potential child tags

        // Modules can contain everything
        self.needs_traversal_cache.insert(
            "mod_item".to_string(),
            self.is_kind_enabled("n") || self.needs_any_child_tags(),
        );

        // Structs can contain fields (tagged as 'm')
        self.needs_traversal_cache.insert(
            "struct_item".to_string(),
            self.is_kind_enabled("s") || self.is_kind_enabled("m"),
        );

        // Enums can contain variants (tagged as 'e')
        self.needs_traversal_cache.insert(
            "enum_item".to_string(),
            self.is_kind_enabled("g") || self.is_kind_enabled("e"),
        );

        // Unions are simple - no child tags typically
        self.needs_traversal_cache
            .insert("union_item".to_string(), self.is_kind_enabled("u"));

        // Traits can contain methods ('m'), associated types ('T'), constants ('C')
        self.needs_traversal_cache.insert(
            "trait_item".to_string(),
            self.is_kind_enabled("i")
                || self.is_kind_enabled("m")
                || self.is_kind_enabled("T")
                || self.is_kind_enabled("C"),
        );

        // Impl blocks can contain methods ('P'), associated types ('T'), constants ('C')
        self.needs_traversal_cache.insert(
            "impl_item".to_string(),
            self.is_kind_enabled("c")
                || self.is_kind_enabled("P")
                || self.is_kind_enabled("T")
                || self.is_kind_enabled("C"),
        );

        // Functions are leaf nodes - no child tags
        self.needs_traversal_cache.insert(
            "function_item".to_string(),
            self.is_kind_enabled("f") || self.is_kind_enabled("P"),
        );

        self.needs_traversal_cache.insert(
            "function_signature_item".to_string(),
            self.is_kind_enabled("m"),
        );

        // Other leaf nodes
        self.needs_traversal_cache
            .insert("associated_type".to_string(), self.is_kind_enabled("T"));
        self.needs_traversal_cache
            .insert("const_item".to_string(), self.is_kind_enabled("C"));
        self.needs_traversal_cache
            .insert("static_item".to_string(), self.is_kind_enabled("v"));
        self.needs_traversal_cache
            .insert("type_item".to_string(), self.is_kind_enabled("t"));
        self.needs_traversal_cache
            .insert("macro_definition".to_string(), self.is_kind_enabled("M"));
    }

    /// Helper to check if we need any child tags (for modules)
    fn needs_any_child_tags(&self) -> bool {
        // If any tag type is enabled, modules might need traversal
        !self.enabled_kinds.is_empty()
    }

    /// Rebuild the traversal optimization cache for Go
    fn rebuild_go_traversal_cache(&mut self) {
        self.needs_traversal_cache.clear();

        // Define what child tags each node type can contain
        self.needs_traversal_cache
            .insert("source_file".to_string(), !self.enabled_kinds.is_empty());

        self.needs_traversal_cache
            .insert("package_clause".to_string(), self.is_kind_enabled("p"));

        self.needs_traversal_cache
            .insert("import_declaration".to_string(), self.is_kind_enabled("P"));

        self.needs_traversal_cache.insert(
            "function_declaration".to_string(),
            self.is_kind_enabled("f"),
        );

        self.needs_traversal_cache
            .insert("method_declaration".to_string(), self.is_kind_enabled("f"));

        self.needs_traversal_cache
            .insert("const_declaration".to_string(), self.is_kind_enabled("c"));

        self.needs_traversal_cache
            .insert("var_declaration".to_string(), self.is_kind_enabled("v"));

        self.needs_traversal_cache.insert(
            "short_var_declaration".to_string(),
            self.is_kind_enabled("v"),
        );

        self.needs_traversal_cache.insert(
            "type_declaration".to_string(),
            self.is_kind_enabled("t")
                || self.is_kind_enabled("s")
                || self.is_kind_enabled("i")
                || self.is_kind_enabled("a"),
        );

        self.needs_traversal_cache.insert(
            "struct_type".to_string(),
            self.is_kind_enabled("s") || self.is_kind_enabled("m"),
        );

        self.needs_traversal_cache.insert(
            "interface_type".to_string(),
            self.is_kind_enabled("i") || self.is_kind_enabled("n"),
        );
    }

    /// Create a new configuration with all kinds enabled by default for C++
    pub fn new_cpp() -> Self {
        let mut enabled_kinds = HashSet::new();
        // Add all possible C++ tag kinds
        enabled_kinds.insert("d".to_string()); // macro definitions
        enabled_kinds.insert("e".to_string()); // enumerators
        enabled_kinds.insert("f".to_string()); // function definitions
        enabled_kinds.insert("g".to_string()); // enumeration names
        enabled_kinds.insert("h".to_string()); // included header files
        enabled_kinds.insert("l".to_string()); // local variables [off]
        enabled_kinds.insert("m".to_string()); // class, struct, and union members
        enabled_kinds.insert("p".to_string()); // function prototypes [off]
        enabled_kinds.insert("s".to_string()); // structure names
        enabled_kinds.insert("t".to_string()); // typedefs
        enabled_kinds.insert("u".to_string()); // union names
        enabled_kinds.insert("v".to_string()); // variable definitions
        enabled_kinds.insert("x".to_string()); // external and forward variable declarations [off]
        enabled_kinds.insert("z".to_string()); // function parameters inside function or prototype definitions [off]
        enabled_kinds.insert("L".to_string()); // goto labels [off]
        enabled_kinds.insert("D".to_string()); // parameters inside macro definitions [off]
        enabled_kinds.insert("c".to_string()); // classes
        enabled_kinds.insert("n".to_string()); // namespaces
        enabled_kinds.insert("A".to_string()); // namespace aliases [off]
        enabled_kinds.insert("N".to_string()); // names imported via using scope::symbol [off]
        enabled_kinds.insert("U".to_string()); // using namespace statements [off]
        enabled_kinds.insert("Z".to_string()); // template parameters [off]

        let mut config = Self {
            enabled_kinds,
            needs_traversal_cache: HashMap::new(),
        };
        config.rebuild_cpp_traversal_cache();
        config
    }

    /// Create a configuration from a kinds string for C++ (e.g., "defg" or "d,e,f,g")
    pub fn from_cpp_kinds_string(kinds_str: &str) -> Self {
        const CPP_KIND_MAPPING: &[(&[&str], &str)] = &[
            (&["d", "macro"], "d"),
            (&["e", "enumerator"], "e"),
            (&["f", "function"], "f"),
            (&["g", "enum"], "g"),
            (&["h", "header"], "h"),
            (&["l", "local"], "l"),
            (&["m", "member"], "m"),
            (&["p", "prototype"], "p"),
            (&["s", "struct"], "s"),
            (&["t", "typedef"], "t"),
            (&["u", "union"], "u"),
            (&["v", "variable"], "v"),
            (&["x", "external"], "x"),
            (&["z", "parameter"], "z"),
            (&["L", "label"], "L"),
            (&["D", "macroparam"], "D"),
            (&["c", "class"], "c"),
            (&["n", "namespace"], "n"),
            (&["A", "alias"], "A"),
            (&["N", "using"], "N"),
            (&["U", "usingnamespace"], "U"),
            (&["Z", "template"], "Z"),
        ];
        Self::from_string(kinds_str, CPP_KIND_MAPPING, |config| {
            config.rebuild_cpp_traversal_cache()
        })
    }

    /// Rebuild the traversal optimization cache for C++
    fn rebuild_cpp_traversal_cache(&mut self) {
        self.needs_traversal_cache.clear();

        self.needs_traversal_cache.insert(
            "translation_unit".to_string(),
            !self.enabled_kinds.is_empty(),
        );

        self.needs_traversal_cache.insert(
            "namespace_definition".to_string(),
            self.is_kind_enabled("n") || self.needs_any_child_tags(),
        );

        self.needs_traversal_cache.insert(
            "class_specifier".to_string(),
            self.is_kind_enabled("c") || self.is_kind_enabled("m"),
        );

        self.needs_traversal_cache.insert(
            "struct_specifier".to_string(),
            self.is_kind_enabled("s") || self.is_kind_enabled("m"),
        );

        self.needs_traversal_cache.insert(
            "union_specifier".to_string(),
            self.is_kind_enabled("u") || self.is_kind_enabled("m"),
        );

        self.needs_traversal_cache.insert(
            "enum_specifier".to_string(),
            self.is_kind_enabled("g") || self.is_kind_enabled("e"),
        );

        self.needs_traversal_cache
            .insert("function_definition".to_string(), self.is_kind_enabled("f"));

        self.needs_traversal_cache
            .insert("function_declarator".to_string(), self.is_kind_enabled("p"));

        self.needs_traversal_cache.insert(
            "declaration".to_string(),
            self.is_kind_enabled("v") || self.is_kind_enabled("t"),
        );

        self.needs_traversal_cache
            .insert("field_declaration".to_string(), self.is_kind_enabled("m"));

        self.needs_traversal_cache
            .insert("preproc_def".to_string(), self.is_kind_enabled("d"));

        self.needs_traversal_cache
            .insert("preproc_include".to_string(), self.is_kind_enabled("h"));

        self.needs_traversal_cache
            .insert("type_definition".to_string(), self.is_kind_enabled("t"));
    }
}
