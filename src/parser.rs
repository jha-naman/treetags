//! # Parser Module
//!
//! Per-thread execution engine for tag generation. Holds the mutable state
//! needed by all three language backends:
//! - `ts_parser` — used by builtin tree-walker parsers
//! - `tags_context` / `grammar_store` — used by basic tag queries
//! - `shared_registry` / `local_instances` — used by WASM plugin parsers
//!
//! Language routing lives in `LanguageParser` / `LanguageParserRegistry`
//! (`language_parser.rs`); this module is a pure execution engine.

use crate::built_in_grammars::{self, QueryConfig};
use crate::builtin_langs::{LanguageDescriptor, OFFICIAL_LANGUAGES};
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
use std::sync::{Arc, OnceLock};
use tree_sitter::Parser as TSParser;
use tree_sitter_tags::TagsContext;

pub(crate) mod c_sharp;
pub(crate) mod common;
pub(crate) mod cpp;
pub(crate) mod dart;
pub(crate) mod go;
mod helper;
pub(crate) mod java;
pub(crate) mod js;
pub(crate) mod kotlin;
pub(crate) mod objective_c;
pub(crate) mod python;
pub(crate) mod rust;
pub(crate) mod swift;
pub(crate) mod terraform;
pub(crate) mod typescript;
pub(crate) mod zig;

pub(crate) use helper::kinds_from_mappings;
pub use helper::{KindInfo, TagKindConfig};

/// Shared grammar and query caches for both official tag styles.
/// Initialized lazily and shared across all worker threads via `Arc`.
/// `_libs` keeps dynamically-loaded grammar libraries alive.
pub(crate) struct GrammarStore {
    pub(crate) grammar_configs: Vec<QueryConfig>,
    pub(crate) extension_config_map: HashMap<String, usize>,
    _libs: Vec<Library>,
    pub(crate) wasm: WasmGrammars,
    languages: HashMap<&'static str, OnceLock<Option<tree_sitter::Language>>>,
    queries: HashMap<&'static str, OnceLock<Result<tree_sitter_tags::TagsConfiguration, String>>>,
}

impl GrammarStore {
    pub(crate) fn has_user_query(&self, extension: &str) -> bool {
        self.extension_config_map
            .get(extension)
            .is_some_and(|&index| matches!(self.grammar_configs[index], QueryConfig::User(_)))
    }

    fn language(&self, desc: &'static LanguageDescriptor) -> Option<tree_sitter::Language> {
        self.languages[desc.lang]
            .get_or_init(|| desc.grammar.language(&self.wasm))
            .clone()
    }

    fn query(
        &self,
        desc: &'static LanguageDescriptor,
    ) -> Option<&tree_sitter_tags::TagsConfiguration> {
        self.queries[desc.lang]
            .get_or_init(|| {
                let language = self
                    .language(desc)
                    .ok_or_else(|| "grammar unavailable".to_owned())?;
                let query = desc
                    .query
                    .ok_or_else(|| "query not implemented".to_owned())?;
                tree_sitter_tags::TagsConfiguration::new(language, query, "").map_err(|error| {
                    let message = format!("invalid tag query for '{}': {error}", desc.lang);
                    eprintln!("treetags: {message}");
                    message
                })
            })
            .as_ref()
            .ok()
    }

    /// Registers official query metadata without loading grammars or compiling
    /// queries. Custom native grammars retain their historical loading behavior.
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
            grammar_configs.push(QueryConfig::User(config_res));
        }

        Self {
            grammar_configs,
            extension_config_map,
            _libs: user_grammars._grammars,
            wasm: WasmGrammars::new(config.wasm_grammars_dir.clone()),
            languages: OFFICIAL_LANGUAGES
                .iter()
                .map(|d| (d.lang, OnceLock::new()))
                .collect(),
            queries: OFFICIAL_LANGUAGES
                .iter()
                .map(|d| (d.lang, OnceLock::new()))
                .collect(),
        }
    }
}

/// Per-thread tag-generation execution engine.
pub struct Parser {
    pub(crate) grammar_store: Arc<GrammarStore>,
    pub tags_context: TagsContext,
    walker_wasm_store: OnceCell<Result<(), String>>,
    query_wasm_store: OnceCell<Result<(), String>>,
    /// Exposed `pub(crate)` so `OfficialLanguageParser` can pass it to language
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

