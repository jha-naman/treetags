//! # Parser Module
//!
//! Per-thread execution engine for tag generation. Holds the mutable state
//! needed by all three language backends:
//! - `ts_parser` — used by builtin tree-walker parsers
//! - `tags_context` / `grammar_store` — used by query-based fallback parsers
//! - `shared_registry` / `local_instances` — used by WASM plugin parsers
//!
//! Language routing lives in `LanguageParser` / `LanguageParserRegistry`
//! (`language_parser.rs`); this module is a pure execution engine.

use crate::built_in_grammars::{self, QueryConfig};
use crate::config::Config;
use crate::plugin::instance::WasmInstance;
use crate::plugin::registry::PluginRegistry;
use crate::tag;
use crate::user_grammars;
use crate::wasm_grammars::{self, WasmGrammars};
use libloading::Library;
use std::cell::OnceCell;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tree_sitter::Parser as TSParser;
use tree_sitter_tags::TagsContext;

pub(crate) mod c_sharp;
pub(crate) mod common;
pub(crate) mod cpp;
pub(crate) mod go;
mod helper;
pub(crate) mod java;
pub(crate) mod js;
pub(crate) mod objective_c;
pub(crate) mod python;
pub(crate) mod rust;
pub(crate) mod swift;
pub(crate) mod typescript;
pub(crate) mod zig;

pub(crate) use helper::kinds_from_mappings;
pub use helper::{KindInfo, TagKindConfig};

/// Shared, immutable grammar data for query-based tag generation.
/// Built once at startup and shared across all worker threads via `Arc`.
/// `_libs` keeps dynamically-loaded grammar libraries alive.
pub(crate) struct GrammarStore {
    pub(crate) grammar_configs: Vec<QueryConfig>,
    pub(crate) extension_config_map: HashMap<String, usize>,
    _libs: Vec<Library>,
    pub(crate) wasm: WasmGrammars,
}

impl GrammarStore {
    /// Builds the store from already-loaded built-in grammars, so callers that
    /// also need the grammars' metadata (e.g. `LanguageParserRegistry`) don't
    /// pay to compile every `TagsConfiguration` twice.
    pub(crate) fn build(
        builtin_grammars: Vec<built_in_grammars::BuiltinGrammar>,
        config: &Config,
    ) -> Self {
        let mut grammar_configs = Vec::new();
        let mut extension_config_map = HashMap::new();

        for grammar in builtin_grammars {
            let index = grammar_configs.len();
            grammar_configs.push(grammar.config);
            for ext in grammar.extensions {
                extension_config_map.insert((*ext).to_string(), index);
            }
        }

        let user_grammars = user_grammars::load(config);
        for (extensions, config_res) in user_grammars.tag_configurations {
            let index = grammar_configs.len();
            if config_res.is_ok() {
                for extension in extensions {
                    extension_config_map.insert(extension, index);
                }
            }
            grammar_configs.push(QueryConfig::Bundled(config_res));
        }

        Self {
            grammar_configs,
            extension_config_map,
            _libs: user_grammars._grammars,
            wasm: WasmGrammars::new(config.wasm_grammars_dir.clone()),
        }
    }
}

