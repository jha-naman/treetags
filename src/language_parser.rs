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

/// Stable index into `LanguageParserRegistry::parsers`.
pub type LangId = usize;

/// Outcome of matching a file to languages by name (extension/pattern), before
/// any content-based disambiguation.
///
/// Today only extension matching populates this, and every extension resolves
/// to at most one candidate, so `Ambiguous` is not yet produced. Later phases
/// (filename patterns, and co-registering `.h` for both C and C++) will start
/// returning `Ambiguous`, which the worker resolves via selectors.
pub enum NameResolution {
    /// Exactly one language matched.
    Unique(LangId),
    /// Several languages matched; ordered by tie-break precedence.
    Ambiguous(Vec<LangId>),
    /// No language matched by name.
    None,
}

/// Maps file names to `LanguageParser` strategies.
///
/// One `Arc<LanguageParserRegistry>` is shared across all worker threads.
/// Each worker thread has its own `Parser` (mutable execution engine), which
/// is created via `create_parser`.
///
/// Each parser is owned once in `parsers`; the lookup tables store `LangId`
/// indices into it. `by_extension` maps an extension to an ordered list of
/// candidate languages (highest-priority first).
pub struct LanguageParserRegistry {
    parsers: Vec<Box<dyn LanguageParser>>,
    by_extension: HashMap<String, Vec<LangId>>,
    /// Set by `--language-force`; when present, every file resolves to this
    /// language regardless of its name.
    forced: Option<LangId>,
    grammar_store: Arc<GrammarStore>,
    /// Kept so `create_parser` can hand a shared compiled registry to each
    /// per-thread `Parser` for WASM execution.
    plugin_registry: Arc<PluginRegistry>,
}