    pub(crate) fn generate_with_walker(
        &mut self,
        desc: &'static LanguageDescriptor,
        code: &[u8],
        path: &str,
        kinds: &TagKindConfig,
        config: &Config,
    ) -> Vec<tag::Tag> {
        let Some(language) = self.grammar_store.language(desc) else {
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
        (desc
            .generate_fn
            .expect("selected walker must be implemented"))(
            &mut self.ts_parser,
            language,
            code,
            path,
            kinds,
            config,
        )
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
        let store = Arc::clone(&self.grammar_store);
        let config = store.extension_config_map.get(extension).and_then(|&i| {
            store
                .grammar_configs
                .get(i)
                .and_then(|result| match result {
                    QueryConfig::User(result) => result.as_ref().ok(),
                    QueryConfig::Official(desc) => store.query(desc),
                })
        });

        let Some(tags_config) = config else {
            return vec![];
        };
        self.generate_query(tags_config, code, file_path_relative_to_tag_file)
    }

    pub(crate) fn generate_official_query(
        &mut self,
        desc: &'static LanguageDescriptor,
        code: &[u8],
        path: &str,
    ) -> Vec<tag::Tag> {
        let store = Arc::clone(&self.grammar_store);
        let Some(query) = store.query(desc) else {
            return vec![];
        };
        self.generate_query(query, code, path)
    }

    fn generate_query(
        &mut self,
        tags_config: &tree_sitter_tags::TagsConfiguration,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
    ) -> Vec<tag::Tag> {
        let mut tags = Vec::new();
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

    /// Compatibility helper using the same registry and style selection as workers.
    #[allow(dead_code)]
    pub fn parse_file(
        &mut self,
        file_path_relative_to_tag_file: &str,
        file_path: &str,
        extension: &str,
        config: &Config,
    ) -> Result<Vec<tag::Tag>, String> {
        let code = fs::read(file_path)
            .map_err(|e| format!("Failed to read file '{}': {}", file_path, e))?;

        use crate::language_parser::LanguageParserRegistry;
        let registry = LanguageParserRegistry::new(config);
        let routing_path =
            std::path::Path::new(file_path_relative_to_tag_file).with_extension(extension);
        let id = crate::tag_processor::select_language(
            &registry,
            config,
            std::path::Path::new(file_path),
            &routing_path,
        )
        .map(|selection| selection.lang);
        Ok(id
            .map(|id| {
                registry.parser(id).generate_tags(
                    self,
                    &code,
                    file_path_relative_to_tag_file,
                    config,
                    std::path::Path::new(file_path),
                )
            })
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod wasm_tests {
    use super::*;

    #[test]
    fn dual_implementation_dispatch_and_query_failure_isolation() {
        use crate::config::tag_styles::{official_language, TagStyle};
        use crate::language_parser::{LanguageParser, OfficialLanguageParser};

        let mut config = Config::for_test();
        let mut desc = official_language("rust").unwrap().clone();
        desc.query = Some("(function_item name: (identifier) @name) @definition.function");
        let desc = Box::leak(Box::new(desc));
        let mut parser = Parser::new(&config);
        let source = b"pub fn example() {}";
        let path = std::path::Path::new("source.rs");
        let basic = OfficialLanguageParser::from_desc(desc, &config).generate_tags(
            &mut parser,
            source,
            "source.rs",
            &config,
            path,
        );
        assert_eq!(basic.len(), 1);
        assert_eq!(basic[0].name, "example");
        assert!(basic[0].kind.is_none() && basic[0].extension_fields.is_none());

        config.tag_preferences.default = TagStyle::WithExtensionFields;
        let rich = OfficialLanguageParser::from_desc(desc, &config).generate_tags(
            &mut parser,
            source,
            "source.rs",
            &config,
            path,
        );
        assert!(rich
            .iter()
            .any(|tag| tag.name == "example" && tag.kind.is_some()));

        let mut broken = desc.clone();
        broken.query = Some("(");
        let broken = Box::leak(Box::new(broken));
        let mut parser = Parser::new(&config);
        assert!(parser.grammar_store.queries["rust"].get().is_none());
        let rich = OfficialLanguageParser::from_desc(broken, &config).generate_tags(
            &mut parser,
            source,
            "source.rs",
            &config,
            path,
        );
        assert!(!rich.is_empty());
        assert!(parser.grammar_store.queries["rust"].get().is_none());
        config.tag_preferences.default = TagStyle::Basic;
        for _ in 0..2 {
            let basic = OfficialLanguageParser::from_desc(broken, &config).generate_tags(
                &mut parser,
                source,
                "source.rs",
                &config,
                path,
            );
            assert!(
                basic.is_empty(),
                "query failure must not fall back to walker"
            );
        }
        assert!(parser.grammar_store.queries["rust"].get().unwrap().is_err());
        config.tag_preferences.default = TagStyle::WithExtensionFields;
        assert!(!OfficialLanguageParser::from_desc(broken, &config)
            .generate_tags(&mut parser, source, "source.rs", &config, path)
            .is_empty());
    }

    #[test]
    fn official_metadata_does_not_load_unused_grammars_or_queries() {
        let parser = Parser::new(&Config::for_test());
        assert!(parser
            .grammar_store
            .languages
            .values()
            .all(|cell| cell.get().is_none()));
        assert!(parser
            .grammar_store
            .queries
            .values()
            .all(|cell| cell.get().is_none()));
    }

    #[test]
    fn newly_added_official_query_is_not_replaced_by_custom_extension_mapping() {
        use crate::config::tag_styles::official_language;
        use crate::language_parser::{LanguageParser, OfficialLanguageParser};

        let config = Config::for_test();
        let mut desc = official_language("rust").unwrap().clone();
        desc.query = Some("(function_item name: (identifier) @name) @definition.function");
        let desc = Box::leak(Box::new(desc));
        let mut parser = Parser::new(&config);
        let store = Arc::get_mut(&mut parser.grammar_store).unwrap();
        let index = store.grammar_configs.len();
        store
            .grammar_configs
            .push(QueryConfig::User(tree_sitter_tags::TagsConfiguration::new(
                tree_sitter_rust::LANGUAGE.into(),
                "",
                "",
            )));
        store.extension_config_map.insert("rs".into(), index);
        let tags = OfficialLanguageParser::from_desc(desc, &config).generate_tags(
            &mut parser,
            b"fn official() {}",
            "source.rs",
            &config,
            std::path::Path::new("source.rs"),
        );
        assert!(tags.iter().any(|tag| tag.name == "official"));
    }

    #[test]
    fn dual_styles_share_a_wasm_grammar_with_separate_parser_stores() {
        use crate::config::tag_styles::{official_language, TagStyle};
        use crate::language_parser::{LanguageParser, OfficialLanguageParser};

        let mut config = Config::for_test();
        config.wasm_grammars_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/grammars/wasm");
        let mut desc = official_language("zig").unwrap().clone();
        // A framework fixture, not the production query to be added in Zig's migration.
        desc.query = Some("(identifier) @name @definition.function");
        let desc = Box::leak(Box::new(desc));
        let mut parser = Parser::new(&config);
        for style in [
            TagStyle::Basic,
            TagStyle::WithExtensionFields,
            TagStyle::Basic,
        ] {
            config.tag_preferences.default = style;
            let tags = OfficialLanguageParser::from_desc(desc, &config).generate_tags(
                &mut parser,
                b"pub fn example() void {}",
                "source.zig",
                &config,
                std::path::Path::new("source.zig"),
            );
            assert!(tags.iter().any(|tag| tag.name == "example"));
            assert_eq!(
                tags.iter().any(|tag| tag.kind.is_some()),
                style == TagStyle::WithExtensionFields
            );
        }
        assert!(parser.walker_wasm_store.get().unwrap().is_ok());
        assert!(parser.query_wasm_store.get().unwrap().is_ok());
    }

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
                ("dart", "void dartOne() {}", None),
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
            ("dart", "void dartOne() {}", "dartOne"),
            ("swift", "struct SwiftOne {}", "SwiftOne"),
            ("rs", "pub fn rust_one() {}", "rust_one"),
            ("ml", "let ocaml_one x = x + 1", "ocaml_one"),
            ("rb", "def ruby_one\nend", "ruby_one"),
            ("zig", "pub fn zig_two() void {}", "zig_two"),
            ("dart", "void dartTwo() {}", "dartTwo"),
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
