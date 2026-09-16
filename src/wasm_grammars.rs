//! External grammars share compiled code; mutable Wasm stores belong to parsers.
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tree_sitter::{wasmtime::Engine, Language, WasmStore};
use tree_sitter_tags::TagsConfiguration;

pub(crate) fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(Engine::default)
}

#[derive(Clone, Copy)]
pub(crate) enum GrammarSource {
    Bundled(fn() -> Language),
    Wasm(&'static WasmGrammar),
}

pub struct WasmGrammar {
    pub name: &'static str,
    pub abi: usize,
    pub query: Option<&'static str>,
}

pub(crate) static ZIG: WasmGrammar = WasmGrammar {
    name: "zig",
    abi: 14,
    query: None,
};
pub(crate) static OCAML: WasmGrammar = WasmGrammar {
    name: "ocaml",
    abi: 14,
    query: Some(include_str!("../queries/ocaml.scm")),
};
pub(crate) const GRAMMARS: &[&WasmGrammar] = &[&ZIG, &OCAML];

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
                eprintln!("treetags: grammar '{}': {error}. Manually install the compatible WASM grammar at {}", grammar.name, path.display());
            }
            result
        }).as_ref().ok()
    }

    fn load(grammar: &WasmGrammar, path: &Path) -> Result<LoadedGrammar, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("cannot read grammar: {e}"))?;
        let mut store = WasmStore::new(engine()).map_err(|e| e.to_string())?;
        let language = store
            .load_language(grammar.name, &bytes)
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

    pub fn check_requested(&self, languages: &[String]) {
        for name in languages
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .collect::<BTreeSet<_>>()
        {
            match GRAMMARS.iter().find(|g| g.name == name) {
                Some(grammar) => {
                    self.get(grammar);
                }
                None => {
                    eprintln!("treetags: unknown external grammar '{name}'; supported: ocaml, zig")
                }
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
        let wrong_abi = WasmGrammar {
            name: "zig",
            abi: 15,
            query: None,
        };
        assert!(WasmGrammars::load(&wrong_abi, &fixture())
            .err()
            .unwrap()
            .contains("incompatible ABI 14"));
        let bad_query = WasmGrammar {
            name: "zig",
            abi: 14,
            query: Some("("),
        };
        assert!(WasmGrammars::load(&bad_query, &fixture())
            .err()
            .unwrap()
            .contains("invalid tag query"));
        let wrong_export = WasmGrammar {
            name: "nonexistent",
            abi: 14,
            query: None,
        };
        assert!(WasmGrammars::load(&wrong_export, &fixture()).is_err());
    }
}