/// Resolves a `--language-force` value against the known language/alias map.
///
/// Returns `Ok(None)` when forcing is disabled (empty or `auto`), `Ok(Some(id))`
/// for a recognized name/alias, and `Err(message)` — listing the known
/// languages — for an unrecognized value.
fn resolve_forced_language(
    force: &str,
    aliases: &HashMap<String, LangId>,
) -> Result<Option<LangId>, String> {
    let trimmed = force.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    if let Some(&id) = aliases.get(&trimmed.to_lowercase()) {
        return Ok(Some(id));
    }
    let mut known: Vec<&str> = aliases.keys().map(|s| s.as_str()).collect();
    known.sort_unstable();
    Err(format!(
        "treetags: unknown --language-force '{}'; known languages: {}",
        force,
        known.join(", ")
    ))
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

        let mut parsers: Vec<Box<dyn LanguageParser>> = Vec::new();
        let mut by_extension: HashMap<String, Vec<LangId>> = HashMap::new();

        // Priorities are applied in order; the first tier to claim an extension
        // owns it (matching the historical `or_insert_with` behaviour). Storing
        // `Vec<LangId>` leaves room for later phases to register several
        // candidates per extension and disambiguate by content.

        // Priority 1: WASM plugins
        let plugin_infos = scan_ext_infos(&config.plugin_dirs, Some(&config.plugins_dir));
        for info in plugin_infos {
            if by_extension.contains_key(&info.ext) {
                continue;
            }
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
            let id = parsers.len();
            parsers.push(Box::new(WasmLanguageParser {
                extension: info.ext.clone(),
                lang,
                kind_infos,
            }));
            by_extension.insert(info.ext, vec![id]);
        }

        // Priority 2: Builtin tree-walker parsers.
        // One parser instance per language, shared across all its extensions.
        for desc in BUILTIN_LANG_DESCRIPTORS {
            let id = parsers.len();
            let mut used = false;
            for ext in desc.extensions {
                if by_extension.contains_key(*ext) {
                    continue;
                }
                by_extension.insert((*ext).to_string(), vec![id]);
                used = true;
            }
            if used {
                parsers.push(Box::new(BuiltinLanguageParser::from_desc(desc, config)));
            }
        }

        // Priority 3: Query grammar fallbacks
        for (extensions, grammar_result) in crate::built_in_grammars::load() {
            if grammar_result.is_err() {
                continue;
            }
            for ext in extensions {
                if by_extension.contains_key(ext) {
                    continue;
                }
                let id = parsers.len();
                parsers.push(Box::new(QueryLanguageParser {
                    extension: ext.to_string(),
                }));
                by_extension.insert(ext.to_string(), vec![id]);
            }
        }

        // Priority 4: User grammars (--user-languages-config).
        // Extensions registered for routing; TagsConfiguration and library
        // lifetimes are held by the shared GrammarStore.
        for ug in &config.user_grammars {
            for ext in
                crate::user_grammars::resolve_extensions(&ug.language_name, ug.extensions.as_ref())
            {
                if by_extension.contains_key(&ext) {
                    continue;
                }
                let id = parsers.len();
                parsers.push(Box::new(QueryLanguageParser {
                    extension: ext.clone(),
                }));
                by_extension.insert(ext, vec![id]);
            }
        }

        // Map language names + builtin aliases to `LangId` for `--language-force`.
        // Canonical names are inserted first so the highest-priority parser wins
        // on any collision; builtin aliases fill in afterwards.
        let mut aliases: HashMap<String, LangId> = HashMap::new();
        for (id, p) in parsers.iter().enumerate() {
            aliases.entry(p.language_name().to_lowercase()).or_insert(id);
        }
        for desc in BUILTIN_LANG_DESCRIPTORS {
            if let Some(&id) = aliases.get(&desc.lang.to_lowercase()) {
                for alias in desc.aliases {
                    aliases.entry(alias.to_lowercase()).or_insert(id);
                }
            }
        }

        let forced = match resolve_forced_language(&config.language_force, &aliases) {
            Ok(f) => f,
            Err(msg) => {
                eprintln!("{}", msg);
                std::process::exit(1);
            }
        };

        Self {
            parsers,
            by_extension,
            forced,
            grammar_store,
            plugin_registry,
        }
    }

    /// Resolves a file to candidate languages by name (currently by extension).
    /// Performs no file IO — content-based disambiguation of `Ambiguous`
    /// results is the caller's responsibility.
    ///
    /// When `--language-force` is set, every file resolves to the forced
    /// language regardless of its name.
    pub fn resolve_by_name(&self, path: &Path) -> NameResolution {
        if let Some(id) = self.forced {
            return NameResolution::Unique(id);
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return NameResolution::None;
        };
        match self.by_extension.get(ext) {
            Some(ids) if ids.len() == 1 => NameResolution::Unique(ids[0]),
            Some(ids) if !ids.is_empty() => NameResolution::Ambiguous(ids.clone()),
            _ => NameResolution::None,
        }
    }

    /// Returns the parser for a resolved `LangId`.
    pub fn parser(&self, id: LangId) -> &dyn LanguageParser {
        self.parsers[id].as_ref()
    }

    /// Returns the `LanguageParser` for the given language name, if any.
    /// Searches by `language_name()`, stopping at first match.
    pub fn for_language(&self, lang: &str) -> Option<&dyn LanguageParser> {
        self.parsers
            .iter()
            .map(|b| b.as_ref())
            .find(|lp| lp.language_name() == lang)
    }

    /// Iterates all registered parsers, deduplicated by language name.
    pub fn all_languages(&self) -> impl Iterator<Item = &dyn LanguageParser> {
        let mut seen = std::collections::HashSet::new();
        self.parsers.iter().filter_map(move |b| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn registry() -> LanguageParserRegistry {
        LanguageParserRegistry::new(&Config::for_test())
    }

    fn registry_forced(lang: &str) -> LanguageParserRegistry {
        let mut cfg = Config::for_test();
        cfg.language_force = lang.to_string();
        LanguageParserRegistry::new(&cfg)
    }

    /// Language selected for a file name, taking the highest-priority candidate
    /// (mirrors the worker's Phase 0 resolution).
    fn lang_for(reg: &LanguageParserRegistry, file: &str) -> Option<String> {
        match reg.resolve_by_name(Path::new(file)) {
            NameResolution::Unique(id) => Some(reg.parser(id).language_name().to_string()),
            NameResolution::Ambiguous(ids) => Some(reg.parser(ids[0]).language_name().to_string()),
            NameResolution::None => None,
        }
    }

    #[test]
    fn resolves_builtin_extensions() {
        let reg = registry();
        assert_eq!(lang_for(&reg, "src/main.rs").as_deref(), Some("rust"));
        assert_eq!(lang_for(&reg, "app.go").as_deref(), Some("go"));
    }

    #[test]
    fn builtin_tree_walker_wins_over_query_fallback() {
        // `.py` is claimed by both the builtin Python tree-walker (priority 2)
        // and the query-grammar fallback (priority 3). The builtin must win.
        let reg = registry();
        assert_eq!(lang_for(&reg, "script.py").as_deref(), Some("python"));
    }

    #[test]
    fn extension_matching_is_case_sensitive() {
        // `.h` -> C, `.H` -> C++: the distinction relies on case-sensitive
        // extension matching (see src/parser/cpp.rs extension tables).
        let reg = registry();
        assert_eq!(lang_for(&reg, "foo.h").as_deref(), Some("c"));
        assert_eq!(lang_for(&reg, "foo.H").as_deref(), Some("c++"));
    }

    #[test]
    fn unknown_and_extensionless_resolve_to_none() {
        let reg = registry();
        assert!(matches!(
            reg.resolve_by_name(Path::new("notes.unknownext")),
            NameResolution::None
        ));
        // Extensionless files are not matched yet (filename patterns land later).
        assert!(matches!(
            reg.resolve_by_name(Path::new("Makefile")),
            NameResolution::None
        ));
    }

    #[test]
    fn language_force_overrides_name_selection() {
        let reg = registry_forced("python");
        // Every file — including non-Python, unknown, and extensionless — becomes Python.
        assert_eq!(lang_for(&reg, "notes.txt").as_deref(), Some("python"));
        assert_eq!(lang_for(&reg, "Makefile").as_deref(), Some("python"));
        assert_eq!(lang_for(&reg, "src/main.rs").as_deref(), Some("python"));
    }

    #[test]
    fn language_force_accepts_aliases_case_insensitively() {
        assert_eq!(lang_for(&registry_forced("golang"), "x.rs").as_deref(), Some("go"));
        assert_eq!(lang_for(&registry_forced("CPP"), "x.rs").as_deref(), Some("c++"));
        assert_eq!(
            lang_for(&registry_forced("JavaScript"), "x.py").as_deref(),
            Some("javascript")
        );
    }

    #[test]
    fn language_force_auto_and_empty_disable_forcing() {
        assert_eq!(lang_for(&registry_forced("auto"), "x.rs").as_deref(), Some("rust"));
        assert_eq!(lang_for(&registry_forced(""), "x.rs").as_deref(), Some("rust"));
    }

    #[test]
    fn resolve_forced_language_cases() {
        let mut aliases = HashMap::new();
        aliases.insert("python".to_string(), 3usize);
        aliases.insert("c++".to_string(), 5usize);
        aliases.insert("cpp".to_string(), 5usize);

        assert_eq!(resolve_forced_language("", &aliases), Ok(None));
        assert_eq!(resolve_forced_language("auto", &aliases), Ok(None));
        assert_eq!(resolve_forced_language("AUTO", &aliases), Ok(None));
        assert_eq!(resolve_forced_language("Python", &aliases), Ok(Some(3)));
        assert_eq!(resolve_forced_language("CPP", &aliases), Ok(Some(5)));

        let err = resolve_forced_language("nope", &aliases).unwrap_err();
        assert!(err.contains("unknown --language-force 'nope'"));
        assert!(err.contains("python"));
    }
}
