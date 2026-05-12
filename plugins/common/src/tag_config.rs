use std::collections::{HashMap, HashSet};

/// Controls which tag kinds the plugin should emit.
///
/// Constructed from the `kinds` string passed in the `Request`. Supports:
/// - Empty string → all default kinds enabled.
/// - Override mode (no `+`/`-`) → only listed kinds enabled.
/// - Modifier mode (`+`/`-` prefix) → start from defaults, apply additions/removals.
///
/// Both single-letter (`"cf"`) and comma-separated long forms (`"class,field"`)
/// are accepted.
pub struct TagKindConfig {
    enabled_kinds: HashSet<String>,
}

impl TagKindConfig {
    /// Parse `kinds_str` using the provided kind mappings.
    ///
    /// `defaults` — `(&[aliases], canonical)` entries enabled by default.
    /// `optionals` — `(&[aliases], canonical)` entries disabled by default.
    pub fn parse(
        kinds_str: &str,
        defaults: &[(&[&str], &str)],
        optionals: &[(&[&str], &str)],
    ) -> Self {
        let mut default_kinds = HashSet::new();
        for &(_, canonical) in defaults {
            default_kinds.insert(canonical.to_string());
        }

        let full_kind_map: HashMap<&str, &str> = defaults
            .iter()
            .chain(optionals.iter())
            .flat_map(|(aliases, canonical)| aliases.iter().map(move |alias| (*alias, *canonical)))
            .collect();

        let has_modifiers = kinds_str.chars().any(|c| c == '+' || c == '-')
            || kinds_str.split(',').any(|s| {
                let t = s.trim();
                t.starts_with('+') || t.starts_with('-')
            });

        let mut enabled_kinds = HashSet::new();

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
                let (op, kind_str) = if entry.starts_with('+') {
                    ('+', &entry[1..])
                } else if entry.starts_with('-') {
                    ('-', &entry[1..])
                } else {
                    ('+', entry.as_str())
                };
                if let Some(canonical) = full_kind_map.get(kind_str) {
                    match op {
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
        } else if kinds_str.trim().is_empty() {
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
                let s = kind_char.to_string();
                if let Some(canonical) = full_kind_map.get(s.as_str()) {
                    enabled_kinds.insert((*canonical).to_string());
                } else {
                    eprintln!("Warning: Unknown tag kind: {}", kind_char);
                }
            }
        }

        Self { enabled_kinds }
    }

    pub fn is_enabled(&self, kind: &str) -> bool {
        self.enabled_kinds.contains(kind)
    }
}
