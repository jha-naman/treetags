use std::collections::HashSet;

/// Configuration for which tag kinds to generate
#[derive(Debug, Clone)]
pub struct TagKindConfig {
    pub enabled_kinds: HashSet<String>,
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

        Self { enabled_kinds }
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

        Self { enabled_kinds }
    }

    /// Create a configuration from a kinds string (e.g. "f,s,c" or "fsc")
    pub fn from_string(kinds_str: &str, kind_mapping: &[(&[&str], &str)]) -> Self {
        let mut enabled_kinds = HashSet::new();

        let full_kind_map: std::collections::HashMap<&str, &str> = kind_mapping
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

        Self { enabled_kinds }
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
        Self::from_string(kinds_str, RUST_KIND_MAPPING)
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
        Self::from_string(kinds_str, GO_KIND_MAPPING)
    }

    /// Check if a tag kind is enabled
    pub fn is_kind_enabled(&self, kind: &str) -> bool {
        self.enabled_kinds.contains(kind)
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

        Self { enabled_kinds }
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
        Self::from_string(kinds_str, CPP_KIND_MAPPING)
    }
}
