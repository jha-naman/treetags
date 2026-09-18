//! External grammars share compiled code; mutable Wasm stores belong to parsers.
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tree_sitter::{
    wasmtime::{Cache, CacheConfig, Config, Engine},
    Language, WasmStore,
};
use tree_sitter_tags::TagsConfiguration;

pub(crate) fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE
        .get_or_init(|| cached_engine(&crate::config::paths::get_cache_dir().join("wasm_grammars")))
}

fn cached_engine(directory: &Path) -> Engine {
    let mut config = Config::new();
    let mut cache_config = CacheConfig::new();
    cache_config.with_directory(directory);
    // Caching is best-effort. Wasmtime handles content/compiler/CPU cache keys,
    // atomic writes, corrupt entries, and cleanup. Never deserialize artifacts
    // alongside the user-supplied grammar ourselves.
    if let Ok(cache) = Cache::new(cache_config) {
        config.cache(Some(cache));
    }
    Engine::new(&config).unwrap_or_else(|_| Engine::default())
}

#[derive(Clone, Copy)]
pub(crate) enum GrammarSource {
    Bundled(fn() -> Language),
    Wasm(&'static WasmGrammar),
}

pub struct WasmGrammar {
    pub name: &'static str,
    pub version: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub abi: usize,
    pub query: Option<&'static str>,
}

pub(crate) static ZIG: WasmGrammar = WasmGrammar {
    name: "zig",
    version: "1.1.2",
    url: "https://github.com/tree-sitter-grammars/tree-sitter-zig/releases/download/v1.1.2/tree-sitter-zig.wasm",
    sha256: "54b3b83dd9c62da5815f06132bc3fc914d9dcc780370b32416446a0b7969e8c6",
    abi: 14,
    query: None,
};
pub(crate) static OBJECTIVE_C: WasmGrammar = WasmGrammar {
    name: "objc",
    version: "3.0.2",
    url: "https://github.com/tree-sitter-grammars/tree-sitter-objc/releases/download/v3.0.2/tree-sitter-objc.wasm",
    sha256: "155bf61fc94941fa9d07c86cd46895f14dfb2549fb7f646faeb83765af05c970",
    abi: 14,
    query: None,
};
pub(crate) static OCAML: WasmGrammar = WasmGrammar {
    name: "ocaml",
    version: "0.24.0",
    url: "https://github.com/tree-sitter/tree-sitter-ocaml/releases/download/v0.24.0/tree-sitter-ocaml.wasm",
    sha256: "a7fb5e4bff6854b9f68123cd79bb170788314e8d4e5adc8f4af835039e4ef571",
    abi: 14,
    query: Some(include_str!("../queries/ocaml.scm")),
};
pub(crate) const GRAMMARS: &[&WasmGrammar] = &[&ZIG, &OBJECTIVE_C, &OCAML];

impl WasmGrammar {
    pub fn path(&self, root: &Path) -> PathBuf {
        root.join(self.abi.to_string())
            .join(format!("tree-sitter-{}.wasm", self.name))
    }
}

pub(crate) struct LoadedGrammar {
    pub language: Language,
    pub tags: Option<TagsConfiguration>,
}

pub(crate) struct WasmGrammars {
    pub root: PathBuf,
    loaded: HashMap<&'static str, OnceLock<Result<LoadedGrammar, String>>>,
}

impl WasmGrammars {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            loaded: GRAMMARS.iter().map(|g| (g.name, OnceLock::new())).collect(),
        }
    }

    /// Failure is cached too: exactly one diagnostic even with concurrent files.
    pub fn get(&self, grammar: &WasmGrammar) -> Option<&LoadedGrammar> {
        self.loaded[grammar.name].get_or_init(|| {
            let path = grammar.path(&self.root);
            let result = Self::load(grammar, &path);
            if let Err(error) = &result {
                eprintln!("treetags: grammar '{}': {error}. Run `treetags grammar install {} --force` to install the compatible WASM grammar at {}", grammar.name, grammar.name, path.display());
            }
            result
        }).as_ref().ok()
    }

    fn load(grammar: &WasmGrammar, path: &Path) -> Result<LoadedGrammar, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("cannot read grammar: {e}"))?;
        Self::validate(grammar, &bytes)
    }

    pub(crate) fn validate(grammar: &WasmGrammar, bytes: &[u8]) -> Result<LoadedGrammar, String> {
        let mut store = WasmStore::new(engine()).map_err(|e| e.to_string())?;
        let language = store
            .load_language(grammar.name, bytes)
            .map_err(|e| e.to_string())?;
        let abi = language.abi_version();
        if abi != grammar.abi
            || !(tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION..=tree_sitter::LANGUAGE_VERSION)
                .contains(&abi)
        {
            return Err(format!("incompatible ABI {abi}, expected {}", grammar.abi));
        }
        let tags = grammar
            .query
            .map(|q| TagsConfiguration::new(language.clone(), q, ""))
            .transpose()
            .map_err(|e| format!("invalid tag query: {e}"))?;
        Ok(LoadedGrammar { language, tags })
    }

    /// Validate configured names without loading grammars before they are used.
    pub fn check_requested(&self, languages: &[String]) {
        for name in languages
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .collect::<BTreeSet<_>>()
        {
            if !GRAMMARS.iter().any(|g| g.name == name) {
                let mut supported: Vec<_> = GRAMMARS.iter().map(|g| g.name).collect();
                supported.sort_unstable();
                eprintln!(
                    "treetags: unknown external grammar '{name}'; supported: {}",
                    supported.join(", ")
                );
            }
        }
    }
}

