//! Configuration handling for the tag generator application.
//!
//! This module is responsible for parsing command line arguments
//! and providing configuration options to the rest of the application.

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use std::{fs, path::Path};

use extras_config::ExtrasConfig;
use fields_config::FieldsConfig;

mod extras_config;
mod fields_config;

/// Subcommands for the application
#[derive(Subcommand, Clone, Debug)]
pub enum Commands {
    /// Generate shell completions
    Completions {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

/// Configuration options for the tag generator.
///
/// Contains all settings that affect the behavior of the application,
/// including file selection, threading, and output options.
#[derive(Parser, Clone, Debug)]
#[command(about = "Generate vi compatible tags for multiple languages", long_about = None)]
pub struct Config {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Name to be used for the tagfile, should not contain path separator
    #[arg(short = 'f', default_value = "tags")]
    pub tag_file: String,

    /// Append tags to existing tag file instead of reginerating the file from scratch.
    /// Need to pass in list of file names for which new tags are to be generated.
    #[arg(long = "append", default_value = "no", verbatim_doc_comment, default_missing_value="true", num_args=0..=1)]
    pub append_raw: String,

    /// List of file names to be processed when `--append` option is passed
    pub file_names: Vec<String>,
    #[arg(long, default_value = "4")]
    /// Number of threads to use for parsing files
    pub workers: usize,
    /// Files/directories matching the pattern will not be used while generating tags
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Recurse into directories encountered in the list of supplied files
    #[arg(short = 'R', long = "recurse", default_value = "no", default_missing_value="true", num_args=0..=1)]
    pub recurse_raw: String,

    /// Field value derived from the `recurse_raw` option field
    #[arg(skip)]
    pub recurse: bool,

    /// Read additional options from file or directory
    #[arg(long = "options", default_value = "")]
    pub options: String,

    /// Whether to sort the files or not.
    /// Values of 'yes', 'on', 'true', '1' set it to true
    /// Values of 'no', '0', 'off', 'false' set it to false
    #[arg(long = "sort", default_value = "yes", verbatim_doc_comment, default_missing_value="true", num_args=0..=1)]
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
    ///
    /// Rust language specific kinds to generate tags for
    #[arg(long = "kinds-rust", default_value = "", verbatim_doc_comment)]
    pub kinds_rust: String,

    /// Rust language specific kinds to generate tags for. Deprecated: `kinds-rust` takes
    /// precedence if it's present
    #[deprecated = "Use --kinds-rust instead"]
    #[arg(long = "rust-kinds", default_value = "", verbatim_doc_comment)]
    pub rust_kinds: String,

    /// Go language specific kinds to generate tags for
    #[arg(long = "kinds-go", default_value = "")]
    pub kinds_go: String,

    /// Go language specific kinds to generate tags for. Deprecated: `kinds-go` takes precedence if
    /// it's present
    #[deprecated = "Use --kinds-go instead"]
    #[arg(long = "go-kinds", default_value = "")]
    pub go_kinds: String,

    /// C++ language specific kinds to generate tags for
    #[arg(long = "kinds-c++", default_value = "")]
    pub cpp_kinds: String,

    /// C language specific kinds to generate tags for
    #[arg(long = "kinds-c", default_value = "")]
    pub c_kinds: String,

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
        // First parse to get the options file path
        let initial_args: Vec<String> = std::env::args().collect();
        let initial_matches = Self::command().get_matches_from(&initial_args);
        let options_path = initial_matches.get_one::<String>("options").unwrap();

        // Combine file options with command line args
        let combined_args = Self::combine_args_with_options(&initial_args, options_path);

        // Parse with combined arguments
        let matches = Self::command().get_matches_from(combined_args);
        let mut config = Self::from_arg_matches(&matches).unwrap();

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

        if let Some(parsed_recurse_val) = config.try_string_to_bool(&config.recurse_raw) {
            config.recurse = parsed_recurse_val;
        } else {
            config.recurse = true;
            filename_misinterpreted_by_raw_bool = Some(config.recurse_raw.clone());
        }

        if let Some(filename) = filename_misinterpreted_by_raw_bool {
            // This filename was consumed by --append or --sort or another option converted from
            // string to bool
            config.file_names.insert(0, filename);
        }

        config.extras_config = ExtrasConfig::from_string(&config.extras);
        config.fields_config = FieldsConfig::from_string(&config.fields);

        config.handle_special_cases(&initial_args);

        config
    }

    /// Combine command line arguments with options from file
    fn combine_args_with_options(original_args: &[String], options_path: &str) -> Vec<String> {
        if options_path.is_empty() {
            return original_args.to_vec();
        }

        let mut combined_args = vec![original_args[0].clone()]; // Keep program name

        // Add options from file first (lower precedence)
        if let Ok(file_options) = Self::read_options_from_path(options_path) {
            combined_args.extend(file_options);
        } else {
            eprintln!("Warning: Could not read options from: {}", options_path);
        }

        // Add original command line args (higher precedence)
        combined_args.extend(original_args.iter().skip(1).cloned());

        combined_args
    }

    /// Read options from file or directory
    fn read_options_from_path(options_path: &str) -> Result<Vec<String>, std::io::Error> {
        let path = Path::new(options_path);
        let content = Self::read_options_content(path)?;

        let mut options = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Simple split by whitespace - clap will handle the parsing
            options.extend(line.split_whitespace().map(String::from));
        }

        Ok(options)
    }

    /// Read content from file or directory
    fn read_options_content(path: &Path) -> Result<String, std::io::Error> {
        if path.is_file() {
            fs::read_to_string(path)
        } else if path.is_dir() {
            let mut content = String::new();
            let mut entries: Vec<_> = fs::read_dir(path)?
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext == "ctags")
                        .unwrap_or(false)
                })
                .collect();

            entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

            for entry in entries {
                if let Ok(file_content) = fs::read_to_string(entry.path()) {
                    content.push_str(&file_content);
                    content.push('\n');
                }
            }

            Ok(content)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Path is neither a file nor a directory",
            ))
        }
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

    /// Get the effective Go kinds configuration, preferring the new format
    pub fn get_go_kinds(&self) -> &str {
        if !self.kinds_go.is_empty() {
            &self.kinds_go
        } else {
            &self.go_kinds
        }
    }

    /// Get the effective Rust kinds configuration, preferring the new format
    pub fn get_rust_kinds(&self) -> &str {
        if !self.kinds_rust.is_empty() {
            &self.kinds_rust
        } else {
            &self.rust_kinds
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

    /// Handle special cases for command line argument combinations
    fn handle_special_cases(&mut self, raw_args: &[String]) {
        // Skip the program name (first argument)
        let user_args: Vec<&str> = raw_args.iter().skip(1).map(|s| s.as_str()).collect();
        
        // Case 1: No arguments at all
        if user_args.is_empty() {
            eprintln!("No files specified. Try \"treetags --help\".");
            std::process::exit(1);
        }
        
        // Case 2: Only "-R" specified
        if user_args.len() == 1 && (user_args[0] == "-R" || user_args[0] == "--recurse") {
            self.file_names = vec![".".to_string()];
            self.recurse = true;
            return;
        }
        
        // Case 3: Only "*" specified
        if user_args.len() == 1 && user_args[0] == "*" {
            self.file_names = vec![".".to_string()];
            self.recurse = true;
            return;
        }

        // Final safety check: if we still have no files, error out
        if self.file_names.is_empty() {
            eprintln!("No files specified. Try \"treetags --help\".");
            std::process::exit(1);
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
