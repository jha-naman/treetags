use crate::tag;
use crate::tags_config::get_tags_config;
use std::fs;
use tree_sitter_tags::TagsConfiguration;
use tree_sitter_tags::TagsContext;

pub struct Parser {
    pub rust_config: TagsConfiguration,
    pub go_config: TagsConfiguration,
    pub js_config: TagsConfiguration,
    pub ruby_config: TagsConfiguration,
    pub python_config: TagsConfiguration,
    pub c_config: TagsConfiguration,
    pub cpp_config: TagsConfiguration,
    pub java_config: TagsConfiguration,
    pub ocaml_config: TagsConfiguration,
    pub php_config: TagsConfiguration,
    pub typescript_config: TagsConfiguration,
    pub elixir_config: TagsConfiguration,
    pub lua_config: TagsConfiguration,
    pub tags_context: TagsContext,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
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
            tags_context: TagsContext::new(),
        }
    }

    pub fn parse_file(
        &mut self,
        file_path_relative_to_tag_file: &str,
        file_path: &str,
        extension: &str,
    ) -> Vec<tag::Tag> {
        let code = fs::read(file_path).expect("expected to read file");
        self.parse(&code, file_path_relative_to_tag_file, extension)
    }

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
