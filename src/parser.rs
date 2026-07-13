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

use crate::built_in_grammars;
use crate::config::Config;
use crate::plugin::instance::WasmInstance;
use crate::plugin::registry::PluginRegistry;
use crate::tag;
use crate::user_grammars;
use libloading::Library;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tree_sitter::Parser as TSParser;
use tree_sitter_tags::{TagsConfiguration, TagsContext};

pub(crate) mod common;
pub(crate) mod cpp;
pub(crate) mod go;
mod helper;
pub(crate) mod js;
pub(crate) mod python;
pub(crate) mod rust;
pub(crate) mod typescript;

pub(crate) use helper::kinds_from_mappings;
pub use helper::{KindInfo, TagKindConfig};

/// Shared, immutable grammar data for query-based tag generation.
/// Built once at startup and shared across all worker threads via `Arc`.
/// `_libs` keeps dynamically-loaded grammar libraries alive.
pub(crate) struct GrammarStore {
    pub(crate) grammar_configs: Vec<Result<TagsConfiguration, tree_sitter_tags::Error>>,
    pub(crate) extension_config_map: HashMap<String, usize>,
    _libs: Vec<Library>,
}

impl GrammarStore {
    pub(crate) fn new(config: &Config) -> Self {
        let mut grammar_configs = Vec::new();
        let mut extension_config_map = HashMap::new();

        for (extensions, config_res) in built_in_grammars::load() {
            let index = grammar_configs.len();
            grammar_configs.push(config_res);
            for ext in extensions {
                extension_config_map.insert(ext.to_string(), index);
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
            grammar_configs.push(config_res);
        }

        Self {
            grammar_configs,
            extension_config_map,
            _libs: user_grammars._grammars,
        }
    }
}

/// Per-thread tag-generation execution engine.
pub struct Parser {
    pub(crate) grammar_store: Arc<GrammarStore>,
    pub tags_context: TagsContext,
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
        Self {
            grammar_store: Arc::new(GrammarStore::new(config)),
            tags_context: TagsContext::new(),
            ts_parser: TSParser::new(),
            shared_registry: None,
            local_instances: HashMap::new(),
        }
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
            ts_parser: TSParser::new(),
            shared_registry: if registry.is_empty() {
                None
            } else {
                Some(registry)
            },
            local_instances: HashMap::new(),
        }
    }

    /// Attempt to generate tags for `extension` using a WASM plugin.
    /// Returns `None` if no plugin handles this extension or the plugin errors.
    pub(crate) fn try_plugin(
        &mut self,
        extension: &str,
        code: &[u8],
        path: &str,
        config: &Config,
    ) -> Option<Vec<tag::Tag>> {
        self.shared_registry.as_ref()?.try_generate(
            &mut self.local_instances,
            extension,
            code,
            path,
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
        if let Some(tags) =
            self.try_plugin(extension, &code, file_path_relative_to_tag_file, config)
        {
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
                return Ok((desc.generate_fn)(
                    &mut self.ts_parser,
                    &code,
                    file_path_relative_to_tag_file,
                    &kind_config,
                    config,
                )
                .unwrap_or_default());
            }
        }

        // Priority 3: tag-query fallback
        Ok(self.generate_by_tag_query(&code, file_path_relative_to_tag_file, extension))
    }
}
