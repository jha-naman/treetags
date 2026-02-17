use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct UserGrammar {
    pub language_name: String,
    pub grammar_lib_path: PathBuf,
    pub extensions: Option<Vec<String>>,
    pub query_file_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
pub struct TOMLConfig {
    pub user_grammars: Vec<UserGrammar>,
}

pub fn load(config_path_override: Option<&PathBuf>) -> Vec<UserGrammar> {
    let config_path = match config_path_override {
        Some(path) => path.clone(),
        None => get_config_path(),
    };

    if !config_path.exists() {
        return vec![];
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
                    return vec![];
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

            toml_config.user_grammars
        }
        Err(e) => {
            eprintln!(
                "Warning: Failed to read config file {}: {}",
                config_path.display(),
                e
            );
            vec![]
        }
    }
}

fn absolutize_path(base_dir: &Path, path: &mut PathBuf) {
    if path.is_relative() {
        *path = base_dir.join(&*path);
    }
}

fn get_config_path() -> PathBuf {
    match xdg::BaseDirectories::with_prefix("treetags") {
        Ok(xdg_dirs) => xdg_dirs.get_config_file("config.toml"),
        Err(_) => {
            // Fallback to ~/.config/treetags/config.toml
            let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            path.push(".config");
            path.push("treetags");
            path.push("config.toml");
            path
        }
    }
}
