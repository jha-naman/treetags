use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::builtin_langs::{BuiltinLangDesc, BUILTIN_LANG_DESCRIPTORS};
use crate::config::Config;
use crate::parser::{kinds_from_mappings, KindInfo, TagKindConfig};
use crate::parser::{GrammarStore, Parser};
use crate::plugin::registry::{scan_ext_infos, PluginRegistry};
use crate::tag::Tag;

// ---------------------------------------------------------------------------
// Core trait
// ---------------------------------------------------------------------------

/// Strategy for generating tags for a specific language.
///
/// Implementations are stateless (or hold only configuration) and are shared
/// across all worker threads via `Arc<LanguageParserRegistry>`.  All mutable
/// parsing state lives in the per-thread `Parser` passed to `generate_tags`.
pub trait LanguageParser: Send + Sync {
    /// Generate tags by delegating to the per-thread `Parser` engine.
    /// `absolute_path` is the source file's absolute path, used by WASM plugins
    /// for cache file naming; ignored by builtin and query parsers.
    fn generate_tags(
        &self,
        parser: &mut Parser,
        code: &[u8],
        path: &str,
        config: &Config,
        absolute_path: &Path,
    ) -> Vec<Tag>;

    /// Kind metadata for `--list-kinds`. Never requires a `Parser`.
    fn kinds(&self) -> Vec<KindInfo>;

    /// Canonical language name, matching the `--kinds-{lang}` CLI argument.
    fn language_name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// Builtin language parser (tree-walker with extension fields)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct BuiltinLanguageParser {
    lang: &'static str,
    kind_config: TagKindConfig,
    kind_defaults: &'static [(&'static [&'static str], &'static str)],
    kind_optionals: &'static [(&'static [&'static str], &'static str)],
    generate_fn: crate::builtin_langs::BuiltinGenerateFn,
}

impl BuiltinLanguageParser {
    pub(crate) fn from_desc(desc: &'static BuiltinLangDesc, config: &Config) -> Self {
        let kinds_str = config.get_kinds(desc.lang);
        let kind_config =
            TagKindConfig::from_string(kinds_str, desc.kind_defaults, desc.kind_optionals);
        Self {
            lang: desc.lang,
            kind_config,
            kind_defaults: desc.kind_defaults,
            kind_optionals: desc.kind_optionals,
            generate_fn: desc.generate_fn,
        }
    }
}

impl LanguageParser for BuiltinLanguageParser {
    fn generate_tags(
        &self,
        parser: &mut Parser,
        code: &[u8],
        path: &str,
        config: &Config,
        _absolute_path: &Path,
    ) -> Vec<Tag> {
        (self.generate_fn)(&mut parser.ts_parser, code, path, &self.kind_config, config)
            .unwrap_or_default()
    }

    fn kinds(&self) -> Vec<KindInfo> {
        kinds_from_mappings(self.kind_defaults, self.kind_optionals)
    }

    fn language_name(&self) -> &str {
        self.lang
    }
}

// ---------------------------------------------------------------------------
// WASM plugin parser
// ---------------------------------------------------------------------------

pub(crate) struct WasmLanguageParser {
    extension: String,
    lang: String,
    kind_infos: Vec<KindInfo>,
}

impl LanguageParser for WasmLanguageParser {
    fn generate_tags(
        &self,
        parser: &mut Parser,
        code: &[u8],
        path: &str,
        config: &Config,
        absolute_path: &Path,
    ) -> Vec<Tag> {
        parser
            .try_plugin(&self.extension, code, path, config, absolute_path)
            .unwrap_or_default()
    }

    fn kinds(&self) -> Vec<KindInfo> {
        self.kind_infos.clone()
    }

    fn language_name(&self) -> &str {
        &self.lang
    }
}

// ---------------------------------------------------------------------------
// Query-based fallback parser (tree-sitter tag queries)
// ---------------------------------------------------------------------------

pub(crate) struct QueryLanguageParser {
    extension: String,
}

impl LanguageParser for QueryLanguageParser {
    fn generate_tags(
        &self,
        parser: &mut Parser,
        code: &[u8],
        path: &str,
        _config: &Config,
        _absolute_path: &Path,
    ) -> Vec<Tag> {
        parser.generate_by_tag_query(code, path, &self.extension)
    }

    fn kinds(&self) -> Vec<KindInfo> {
        vec![]
    }

