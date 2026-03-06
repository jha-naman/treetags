use std::collections::HashSet;

/// Configuration for which tag kinds to generate
#[derive(Debug, Clone)]
pub struct TagKindConfig {
    pub enabled_kinds: HashSet<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DEFAULT_KINDS_MAPPING: &[(&[&str], &str)] = &[
        (&["f", "function"], "f"),
        (&["s", "struct"], "s"),
        (&["c", "class"], "c"),
    ];

    const TEST_OPTIONAL_KINDS_MAPPING: &[(&[&str], &str)] =
        &[(&["m", "member"], "m"), (&["v", "variable"], "v")];

    #[test]
    fn test_from_string_override_mode_concatenated() {
        let config = TagKindConfig::from_string(
            "fsm",
            TEST_DEFAULT_KINDS_MAPPING,
            &TEST_OPTIONAL_KINDS_MAPPING,
        );

        assert!(config.is_kind_enabled("f"));
        assert!(config.is_kind_enabled("s"));
        assert!(config.is_kind_enabled("m"));
        assert!(!config.is_kind_enabled("c")); // Not in input, should be disabled
        assert!(!config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_string_override_mode_comma_separated() {
        let config = TagKindConfig::from_string(
            "f,struct,m",
            TEST_DEFAULT_KINDS_MAPPING,
            &TEST_OPTIONAL_KINDS_MAPPING,
        );

        assert!(config.is_kind_enabled("f"));
        assert!(config.is_kind_enabled("s")); // "struct" maps to "s"
        assert!(config.is_kind_enabled("m"));
        assert!(!config.is_kind_enabled("c")); // Not in input, should be disabled
        assert!(!config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_string_modifier_mode_concatenated() {
        let config = TagKindConfig::from_string(
            "+m-c",
            TEST_DEFAULT_KINDS_MAPPING,
            &TEST_OPTIONAL_KINDS_MAPPING,
        );

        assert!(config.is_kind_enabled("f")); // From defaults
        assert!(config.is_kind_enabled("s")); // From defaults
        assert!(!config.is_kind_enabled("c")); // Removed by -c
        assert!(config.is_kind_enabled("m")); // Added by +m
        assert!(!config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_string_modifier_mode_comma_separated() {
        let config = TagKindConfig::from_string(
            "+member, -class, +variable",
            TEST_DEFAULT_KINDS_MAPPING,
            &TEST_OPTIONAL_KINDS_MAPPING,
        );

        assert!(config.is_kind_enabled("f")); // From defaults
        assert!(config.is_kind_enabled("s")); // From defaults
        assert!(!config.is_kind_enabled("c")); // Removed by -class
        assert!(config.is_kind_enabled("m")); // Added by +member
        assert!(config.is_kind_enabled("v")); // Added by +variable
    }

    #[test]
    fn test_from_string_modifier_mode_mixed() {
        let config = TagKindConfig::from_string(
            "+m-s+v",
            TEST_DEFAULT_KINDS_MAPPING,
            &TEST_OPTIONAL_KINDS_MAPPING,
        );

        assert!(config.is_kind_enabled("f")); // From defaults
        assert!(!config.is_kind_enabled("s")); // Removed by -s
        assert!(config.is_kind_enabled("c")); // From defaults
        assert!(config.is_kind_enabled("m")); // Added by +m
        assert!(config.is_kind_enabled("v")); // Added by +v
    }

    #[test]
    fn test_from_string_empty_input() {
        let config = TagKindConfig::from_string(
            "",
            TEST_DEFAULT_KINDS_MAPPING,
            &TEST_OPTIONAL_KINDS_MAPPING,
        );

        // Empty input should now result in default kinds enabled
        assert!(config.is_kind_enabled("f"));
        assert!(config.is_kind_enabled("s"));
        assert!(config.is_kind_enabled("c"));
        assert!(!config.is_kind_enabled("m")); // Optional, not enabled by default
        assert!(!config.is_kind_enabled("v")); // Optional, not enabled by default
    }

    #[test]
    fn test_from_cpp_kinds_string_default() {
        let config = TagKindConfig::from_cpp_kinds_string("");

        // Should have default kinds enabled for empty input
        assert!(config.is_kind_enabled("d"));
        assert!(config.is_kind_enabled("e"));
        assert!(config.is_kind_enabled("f"));
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
        assert!(config.is_kind_enabled("c")); // class

        // Non-default kinds should not be enabled
        assert!(!config.is_kind_enabled("l")); // local
        assert!(!config.is_kind_enabled("p")); // prototype
    }
}

impl TagKindConfig {
    fn rust_config_data() -> (
        &'static [(&'static [&'static str], &'static str)],
        &'static [(&'static [&'static str], &'static str)],
    ) {
        const DEFAULTS: &[(&[&str], &str)] = &[
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
        const OPTIONALS: &[(&[&str], &str)] = &[];
        (DEFAULTS, OPTIONALS)
    }

    fn go_config_data() -> (
        &'static [(&'static [&'static str], &'static str)],
        &'static [(&'static [&'static str], &'static str)],
    ) {
        const DEFAULTS: &[(&[&str], &str)] = &[
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
        const OPTIONALS: &[(&[&str], &str)] = &[];
        (DEFAULTS, OPTIONALS)
    }

    /// Create a configuration from a kinds string with support for default kinds and +/- modifiers
    ///
    /// # Arguments
    /// * `kinds_str` - The kinds string (e.g., "+f,-m", "fsc", "f,s,c")
    /// * `defaults_mapping` - Mapping for kinds enabled by default
    /// * `optionals_mapping` - Mapping for kinds disabled by default
    ///
    /// # Behavior
    /// - If no +/- prefixes are used: only explicitly listed kinds are enabled (override mode)
    /// - If +/- prefixes are used: start with default_kinds, then apply modifications
    /// - `+kind`: add kind to enabled set
    /// - `-kind`: remove kind from enabled set
    pub fn from_string(
        kinds_str: &str,
        defaults_mapping: &[(&[&str], &str)],
        optionals_mapping: &[(&[&str], &str)],
    ) -> Self {
        // 1. Build the default_kinds set from defaults_mapping
        let mut default_kinds = HashSet::new();
        for &(_, canonical) in defaults_mapping {
            default_kinds.insert(canonical.to_string());
        }

        let mut enabled_kinds = HashSet::new();

        // Build the full mapping from aliases to canonical forms
        let full_kind_map: std::collections::HashMap<&str, &str> = defaults_mapping
            .iter()
            .chain(optionals_mapping.iter())
            .flat_map(|(aliases, canonical)| aliases.iter().map(move |alias| (*alias, *canonical)))
            .collect();

        // Check if any entry has +/- prefix to determine mode
        let has_modifiers = kinds_str.chars().any(|c| c == '+' || c == '-')
            || kinds_str.split(',').any(|s| {
                let trimmed = s.trim();
                trimmed.starts_with('+') || trimmed.starts_with('-')
            });

        if has_modifiers {
            // Modifier mode: start with defaults and apply changes
            enabled_kinds = default_kinds;

            let entries: Vec<String> = if kinds_str.contains(',') {
                kinds_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
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
            if kinds_str.trim().is_empty() {
                enabled_kinds = default_kinds;
            } else if kinds_str.contains(',') {
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

    /// Create a configuration from a kinds string for Rust (e.g., "nsf" or "n,s,f")
    pub fn from_rust_kinds_string(kinds_str: &str) -> Self {
        let (defaults, optionals) = Self::rust_config_data();
        Self::from_string(kinds_str, defaults, optionals)
    }

    /// Create a configuration from a kinds string for Go (e.g., "pfc" or "p,f,c")
    pub fn from_go_kinds_string(kinds_str: &str) -> Self {
        let (defaults, optionals) = Self::go_config_data();
        Self::from_string(kinds_str, defaults, optionals)
    }

    /// Check if a tag kind is enabled
    pub fn is_kind_enabled(&self, kind: &str) -> bool {
        self.enabled_kinds.contains(kind)
    }

    fn cpp_config_data() -> (
        &'static [(&'static [&'static str], &'static str)],
        &'static [(&'static [&'static str], &'static str)],
    ) {
        const DEFAULTS: &[(&[&str], &str)] = &[
            (&["d", "macro"], "d"),
            (&["e", "enumerator"], "e"),
            (&["f", "function"], "f"),
            (&["g", "enum"], "g"),
            (&["h", "header"], "h"),
            (&["m", "member"], "m"),
            (&["s", "struct"], "s"),
            (&["t", "typedef"], "t"),
            (&["u", "union"], "u"),
            (&["v", "variable"], "v"),
            (&["c", "class"], "c"),
        ];
        const OPTIONALS: &[(&[&str], &str)] = &[
            (&["l", "local"], "l"),
            (&["p", "prototype"], "p"),
            (&["x", "externvar"], "x"),
            (&["z", "parameter"], "z"),
            (&["L", "label"], "L"),
            (&["D", "macroparam"], "D"),
            (&["n", "namespace"], "n"),
            (&["A", "alias"], "A"),
            (&["N", "name"], "N"),
            (&["U", "using"], "U"),
            (&["Z", "tparam"], "Z"),
            (&["M", "module"], "M"),
        ];
        (DEFAULTS, OPTIONALS)
    }

    /// Create a configuration from a kinds string for C++ (e.g., "defg", "+f,-m", or "d,e,f,g")
    pub fn from_cpp_kinds_string(kinds_str: &str) -> Self {
        let (defaults, optionals) = Self::cpp_config_data();
        Self::from_string(kinds_str, defaults, optionals)
    }

    fn c_config_data() -> (
        &'static [(&'static [&'static str], &'static str)],
        &'static [(&'static [&'static str], &'static str)],
    ) {
        const DEFAULTS: &[(&[&str], &str)] = &[
            (&["d", "macro"], "d"),
            (&["e", "enumerator"], "e"),
            (&["f", "function"], "f"),
            (&["g", "enum"], "g"),
            (&["h", "header"], "h"),
            (&["m", "member"], "m"),
            (&["s", "struct"], "s"),
            (&["t", "typedef"], "t"),
            (&["u", "union"], "u"),
            (&["v", "variable"], "v"),
        ];
        const OPTIONALS: &[(&[&str], &str)] = &[
            (&["l", "local"], "l"),
            (&["p", "prototype"], "p"),
            (&["x", "externvar"], "x"),
            (&["z", "parameter"], "z"),
            (&["L", "label"], "L"),
            (&["D", "macroparam"], "D"),
        ];
        (DEFAULTS, OPTIONALS)
    }

    /// Create a configuration from a kinds string for C (e.g., "defg", "+f,-m", or "d,e,f,g")
    pub fn from_c_kinds_string(kinds_str: &str) -> Self {
        let (defaults, optionals) = Self::c_config_data();
        Self::from_string(kinds_str, defaults, optionals)
    }

    fn javascript_config_data() -> (
        &'static [(&'static [&'static str], &'static str)],
        &'static [(&'static [&'static str], &'static str)],
    ) {
        const DEFAULTS: &[(&[&str], &str)] = &[
            (&["f", "function"], "f"),
            (&["c", "classes"], "c"),
            (&["m", "methods"], "m"),
            (&["p", "properties"], "p"),
            (&["C", "constants"], "C"),
            (&["v", "global variables"], "v"),
            (&["g", "generators"], "g"),
            (&["G", "getters"], "G"),
            (&["S", "setters"], "S"),
            (&["M", "fields"], "M"),
        ];
        const OPTIONALS: &[(&[&str], &str)] = &[];
        (DEFAULTS, OPTIONALS)
    }

    /// Create a configuration from a kinds string for JavaScript
    pub fn from_javascript_kinds_string(kinds_str: &str) -> Self {
        let (defaults, optionals) = Self::javascript_config_data();
        Self::from_string(kinds_str, defaults, optionals)
    }

    fn python_config_data() -> (
        &'static [(&'static [&'static str], &'static str)],
        &'static [(&'static [&'static str], &'static str)],
    ) {
        const DEFAULTS: &[(&[&str], &str)] = &[
            (&["c", "classes"], "c"),
            (&["f", "function"], "f"),
            (&["m", "class members"], "m"),
            (&["v", "variables"], "v"),
            (&["I", "name referring a module defined in other file"], "I"),
            (&["i", "module"], "i"),
            (
                &[
                    "Y",
                    "name referring to a class/variable/function/module defined in other module",
                ],
                "Y",
            ),
        ];
        const OPTIONALS: &[(&[&str], &str)] = &[
            (&["z", "function parameters"], "z"),
            (&["l", "local variables"], "l"),
        ];
        (DEFAULTS, OPTIONALS)
    }

