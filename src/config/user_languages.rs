use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct GrammarConfig {
    pub library_path: PathBuf,
    pub extensions: Vec<String>,
    pub query_file: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigFile {
    grammars: HashMap<String, GrammarConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct UserLanguagesConfig {
    /// Map from file extension to grammar name and config
    pub extension_map: HashMap<String, (String, GrammarConfig)>,
}

impl UserLanguagesConfig {
    pub fn load() -> Self {
        let config_path = Self::get_config_path();

        if !config_path.exists() {
            return Self::default();
        }

        match fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str::<ConfigFile>(&content) {
                Ok(config_file) => {
                    let mut extension_map = HashMap::new();

                    for (grammar_name, grammar_config) in config_file.grammars {
                        for ext in &grammar_config.extensions {
                            extension_map.insert(
                                ext.clone(),
                                (grammar_name.clone(), grammar_config.clone()),
                            );
                        }
                    }

                    Self { extension_map }
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to parse config file {}: {}",
                        config_path.display(),
                        e
                    );
                    Self::default()
                }
            },
            Err(e) => {
                eprintln!(
                    "Warning: Failed to read config file {}: {}",
                    config_path.display(),
                    e
                );
                Self::default()
            }
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

    pub fn get_grammar_for_extension(&self, extension: &str) -> Option<&(String, GrammarConfig)> {
        self.extension_map.get(extension)
    }
}

// Fallback implementation for dirs crate functionality
mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}
