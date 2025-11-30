//! # Parser Module
//!
//! This module implements the core parsing functionality for generating Vi compatible tags
//! across multiple programming languages using tree-sitter.
//!
//! The `Parser` struct maintains configuration for each supported language and provides
//! methods to parse files and generate tags from source code.

use crate::config::user_languages::GrammarConfig;
use crate::queries;
use crate::tag;
use crate::tags_config::get_tags_config;
use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::fs;
use tree_sitter::{Language, Parser as TSParser};
use tree_sitter_tags::TagsConfiguration;
use tree_sitter_tags::TagsContext;

mod common;
mod cpp;
mod go;
mod helper;
mod rust;

/// Holds dynamically loaded grammar libraries
pub struct DynamicGrammar {
    _library: Library, // Keep library alive
    config: TagsConfiguration,
}

/// Parser manages the parsing configurations for all supported languages
/// and provides methods to generate tags from source files.
pub struct Parser {
    /// Configuration for JavaScript language
    pub js_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for Ruby language
    pub ruby_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
    /// Configuration for Python language
    pub python_config: Result<TagsConfiguration, tree_sitter_tags::Error>,
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
    /// Dynamically loaded user grammars
    pub user_grammars: HashMap<String, DynamicGrammar>,
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
            user_grammars: HashMap::new(),
        }
    }

    /// Load a user-defined grammar
    fn load_user_grammar(
        &mut self,
        grammar_name: &str,
        grammar_config: &GrammarConfig,
    ) -> Result<(), String> {
        // Load the dynamic library
        let library = unsafe {
            Library::new(&grammar_config.library_path).map_err(|e| {
                format!(
                    "Failed to load grammar library {}: {}",
                    grammar_config.library_path.display(),
                    e
                )
            })?
        };

        // Get the language function (typically named "tree_sitter_<language>")
        let language_fn: Symbol<unsafe extern "C" fn() -> Language> = unsafe {
            library
                .get(format!("tree_sitter_{}", grammar_name.replace('-', "_")).as_bytes())
                .map_err(|e| {
                    format!(
                        "Failed to find language function tree_sitter_{} in {}: {}",
                        grammar_name.replace('-', "_"),
                        grammar_config.library_path.display(),
                        e
                    )
                })?
        };

        let language = unsafe { language_fn() };

        // Load the tags query
        let tags_query = fs::read_to_string(&grammar_config.query_file).map_err(|e| {
            format!(
                "Failed to read tags query file {}: {}",
                grammar_config.query_file.display(),
                e
            )
        })?;

        // Create tags configuration
        let config = TagsConfiguration::new(language, &tags_query, "").map_err(|e| {
            format!(
                "Failed to create tags configuration for {}: {}",
                grammar_name, e
            )
        })?;

        let dynamic_grammar = DynamicGrammar {
            _library: library,
            config,
        };

        self.user_grammars
            .insert(grammar_name.to_string(), dynamic_grammar);

        Ok(())
    }

    /// Generate tags by first trying user grammars, then built-in configurations
    pub fn generate_by_tag_query_with_user_support(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        extension: &str,
        config: Option<&crate::config::Config>,
    ) -> Vec<tag::Tag> {
        // First try user-defined grammars if config is provided
        let mut source_name = String::new();
        let tags_config: Option<&TagsConfiguration> = if let Some(config) = config {
            if let Some((grammar_name, grammar_config)) =
                config.user_languages.get_grammar_for_extension(extension)
            {
                // Load grammar if not already loaded
                if !self.user_grammars.contains_key(grammar_name) {
                    if let Err(e) = self.load_user_grammar(grammar_name, grammar_config) {
                        eprintln!("Warning: {}", e);
                        // Fall through to built-in languages
                    }
                }

                // Try to use user grammar if loaded
                if let Some(grammar) = self.user_grammars.get(grammar_name) {
                    source_name = format!("user grammar '{}'", grammar_name);
                    Some(&grammar.config)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
        .or_else(|| {
            // Fall back to built-in language configurations
            let (config, name) = match extension {
                "js" | "jsx" => (self.js_config.as_ref().ok(), "built-in 'js' grammar"),
                "rb" => (self.ruby_config.as_ref().ok(), "built-in 'rb' grammar"),
                "py" | "pyw" => (self.python_config.as_ref().ok(), "built-in 'py' grammar"),
                "java" => (self.java_config.as_ref().ok(), "built-in 'java' grammar"),
                "ml" => (self.ocaml_config.as_ref().ok(), "built-in 'ocaml' grammar"),
                "php" => (self.php_config.as_ref().ok(), "built-in 'php' grammar"),
                "ts" | "tsx" => (
                    self.typescript_config.as_ref().ok(),
                    "built-in 'ts' grammar",
                ),
                "ex" => (
                    self.elixir_config.as_ref().ok(),
                    "built-in 'elixir' grammar",
                ),
                "lua" => (self.lua_config.as_ref().ok(), "built-in 'lua' grammar"),
                "cs" => (self.csharp_config.as_ref().ok(), "built-in 'cs' grammar"),
                "sh" | "bash" => (self.bash_config.as_ref().ok(), "built-in 'bash' grammar"),
                "scala" => (self.scala_config.as_ref().ok(), "built-in 'scala' grammar"),
                "jl" => (self.julia_config.as_ref().ok(), "built-in 'julia' grammar"),
                _ => (None, ""),
            };
            if let Some(cfg) = config {
                source_name = name.to_string();
                Some(cfg)
            } else {
                None
            }
        });

        // Generate tags inline instead of calling separate method
        if let Some(tags_config) = tags_config {
            let result = self.tags_context.generate_tags(tags_config, code, None);

            match result {
                Ok((raw_tags, _)) => {
                    let mut tags = Vec::new();
                    for tag_result in raw_tags {
                        match tag_result {
                            Ok(tag) if tag.is_definition => {
                                match tag::Tag::from_ts_tag(
                                    tag,
                                    code,
                                    file_path_relative_to_tag_file,
                                ) {
                                    Ok(new_tag) => tags.push(new_tag),
                                    Err(e) => eprintln!("Error creating tag: {}", e),
                                }
                            }
                            Ok(_) => {} // Skip non-definition tags
                            Err(e) => eprintln!("Error in tag generation: {}", e),
                        }
                    }
                    tags
                }
                Err(e) => {
                    eprintln!("Error generating tags with {}: {}", source_name, e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
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
            Ok(self.generate_by_tag_query_with_user_support(
                &code,
                file_path_relative_to_tag_file,
                extension,
                Some(config),
            ))
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
        self.generate_by_tag_query_with_user_support(
            code,
            file_path_relative_to_tag_file,
            extension,
            None,
        )
    }
}
