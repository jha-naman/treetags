//! # Parser Module
//!
//! This module implements the core parsing functionality for generating Vi compatible tags
//! across multiple programming languages using tree-sitter.
//!
//! The `Parser` struct maintains configuration for each supported language and provides
//! methods to parse files and generate tags from source code.

use crate::queries;
use crate::tag;
use crate::tags_config::get_tags_config;
use std::fs;
use tree_sitter::Parser as TSParser;
use tree_sitter_tags::TagsConfiguration;
use tree_sitter_tags::TagsContext;

mod common;
mod cpp;
mod go;
mod helper;
mod rust;

/// Parser manages the parsing configurations for all supported languages
/// and provides methods to generate tags from source files.
pub struct Parser {
    /// Configuration for JavaScript language
    pub js_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for Ruby language
    pub ruby_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for Python language
    pub python_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for C language
    pub c_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for C++ language
    pub cpp_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for Java language
    pub java_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for OCaml language
    pub ocaml_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for PHP language
    pub php_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for TypeScript language
    pub typescript_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for Elixir language
    pub elixir_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for Lua language
    pub lua_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for C# language
    pub csharp_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for Bash language,
    pub bash_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for Scala language
    pub scala_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for Julia language
    pub julia_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Context for generating tags
    pub tags_context: TagsContext,
    /// Parser for generating tags using tree walking
    pub ts_parser: TSParser,
}

impl Default for Parser {
    /// Creates a new Parser with default configurations
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// Creates a new Parser instance with configurations for all supported languages
    pub fn new() -> Self {
        Self {
            js_config: get_tags_config(
                tree_sitter_javascript::LANGUAGE.into(),
                tree_sitter_javascript::TAGS_QUERY,
                "javascript",
            ),
            ruby_config: get_tags_config(
                tree_sitter_ruby::LANGUAGE.into(),
                tree_sitter_ruby::TAGS_QUERY,
                "ruby",
            ),
            python_config: get_tags_config(
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::TAGS_QUERY,
                "python",
            ),
            c_config: get_tags_config(
                tree_sitter_c::LANGUAGE.into(),
                tree_sitter_c::TAGS_QUERY,
                "c",
            ),
            cpp_config: get_tags_config(
                tree_sitter_cpp::LANGUAGE.into(),
                tree_sitter_cpp::TAGS_QUERY,
                "c++",
            ),
            java_config: get_tags_config(
                tree_sitter_java::LANGUAGE.into(),
                tree_sitter_java::TAGS_QUERY,
                "java",
            ),
            ocaml_config: get_tags_config(
                tree_sitter_ocaml::LANGUAGE_OCAML.into(),
                tree_sitter_ocaml::TAGS_QUERY,
                "ocaml",
            ),
            php_config: get_tags_config(
                tree_sitter_php::LANGUAGE_PHP.into(),
                tree_sitter_php::TAGS_QUERY,
                "php",
            ),
            typescript_config: get_tags_config(
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                tree_sitter_typescript::TAGS_QUERY,
                "typescript",
            ),
            elixir_config: get_tags_config(
                tree_sitter_elixir::LANGUAGE.into(),
                tree_sitter_elixir::TAGS_QUERY,
                "elixir",
            ),
            lua_config: get_tags_config(
                tree_sitter_lua::LANGUAGE.into(),
                tree_sitter_lua::TAGS_QUERY,
                "lua",
            ),
            csharp_config: get_tags_config(
                tree_sitter_c_sharp::LANGUAGE.into(),
                queries::C_SHARP_TAGS_QUERY,
                "c#",
            ),
            bash_config: get_tags_config(
                tree_sitter_bash::LANGUAGE.into(),
                queries::BASH_TAGS_QUERY,
                "bash",
            ),
            scala_config: get_tags_config(
                tree_sitter_scala::LANGUAGE.into(),
                queries::SCALA_TAGS_QUERY,
                "scala",
            ),
            julia_config: get_tags_config(
                tree_sitter_julia::LANGUAGE.into(),
                queries::JULIA_TAGS_QUERY,
                "julia",
            ),
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
            // Fallback to tags generated by TAGS quries
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
        let config = match extension {
            "js" | "jsx" => self.js_config.as_ref().ok(),
            "rb" => self.ruby_config.as_ref().ok(),
            "py" | "pyw" => self.python_config.as_ref().ok(),
            "c" | "h" | "i" => self.c_config.as_ref().ok(),
            "cc" | "cpp" | "CPP" | "cxx" | "c++" | "cp" | "C" | "cppm" | "ixx" | "ii" | "H"
            | "hh" | "hpp" | "HPP" | "hxx" | "h++" | "tcc" => self.cpp_config.as_ref().ok(),
            "java" => self.java_config.as_ref().ok(),
            "ml" => self.ocaml_config.as_ref().ok(),
            "php" => self.php_config.as_ref().ok(),
            "ts" | "tsx" => self.typescript_config.as_ref().ok(),
            "ex" => self.elixir_config.as_ref().ok(),
            "lua" => self.lua_config.as_ref().ok(),
            "cs" => self.csharp_config.as_ref().ok(),
            "sh" | "bash" => self.bash_config.as_ref().ok(),
            "scala" => self.scala_config.as_ref().ok(),
            "jl" => self.julia_config.as_ref().ok(),
            _ => None,
        };

        let mut tags: Vec<tag::Tag> = Vec::new();
        if config.is_none() {
            return tags;
        }

        let tags_config = config.unwrap();

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
