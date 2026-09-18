use anyhow::Context;
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
    load_checked(config_path_override).unwrap_or_else(|err| {
        eprintln!("Warning: {err:#}");
        TOMLConfig::default()
    })
}

pub fn load_checked(config_path_override: Option<&PathBuf>) -> anyhow::Result<TOMLConfig> {
    let config_path = match config_path_override {
        Some(path) => path.clone(),
        None => get_config_path(),
    };

    let content = match fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(TOMLConfig::default()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("Failed to read config file {}", config_path.display()))
        }
    };
    let mut config: TOMLConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse config file {}", config_path.display()))?;
    if let Some(directory) = config_path.parent() {
        for grammar in &mut config.user_grammars {
            absolutize_path(directory, &mut grammar.grammar_lib_path);
            if let Some(query) = &mut grammar.query_file_path {
                absolutize_path(directory, query);
            }
        }
    }
    Ok(config)
}

fn absolutize_path(base_dir: &Path, path: &mut PathBuf) {
    if path.is_relative() {
        *path = base_dir.join(&*path);
    }
}

fn get_config_path() -> PathBuf {
    super::paths::get_config_path()
}
