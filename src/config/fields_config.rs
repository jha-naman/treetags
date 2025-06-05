//! Configuration for extension fields in tag output.
//!
//! This module handles parsing and managing which extension fields
//! should be included in the generated tags output.

use std::collections::HashSet;

/// Configuration for extension fields
#[derive(Debug, Clone)]
pub struct FieldsConfig {
    /// Enabled extension fields
    pub enabled_fields: HashSet<String>,
}

impl FieldsConfig {
    pub fn new() -> Self {
        let mut enabled_fields = HashSet::new();

        // Enable fields that are enabled by default in ctags
        // Based on ctags field table: N, F, P, T, f, k, s, t are enabled by default
        enabled_fields.insert("name".to_string()); // N - tag name (always present in tags)
        enabled_fields.insert("input".to_string()); // F - input file (always present in tags)
        enabled_fields.insert("pattern".to_string()); // P - pattern (always present in tags)
        enabled_fields.insert("scope".to_string()); // s - scope of tag definition
        enabled_fields.insert("typeref".to_string()); // t - type and name of variable/typedef

        Self { enabled_fields }
    }

    pub fn from_string(fields_str: &str) -> Self {
        let mut config = Self::new(); // Start with ctags defaults

        // Handle concatenated single characters (like "nksSafet") vs comma-separated
        let parts: Vec<&str> =
            if fields_str.contains(',') || fields_str.contains('+') || fields_str.contains('-') {
                // Handle comma-separated or +/- prefixed format
                fields_str.split(',').map(|s| s.trim()).collect()
            } else {
                // Handle concatenated single characters - split each character
                fields_str
                    .chars()
                    .map(|c| {
                        match c {
                            'n' => "n",
                            'k' => "k",
                            's' => "s",
                            'S' => "S",
                            'a' => "a",
                            'f' => "f",
                            'e' => "e",
                            't' => "t",
                            'N' => "N",
                            'F' => "F",
                            'P' => "P",
                            'T' => "T",
                            'C' => "C",
                            'E' => "E",
                            'K' => "K",
                            'R' => "R",
                            'Z' => "Z",
                            'l' => "l",
                            'm' => "m",
                            'o' => "o",
                            'p' => "p",
                            'r' => "r",
                            'x' => "x",
                            'z' => "z",
                            _ => "", // Ignore unknown characters
                        }
                    })
                    .filter(|s| !s.is_empty())
                    .collect()
            };

        for part in parts {
            let part = part.trim();
            if part.starts_with('+') {
                let field = &part[1..];
                match field {
                    "n" | "line" => {
                        config.enabled_fields.insert("line".to_string());
                    }
                    "S" | "signature" => {
                        config.enabled_fields.insert("signature".to_string());
                    }
                    "s" | "scope" => {
                        config.enabled_fields.insert("scope".to_string());
                    }
                    "k" | "kind" => {
                        config.enabled_fields.insert("kind".to_string());
                    }
                    "a" | "access" => {
                        config.enabled_fields.insert("access".to_string());
                    }
                    "f" | "file" => {
                        config.enabled_fields.insert("file".to_string());
                    }
                    "e" | "end" => {
                        config.enabled_fields.insert("end".to_string());
                    }
                    "t" | "typeref" => {
                        config.enabled_fields.insert("typeref".to_string());
                    }
                    _ => eprintln!("Warning: Unknown field: {}", field),
                }
            } else if part.starts_with('-') {
                let field = &part[1..];
                match field {
                    "n" | "line" => {
                        config.enabled_fields.remove("line");
                    }
                    "S" | "signature" => {
                        config.enabled_fields.remove("signature");
                    }
                    "s" | "scope" => {
                        config.enabled_fields.remove("scope");
                    }
                    "k" | "kind" => {
                        config.enabled_fields.remove("kind");
                    }
                    "a" | "access" => {
                        config.enabled_fields.remove("access");
                    }
                    "f" | "file" => {
                        config.enabled_fields.remove("file");
                    }
                    "e" | "end" => {
                        config.enabled_fields.remove("end");
                    }
                    "t" | "typeref" => {
                        config.enabled_fields.remove("typeref");
                    }
                    _ => eprintln!("Warning: Unknown field: {}", field),
                }
            } else {
                // Handle bare field names (from concatenated format)
                match part {
                    "n" | "line" => {
                        config.enabled_fields.insert("line".to_string());
                    }
                    "S" | "signature" => {
                        config.enabled_fields.insert("signature".to_string());
                    }
                    "s" | "scope" => {
                        config.enabled_fields.insert("scope".to_string());
                    }
                    "k" | "kind" => {
                        config.enabled_fields.insert("kind".to_string());
                    }
                    "a" | "access" => {
                        config.enabled_fields.insert("access".to_string());
                    }
                    "f" | "file" => {
                        config.enabled_fields.insert("file".to_string());
                    }
                    "e" | "end" => {
                        config.enabled_fields.insert("end".to_string());
                    }
                    "t" | "typeref" => {
                        config.enabled_fields.insert("typeref".to_string());
                    }
                    // Add other field mappings as needed
                    _ => eprintln!("Warning: Unknown field: {}", part),
                }
            }
        }

        config
    }