impl GrammarSource {
    pub fn language(self, grammars: &WasmGrammars) -> Option<Language> {
        match self {
            Self::Bundled(load) => Some(load()),
            Self::Wasm(grammar) => grammars.get(grammar).map(|g| g.language.clone()),
        }
    }

    pub fn external(self) -> Option<&'static WasmGrammar> {
        match self {
            Self::Wasm(grammar) => Some(grammar),
            Self::Bundled(_) => None,
        }
    }
}

pub(crate) fn attach_store(parser: &mut tree_sitter::Parser) -> Result<(), String> {
    let store = WasmStore::new(engine()).map_err(|e| e.to_string())?;
    parser.set_wasm_store(store).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/grammars/wasm/14/tree-sitter-zig.wasm")
    }

    #[test]
    fn cached_language_survives_loader_store_and_file_removal() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("14")).unwrap();
        let path = ZIG.path(root.path());
        std::fs::copy(fixture(), &path).unwrap();
        let grammars = WasmGrammars::new(root.path().to_owned());
        let language = grammars.get(&ZIG).unwrap().language.clone();
        std::fs::remove_file(&path).unwrap();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let grammars = &grammars;
                let language = &language;
                scope.spawn(move || {
                    assert_eq!(&grammars.get(&ZIG).unwrap().language, language);
                    let mut parser = tree_sitter::Parser::new();
                    attach_store(&mut parser).unwrap();
                    parser.set_language(language).unwrap();
                    for _ in 0..3 {
                        assert!(!parser
                            .parse("pub fn example() void {}", None)
                            .unwrap()
                            .root_node()
                            .has_error());
                    }
                });
            }
        });
    }

    #[test]
    fn failures_are_cached_and_wrong_abi_or_query_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let grammars = WasmGrammars::new(root.path().to_owned());
        assert!(grammars.get(&ZIG).is_none());
        std::fs::create_dir(root.path().join("14")).unwrap();
        std::fs::copy(fixture(), ZIG.path(root.path())).unwrap();
        assert!(grammars.get(&ZIG).is_none());
        let wrong_abi = WasmGrammar { abi: 15, ..ZIG };
        assert!(WasmGrammars::load(&wrong_abi, &fixture())
            .err()
            .unwrap()
            .contains("incompatible ABI 14"));
        let bad_query = WasmGrammar {
            query: Some("("),
            ..ZIG
        };
        assert!(WasmGrammars::load(&bad_query, &fixture())
            .err()
            .unwrap()
            .contains("invalid tag query"));
        let wrong_export = WasmGrammar {
            name: "nonexistent",
            ..ZIG
        };
        assert!(WasmGrammars::load(&wrong_export, &fixture()).is_err());
    }
}
