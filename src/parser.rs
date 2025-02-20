use crate::{c, cpp, elixir, go, java, js, lua, ocaml, php, python, ruby, rust, tag, typescript};
use std::fs;
use std::sync::{Arc, Mutex};
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

impl Parser {
    pub fn new() -> Self {
        Self {
            rust_config: rust::get_tags_config(),
            go_config: go::get_tags_config(),
            js_config: js::get_tags_config(),
            ruby_config: ruby::get_tags_config(),
            python_config: python::get_tags_config(),
            c_config: c::get_tags_config(),
            cpp_config: cpp::get_tags_config(),
            java_config: java::get_tags_config(),
            ocaml_config: ocaml::get_tags_config(),
            php_config: php::get_tags_config(),
            typescript_config: typescript::get_tags_config(),
            elixir_config: elixir::get_tags_config(),
            lua_config: lua::get_tags_config(),
            tags_context: TagsContext::new(),
        }
    }

    pub fn parse_file(
        &mut self,
        tags_lock: &mut Arc<Mutex<Vec<tag::Tag>>>,
        file_path_relative_to_tag_file: &str,
        file_path: &str,
        extension: &str,
    ) {
        let code = fs::read(file_path).expect("expected to read file");
        self.parse(&code, file_path_relative_to_tag_file, extension, tags_lock);
    }

    pub fn parse(
        &mut self,
        code: &Vec<u8>,
        file_path_relative_to_tag_file: &str,
        extension: &str,
        tags_lock: &mut Arc<Mutex<Vec<tag::Tag>>>,
    ) {
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

        if let None = config {
            return;
        }

        let tags_config = config.unwrap();
        let mut tags: Vec<tag::Tag> = Vec::new();

        let result = self.tags_context.generate_tags(&tags_config, &code, None);

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

                            tags.push(tag::Tag::new(tag, &code, file_path_relative_to_tag_file));
                        }
                    }
                }

                let mut tags_guard = tags_lock.lock().unwrap();
                tags_guard.append(&mut tags);
            }
        }
    }
}
