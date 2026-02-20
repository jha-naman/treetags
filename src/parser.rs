//! # Parser Module
//!
//! This module implements the core parsing functionality for generating Vi compatible tags
//! across multiple programming languages using tree-sitter.
//!
//! The `Parser` struct maintains configuration for each supported language and provides
//! methods to parse files and generate tags from source code.

use crate::built_in_grammars;
use crate::config::Config;
use crate::tag;
use crate::user_grammars;
use libloading::Library;
use std::collections::HashMap;
use std::fs;
use tree_sitter::Parser as TSParser;
use tree_sitter_tags::{TagsConfiguration, TagsContext};

mod common;
mod cpp;
mod go;
mod helper;
mod js;
mod rust;

/// Parser manages the parsing configurations for all supported languages
/// and provides methods to generate tags from source files.
pub struct Parser {
    /// Storage for all grammar configurations (built-in and user-provided)
    pub grammar_configs: Vec<Result<TagsConfiguration, tree_sitter_tags::Error>>,

    /// Map of file extension to index in grammar_configs
    pub extension_config_map: HashMap<String, usize>,

    // Keep the user provided grammars alive
    _user_grammars: Vec<Library>,

    /// Context for generating tags
    pub tags_context: TagsContext,
    /// Parser for generating tags using tree walking
    pub ts_parser: TSParser,
}

impl Default for Parser {
    /// Creates a new Parser with default configurations
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl Parser {
    /// Creates a new Parser instance with configurations for all supported languages
    pub fn new(config: &Config) -> Self {
        let mut grammar_configs = Vec::new();
        let mut extension_config_map = HashMap::new();

        // Helper to add a config and map extensions to it
        let mut add_config = |config_res: Result<TagsConfiguration, tree_sitter_tags::Error>,
                              extensions: &[&str]| {
            let index = grammar_configs.len();
            grammar_configs.push(config_res);
            for ext in extensions {
                extension_config_map.insert(ext.to_string(), index);
            }
        };

        // 1. Load Built-in Grammars
        for (extensions, config_res) in built_in_grammars::load() {
            add_config(config_res, &extensions);
        }

        // 2. Load User Grammars
        let user_grammars = user_grammars::load(config);
        for (extensions, config_res) in user_grammars.tag_configurations {
            let index = grammar_configs.len();
            grammar_configs.push(config_res);

            // Only map extensions if the config loaded successfully, but push result anyway to keep indices valid
            if grammar_configs.last().unwrap().is_ok() {
                for extension in extensions {
                    extension_config_map.insert(extension, index);
                }
            }
        }

        Self {
            grammar_configs,
            extension_config_map,
            _user_grammars: user_grammars._grammars,

            tags_context: TagsContext::new(),
            ts_parser: TSParser::new(),
        }
    }

    /// Generates tags by walking the parsed tree with configuration
    pub fn generate_by_walking_with_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        extension: &str,
        config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        match extension {
            "rs" => self.generate_rust_tags_with_user_config(
                code,
                file_path_relative_to_tag_file,
                config,
            ),
            "go" => {
                self.generate_go_tags_with_user_config(code, file_path_relative_to_tag_file, config)
            }
            "c" | "h" | "i" => {
                self.generate_c_tags_with_user_config(code, file_path_relative_to_tag_file, config)
            }
            "cc" | "cpp" | "CPP" | "cxx" | "c++" | "cp" | "C" | "cppm" | "ixx" | "ii" | "H"
            | "hh" | "hpp" | "HPP" | "hxx" | "h++" | "tcc" => self
                .generate_cpp_tags_with_user_config(code, file_path_relative_to_tag_file, config),
            "js" | "jsx" | "mjs" | "cjs" => {
                self.generate_js_tags_with_user_config(code, file_path_relative_to_tag_file, config)
            }
            _ => None,
        }
    }

    /// Parses a file from the filesystem and generates tags with configuration
    ///
    /// # Arguments
    ///
    /// * `file_path_relative_to_tag_file` - Path to the file relative to the tags file
    /// * `file_path` - Absolute path to the file
    /// * `extension` - File extension used to determine the language
    /// * `config` - Configuration for tag generation
    ///
    /// # Returns
    ///
    /// A Result containing a vector of `Tag` objects generated from the file, or an error message
    pub fn parse_file_with_config(
        &mut self,
        file_path_relative_to_tag_file: &str,
        file_path: &str,
        extension: &str,
        config: &crate::config::Config,
    ) -> Result<Vec<tag::Tag>, String> {
        let code = fs::read(file_path)
            .map_err(|e| format!("Failed to read file '{}': {}", file_path, e))?;

        // Try to generate tags with extension fields support first
        if let Some(tags) = self.generate_by_walking_with_config(
            &code,
            file_path_relative_to_tag_file,
            extension,
            config,
        ) {
            Ok(tags)
        } else {
            // Fallback to tags generated by TAGS queries
            Ok(self.generate_by_tag_query(&code, file_path_relative_to_tag_file, extension))
        }
    }