    pub fn is_field_enabled(&self, field: &str) -> bool {
        self.enabled_fields.contains(field)
    }
}

impl Default for FieldsConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_fields() {
        let config = FieldsConfig::new();

        // Check default fields are enabled
        assert!(config.is_field_enabled("name"));
        assert!(config.is_field_enabled("input"));
        assert!(config.is_field_enabled("pattern"));
        assert!(config.is_field_enabled("scope"));
        assert!(config.is_field_enabled("typeref"));

        // Check non-default fields are not enabled
        assert!(!config.is_field_enabled("line"));
        assert!(!config.is_field_enabled("signature"));
        assert!(!config.is_field_enabled("kind"));
    }

    #[test]
    fn test_concatenated_single_characters() {
        let config = FieldsConfig::from_string("nksSafet");

        assert!(config.is_field_enabled("line")); // n
        assert!(config.is_field_enabled("kind")); // k
        assert!(config.is_field_enabled("scope")); // s
        assert!(config.is_field_enabled("signature")); // S
        assert!(config.is_field_enabled("access")); // a
        assert!(config.is_field_enabled("file")); // f
        assert!(config.is_field_enabled("end")); // e
        assert!(config.is_field_enabled("typeref")); // t

        // Default fields should still be present
        assert!(config.is_field_enabled("name"));
        assert!(config.is_field_enabled("input"));
        assert!(config.is_field_enabled("pattern"));
    }

    #[test]
    fn test_comma_separated_short_names() {
        let config = FieldsConfig::from_string("n,k,S,a");

        assert!(config.is_field_enabled("line"));
        assert!(config.is_field_enabled("kind"));
        assert!(config.is_field_enabled("signature"));
        assert!(config.is_field_enabled("access"));

        // Should not have fields not specified
        assert!(!config.is_field_enabled("file"));
        assert!(!config.is_field_enabled("end"));

        // Default fields should still be present
        assert!(config.is_field_enabled("name"));
        assert!(config.is_field_enabled("input"));
        assert!(config.is_field_enabled("pattern"));
        assert!(config.is_field_enabled("scope"));
        assert!(config.is_field_enabled("typeref"));
    }

    #[test]
    fn test_comma_separated_long_names() {
        let config = FieldsConfig::from_string("line,kind,signature,access");

        assert!(config.is_field_enabled("line"));
        assert!(config.is_field_enabled("kind"));
        assert!(config.is_field_enabled("signature"));
        assert!(config.is_field_enabled("access"));

        // Default fields should still be present
        assert!(config.is_field_enabled("name"));
        assert!(config.is_field_enabled("input"));
        assert!(config.is_field_enabled("pattern"));
        assert!(config.is_field_enabled("scope"));
        assert!(config.is_field_enabled("typeref"));
    }

    #[test]
    fn test_plus_prefix_addition() {
        let config = FieldsConfig::from_string("+n,+S,+a");

        assert!(config.is_field_enabled("line"));
        assert!(config.is_field_enabled("signature"));
        assert!(config.is_field_enabled("access"));

        // Default fields should still be present
        assert!(config.is_field_enabled("name"));
        assert!(config.is_field_enabled("input"));
        assert!(config.is_field_enabled("pattern"));
        assert!(config.is_field_enabled("scope"));
        assert!(config.is_field_enabled("typeref"));
    }

    #[test]
    fn test_minus_prefix_removal() {
        let config = FieldsConfig::from_string("-s,-t");

        // Default fields that were removed
        assert!(!config.is_field_enabled("scope"));
        assert!(!config.is_field_enabled("typeref"));

        // Other default fields should still be present
        assert!(config.is_field_enabled("name"));
        assert!(config.is_field_enabled("input"));
        assert!(config.is_field_enabled("pattern"));
    }

    #[test]
    fn test_mixed_plus_minus() {
        let config = FieldsConfig::from_string("+n,+S,-s,-t");

        // Added fields
        assert!(config.is_field_enabled("line"));
        assert!(config.is_field_enabled("signature"));

        // Removed fields
        assert!(!config.is_field_enabled("scope"));
        assert!(!config.is_field_enabled("typeref"));

        // Remaining default fields
        assert!(config.is_field_enabled("name"));
        assert!(config.is_field_enabled("input"));
        assert!(config.is_field_enabled("pattern"));
    }

    #[test]
    fn test_long_names_with_prefixes() {
        let config = FieldsConfig::from_string("+line,+signature,-scope");

        assert!(config.is_field_enabled("line"));
        assert!(config.is_field_enabled("signature"));
        assert!(!config.is_field_enabled("scope"));

        // Other defaults should remain
        assert!(config.is_field_enabled("name"));
        assert!(config.is_field_enabled("input"));
        assert!(config.is_field_enabled("pattern"));
        assert!(config.is_field_enabled("typeref"));
    }

    #[test]
    fn test_empty_string() {
        let config = FieldsConfig::from_string("");

        // Should have only default fields
        assert!(config.is_field_enabled("name"));
        assert!(config.is_field_enabled("input"));
        assert!(config.is_field_enabled("pattern"));
        assert!(config.is_field_enabled("scope"));
        assert!(config.is_field_enabled("typeref"));

        assert!(!config.is_field_enabled("line"));
        assert!(!config.is_field_enabled("signature"));
        assert!(!config.is_field_enabled("kind"));
    }

    #[test]
    fn test_unknown_fields_ignored() {
        let config = FieldsConfig::from_string("n,unknown,S");

        assert!(config.is_field_enabled("line"));
        assert!(config.is_field_enabled("signature"));
        assert!(!config.is_field_enabled("unknown"));
    }

    #[test]
    fn test_whitespace_handling() {
        let config = FieldsConfig::from_string(" n , S , a ");

        assert!(config.is_field_enabled("line"));
        assert!(config.is_field_enabled("signature"));
        assert!(config.is_field_enabled("access"));
    }

    #[test]
    fn test_all_single_character_fields() {
        let config = FieldsConfig::from_string("nksSafetNFPTCEKRZlmopxz");

        // Test all supported single character mappings
        assert!(config.is_field_enabled("line")); // n
        assert!(config.is_field_enabled("kind")); // k
        assert!(config.is_field_enabled("scope")); // s
        assert!(config.is_field_enabled("signature")); // S
        assert!(config.is_field_enabled("access")); // a
        assert!(config.is_field_enabled("file")); // f
        assert!(config.is_field_enabled("end")); // e
        assert!(config.is_field_enabled("typeref")); // t

        // Default fields should still be present
        assert!(config.is_field_enabled("name"));
        assert!(config.is_field_enabled("input"));
        assert!(config.is_field_enabled("pattern"));
    }
}
