use std::path::PathBuf;

pub fn get_treetags_dir() -> PathBuf {
    match xdg::BaseDirectories::with_prefix("treetags") {
        Ok(xdg_dirs) => xdg_dirs.get_config_home(),
        Err(_) => {
            // Fallback to ~/.config/treetags
            let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            path.push(".config");
            path.push("treetags");
            path
        }
    }
}

pub fn get_config_path() -> PathBuf {
    get_treetags_dir().join("config.toml")
}

pub fn get_default_plugins_dir() -> PathBuf {
    get_treetags_dir().join("plugins")
}
