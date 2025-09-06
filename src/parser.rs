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
    pub js_config: TagsConfiguration,
    /// Configuration for Ruby language
    pub ruby_config: TagsConfiguration,
    /// Configuration for Python language
    pub python_config: TagsConfiguration,
    /// Configuration for C language
    pub c_config: TagsConfiguration,
    /// Configuration for C++ language
    pub cpp_config: TagsConfiguration,
    /// Configuration for Java language
    pub java_config: TagsConfiguration,
    /// Configuration for OCaml language
    pub ocaml_config: TagsConfiguration,
    /// Configuration for PHP language
    pub php_config: TagsConfiguration,
    /// Configuration for TypeScript language
    pub typescript_config: TagsConfiguration,
    /// Configuration for Elixir language
    pub elixir_config: TagsConfiguration,
    /// Configuration for Lua language
    pub lua_config: TagsConfiguration,
    /// Configuration for C# language
    pub csharp_config: TagsConfiguration,
    /// Configuration for Bash language,
    pub bash_config: TagsConfiguration,
    /// Configuration for Scala language
    pub scala_config: TagsConfiguration,
    /// Configuration for Julia language
    pub julia_config: TagsConfiguration,
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
            ),
            ruby_config: get_tags_config(
                tree_sitter_ruby::LANGUAGE.into(),
                tree_sitter_ruby::TAGS_QUERY,
            ),
            python_config: get_tags_config(
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::TAGS_QUERY,
            ),
            c_config: get_tags_config(tree_sitter_c::LANGUAGE.into(), tree_sitter_c::TAGS_QUERY),
            cpp_config: get_tags_config(
                tree_sitter_cpp::LANGUAGE.into(),
                tree_sitter_cpp::TAGS_QUERY,
            ),
            java_config: get_tags_config(
                tree_sitter_java::LANGUAGE.into(),
                tree_sitter_java::TAGS_QUERY,
            ),
            ocaml_config: get_tags_config(
                tree_sitter_ocaml::LANGUAGE_OCAML.into(),
                tree_sitter_ocaml::TAGS_QUERY,
            ),
            php_config: get_tags_config(
                tree_sitter_php::LANGUAGE_PHP.into(),
                tree_sitter_php::TAGS_QUERY,
            ),
            typescript_config: get_tags_config(
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                tree_sitter_typescript::TAGS_QUERY,
            ),
            elixir_config: get_tags_config(
                tree_sitter_elixir::LANGUAGE.into(),
                tree_sitter_elixir::TAGS_QUERY,
            ),
            lua_config: get_tags_config(
                tree_sitter_lua::LANGUAGE.into(),
                tree_sitter_lua::TAGS_QUERY,
            ),
            csharp_config: get_tags_config(
                tree_sitter_c_sharp::LANGUAGE.into(),
                queries::C_SHARP_TAGS_QUERY,
            ),
            bash_config: get_tags_config(
                tree_sitter_bash::LANGUAGE.into(),
                queries::BASH_TAGS_QUERY,
            ),
            scala_config: get_tags_config(
                tree_sitter_scala::LANGUAGE.into(),
                queries::SCALA_TAGS_QUERY,
            ),
            julia_config: get_tags_config(
                tree_sitter_julia::LANGUAGE.into(),
                queries::JULIA_TAGS_QUERY,
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
    /// A vector of `Tag` objects generated from the file
    pub fn parse_file_with_config(
        &mut self,
        file_path_relative_to_tag_file: &str,
        file_path: &str,
        extension: &str,
        config: &crate::config::Config,
    ) -> Vec<tag::Tag> {
        let code = fs::read(file_path).expect("expected to read file");

        // Try to generate tags with extension fields support first
        if let Some(tags) = self.generate_by_walking_with_config(
            &code,
            file_path_relative_to_tag_file,
            extension,
            config,
        ) {
            tags
        } else {
            // Fallback to tags generated by TAGS quries
            self.generate_by_tag_query(&code, file_path_relative_to_tag_file, extension)
        }
    }

    /// Generates Rust tags with user configuration
    pub fn generate_rust_tags_with_user_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        // Parse rust-kinds configuration
        let tag_config = if config.rust_kinds.is_empty() {
            helper::TagKindConfig::new_rust() // Default: all kinds enabled
        } else {
            helper::TagKindConfig::from_rust_kinds_string(&config.rust_kinds)
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
        // Parse go-kinds configuration
        let tag_config = if config.go_kinds.is_empty() {
            helper::TagKindConfig::new_go() // Default: all kinds enabled
        } else {
            helper::TagKindConfig::from_go_kinds_string(&config.go_kinds)
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
            helper::TagKindConfig::new_cpp() // Default: all kinds enabled
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
            "js" | "jsx" => Some(&self.js_config),
            "rb" => Some(&self.ruby_config),
            "py" | "pyw" => Some(&self.python_config),
            "c" | "h" | "i" => Some(&self.c_config),
            "cc" | "cpp" | "CPP" | "cxx" | "c++" | "cp" | "C" | "cppm" | "ixx" | "ii" | "H"
            | "hh" | "hpp" | "HPP" | "hxx" | "h++" | "tcc" => Some(&self.cpp_config),
            "java" => Some(&self.java_config),
            "ml" => Some(&self.ocaml_config),
            "php" => Some(&self.php_config),
            "ts" | "tsx" => Some(&self.typescript_config),
            "ex" => Some(&self.elixir_config),
            "lua" => Some(&self.lua_config),
            "cs" => Some(&self.csharp_config),
            "sh" | "bash" => Some(&self.bash_config),
            "scala" => Some(&self.scala_config),
            "jl" => Some(&self.julia_config),
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

                            tags.push(tag::Tag::new(tag, code, file_path_relative_to_tag_file));
                        }
                    }
                }
            }
        }

        tags
    }
}