    /// Create a configuration from a kinds string for JavaScript
    pub fn from_python_kinds_string(kinds_str: &str) -> Self {
        let (defaults, optionals) = Self::python_config_data();
        Self::from_string(kinds_str, defaults, optionals)
    }

    fn typescript_config_data() -> (
        &'static [(&'static [&'static str], &'static str)],
        &'static [(&'static [&'static str], &'static str)],
    ) {
        const DEFAULTS: &[(&[&str], &str)] = &[
            (&["f", "function"], "f"),
            (&["c", "class"], "c"),
            (&["i", "interface"], "f"),
            (&["g", "enum"], "g"),
            (&["e", "enumarator"], "e"),
            (&["m", "method"], "m"),
            (&["n", "namespace"], "n"),
            (&["p", "property"], "p"),
            (&["v", "global variables"], "v"),
            (&["C", "constants"], "p"),
            (&["G", "generators"], "g"),
            (&["a", "alias"], "a"),
        ];
        const OPTIONALS: &[(&[&str], &str)] = &[
            (&["z", "function parameter"], "z"),
            (&["l", "local variable"], "l"),
        ];
        (DEFAULTS, OPTIONALS)
    }

    /// Create a configuration from a kinds string for TypeScript
    pub fn from_typescript_kinds_string(kinds_str: &str) -> Self {
        let (defaults, optionals) = Self::typescript_config_data();
        Self::from_string(kinds_str, defaults, optionals)
    }
}
