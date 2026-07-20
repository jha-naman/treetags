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
    /// File extension used to look up the compiled grammar in `GrammarStore`.
    extension: String,
    /// Canonical language name (e.g. `ruby`, `shell`), used for `--list-kinds`
    /// and `--language-force`.
    lang: String,
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
        &self.lang
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
    /// Filename glob patterns in registration (priority) order. Matched against
    /// the basename before extensions.
    by_pattern: Vec<(String, LangId)>,
    /// Interpreter name (lowercased) → candidate languages, for `#!` shebang
    /// resolution. Candidates are in tier/registration order.
    by_interpreter: HashMap<String, Vec<LangId>>,
    /// Language name / alias (lowercased) → language, for `--language-force` and
    /// editor-modeline resolution.
    aliases: HashMap<String, LangId>,
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
        // Force aliases collected per source, applied after canonical names so
        // that a canonical name always wins over an alias on collision.
        let mut alias_specs: Vec<(LangId, String)> = Vec::new();
        // Filename glob patterns in tier/registration order.
        let mut by_pattern: Vec<(String, LangId)> = Vec::new();
        // Interpreter names collected per source, in tier/registration order.
        let mut interp_specs: Vec<(LangId, String)> = Vec::new();

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
            for alias in &info.aliases {
                alias_specs.push((id, alias.clone()));
            }
            for pat in &info.patterns {
                by_pattern.push((pat.clone(), id));
            }
            for interp in &info.interpreters {
                interp_specs.push((id, interp.clone()));
            }
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
            let mut claimed = false;
            for ext in desc.extensions {
                if by_extension.contains_key(*ext) {
                    continue;
                }
                by_extension.insert((*ext).to_string(), vec![id]);
                claimed = true;
            }
            // Register the language whenever it claims an extension or carries
            // name metadata, so its patterns/interpreters/aliases survive even
            // when a higher-priority source (e.g. a plugin) claims all of its
            // extensions.
            let has_metadata = !desc.aliases.is_empty()
                || !desc.patterns.is_empty()
                || !desc.interpreters.is_empty();
            if claimed || has_metadata {
                for alias in desc.aliases {
                    alias_specs.push((id, (*alias).to_string()));
                }
                for pat in desc.patterns {
                    by_pattern.push(((*pat).to_string(), id));
                }
                for interp in desc.interpreters {
                    interp_specs.push((id, (*interp).to_string()));
                }
                parsers.push(Box::new(BuiltinLanguageParser::from_desc(desc, config)));
            }
        }

        // Priority 3: Query grammar fallbacks
        for grammar in crate::built_in_grammars::load() {
            if grammar.config.is_err() {
                continue;
            }
            // A grammar spans several extensions; its aliases and patterns attach
            // to the first (representative) parser created for it.
            let mut rep_id: Option<LangId> = None;
            for ext in grammar.extensions {
                if by_extension.contains_key(*ext) {
                    continue;
                }
                let id = parsers.len();
                rep_id.get_or_insert(id);
                parsers.push(Box::new(QueryLanguageParser {
                    extension: (*ext).to_string(),
                    lang: grammar.lang.to_string(),
                }));
                by_extension.insert((*ext).to_string(), vec![id]);
            }
            // If every extension was already claimed, still register the
            // grammar's metadata against a representative parser so its
            // aliases/patterns/interpreters are not lost. The parser's
            // extension is used only for GrammarStore lookup, which is keyed
            // independently of who claims the extension in `by_extension`.
            let has_metadata = !grammar.aliases.is_empty()
                || !grammar.patterns.is_empty()
                || !grammar.interpreters.is_empty();
            let rep_id = rep_id.or_else(|| {
                if !has_metadata {
                    return None;
                }
                let id = parsers.len();
                let ext = grammar.extensions.first().copied().unwrap_or_default();
                parsers.push(Box::new(QueryLanguageParser {
                    extension: ext.to_string(),
                    lang: grammar.lang.to_string(),
                }));
                Some(id)
            });
            if let Some(id) = rep_id {
                for alias in grammar.aliases {
                    alias_specs.push((id, (*alias).to_string()));
                }
                for pat in grammar.patterns {
                    by_pattern.push(((*pat).to_string(), id));
                }
                for interp in grammar.interpreters {
                    interp_specs.push((id, (*interp).to_string()));
                }
            }
        }

        // Priority 4: User grammars (--user-languages-config).
        // Extensions registered for routing; TagsConfiguration and library
        // lifetimes are held by the shared GrammarStore.
        for ug in &config.user_grammars {
            let mut rep_id: Option<LangId> = None;
            for ext in
                crate::user_grammars::resolve_extensions(&ug.language_name, ug.extensions.as_ref())
            {
                if by_extension.contains_key(&ext) {
                    continue;
                }
                let id = parsers.len();
                rep_id.get_or_insert(id);
                parsers.push(Box::new(QueryLanguageParser {
                    extension: ext.clone(),
                    lang: ug.language_name.clone(),
                }));
                by_extension.insert(ext, vec![id]);
            }
            // Patterns attach to the grammar's representative parser. A grammar
            // needs at least one extension so its compiled config is reachable in
            // GrammarStore (which is keyed by extension); pattern-only user
            // grammars are therefore not supported and their patterns are skipped.
            if let Some(id) = rep_id {
                for pat in &ug.patterns {
                    by_pattern.push((pat.clone(), id));
                }
                for interp in &ug.interpreters {
                    interp_specs.push((id, interp.clone()));
                }
            } else if !ug.patterns.is_empty() || !ug.interpreters.is_empty() {
                eprintln!(
                    "treetags: ignoring patterns/interpreters for user grammar '{}': it declares no extensions",
                    ug.language_name
                );
            }
        }

        // Register `.h` as ambiguous between C and C++, resolved by content.
        // Only when `.h` is owned solely by the builtin C parser (i.e. not
        // claimed by a plugin or user grammar); C stays first so it remains the
        // default when the selector is inconclusive.
        let c_id = parsers.iter().position(|p| p.language_name() == "c");
        let cpp_id = parsers.iter().position(|p| p.language_name() == "c++");
        if let (Some(c_id), Some(cpp_id)) = (c_id, cpp_id) {
            if by_extension.get("h").map(Vec::as_slice) == Some(&[c_id]) {
                by_extension.insert("h".to_string(), vec![c_id, cpp_id]);
            }
        }

        // Apply user langmap edits (`--map-<LANG>` / `--langmap`) over the
        // built-in defaults. An unknown language name is a fatal error.
        for edit in &config.lang_map_edits.edits {
            match parsers
                .iter()
                .position(|p| p.language_name().eq_ignore_ascii_case(edit.lang()))
            {
                Some(id) => edit.apply(id, &mut by_extension, &mut by_pattern),
                None => {
                    eprintln!(
                        "treetags: unknown language '{}' in --map/--langmap",
                        edit.lang()
                    );
                    std::process::exit(1);
                }
            }
        }

        // Map language names + aliases to `LangId` for `--language-force`.
        // Canonical names are inserted first so the highest-priority parser wins
        // on any collision; aliases (from every source) fill in afterwards.
        let mut aliases: HashMap<String, LangId> = HashMap::new();
        for (id, p) in parsers.iter().enumerate() {
            aliases
                .entry(p.language_name().to_lowercase())
                .or_insert(id);
        }
        for (id, alias) in alias_specs {
            aliases.entry(alias.to_lowercase()).or_insert(id);
        }

        // Interpreter name -> candidate languages, preserving tier order.
        let mut by_interpreter: HashMap<String, Vec<LangId>> = HashMap::new();
        for (id, interp) in interp_specs {
            by_interpreter
                .entry(interp.to_lowercase())
                .or_default()
                .push(id);
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
            by_pattern,
            by_interpreter,
            aliases,
            forced,
            grammar_store,
            plugin_registry,
        }
    }

    /// Resolves a file to candidate languages by name. Performs no file IO —
    /// content-based disambiguation of `Ambiguous` results is the caller's
    /// responsibility.
    ///
    /// Resolution order (matching ctags): `--language-force`, then filename
    /// glob patterns, then file extension. Patterns take full precedence — if
    /// any pattern matches the basename, the extension is not consulted.
    pub fn resolve_by_name(&self, path: &Path) -> NameResolution {
        if let Some(id) = self.forced {
            return NameResolution::Unique(id);
        }

        // Stage 1: filename patterns against the basename.
        let mut cands: Vec<LangId> = Vec::new();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            for (pat, id) in &self.by_pattern {
                if crate::lang_resolve::glob_match(pat, name) {
                    cands.push(*id);
                }
            }
        }

        // Stage 2: file extension (only when no pattern matched).
        if cands.is_empty() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if let Some(ids) = self.by_extension.get(ext) {
                    cands.extend_from_slice(ids);
                }
            }
        }

        self.finalize_candidates(cands)
    }

    /// Deduplicates candidates by language name (preserving order) and collapses
    /// to `Unique`/`Ambiguous`/`None`. Dedup keeps a single candidate when the
    /// same language is reached via multiple keys (e.g. a plugin declaring a
    /// pattern across several of its extensions).
    fn finalize_candidates(&self, cands: Vec<LangId>) -> NameResolution {
        let mut seen = std::collections::HashSet::new();
        let mut unique: Vec<LangId> = Vec::new();
        for id in cands {
            if seen.insert(self.parsers[id].language_name()) {
                unique.push(id);
            }
        }
        match unique.len() {
            0 => NameResolution::None,
            1 => NameResolution::Unique(unique[0]),
            _ => NameResolution::Ambiguous(unique),
        }
    }

    /// Resolves a language from a `#!` shebang at the start of `content`.
    /// Returns the highest-priority language registered for the interpreter, or
    /// `None`. Falls back to a version-stripped interpreter name (e.g.
    /// `python3.11` → `python`) when the exact name is unknown.
    ///
    /// The caller is responsible for gating this (executable bit / `-G`).
    pub fn resolve_by_shebang(&self, content: &[u8]) -> Option<LangId> {
        let interp = crate::lang_resolve::parse_shebang(content)?;
        let key = interp.to_lowercase();
        if let Some(ids) = self.by_interpreter.get(&key) {
            return ids.first().copied();
        }
        let stripped = key.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        if stripped.len() != key.len() {
            if let Some(ids) = self.by_interpreter.get(stripped) {
                return ids.first().copied();
            }
        }
        None
    }

    /// Picks a single language from an ambiguous candidate set using a
    /// content-based selector. Returns `None` when no selector applies to the
    /// candidate group, leaving the caller to fall back to the highest-priority
    /// candidate.
    ///
    /// `prefix` is a bounded head of the file's content.
    pub fn disambiguate(&self, cands: &[LangId], prefix: &[u8]) -> Option<LangId> {
        let name_of = |id: LangId| self.parsers[id].language_name();
        let has = |name: &str| cands.iter().any(|&id| name_of(id) == name);

        // C vs C++ (e.g. a `.h` header): default to C, choose C++ on evidence.
        if has("c") && has("c++") {
            let want = if crate::lang_resolve::looks_like_cpp(prefix) {
                "c++"
            } else {
                "c"
            };
            return cands.iter().copied().find(|&id| name_of(id) == want);
        }

        None
    }

    /// Resolves a language name or alias (case-insensitive) to a `LangId`.
    /// Also accepts Emacs mode names by stripping a trailing `-mode`.
    pub fn language_id(&self, name: &str) -> Option<LangId> {
        let key = name.trim().to_lowercase();
        if let Some(&id) = self.aliases.get(&key) {
            return Some(id);
        }
        key.strip_suffix("-mode")
            .and_then(|base| self.aliases.get(base).copied())
    }

    /// Resolves a language from an editor modeline in the file's head/tail.
    /// The mode/filetype name is mapped through the language/alias table.
    ///
    /// The caller is responsible for gating this (matching ctags: `-G` only).
    pub fn resolve_by_modeline(&self, head: &[u8], tail: &[u8]) -> Option<LangId> {
        let mode = crate::lang_resolve::parse_modeline(head, tail)?;
        self.language_id(&mode)
    }

    /// Returns the parser for a resolved `LangId`.
    pub fn parser(&self, id: LangId) -> &dyn LanguageParser {
        self.parsers[id].as_ref()
    }

    /// Effective name mappings per language: `(language, sorted extensions,
    /// patterns in priority order)`. Powers `--list-maps`.
    pub fn language_maps(&self) -> Vec<(String, Vec<String>, Vec<String>)> {
        use std::collections::{BTreeMap, BTreeSet};
        let mut exts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut pats: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (ext, ids) in &self.by_extension {
            for &id in ids {
                exts.entry(self.parsers[id].language_name().to_string())
                    .or_default()
                    .insert(ext.clone());
            }
        }
        for (pat, id) in &self.by_pattern {
            pats.entry(self.parsers[*id].language_name().to_string())
                .or_default()
                .push(pat.clone());
        }
        let mut langs: BTreeSet<String> = exts.keys().cloned().collect();
        langs.extend(pats.keys().cloned());
        langs
            .into_iter()
            .map(|lang| {
                let e = exts.remove(&lang).unwrap_or_default();
                let p = pats.remove(&lang).unwrap_or_default();
                (lang, e.into_iter().collect(), p)
            })
            .collect()
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
        // Extensionless files with no matching pattern are skipped (no Make parser).
        assert!(matches!(
            reg.resolve_by_name(Path::new("Makefile")),
            NameResolution::None
        ));
        assert!(matches!(
            reg.resolve_by_name(Path::new("README")),
            NameResolution::None
        ));
    }

    #[test]
    fn filename_patterns_match_extensionless_files() {
        let reg = registry();
        assert_eq!(lang_for(&reg, "Rakefile").as_deref(), Some("ruby"));
        assert_eq!(lang_for(&reg, "Gemfile").as_deref(), Some("ruby"));
        assert_eq!(lang_for(&reg, "project/.bashrc").as_deref(), Some("shell"));
        assert_eq!(lang_for(&reg, "PKGBUILD").as_deref(), Some("shell"));
        assert_eq!(lang_for(&reg, "SConstruct").as_deref(), Some("python"));
    }

    #[test]
    fn glob_patterns_match_by_basename() {
        let reg = registry();
        assert_eq!(lang_for(&reg, "foo.gemspec").as_deref(), Some("ruby"));
        assert_eq!(lang_for(&reg, "tasks.rake").as_deref(), Some("ruby"));
        assert_eq!(lang_for(&reg, "src/prompt.zsh").as_deref(), Some("shell"));
    }

    #[test]
    fn h_header_disambiguates_c_vs_cpp() {
        let reg = registry();
        let ids = match reg.resolve_by_name(Path::new("include/foo.h")) {
            NameResolution::Ambiguous(ids) => ids,
            _ => panic!("expected .h to resolve as ambiguous C/C++"),
        };
        // Tie-break default (first candidate) is C.
        assert_eq!(reg.parser(ids[0]).language_name(), "c");
        // Content chooses the language.
        assert_eq!(
            reg.disambiguate(&ids, b"class Foo {\npublic:\n};")
                .map(|id| reg.parser(id).language_name()),
            Some("c++")
        );
        assert_eq!(
            reg.disambiguate(&ids, b"int add(int a, int b);")
                .map(|id| reg.parser(id).language_name()),
            Some("c")
        );
    }

    #[test]
    fn langmap_edits_apply() {
        use crate::config::lang_map::{LangMapEdit, LangMapEdits};

        // Add an extension and a pattern.
        let mut cfg = Config::for_test();
        cfg.lang_map_edits = LangMapEdits {
            edits: vec![
                LangMapEdit::AddExt {
                    lang: "c".into(),
                    ext: "qc".into(),
                },
                LangMapEdit::AddPattern {
                    lang: "ruby".into(),
                    pattern: "Jarfile".into(),
                },
            ],
        };
        let reg = LanguageParserRegistry::new(&cfg);
        assert_eq!(lang_for(&reg, "widget.qc").as_deref(), Some("c"));
        assert_eq!(lang_for(&reg, "Jarfile").as_deref(), Some("ruby"));

        // Remove C from `.h`, collapsing the ambiguity to C++.
        let mut cfg = Config::for_test();
        cfg.lang_map_edits = LangMapEdits {
            edits: vec![LangMapEdit::RemoveExt {
                lang: "c".into(),
                ext: "h".into(),
            }],
        };
        let reg = LanguageParserRegistry::new(&cfg);
        assert_eq!(lang_for(&reg, "foo.h").as_deref(), Some("c++"));

        // Replace a language's extensions wholesale.
        let mut cfg = Config::for_test();
        cfg.lang_map_edits = LangMapEdits {
            edits: vec![LangMapEdit::Replace {
                lang: "python".into(),
                exts: vec!["mypy".into()],
                patterns: vec![],
            }],
        };
        let reg = LanguageParserRegistry::new(&cfg);
        assert_eq!(lang_for(&reg, "a.mypy").as_deref(), Some("python"));
    }

    #[test]
    fn shadowed_builtin_keeps_patterns_and_interpreters() {
        // A plugin that claims all of Python's extensions must not erase the
        // builtin Python language's filename patterns / shebang interpreters.
        let dir = tempfile::tempdir().unwrap();
        let pdir = dir.path().join("pyplugin");
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(
            pdir.join("plugin.toml"),
            format!(
                "name = \"pyplugin\"\n\
                 version = \"0.1.0\"\n\
                 abi_version = {}\n\
                 wasm_file = \"plugin.wasm\"\n\
                 language = \"pyplugin\"\n\
                 extensions = [\"py\", \"pyw\", \"pyi\"]\n",
                crate::plugin::PLUGIN_ABI_VERSION
            ),
        )
        .unwrap();
        std::fs::write(pdir.join("plugin.wasm"), b"").unwrap();

        let mut cfg = Config::for_test();
        cfg.plugin_dirs = vec![dir.path().to_path_buf()];
        let reg = LanguageParserRegistry::new(&cfg);

        // The plugin owns the Python extensions ...
        assert_eq!(lang_for(&reg, "app.py").as_deref(), Some("pyplugin"));
        // ... but the builtin Python language's name metadata survives.
        assert_eq!(lang_for(&reg, "SConstruct").as_deref(), Some("python"));
        assert_eq!(
            reg.resolve_by_shebang(b"#!/usr/bin/env python3\n")
                .map(|id| reg.parser(id).language_name().to_string())
                .as_deref(),
            Some("python")
        );
    }

    #[test]
    fn plain_c_and_cpp_extensions_stay_unique() {
        let reg = registry();
        // Only `.h` is ambiguous; dedicated extensions remain unique.
        assert_eq!(lang_for(&reg, "a.c").as_deref(), Some("c"));
        assert_eq!(lang_for(&reg, "a.cpp").as_deref(), Some("c++"));
        assert_eq!(lang_for(&reg, "a.hpp").as_deref(), Some("c++"));
    }

    #[test]
    fn modeline_resolves_language() {
        let reg = registry();
        let lang = |h: &[u8], t: &[u8]| {
            reg.resolve_by_modeline(h, t)
                .map(|id| reg.parser(id).language_name().to_string())
        };
        // Vim `ft` names go through the alias map (`cpp` -> c++).
        assert_eq!(lang(b"// vim: set ft=cpp:", b"").as_deref(), Some("c++"));
        assert_eq!(
            lang(b"# -*- mode: python -*-", b"").as_deref(),
            Some("python")
        );
        // Emacs `-mode` suffix is stripped, then alias-resolved (`sh` -> shell).
        assert_eq!(
            lang(b"x\n# Local Variables:\n# mode: sh-mode\n# End:\n", b"").as_deref(),
            Some("shell")
        );
        assert_eq!(lang(b"nothing here", b"").as_deref(), None);
    }

    #[test]
    fn shebang_resolves_interpreter() {
        let reg = registry();
        let lang = |content: &[u8]| {
            reg.resolve_by_shebang(content)
                .map(|id| reg.parser(id).language_name().to_string())
        };
        assert_eq!(lang(b"#!/usr/bin/env python3\n").as_deref(), Some("python"));
        assert_eq!(lang(b"#!/bin/bash\n").as_deref(), Some("shell"));
        assert_eq!(lang(b"#!/bin/sh\n").as_deref(), Some("shell"));
        assert_eq!(lang(b"#!/usr/bin/ruby\n").as_deref(), Some("ruby"));
        assert_eq!(
            lang(b"#!/usr/bin/env node\n").as_deref(),
            Some("javascript")
        );
        // Version-stripped fallback: python3.11 -> python.
        assert_eq!(lang(b"#!/usr/bin/python3.11\n").as_deref(), Some("python"));
        // No shebang or unknown interpreter -> None.
        assert!(lang(b"print(1)\n").is_none());
        assert!(lang(b"#!/usr/bin/perl\n").is_none());
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
        assert_eq!(
            lang_for(&registry_forced("golang"), "x.rs").as_deref(),
            Some("go")
        );
        assert_eq!(
            lang_for(&registry_forced("CPP"), "x.rs").as_deref(),
            Some("c++")
        );
        assert_eq!(
            lang_for(&registry_forced("JavaScript"), "x.py").as_deref(),
            Some("javascript")
        );
    }

    #[test]
    fn query_fallback_has_proper_language_name() {
        let reg = registry();
        // Query-fallback languages report their proper name, not the extension.
        assert_eq!(lang_for(&reg, "app.rb").as_deref(), Some("ruby"));
        assert_eq!(lang_for(&reg, "run.sh").as_deref(), Some("shell"));
        assert_eq!(lang_for(&reg, "prog.cs").as_deref(), Some("c#"));
        assert!(reg.for_language("ruby").is_some());
        assert!(reg.for_language("shell").is_some());
        // The bare extension is no longer a language name.
        assert!(reg.for_language("rb").is_none());
    }

    #[test]
    fn language_force_resolves_query_fallback_names_and_aliases() {
        assert_eq!(
            lang_for(&registry_forced("ruby"), "x.txt").as_deref(),
            Some("ruby")
        );
        assert_eq!(
            lang_for(&registry_forced("shell"), "x.txt").as_deref(),
            Some("shell")
        );
        // `bash`/`sh` are aliases of `shell`; `csharp` is an alias of `c#`.
        assert_eq!(
            lang_for(&registry_forced("bash"), "x.txt").as_deref(),
            Some("shell")
        );
        assert_eq!(
            lang_for(&registry_forced("csharp"), "x.txt").as_deref(),
            Some("c#")
        );
    }

    #[test]
    fn language_force_auto_and_empty_disable_forcing() {
        assert_eq!(
            lang_for(&registry_forced("auto"), "x.rs").as_deref(),
            Some("rust")
        );
        assert_eq!(
            lang_for(&registry_forced(""), "x.rs").as_deref(),
            Some("rust")
        );
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
