//! Configuration handling for the tag generator application.
//!
//! This module is responsible for parsing command line arguments
//! and providing configuration options to the rest of the application.

use clap::Parser;
use std::fs;
use std::path::Path;

/// Configuration options for the tag generator.
///
/// Contains all settings that affect the behavior of the application,
/// including file selection, threading, and output options.
#[derive(Parser)]
#[command(about = "Generate vi compatible tags for multiple languages", long_about = None)]
pub struct Config {
    /// Name to be used for the tagfile, should not contain path separator
    #[arg(short = 'f', default_value = "tags")]
    pub tag_file: String,

    /// Append tags to existing tag file instead of reginerating the file from scratch.
    /// Need to pass in list of file names for which new tags are to be generated.
    /// Will panic if the tag file doesn't already exist in current or one of the parent
    /// directories.
    #[arg(long = "append", verbatim_doc_comment, default_value = "false")]
    pub append_raw: String,
    /// Field value derived from the `append_raw` string field
    #[arg(skip)]
    pub append: bool,

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

    /// Value passed in this arg is currently being ignored.
    /// Kept for compatibility with `vim-gutentags` plugin.
    #[arg(long = "extras", default_value = "", verbatim_doc_comment)]
    pub _extras: String,
    /// Value passed in this arg is currently being ignored.
    /// Kept for compatibility with `tagbar` plugin.
    #[arg(long = "format", default_value = "", verbatim_doc_comment)]
    pub _format: String,
    /// Value passed in this arg is currently being ignored.
    /// Kept for compatibility with `tagbar` plugin.
    #[arg(long = "excmd", default_value = "", verbatim_doc_comment)]
    pub _excmd: String,
    /// Value passed in this arg is currently being ignored.
    /// Kept for compatibility with `tagbar` plugin.
    #[arg(long = "fields", default_value = "", verbatim_doc_comment)]
    pub _fields: String,
    /// Value passed in this arg is currently being ignored.
    /// Kept for compatibility with `tagbar` plugin.
    #[arg(long = "language-force", default_value = "", verbatim_doc_comment)]
    pub _language_force: String,
    /// Value passed in this arg is currently being ignored.
    /// Kept for compatibility with `tagbar` plugin.
    #[arg(long = "rust-kinds", default_value = "", verbatim_doc_comment)]
    pub _rust_kinds: String,
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
        config.sort = config.string_to_bool(&config.sort_raw);
        config.append = config.string_to_bool(&config.append_raw);

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
    /// * `true` for values: "yes", "on", "true", "1" (case-insensitive)
    /// * `false` for values: "no", "off", "false", "0" (case-insensitive)
    ///
    /// # Panics
    ///
    /// Panics if the input value doesn't match any of the accepted values.
    /// ```
    fn string_to_bool(&self, value: &str) -> bool {
        match value.to_lowercase().as_str() {
            "yes" | "on" | "true" | "1" => true,
            "no" | "off" | "false" | "0" => false,
            _ => panic!("Invalid value passed: '{}'", value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_true_values() {
        let config = Config::new();
        assert_eq!(config.string_to_bool("yes"), true);
        assert_eq!(config.string_to_bool("YES"), true);
        assert_eq!(config.string_to_bool("on"), true);
        assert_eq!(config.string_to_bool("ON"), true);
        assert_eq!(config.string_to_bool("true"), true);
        assert_eq!(config.string_to_bool("TRUE"), true);
        assert_eq!(config.string_to_bool("1"), true);
    }

    #[test]
    fn test_false_values() {
        let config = Config::new();
        assert_eq!(config.string_to_bool("no"), false);
        assert_eq!(config.string_to_bool("NO"), false);
        assert_eq!(config.string_to_bool("off"), false);
        assert_eq!(config.string_to_bool("OFF"), false);
        assert_eq!(config.string_to_bool("false"), false);
        assert_eq!(config.string_to_bool("FALSE"), false);
        assert_eq!(config.string_to_bool("0"), false);
    }

    #[test]
    #[should_panic(expected = "Invalid value passed")]
    fn test_invalid_value() {
        let config = Config::new();
        config.string_to_bool("invalid");
    }

    #[test]
    #[should_panic(expected = "Invalid value passed")]
    fn test_empty_string() {
        let config = Config::new();
        config.string_to_bool("");
    }
}
