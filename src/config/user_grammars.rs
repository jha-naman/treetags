use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct UserGrammar {
    pub language_name: String,
    pub grammar_lib_path: PathBuf,
    pub extensions: Option<Vec<String>>,
    /// `fnmatch`-style filename globs (matched against the basename) that select
    /// this grammar, e.g. `Rakefile` or `*.bzl`. Defaults to empty.
    #[serde(default)]
    pub patterns: Vec<String>,
    /// Interpreter names matched against a `#!` shebang line. Defaults to empty.
    #[serde(default)]
    pub interpreters: Vec<String>,
    pub query_file_path: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TOMLConfig {
    #[serde(default)]
    pub user_grammars: Vec<UserGrammar>,
    #[serde(default)]
    pub wasm_grammars: WasmGrammarConfig,
}

#[derive(Debug, Default, Deserialize)]
pub struct WasmGrammarConfig {
    #[serde(default)]
    pub languages: Vec<String>,
}

pub fn load(config_path_override: Option<&PathBuf>) -> TOMLConfig {
    let config_path = match config_path_override {
        Some(path) => path.clone(),
        None => get_config_path(),
    };

    if !config_path.exists() {
        return TOMLConfig::default();
    }

    match fs::read_to_string(&config_path) {
        Ok(content) => {
            let mut toml_config: TOMLConfig = match toml::from_str(&content) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to parse config file {}: {}",
                        config_path.display(),
                        e
                    );
                    return TOMLConfig::default();
                }
            };

            if let Some(config_dir) = config_path.parent() {
                for grammar in &mut toml_config.user_grammars {
                    absolutize_path(config_dir, &mut grammar.grammar_lib_path);
                    if let Some(query_path) = &mut grammar.query_file_path {
                        absolutize_path(config_dir, query_path);
                    }
                }
            }

            toml_config
        }
        Err(e) => {
            eprintln!(
                "Warning: Failed to read config file {}: {}",
                config_path.display(),
                e
            );
            TOMLConfig::default()
        }
    }
}

fn absolutize_path(base_dir: &Path, path: &mut PathBuf) {
    if path.is_relative() {
        *path = base_dir.join(&*path);
    }
}

fn get_config_path() -> PathBuf {
    super::paths::get_config_path()
}