/// Per-thread tag-generation execution engine.
pub struct Parser {
    pub(crate) grammar_store: Arc<GrammarStore>,
    pub tags_context: TagsContext,
    walker_wasm_store: OnceCell<Result<(), String>>,
    query_wasm_store: OnceCell<Result<(), String>>,
    /// Exposed `pub(crate)` so `BuiltinLanguageParser` can pass it to language
    /// free-functions without going through an extra method call.
    pub(crate) ts_parser: TSParser,
    pub(crate) shared_registry: Option<Arc<PluginRegistry>>,
    pub(crate) local_instances: HashMap<String, WasmInstance>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl Parser {
    pub fn new(config: &Config) -> Self {
        let registry = crate::language_parser::LanguageParserRegistry::new(config);
        registry.check_requested_grammars(config);
        registry.create_parser()
    }

    /// Creates a per-thread Parser sharing pre-built grammar and plugin data.
    /// Called only by `LanguageParserRegistry::create_parser`.
    pub(crate) fn with_store_and_registry(
        store: Arc<GrammarStore>,
        registry: Arc<PluginRegistry>,
    ) -> Self {
        Self {
            grammar_store: store,
            tags_context: TagsContext::new(),
            walker_wasm_store: OnceCell::new(),
            query_wasm_store: OnceCell::new(),
            ts_parser: TSParser::new(),
            shared_registry: if registry.is_empty() {
                None
            } else {
                Some(registry)
            },
            local_instances: HashMap::new(),
        }
    }

    pub(crate) fn generate_builtin(
        &mut self,
        desc: &crate::builtin_langs::BuiltinLangDesc,
        code: &[u8],
        path: &str,
        kinds: &TagKindConfig,
        config: &Config,
    ) -> Vec<tag::Tag> {
        let Some(language) = desc.grammar.language(&self.grammar_store.wasm) else {
            return vec![];
        };
        if language.is_wasm()
            && self
                .walker_wasm_store
                .get_or_init(|| {
                    let result = wasm_grammars::attach_store(&mut self.ts_parser);
                    if let Err(error) = &result {
                        eprintln!("treetags: cannot initialize walker WASM store: {error}");
                    }
                    result
                })
                .is_err()
        {
            return vec![];
        }
        (desc.generate_fn)(&mut self.ts_parser, language, code, path, kinds, config)
            .unwrap_or_default()
    }

    /// Attempt to generate tags for `extension` using a WASM plugin.
    /// Returns `None` if no plugin handles this extension or the plugin errors.
    pub(crate) fn try_plugin(
        &mut self,
        extension: &str,
        code: &[u8],
        path: &str,
        config: &Config,
        absolute_path: &std::path::Path,
    ) -> Option<Vec<tag::Tag>> {
        self.shared_registry.as_ref()?.try_generate(
            &mut self.local_instances,
            extension,
            code,
            path,
            absolute_path,
            config,
        )
    }

    /// Generate tags via tree-sitter tag queries (fallback for non-builtin languages).
    pub(crate) fn generate_by_tag_query(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        extension: &str,
    ) -> Vec<tag::Tag> {
        let config = self
            .grammar_store
            .extension_config_map
            .get(extension)
            .and_then(|&i| {
                self.grammar_store
                    .grammar_configs
                    .get(i)
                    .and_then(|result| match result {
                        QueryConfig::Bundled(result) => result.as_ref().ok(),
                        QueryConfig::Wasm(grammar) => self
                            .grammar_store
                            .wasm
                            .get(grammar)
                            .and_then(|g| g.tags.as_ref()),
                    })
            });

        let mut tags: Vec<tag::Tag> = Vec::new();

        let tags_config = if let Some(config) = config {
            config
        } else {
            return tags;
        };

        if tags_config.language.is_wasm()
            && self
                .query_wasm_store
                .get_or_init(|| {
                    let result = wasm_grammars::attach_store(&mut self.tags_context.parser);
                    if let Err(error) = &result {
                        eprintln!("treetags: cannot initialize query WASM store: {error}");
                    }
                    result
                })
                .is_err()
        {
            return tags;
        }
        let result = self.tags_context.generate_tags(tags_config, code, None);

        match result {
            Err(err) => eprintln!("Error generating tags for file: {}", err),
            Ok(valid_result) => {
                let (raw_tags, _) = valid_result;
                let file_name: std::sync::Arc<str> =
                    std::sync::Arc::from(file_path_relative_to_tag_file);
                for tag in raw_tags {
                    match tag {
                        Err(error) => eprintln!("Error generating tags for file: {}", error),
                        Ok(tag) => {
                            if !tag.is_definition {
                                continue;
                            }
                            match tag::Tag::from_ts_tag(tag, code, file_name.clone()) {
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

    /// Read file bytes and generate tags, applying the three-priority dispatch
    #[allow(dead_code)]
    /// (WASM plugin → builtin tree-walker → tag-query fallback) without
    /// requiring a `LanguageParserRegistry`.  Intended for tests and
    /// one-off external callers.
    pub fn parse_file(
        &mut self,
        file_path_relative_to_tag_file: &str,
        file_path: &str,
        extension: &str,
        config: &Config,
    ) -> Result<Vec<tag::Tag>, String> {
        let code = fs::read(file_path)
            .map_err(|e| format!("Failed to read file '{}': {}", file_path, e))?;

        // Priority 1: WASM plugin
        if let Some(tags) = self.try_plugin(
            extension,
            &code,
            file_path_relative_to_tag_file,
            config,
            std::path::Path::new(file_path),
        ) {
            return Ok(tags);
        }

        // Priority 2: builtin tree-walker (data-driven, no match needed)
        for desc in crate::builtin_langs::BUILTIN_LANG_DESCRIPTORS {
            if desc.extensions.contains(&extension) {
                let kind_config = TagKindConfig::from_string(
                    config.get_kinds(desc.lang),
                    desc.kind_defaults,
                    desc.kind_optionals,
                );
                return Ok(self.generate_builtin(
                    desc,
                    &code,
                    file_path_relative_to_tag_file,
                    &kind_config,
                    config,
                ));
            }
        }

        // Priority 3: tag-query fallback
        Ok(self.generate_by_tag_query(&code, file_path_relative_to_tag_file, extension))
    }
}

#[cfg(test)]
mod wasm_tests {
    use super::*;

    #[test]
    fn cached_store_failures_skip_wasm_but_allow_native_languages() {
        let mut config = Config::for_test();
        config.wasm_grammars_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/grammars/wasm");
        let mut parser = Parser::new(&config);
        parser
            .walker_wasm_store
            .set(Err("test failure".into()))
            .unwrap();
        parser
            .query_wasm_store
            .set(Err("test failure".into()))
            .unwrap();
        let files = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            for (extension, source, expected_name) in [
                ("zig", "pub fn zig_one() void {}", None),
                ("swift", "struct SwiftOne {}", None),
                ("ml", "let ocaml_one x = x + 1", None),
                ("rs", "pub fn rust_one() {}", Some("rust_one")),
                ("rb", "def ruby_one\nend", Some("ruby_one")),
            ] {
                let path = files.path().join(format!("source.{extension}"));
                fs::write(&path, source).unwrap();
                let tags = parser
                    .parse_file("source", path.to_str().unwrap(), extension, &config)
                    .unwrap();
                if let Some(name) = expected_name {
                    assert!(tags.iter().any(|tag| tag.name == name), "missing {name}");
                } else {
                    assert!(tags.is_empty(), "cached failure ignored for {extension}");
                }
            }
        }
    }

    #[test]
    fn direct_parser_switches_between_native_and_wasm_languages() {
        let mut config = Config::for_test();
        config.wasm_grammars_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/grammars/wasm");
        let mut parser = Parser::new(&config);
        let files = tempfile::tempdir().unwrap();
        for (extension, source, name) in [
            ("zig", "pub fn zig_one() void {}", "zig_one"),
            ("swift", "struct SwiftOne {}", "SwiftOne"),
            ("rs", "pub fn rust_one() {}", "rust_one"),
            ("ml", "let ocaml_one x = x + 1", "ocaml_one"),
            ("rb", "def ruby_one\nend", "ruby_one"),
            ("zig", "pub fn zig_two() void {}", "zig_two"),
            ("swift", "struct SwiftTwo {}", "SwiftTwo"),
            ("ml", "let ocaml_two x = x * 2", "ocaml_two"),
        ] {
            let path = files.path().join(format!("source.{extension}"));
            fs::write(&path, source).unwrap();
            let tags = parser
                .parse_file("source", path.to_str().unwrap(), extension, &config)
                .unwrap();
            assert!(
                tags.iter().any(|tag| tag.name == name),
                "missing {name}: {tags:?}"
            );
        }
    }
}
