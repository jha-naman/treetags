use std::collections::HashSet;

/// Metadata about a single tag kind for a language.
#[derive(Debug, Clone)]
pub struct KindInfo {
    pub letter: String,
    pub name: String,
    pub default: bool,
}

/// Configuration for which tag kinds to generate
#[derive(Debug, Clone)]
pub struct TagKindConfig {
    pub enabled_kinds: HashSet<String>,
}

impl TagKindConfig {
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
        let mut default_kinds = HashSet::new();
        for &(_, canonical) in defaults_mapping {
            default_kinds.insert(canonical.to_string());
        }

        let mut enabled_kinds = HashSet::new();

        let full_kind_map: std::collections::HashMap<&str, &str> = defaults_mapping
            .iter()
            .chain(optionals_mapping.iter())
            .flat_map(|(aliases, canonical)| aliases.iter().map(move |alias| (*alias, *canonical)))
            .collect();

        let has_modifiers = kinds_str.chars().any(|c| c == '+' || c == '-')
            || kinds_str.split(',').any(|s| {
                let trimmed = s.trim();
                trimmed.starts_with('+') || trimmed.starts_with('-')
            });

        if has_modifiers {
            enabled_kinds = default_kinds;

            let entries: Vec<String> = if kinds_str.contains(',') {
                kinds_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
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
                    ('+', entry.as_str())
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

    /// Check if a tag kind is enabled
    pub fn is_kind_enabled(&self, kind: &str) -> bool {
        self.enabled_kinds.contains(kind)
    }
}

/// Compute a `Vec<KindInfo>` from the static mapping slices used by builtin language descriptors.
pub fn kinds_from_mappings(
    defaults: &[(&[&str], &str)],
    optionals: &[(&[&str], &str)],
) -> Vec<KindInfo> {
    let mut kinds: Vec<KindInfo> = defaults
        .iter()
        .map(|(aliases, _)| KindInfo {
            letter: aliases[0].to_string(),
            name: if aliases.len() > 1 {
                aliases[1]
            } else {
                aliases[0]
            }
            .to_string(),
            default: true,
        })
        .collect();
    for (aliases, _) in optionals {
        kinds.push(KindInfo {
            letter: aliases[0].to_string(),
            name: if aliases.len() > 1 {
                aliases[1]
            } else {
                aliases[0]
            }
            .to_string(),
            default: false,
        });
    }
    kinds
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
            TEST_OPTIONAL_KINDS_MAPPING,
        );

        assert!(config.is_kind_enabled("f"));
        assert!(config.is_kind_enabled("s"));
        assert!(config.is_kind_enabled("m"));
        assert!(!config.is_kind_enabled("c"));
        assert!(!config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_string_override_mode_comma_separated() {
        let config = TagKindConfig::from_string(
            "f,struct,m",
            TEST_DEFAULT_KINDS_MAPPING,
            TEST_OPTIONAL_KINDS_MAPPING,
        );

        assert!(config.is_kind_enabled("f"));
        assert!(config.is_kind_enabled("s"));
        assert!(config.is_kind_enabled("m"));
        assert!(!config.is_kind_enabled("c"));
        assert!(!config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_string_modifier_mode_concatenated() {
        let config = TagKindConfig::from_string(
            "+m-c",
            TEST_DEFAULT_KINDS_MAPPING,
            TEST_OPTIONAL_KINDS_MAPPING,
        );

        assert!(config.is_kind_enabled("f"));
        assert!(config.is_kind_enabled("s"));
        assert!(!config.is_kind_enabled("c"));
        assert!(config.is_kind_enabled("m"));
        assert!(!config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_string_modifier_mode_comma_separated() {
        let config = TagKindConfig::from_string(
            "+member, -class, +variable",
            TEST_DEFAULT_KINDS_MAPPING,
            TEST_OPTIONAL_KINDS_MAPPING,
        );

        assert!(config.is_kind_enabled("f"));
        assert!(config.is_kind_enabled("s"));
        assert!(!config.is_kind_enabled("c"));
        assert!(config.is_kind_enabled("m"));
        assert!(config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_string_modifier_mode_mixed() {
        let config = TagKindConfig::from_string(
            "+m-s+v",
            TEST_DEFAULT_KINDS_MAPPING,
            TEST_OPTIONAL_KINDS_MAPPING,
        );

        assert!(config.is_kind_enabled("f"));
        assert!(!config.is_kind_enabled("s"));
        assert!(config.is_kind_enabled("c"));
        assert!(config.is_kind_enabled("m"));
        assert!(config.is_kind_enabled("v"));
    }

    #[test]
    fn test_from_string_empty_input() {
        let config =
            TagKindConfig::from_string("", TEST_DEFAULT_KINDS_MAPPING, TEST_OPTIONAL_KINDS_MAPPING);

        assert!(config.is_kind_enabled("f"));
        assert!(config.is_kind_enabled("s"));
        assert!(config.is_kind_enabled("c"));
        assert!(!config.is_kind_enabled("m"));
        assert!(!config.is_kind_enabled("v"));
    }
}
