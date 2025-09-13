use std::collections::HashSet;

/// Configuration for which tag kinds to generate
#[derive(Debug, Clone)]
pub struct TagKindConfig {
    pub enabled_kinds: HashSet<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KIND_MAPPING: &[(&[&str], &str)] = &[
        (&["f", "function"], "f"),
        (&["s", "struct"], "s"),
        (&["c", "class"], "c"),
        (&["m", "member"], "m"),
        (&["v", "variable"], "v"),
    ];

    fn test_default_kinds() -> HashSet<String> {
        let mut defaults = HashSet::new();
        defaults.insert("f".to_string());
        defaults.insert("s".to_string());
        defaults.insert("c".to_string());
        defaults
    }

    #[test]
    fn test_from_string_override_mode_concatenated() {
        let config = TagKindConfig::from_string("fsm", TEST_KIND_MAPPING, &test_default_kinds());
        
        assert!(config.is_kind_enabled("f"));
        assert!(config.is_kind_enabled("s"));
        assert!(config.is_kind_enabled("m"));
        assert!(!config.is_kind_enabled("c")); // Not in input, should be disabled
        assert!(!config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_string_override_mode_comma_separated() {
        let config = TagKindConfig::from_string("f,struct,m", TEST_KIND_MAPPING, &test_default_kinds());
        
        assert!(config.is_kind_enabled("f"));
        assert!(config.is_kind_enabled("s")); // "struct" maps to "s"
        assert!(config.is_kind_enabled("m"));
        assert!(!config.is_kind_enabled("c")); // Not in input, should be disabled
        assert!(!config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_string_modifier_mode_concatenated() {
        let config = TagKindConfig::from_string("+m-c", TEST_KIND_MAPPING, &test_default_kinds());
        
        assert!(config.is_kind_enabled("f")); // From defaults
        assert!(config.is_kind_enabled("s")); // From defaults
        assert!(!config.is_kind_enabled("c")); // Removed by -c
        assert!(config.is_kind_enabled("m")); // Added by +m
        assert!(!config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_string_modifier_mode_comma_separated() {
        let config = TagKindConfig::from_string("+member, -class, +variable", TEST_KIND_MAPPING, &test_default_kinds());
        
        assert!(config.is_kind_enabled("f")); // From defaults
        assert!(config.is_kind_enabled("s")); // From defaults
        assert!(!config.is_kind_enabled("c")); // Removed by -class
        assert!(config.is_kind_enabled("m")); // Added by +member
        assert!(config.is_kind_enabled("v")); // Added by +variable
    }

    #[test]
    fn test_from_string_modifier_mode_mixed() {
        let config = TagKindConfig::from_string("+m-s+v", TEST_KIND_MAPPING, &test_default_kinds());
        
        assert!(config.is_kind_enabled("f")); // From defaults
        assert!(!config.is_kind_enabled("s")); // Removed by -s
        assert!(config.is_kind_enabled("c")); // From defaults
        assert!(config.is_kind_enabled("m")); // Added by +m
        assert!(config.is_kind_enabled("v")); // Added by +v
    }

    #[test]
    fn test_from_string_empty_input() {
        let config = TagKindConfig::from_string("", TEST_KIND_MAPPING, &test_default_kinds());
        
        // Empty input in override mode should result in no enabled kinds
        assert!(!config.is_kind_enabled("f"));
        assert!(!config.is_kind_enabled("s"));
        assert!(!config.is_kind_enabled("c"));
        assert!(!config.is_kind_enabled("m"));
        assert!(!config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_cpp_kinds_string_default() {
        let config = TagKindConfig::from_cpp_kinds_string("");
        
        // Should have no kinds enabled for empty input (override mode)
        assert!(!config.is_kind_enabled("d"));
        assert!(!config.is_kind_enabled("e"));
        assert!(!config.is_kind_enabled("f"));
    }

    #[test]
    fn test_from_cpp_kinds_string_override_mode() {
        let config = TagKindConfig::from_cpp_kinds_string("def");
        
        // Only specified kinds should be enabled
        assert!(config.is_kind_enabled("d")); // macro
        assert!(config.is_kind_enabled("e")); // enumerator
        assert!(config.is_kind_enabled("f")); // function
        assert!(!config.is_kind_enabled("g")); // enum - not specified
        assert!(!config.is_kind_enabled("h")); // header - not specified
        assert!(!config.is_kind_enabled("m")); // member - not specified
    }

    #[test]
    fn test_from_cpp_kinds_string_override_mode_comma() {
        let config = TagKindConfig::from_cpp_kinds_string("macro,function,class");
        
        assert!(config.is_kind_enabled("d")); // macro
        assert!(config.is_kind_enabled("f")); // function
        assert!(config.is_kind_enabled("c")); // class
        assert!(!config.is_kind_enabled("e")); // enumerator - not specified
        assert!(!config.is_kind_enabled("g")); // enum - not specified
    }

    #[test]
    fn test_from_cpp_kinds_string_modifier_mode() {
        let config = TagKindConfig::from_cpp_kinds_string("+c-m");
        
        // Should start with defaults and apply modifications
        assert!(config.is_kind_enabled("d")); // macro - from defaults
        assert!(config.is_kind_enabled("e")); // enumerator - from defaults
        assert!(config.is_kind_enabled("f")); // function - from defaults
        assert!(config.is_kind_enabled("g")); // enum - from defaults
        assert!(config.is_kind_enabled("h")); // header - from defaults
        assert!(!config.is_kind_enabled("m")); // member - removed by -m
        assert!(config.is_kind_enabled("s")); // struct - from defaults
        assert!(config.is_kind_enabled("t")); // typedef - from defaults
        assert!(config.is_kind_enabled("u")); // union - from defaults
        assert!(config.is_kind_enabled("v")); // variable - from defaults
        assert!(config.is_kind_enabled("c")); // class - added by +c
    }

    #[test]
    fn test_from_cpp_kinds_string_modifier_mode_comma() {
        let config = TagKindConfig::from_cpp_kinds_string("+class, -member, +local");
        
        // Should start with defaults and apply modifications
        assert!(config.is_kind_enabled("d")); // macro - from defaults
        assert!(config.is_kind_enabled("f")); // function - from defaults
        assert!(!config.is_kind_enabled("m")); // member - removed by -member
        assert!(config.is_kind_enabled("c")); // class - added by +class
        assert!(config.is_kind_enabled("l")); // local - added by +local
    }

    #[test]
    fn test_from_cpp_kinds_string_all_defaults() {
        let config = TagKindConfig::from_cpp_kinds_string("+");
        
        // Just "+" should enable all defaults (though this is a bit of an edge case)
        // The + without a kind should be ignored, leaving us with defaults
        assert!(config.is_kind_enabled("d")); // macro
        assert!(config.is_kind_enabled("e")); // enumerator
        assert!(config.is_kind_enabled("f")); // function
        assert!(config.is_kind_enabled("g")); // enum
        assert!(config.is_kind_enabled("h")); // header
        assert!(config.is_kind_enabled("m")); // member
        assert!(config.is_kind_enabled("s")); // struct
        assert!(config.is_kind_enabled("t")); // typedef
        assert!(config.is_kind_enabled("u")); // union
        assert!(config.is_kind_enabled("v")); // variable
        
        // Non-default kinds should not be enabled
        assert!(!config.is_kind_enabled("c")); // class
        assert!(!config.is_kind_enabled("l")); // local
        assert!(!config.is_kind_enabled("p")); // prototype
    }
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

    /// Create a configuration from a kinds string with support for default kinds and +/- modifiers
    /// 
    /// # Arguments
    /// * `kinds_str` - The kinds string (e.g., "+f,-m", "fsc", "f,s,c")
    /// * `kind_mapping` - Mapping from aliases to canonical kind codes
    /// * `default_kinds` - Set of kinds enabled by default for this language
    /// 
    /// # Behavior
    /// - If no +/- prefixes are used: only explicitly listed kinds are enabled (override mode)
    /// - If +/- prefixes are used: start with default_kinds, then apply modifications
    /// - `+kind`: add kind to enabled set
    /// - `-kind`: remove kind from enabled set
    pub fn from_string(
        kinds_str: &str, 
        kind_mapping: &[(&[&str], &str)], 
        default_kinds: &HashSet<String>
    ) -> Self {
        let mut enabled_kinds = HashSet::new();
        
        // Build the full mapping from aliases to canonical forms
        let full_kind_map: std::collections::HashMap<&str, &str> = kind_mapping
            .iter()
            .flat_map(|(aliases, canonical)| aliases.iter().map(move |alias| (*alias, *canonical)))
            .collect();
        
        // Check if any entry has +/- prefix to determine mode
        let has_modifiers = kinds_str.chars().any(|c| c == '+' || c == '-') ||
                           kinds_str.split(',').any(|s| {
                               let trimmed = s.trim();
                               trimmed.starts_with('+') || trimmed.starts_with('-')
                           });
        
        if has_modifiers {
            // Modifier mode: start with defaults and apply changes
            enabled_kinds = default_kinds.clone();
            
            let entries: Vec<String> = if kinds_str.contains(',') {
                kinds_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
            } else {
                // For non-comma format, we need to parse character by character with +/- prefixes
                let mut entries = Vec::new();
                let mut chars = kinds_str.chars().peekable();
                
                while let Some(ch) = chars.next() {
                    if ch == '+' || ch == '-' {
                        if let Some(next_ch) = chars.next() {
                            if !next_ch.is_whitespace() {
                                entries.push(format!("{}{}", ch, next_ch));
                            }
                        }
                    } else if !ch.is_whitespace() {
                        entries.push(ch.to_string());
                    }
                }
                entries
            };
            
            for entry in entries {
                let (operation, kind_str) = if entry.starts_with('+') {
                    ('+', &entry[1..])
                } else if entry.starts_with('-') {
                    ('-', &entry[1..])
                } else {
                    ('+', entry.as_str()) // Default to add if no prefix
                };
                
                if let Some(canonical) = full_kind_map.get(kind_str) {
                    match operation {
                        '+' => {
                            enabled_kinds.insert((*canonical).to_string());
                        }
                        '-' => {
                            enabled_kinds.remove(*canonical);
                        }
                        _ => unreachable!(),
                    }
                } else {
                    eprintln!("Warning: Unknown tag kind: {}", kind_str);
                }
            }
        } else {
            // Override mode: only include explicitly listed kinds
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
        }
        
        Self { enabled_kinds }
    }

    /// Create a configuration from a kinds string (e.g. "f,s,c" or "fsc")
    pub fn from_string_legacy(kinds_str: &str, kind_mapping: &[(&[&str], &str)]) -> Self {
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
        Self::from_string_legacy(kinds_str, RUST_KIND_MAPPING)
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
        Self::from_string_legacy(kinds_str, GO_KIND_MAPPING)
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

    /// Create a configuration from a kinds string for C++ (e.g., "defg", "+f,-m", or "d,e,f,g")
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
            (&["x", "externvar"], "x"),
            (&["z", "parameter"], "z"),
            (&["L", "label"], "L"),
            (&["D", "macroparam"], "D"),
            (&["c", "class"], "c"),
            (&["n", "namespace"], "n"),
            (&["A", "alias"], "A"),
            (&["N", "name"], "N"),
            (&["U", "using"], "U"),
            (&["Z", "tparam"], "Z"),
        ];
        
        // Default enabled kinds for C++
        let mut default_kinds = HashSet::new();
        default_kinds.insert("d".to_string()); // macro
        default_kinds.insert("e".to_string()); // enumerator
        default_kinds.insert("f".to_string()); // function
        default_kinds.insert("g".to_string()); // enum
        default_kinds.insert("h".to_string()); // header
        default_kinds.insert("m".to_string()); // member
        default_kinds.insert("s".to_string()); // struct
        default_kinds.insert("t".to_string()); // typedef
        default_kinds.insert("u".to_string()); // union
        default_kinds.insert("v".to_string()); // variable
        
        Self::from_string(kinds_str, CPP_KIND_MAPPING, &default_kinds)
    }
}