    /// Generates Rust tags with user configuration
    pub fn generate_rust_tags_with_user_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        let effective_kinds = config.get_rust_kinds();
        let tag_config = if effective_kinds.is_empty() {
            helper::TagKindConfig::new_rust() // Default: all kinds enabled
        } else {
            helper::TagKindConfig::from_rust_kinds_string(effective_kinds)
        };

        // Call the new method that accepts user config
        self.generate_rust_tags_with_full_config(
            code,
            file_path_relative_to_tag_file,
            &tag_config,
            config,
        )
    }

    /// Generates Go tags with user configuration
    pub fn generate_go_tags_with_user_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        let effective_kinds = config.get_go_kinds();
        let tag_config = if effective_kinds.is_empty() {
            helper::TagKindConfig::new_go() // Default: all kinds enabled
        } else {
            helper::TagKindConfig::from_go_kinds_string(effective_kinds)
        };

        // Call the new method that accepts user config
        self.generate_go_tags_with_full_config(
            code,
            file_path_relative_to_tag_file,
            &tag_config,
            config,
        )
    }

    /// Generates JavaScript tags with user configuration
    pub fn generate_js_tags_with_user_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        // Default to all kinds enabled for JS as specific config method isn't in scope of this task
        // In a real scenario we'd add `get_js_kinds()` to config
        let tag_config = helper::TagKindConfig::new_js();

        self.generate_js_tags_with_full_config(
            code,
            file_path_relative_to_tag_file,
            &tag_config,
            config,
        )
    }

    /// Generates C++ tags with user configuration
    pub fn generate_cpp_tags_with_user_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        // Parse cpp-kinds configuration
        let tag_config = if config.cpp_kinds.is_empty() {
            helper::TagKindConfig::new_cpp()
        } else {
            helper::TagKindConfig::from_cpp_kinds_string(&config.cpp_kinds)
        };

        // Call the new method that accepts user config
        self.generate_cpp_tags_with_full_config(
            code,
            file_path_relative_to_tag_file,
            &tag_config,
            config,
        )
    }

    /// Generates C tags with user configuration
    pub fn generate_c_tags_with_user_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        // Parse c-kinds configuration
        let tag_config = if config.c_kinds.is_empty() {
            helper::TagKindConfig::new_c()
        } else {
            helper::TagKindConfig::from_c_kinds_string(&config.c_kinds)
        };

        // Call the new method that accepts user config
        self.generate_cpp_tags_with_full_config(
            code,
            file_path_relative_to_tag_file,
            &tag_config,
            config,
        )
    }

    /// Parses source code and generates tags
    ///
    /// # Arguments
    ///
    /// * `code` - Source code bytes
    /// * `file_path_relative_to_tag_file` - Path to the file relative to the tags file
    /// * `extension` - File extension used to determine the language
    ///
    /// # Returns
    ///
    /// A vector of `Tag` objects generated from the provided code
    pub fn generate_by_tag_query(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        extension: &str,
    ) -> Vec<tag::Tag> {
        let config = self.extension_config_map.get(extension).and_then(|&i| {
            self.grammar_configs
                .get(i)
                .and_then(|result| result.as_ref().ok())
        });

        let mut tags: Vec<tag::Tag> = Vec::new();

        let tags_config = if let Some(config) = config {
            config
        } else {
            return tags;
        };

        let result = self.tags_context.generate_tags(tags_config, code, None);

        match result {
            Err(err) => eprintln!("Error generating tags for file: {}", err),
            Ok(valid_result) => {
                let (raw_tags, _) = valid_result;
                for tag in raw_tags {
                    match tag {
                        Err(error) => eprintln!("Error generating tags for file: {}", error),
                        Ok(tag) => {
                            if !tag.is_definition {
                                continue;
                            }

                            match tag::Tag::from_ts_tag(tag, code, file_path_relative_to_tag_file) {
                                Ok(new_tag) => tags.push(new_tag),
                                Err(error_msg) => {
                                    eprintln!("{}", error_msg);
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
        }

        tags
    }
}
