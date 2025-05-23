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
use tree_sitter_tags::TagsConfiguration;
use tree_sitter_tags::TagsContext;

/// Parser manages the parsing configurations for all supported languages
/// and provides methods to generate tags from source files.
pub struct Parser {
    /// Configuration for Rust language
    pub rust_config: TagsConfiguration,
    /// Configuration for Go language
    pub go_config: TagsConfiguration,
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
    /// Context for generating tags
    pub tags_context: TagsContext,
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
            rust_config: get_tags_config(
                tree_sitter_rust::LANGUAGE.into(),
                tree_sitter_rust::TAGS_QUERY,
            ),
            go_config: get_tags_config(tree_sitter_go::LANGUAGE.into(), tree_sitter_go::TAGS_QUERY),
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
            tags_context: TagsContext::new(),
        }
    }

    /// Parses a file from the filesystem and generates tags
    ///
    /// # Arguments
    ///
    /// * `file_path_relative_to_tag_file` - Path to the file relative to the tags file
    /// * `file_path` - Absolute path to the file
    /// * `extension` - File extension used to determine the language
    ///
    /// # Returns
    ///
    /// A vector of `Tag` objects generated from the file
    pub fn parse_file(
        &mut self,
        file_path_relative_to_tag_file: &str,
        file_path: &str,
        extension: &str,
    ) -> Vec<tag::Tag> {
        let code = fs::read(file_path).expect("expected to read file");
        self.parse(&code, file_path_relative_to_tag_file, extension)
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
    pub fn parse(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        extension: &str,
    ) -> Vec<tag::Tag> {
        let config = match extension {
            "rs" => Some(&self.rust_config),
            "go" => Some(&self.go_config),
            "js" => Some(&self.js_config),
            "jsx" => Some(&self.js_config),
            "rb" => Some(&self.ruby_config),
            "py" => Some(&self.python_config),
            "c" => Some(&self.c_config),
            "cpp" => Some(&self.cpp_config),
            "cc" => Some(&self.cpp_config),
            "cxx" => Some(&self.cpp_config),
            "java" => Some(&self.java_config),
            "ml" => Some(&self.ocaml_config),
            "php" => Some(&self.php_config),
            "ts" => Some(&self.typescript_config),
            "tsx" => Some(&self.typescript_config),
            "ex" => Some(&self.elixir_config),
            "lua" => Some(&self.lua_config),
            "cs" => Some(&self.csharp_config),
            "sh" => Some(&self.bash_config),
            "bash" => Some(&self.bash_config),
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
