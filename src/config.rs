//! Configuration handling for the tag generator application.
//!
//! This module is responsible for parsing command line arguments
//! and providing configuration options to the rest of the application.

use clap::Parser;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

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
                // ... existing +field handling
            } else if part.starts_with('-') {
                let field = &part[1..];
                // ... existing -field handling
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

/// Configuration options for the tag generator.
///
/// Contains all settings that affect the behavior of the application,
/// including file selection, threading, and output options.
#[derive(Parser, Clone)]
#[command(about = "Generate vi compatible tags for multiple languages", long_about = None)]
pub struct Config {
    /// Name to be used for the tagfile, should not contain path separator
    #[arg(short = 'f', default_value = "tags")]
    pub tag_file: String,

    /// Append tags to existing tag file instead of reginerating the file from scratch.
    /// Need to pass in list of file names for which new tags are to be generated.
    /// Will panic if the tag file doesn't already exist in current or one of the parent
    /// directories.
    #[arg(long = "append", default_value = "no", verbatim_doc_comment)]
    pub append_raw: String,

    /// List of file names to be processed when `--append` option is passed
    pub file_names: Vec<String>,
    #[arg(long, default_value = "4")]
    /// Number of threads to use for parsing files
    pub workers: usize,
    /// Files/directories matching the pattern will not be used while generating tags
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Value passed in this arg is currently being ignored.
    /// Kept for compatibility with `vim-gutentags` plugin.
    #[arg(long = "options", default_value = "", verbatim_doc_comment)]
    pub _options: String,

    /// Whether to sort the files or not.
    /// Values of 'yes', 'on', 'true', '1' set it to true
    /// Values of 'no', '0', 'off', 'false' set it to false
    #[arg(long = "sort", default_value = "yes", verbatim_doc_comment)]
    pub sort_raw: String,
    /// Field value derived from the `sort_raw` string field
    #[arg(skip)]
    pub sort: bool,

    /// Field value derived from the `append_raw` option field
    #[arg(skip)]
    pub append: bool,

    /// Enable extra tag information (e.g., +q for qualified tags, +f for file scope)
    #[arg(long = "extras", default_value = "", verbatim_doc_comment)]
    pub extras: String,
    /// Value passed in this arg is currently being ignored.
    /// Kept for compatibility with `tagbar` plugin.
    #[arg(long = "format", default_value = "", verbatim_doc_comment)]
    pub _format: String,
    /// Value passed in this arg is currently being ignored.
    /// Kept for compatibility with `tagbar` plugin.
    #[arg(long = "excmd", default_value = "", verbatim_doc_comment)]
    pub _excmd: String,
    /// Include selected extension fields (e.g., +l for line numbers, +S for signatures)
    #[arg(long = "fields", default_value = "", verbatim_doc_comment)]
    pub fields: String,
    /// Value passed in this arg is currently being ignored.
    /// Kept for compatibility with `tagbar` plugin.
    #[arg(long = "language-force", default_value = "", verbatim_doc_comment)]
    pub _language_force: String,
    /// Comma-separated list of Rust tag kinds to generate.
    /// Available kinds: n(module), s(struct), g(enum), u(union), i(trait),
    /// f(function), c(impl), m(field), e(enum_variant), C(constant), v(variable),
    /// t(type_alias), M(macro)
    #[arg(long = "rust-kinds", default_value = "", verbatim_doc_comment)]
    pub rust_kinds: String,

    /// Parsed fields configuration  
    #[clap(skip)]
    pub fields_config: FieldsConfig,

    /// Parsed extras configuration
    #[clap(skip)]
    pub extras_config: ExtrasConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl Config {
    /// Creates a new configuration from command line arguments.
    ///
    /// Parses the command line arguments and sets up the configuration
    /// with appropriate defaults for any options not explicitly provided.
    ///
    /// # Returns
    ///
    /// A new `Config` instance with parsed arguments and defaults.
    pub fn new() -> Config {
        let mut config = Self::parse();
        config.validate();
        config.parse_file_args();

        // value_str is not a valid boolean string. Assume it's a filename.
        let mut filename_misinterpreted_by_raw_bool: Option<String> = None;

        if let Some(parsed_sort_val) = config.try_string_to_bool(&config.sort_raw) {
            config.sort = parsed_sort_val;
        } else {
            config.sort = true;
            filename_misinterpreted_by_raw_bool = Some(config.sort_raw.clone());
        }

        // Handle append option
        if let Some(parsed_append_val) = config.try_string_to_bool(&config.append_raw) {
            config.append = parsed_append_val;
        } else {
            config.append = true;
            filename_misinterpreted_by_raw_bool = Some(config.append_raw.clone());
        }

        if let Some(filename) = filename_misinterpreted_by_raw_bool {
            // This filename was consumed by --append or --sort
            config.file_names.insert(0, filename);
        }

        config.extras_config = ExtrasConfig::from_string(&config.extras);
        config.fields_config = FieldsConfig::from_string(&config.fields);

        config
    }

    fn parse_file_args(&mut self) {
        for pattern in &self.exclude.clone() {
            match pattern.strip_prefix("@") {
                None => continue,
                Some(file_name) => {
                    let file = match fs::read_to_string(file_name) {
                        Ok(contents) => contents,
                        Err(_) => {
                            eprintln!("Could not read file from exclude pattern: {}", pattern);
                            std::process::exit(1);
                        }
                    };

                    self.exclude
                        .extend(file.lines().map(|line| line.to_string()));
                }
            }
        }
    }

    fn validate(&self) {
        let tag_file = Path::new(&self.tag_file);
        let mut path_components = tag_file.components();
        let _ = path_components.next();
        if path_components.next().is_some() {
            eprintln!(
                "tagfile should only contain the tagfile name, not the path: {}",
                tag_file.display()
            );
            std::process::exit(1);
        }

        self.validate_file_args();
    }

    fn validate_file_args(&self) {
        for pattern in &self.exclude {
            match pattern.strip_prefix("@") {
                None => continue,
                Some(file_name) => match fs::exists(file_name) {
                    Ok(_) => {}
                    Err(_) => {
                        eprintln!("Could not read file from exclude pattern: {}", pattern);
                        std::process::exit(1);
                    }
                },
            }
        }
    }

    /// Converts a string value to a boolean based on predefined mappings.
    ///
    /// # Arguments
    ///
    /// * `value` - A string slice that should be converted to a boolean
    ///
    /// # Returns
    ///
    /// * `Some(true)` for values: "yes", "on", "true", "1" (case-insensitive)
    /// * `Some(false)` for values: "no", "off", "false", "0" (case-insensitive)
    /// * `None` for other values
    /// ```
    fn try_string_to_bool(&self, value: &str) -> Option<bool> {
        match value.to_lowercase().as_str() {
            "yes" | "on" | "true" | "1" => Some(true),
            "no" | "off" | "false" | "0" => Some(false),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_true_values() {
        let config = Config::new();
        assert_eq!(config.try_string_to_bool("yes"), Some(true));
        assert_eq!(config.try_string_to_bool("YES"), Some(true));
        assert_eq!(config.try_string_to_bool("on"), Some(true));
        assert_eq!(config.try_string_to_bool("ON"), Some(true));
        assert_eq!(config.try_string_to_bool("true"), Some(true));
        assert_eq!(config.try_string_to_bool("TRUE"), Some(true));
        assert_eq!(config.try_string_to_bool("1"), Some(true));
    }

    #[test]
    fn test_false_values() {
        let config = Config::new();
        assert_eq!(config.try_string_to_bool("no"), Some(false));
        assert_eq!(config.try_string_to_bool("NO"), Some(false));
        assert_eq!(config.try_string_to_bool("off"), Some(false));
        assert_eq!(config.try_string_to_bool("OFF"), Some(false));
        assert_eq!(config.try_string_to_bool("false"), Some(false));
        assert_eq!(config.try_string_to_bool("FALSE"), Some(false));
        assert_eq!(config.try_string_to_bool("0"), Some(false));
    }

    #[test]
    fn test_invalid_value() {
        let config = Config::new();
        assert_eq!(config.try_string_to_bool("invalid"), None);
        assert_eq!(config.try_string_to_bool(""), None);
    }
}
