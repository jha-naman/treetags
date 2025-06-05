//! Configuration for extra tag information.
//!
//! This module handles parsing and managing extra tag information
//! such as qualified tags and file scope settings.

/// Configuration for extra tag information
#[derive(Debug, Clone)]
pub struct ExtrasConfig {
    /// Enable qualified tags (include scope information)
    pub qualified: bool,
    /// Enable file scope tags
    pub file_scope: bool,
}

impl ExtrasConfig {
    pub fn new() -> Self {
        Self {
            qualified: false,
            file_scope: false,
        }
    }

    pub fn from_string(extras_str: &str) -> Self {
        let mut config = Self::new();

        for part in extras_str.split(',') {
            let part = part.trim();
            if part.starts_with('+') {
                match &part[1..] {
                    "q" | "qualified" => config.qualified = true,
                    "f" | "fileScope" => config.file_scope = true,
                    _ => eprintln!("Warning: Unknown extra: {}", part),
                }
            } else if part.starts_with('-') {
                match &part[1..] {
                    "q" | "qualified" => config.qualified = false,
                    "f" | "fileScope" => config.file_scope = false,
                    _ => eprintln!("Warning: Unknown extra: {}", part),
                }
            }
        }

        config
    }
}

impl Default for ExtrasConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_extras() {
        let config = ExtrasConfig::new();

        // Check default values
        assert!(!config.qualified);
        assert!(!config.file_scope);
    }

    #[test]
    fn test_empty_string() {
        let config = ExtrasConfig::from_string("");

        // Should have default values
        assert!(!config.qualified);
        assert!(!config.file_scope);
    }

    #[test]
    fn test_plus_qualified_short() {
        let config = ExtrasConfig::from_string("+q");

        assert!(config.qualified);
        assert!(!config.file_scope);
    }

    #[test]
    fn test_plus_qualified_long() {
        let config = ExtrasConfig::from_string("+qualified");

        assert!(config.qualified);
        assert!(!config.file_scope);
    }

    #[test]
    fn test_plus_file_scope_short() {
        let config = ExtrasConfig::from_string("+f");

        assert!(!config.qualified);
        assert!(config.file_scope);
    }

    #[test]
    fn test_plus_file_scope_long() {
        let config = ExtrasConfig::from_string("+fileScope");

        assert!(!config.qualified);
        assert!(config.file_scope);
    }

    #[test]
    fn test_multiple_plus_options() {
        let config = ExtrasConfig::from_string("+q,+f");

        assert!(config.qualified);
        assert!(config.file_scope);
    }

    #[test]
    fn test_multiple_plus_options_long() {
        let config = ExtrasConfig::from_string("+qualified,+fileScope");

        assert!(config.qualified);
        assert!(config.file_scope);
    }

    #[test]
    fn test_mixed_short_long() {
        let config = ExtrasConfig::from_string("+q,+fileScope");

        assert!(config.qualified);
        assert!(config.file_scope);
    }

    #[test]
    fn test_minus_qualified_short() {
        let config = ExtrasConfig::from_string("-q");

        assert!(!config.qualified);
        assert!(!config.file_scope);
    }

    #[test]
    fn test_minus_qualified_long() {
        let config = ExtrasConfig::from_string("-qualified");

        assert!(!config.qualified);
        assert!(!config.file_scope);
    }

    #[test]
    fn test_minus_file_scope_short() {
        let config = ExtrasConfig::from_string("-f");

        assert!(!config.qualified);
        assert!(!config.file_scope);
    }

    #[test]
    fn test_minus_file_scope_long() {
        let config = ExtrasConfig::from_string("-fileScope");

        assert!(!config.qualified);
        assert!(!config.file_scope);
    }

    #[test]
    fn test_plus_then_minus() {
        let config = ExtrasConfig::from_string("+q,+f,-q");

        assert!(!config.qualified); // Should be disabled by -q
        assert!(config.file_scope); // Should remain enabled
    }

    #[test]
    fn test_minus_then_plus() {
        let config = ExtrasConfig::from_string("-q,-f,+q");

        assert!(config.qualified); // Should be enabled by +q
        assert!(!config.file_scope); // Should remain disabled
    }

    #[test]
    fn test_whitespace_handling() {
        let config = ExtrasConfig::from_string(" +q , +f ");

        assert!(config.qualified);
        assert!(config.file_scope);
    }

    #[test]
    fn test_unknown_options_ignored() {
        let config = ExtrasConfig::from_string("+q,+unknown,+f");

        assert!(config.qualified);
        assert!(config.file_scope);
        // Unknown option should be ignored (warning printed to stderr)
    }

    #[test]
    fn test_complex_combination() {
        let config = ExtrasConfig::from_string("+qualified,-fileScope,+f,-q,+qualified");

        assert!(config.qualified); // Last +qualified should win
        assert!(config.file_scope); // +f should win over -fileScope
    }

    #[test]
    fn test_default_trait() {
        let config = ExtrasConfig::default();

        assert!(!config.qualified);
        assert!(!config.file_scope);
    }
}