    fn language_name(&self) -> &str {
        &self.extension
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Maps file extensions to `LanguageParser` strategies.
///
/// One `Arc<LanguageParserRegistry>` is shared across all worker threads.
/// Each worker thread has its own `Parser` (mutable execution engine), which
/// is created via `create_parser`.
pub struct LanguageParserRegistry {
    by_extension: HashMap<String, Box<dyn LanguageParser>>,
    grammar_store: Arc<GrammarStore>,
    /// Kept so `create_parser` can hand a shared compiled registry to each
    /// per-thread `Parser` for WASM execution.
    plugin_registry: Arc<PluginRegistry>,
}

impl LanguageParserRegistry {
    /// Build the full registry, loading and JIT-compiling WASM plugins.
    /// Call this once at startup and share the result via `Arc`.
    pub fn new(config: &Config) -> Self {
        let grammar_store = Arc::new(GrammarStore::new(config));
        let plugin_registry = Arc::new(PluginRegistry::scan(
            &config.plugin_dirs,
            Some(&config.plugins_dir),
            &config.plugin_cache,
        ));

        let mut map: HashMap<String, Box<dyn LanguageParser>> = HashMap::new();

        // Priority 1: WASM plugins
        let plugin_infos = scan_ext_infos(&config.plugin_dirs, Some(&config.plugins_dir));
        for info in plugin_infos {
            let lang = info.lang.clone().unwrap_or_else(|| info.ext.clone());
            let kind_infos: Vec<KindInfo> = info
                .kinds
                .into_iter()
                .map(|mk| KindInfo {
                    letter: mk.letter,
                    name: mk.name,
                    default: mk.default,
                })
                .collect();
            map.entry(info.ext.clone()).or_insert_with(|| {
                Box::new(WasmLanguageParser {
                    extension: info.ext,
                    lang,
                    kind_infos,
                })
            });
        }

        // Priority 2: Builtin tree-walker parsers
        for desc in BUILTIN_LANG_DESCRIPTORS {
            let lp = BuiltinLanguageParser::from_desc(desc, config);
            for ext in desc.extensions {
                map.entry(ext.to_string())
                    .or_insert_with(|| Box::new(lp.clone()));
            }
        }

        // Priority 3: Query grammar fallbacks
        for (extensions, grammar_result) in crate::built_in_grammars::load() {
            if grammar_result.is_ok() {
                for ext in extensions {
                    map.entry(ext.to_string()).or_insert_with(|| {
                        Box::new(QueryLanguageParser {
                            extension: ext.to_string(),
                        })
                    });
                }
            }
        }

        // Priority 4: User grammars (--user-languages-config).
        // Extensions registered for routing; TagsConfiguration and library
        // lifetimes are held by the shared GrammarStore.
        for ug in &config.user_grammars {
            for ext in
                crate::user_grammars::resolve_extensions(&ug.language_name, ug.extensions.as_ref())
            {
                map.entry(ext.clone())
                    .or_insert_with(|| Box::new(QueryLanguageParser { extension: ext }));
            }
        }

        Self {
            by_extension: map,
            grammar_store,
            plugin_registry,
        }
    }

    /// Returns the `LanguageParser` for the given file extension, if any.
    pub fn for_extension(&self, ext: &str) -> Option<&dyn LanguageParser> {
        self.by_extension.get(ext).map(|b| b.as_ref())
    }

    /// Returns the `LanguageParser` for the given language name, if any.
    /// Searches by `language_name()`, stopping at first match.
    pub fn for_language(&self, lang: &str) -> Option<&dyn LanguageParser> {
        self.by_extension
            .values()
            .map(|b| b.as_ref())
            .find(|lp| lp.language_name() == lang)
    }

    /// Iterates all registered parsers, deduplicated by language name.
    pub fn all_languages(&self) -> impl Iterator<Item = &dyn LanguageParser> {
        let mut seen = std::collections::HashSet::new();
        self.by_extension.values().filter_map(move |b| {
            let lp = b.as_ref();
            if seen.insert(lp.language_name().to_string()) {
                Some(lp)
            } else {
                None
            }
        })
    }

    /// Creates a per-thread `Parser` that shares this registry's compiled WASM modules.
    pub fn create_parser(&self) -> Parser {
        Parser::with_store_and_registry(
            Arc::clone(&self.grammar_store),
            Arc::clone(&self.plugin_registry),
        )
    }
}
