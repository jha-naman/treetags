use super::instance::{Request, Tag as PluginTag, WasmInstance};
use super::manifest::PluginManifest;
use super::shared::SharedPlugin;
use crate::config::Config;
use crate::split_by_newlines::split_by_newlines;
use crate::tag::Tag;
use indexmap::IndexMap;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use wasmtime::Engine;

struct PluginEntry {
    wasm_path: PathBuf,
    language: Option<String>,
    aliases: Vec<String>,
    patterns: Vec<String>,
    interpreters: Vec<String>,
    name: String,
    kinds: Vec<super::manifest::ManifestKind>,
}

struct ExtPlugin {
    wasm_path: PathBuf,
    language: Option<String>,
    name: String,
}

/// Language name and file extensions for a detected plugin.
pub struct PluginInfo {
    pub language: String,
    pub extensions: Vec<String>,
}

/// Shared-across-threads plugin registry. On the first file processed for a given
/// extension, its `.wasm` is JIT-compiled into a `SharedPlugin` stored in `compiled`.
/// Worker threads each lazily create their own per-thread `WasmInstance` from the
/// shared compiled plugin via `SharedPlugin::create_instance()`.
pub struct PluginRegistry {
    entries: HashMap<String, PluginEntry>,
    ext_plugins: HashMap<String, ExtPlugin>,
    compiled: HashMap<PathBuf, OnceLock<Option<SharedPlugin>>>,
    engine: Engine,
    /// Plugins opted in for cache file access.
    cache_enabled_plugins: HashSet<String>,
    /// Per-project cache root: ~/.cache/treetags/<hash-of-cwd>/.
    /// None when no plugins have cache access enabled.
    project_cache_root: Option<PathBuf>,
}

impl PluginRegistry {
    /// Scans `dirs` for `plugin.toml` manifests. WASM binaries are JIT-compiled lazily
    /// on first use, at most once per unique `.wasm` file.
    /// `cache_plugins` is the list of plugin names granted cache file access.
    pub fn scan(
        dirs: &[PathBuf],
        recursive_dir: Option<&PathBuf>,
        cache_plugins: &[String],
    ) -> Self {
        let entries = scan_to_entries(dirs, recursive_dir);

        let mut ext_plugins: HashMap<String, ExtPlugin> = HashMap::new();
        let mut compiled: HashMap<PathBuf, OnceLock<Option<SharedPlugin>>> = HashMap::new();

        for (ext, entry) in &entries {
            compiled
                .entry(entry.wasm_path.clone())
                .or_insert_with(OnceLock::new);
            ext_plugins.insert(
                ext.clone(),
                ExtPlugin {
                    wasm_path: entry.wasm_path.clone(),
                    language: entry.language.clone(),
                    name: entry.name.clone(),
                },
            );
        }

        let cache_enabled_plugins: HashSet<String> = cache_plugins.iter().cloned().collect();
        let project_cache_root = if cache_enabled_plugins.is_empty() {
            None
        } else {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let project_hash = format!("{:016x}", fnv1a_64(cwd.as_os_str().as_encoded_bytes()));
            Some(crate::config::paths::get_cache_dir().join(project_hash))
        };

        Self {
            entries,
            ext_plugins,
            compiled,
            engine: Engine::default(),
            cache_enabled_plugins,
            project_cache_root,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ext_plugins.is_empty()
    }

    /// Returns info about all detected plugins, grouped by plugin identity (wasm path).
    /// Each entry has the display language name (falls back to plugin name) and sorted extensions.
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        let mut by_wasm: HashMap<&PathBuf, (String, Vec<String>)> = HashMap::new();
        for (ext, entry) in &self.entries {
            let display = entry.language.as_deref().unwrap_or(&entry.name).to_string();
            let slot = by_wasm
                .entry(&entry.wasm_path)
                .or_insert_with(|| (display, Vec::new()));
            slot.1.push(ext.clone());
        }
        let mut result: Vec<PluginInfo> = by_wasm
            .into_values()
            .map(|(language, mut extensions)| {
                extensions.sort();
                PluginInfo {
                    language,
                    extensions,
                }
            })
            .collect();
        result.sort_by(|a, b| a.language.cmp(&b.language));
        result
    }

    /// Attempts to generate tags for `extension` using a plugin.
    ///
    /// `local_instances` is the calling thread's per-thread instance cache.
    /// On the first call for a given extension, the plugin is JIT-compiled (once, shared
    /// across threads) and a new `WasmInstance` is created for this thread.
    /// Returns `None` if no plugin is registered or if the plugin fails.
    pub fn try_generate(
        &self,
        local_instances: &mut HashMap<String, WasmInstance>,
        extension: &str,
        source: &[u8],
        file_path: &str,
        absolute_path: &Path,
        config: &Config,
    ) -> Option<Vec<Tag>> {
        let ep = self.ext_plugins.get(extension)?;

        let plugin_cache_dir: Option<PathBuf> = if self.cache_enabled_plugins.contains(&ep.name) {
            self.project_cache_root
                .as_ref()
                .map(|root| root.join(&ep.name))
        } else {
            None
        };

        // Lazily JIT-compile the plugin the first time a file with this extension is seen.
        // On failure the OnceLock stores None permanently — no retry, error printed once.
        let shared = self
            .compiled
            .get(&ep.wasm_path)?
            .get_or_init(|| {
                SharedPlugin::from_file(&self.engine, &ep.wasm_path)
                    .map_err(|e| {
                        eprintln!(
                            "treetags: plugin load error for {}: {e}",
                            ep.wasm_path.display()
                        )
                    })
                    .ok()
            })
            .as_ref()?;

        // Lazy-create a per-thread WasmInstance from the now-compiled shared plugin.
        // The cache dir is preopened as "." in the plugin's WASI sandbox.
        let instance = match local_instances.entry(extension.to_string()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                let inst = shared
                    .create_instance(plugin_cache_dir.as_deref())
                    .map_err(|e| eprintln!("treetags: plugin init error for .{extension}: {e}"))
                    .ok()?;
                e.insert(inst)
            }
        };

        let language = ep.language.as_deref();
        let kinds = language
            .map(|lang| config.get_kinds(lang))
            .unwrap_or("")
            .to_string();

        let cache_file = plugin_cache_dir
            .as_ref()
            .map(|_| cache_filename(absolute_path));

        let req = Request {
            file_path: file_path.to_string(),
            kinds,
            extras: config.extras.clone(),
            fields: config.fields.clone(),
            cache_file,
        };

        match instance.generate(&req, source) {
            Err(e) => {
                eprintln!("treetags: plugin call error for .{extension}: {e}");
                None
            }
            Ok(Err(msg)) => {
                eprintln!("treetags: plugin error for .{extension}: {msg}");
                None
            }
            Ok(Ok(plugin_tags)) => {
                let source_lines = split_by_newlines(source);
                Some(convert_tags(plugin_tags, &source_lines, file_path))
            }
        }
    }
}

/// FNV-1a 64-bit hash — deterministic across runs, no new dependencies.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    const BASIS: u64 = 14695981039346656037;
    const PRIME: u64 = 1099511628211;
    let mut h = BASIS;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}

/// Returns the cache filename for a source file: a 16-hex-digit FNV-1a hash + ".cache".
fn cache_filename(abs_path: &Path) -> String {
    format!(
        "{:016x}.cache",
        fnv1a_64(abs_path.as_os_str().as_encoded_bytes())
    )
}

/// Per-extension plugin metadata, returned without loading any WASM.
pub struct PluginExtInfo {
    pub ext: String,
    pub lang: Option<String>,
    pub aliases: Vec<String>,
    pub patterns: Vec<String>,
    pub interpreters: Vec<String>,
    pub kinds: Vec<super::manifest::ManifestKind>,
}

/// Scans plugin manifests (no WASM loading) and returns per-extension plugin info.
/// Used by `LanguageParserRegistry` to build `WasmLanguageParser` stubs.
pub fn scan_ext_infos(dirs: &[PathBuf], plugins_dir: Option<&PathBuf>) -> Vec<PluginExtInfo> {
    scan_to_entries(dirs, plugins_dir)
        .into_iter()
        .map(|(ext, entry)| PluginExtInfo {
            ext,
            lang: entry.language,
            aliases: entry.aliases,
            patterns: entry.patterns,
            interpreters: entry.interpreters,
            kinds: entry.kinds,
        })
        .collect()
}

/// Prints a formatted table of all detected plugins to stdout.
pub fn print_plugin_list(dirs: &[PathBuf], plugins_dir: &PathBuf) {
    println!("Plugin directory: {}", plugins_dir.display());
    let registry = PluginRegistry::scan(dirs, Some(plugins_dir), &[]);
    let plugins = registry.list_plugins();
    if plugins.is_empty() {
        println!("No plugins detected.");
    } else {
        let col_width = plugins
            .iter()
            .map(|p| p.language.len())
            .max()
            .unwrap_or(0)
            .max("LANGUAGE".len());
        println!("{:<col_width$}    EXTENSIONS", "LANGUAGE");
        for info in plugins {
            println!(
                "{:<col_width$}    {}",
                info.language,
                info.extensions.join(", ")
            );
        }
    }
}

/// Returns the set of language names declared by plugins found in the given dirs.
/// Silently skips malformed manifests. No WASM is loaded.
pub fn scan_language_names(
    dirs: &[PathBuf],
    plugins_dir: Option<&PathBuf>,
) -> std::collections::HashSet<String> {
    scan_to_entries(dirs, plugins_dir)
        .into_values()
        .filter_map(|e| e.language)
        .collect()
}

/// Scans dirs for plugin.toml manifests and builds the extension→entry map.
/// No WASM is loaded. This is the shared foundation for both `PluginRegistry::scan`
/// and `scan_language_names`.
fn scan_to_entries(
    dirs: &[PathBuf],
    recursive_dir: Option<&PathBuf>,
) -> HashMap<String, PluginEntry> {
    let mut entries: HashMap<String, PluginEntry> = HashMap::new();

    // Scan the recursive default dir first so explicit --plugin-dir entries take precedence.
    if let Some(r_dir) = recursive_dir {
        scan_recursive(r_dir, &mut entries);
    }

    for dir in dirs {
        let manifest_path = dir.join("plugin.toml");
        if !manifest_path.exists() {
            // Search one level deep for plugin.toml files.
            if let Ok(read_dir) = std::fs::read_dir(dir) {
                for entry in read_dir.flatten() {
                    let sub = entry.path().join("plugin.toml");
                    if sub.exists() {
                        load_manifest(&sub, &mut entries);
                    }
                }
            }
        } else {
            load_manifest(&manifest_path, &mut entries);
        }
    }

    entries
}

fn scan_recursive(dir: &Path, entries: &mut HashMap<String, PluginEntry>) {
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_recursive(&path, entries);
            } else if path
                .file_name()
                .map(|n| n == "plugin.toml")
                .unwrap_or(false)
            {
                load_manifest(&path, entries);
            }
        }
    }
}

fn load_manifest(manifest_path: &Path, entries: &mut HashMap<String, PluginEntry>) {
    let dir = match manifest_path.parent() {
        Some(d) => d.to_path_buf(),
        None => return,
    };
    let text = match std::fs::read_to_string(manifest_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("treetags: cannot read {}: {e}", manifest_path.display());
            return;
        }
    };
    let manifest: PluginManifest = match toml::from_str(&text) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("treetags: bad manifest {}: {e}", manifest_path.display());
            return;
        }
    };
    if manifest.abi_version != super::PLUGIN_ABI_VERSION {
        eprintln!(
            "treetags: plugin '{}' targets ABI version {}, \
             but treetags supports ABI version {}",
            manifest.name,
            manifest.abi_version,
            super::PLUGIN_ABI_VERSION
        );
        return;
    }
    let wasm_path = manifest.wasm_path(&dir);
    if !wasm_path.exists() {
        eprintln!(
            "treetags: plugin {} wasm_file '{}' not found",
            manifest.name,
            wasm_path.display()
        );
        return;
    }
    let language = manifest.language.clone();
    let aliases = manifest.aliases.clone();
    let patterns = manifest.patterns.clone();
    let interpreters = manifest.interpreters.clone();
    let name = manifest.name.clone();
    let kinds = manifest.kinds.clone().unwrap_or_default();
    for ext in &manifest.extensions {
        entries.insert(
            ext.clone(),
            PluginEntry {
                wasm_path: wasm_path.clone(),
                language: language.clone(),
                aliases: aliases.clone(),
                patterns: patterns.clone(),
                interpreters: interpreters.clone(),
                name: name.clone(),
                kinds: kinds.clone(),
            },
        );
    }
}

fn convert_tags(
    plugin_tags: Vec<PluginTag>,
    source_lines: &[Vec<u8>],
    file_path: &str,
) -> Vec<Tag> {
    plugin_tags
        .into_iter()
        .map(|t| {
            let address = format_address(source_lines, t.line);
            let mut ext_fields = IndexMap::new();
            if let Some(end) = t.end_line {
                ext_fields.insert("end".to_string(), end.to_string());
            }
            let mut extra: Vec<(String, String)> = t.extension_fields;
            extra.sort_unstable_by_key(|(k, _)| k.clone());
            for (k, v) in extra {
                ext_fields.insert(k, v);
            }
            Tag {
                name: t.name,
                file_name: file_path.to_string(),
                address,
                kind: Some(t.kind),
                extension_fields: if ext_fields.is_empty() {
                    None
                } else {
                    Some(ext_fields)
                },
            }
        })
        .collect()
}

fn format_address(lines: &[Vec<u8>], line: u32) -> String {
    let line_bytes = lines
        .get(line.saturating_sub(1) as usize)
        .map(|v| v.as_slice())
        .unwrap_or(b"");
    let line_str = String::from_utf8_lossy(line_bytes);
    let mut escaped = line_str.replace('\\', "\\\\").replace('/', "\\/");
    if escaped.len() > 96 {
        let at = (0..=96)
            .rev()
            .find(|&i| escaped.is_char_boundary(i))
            .unwrap_or(0);
        escaped.truncate(at);
        format!("/^{}/;\"", escaped)
    } else {
        format!("/^{}$/;\"", escaped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_scan_recursive() {
        let dir = tempdir().unwrap();
        let plugin_a = dir.path().join("plugin-a");
        let plugin_b = dir.path().join("nested/plugin-b");
        fs::create_dir_all(&plugin_a).unwrap();
        fs::create_dir_all(&plugin_b).unwrap();

        fs::write(
            plugin_a.join("plugin.toml"),
            r#"
name = "plugin-a"
version = "0.1.0"
abi_version = 3
wasm_file = "plugin.wasm"
extensions = ["a"]
"#,
        )
        .unwrap();
        fs::write(plugin_a.join("plugin.wasm"), "").unwrap();

        fs::write(
            plugin_b.join("plugin.toml"),
            r#"
name = "plugin-b"
version = "0.1.0"
abi_version = 3
wasm_file = "plugin.wasm"
extensions = ["b"]
"#,
        )
        .unwrap();
        fs::write(plugin_b.join("plugin.wasm"), "").unwrap();

        let registry = PluginRegistry::scan(&[], Some(&dir.path().to_path_buf()), &[]);
        assert!(registry.entries.contains_key("a"));
        assert!(registry.entries.contains_key("b"));
        assert_eq!(registry.entries.len(), 2);
    }

    #[test]
    fn test_list_plugins() {
        let dir = tempdir().unwrap();
        let plugin_java = dir.path().join("java");
        fs::create_dir_all(&plugin_java).unwrap();

        fs::write(
            plugin_java.join("plugin.toml"),
            r#"
name = "java-plugin"
version = "0.1.0"
abi_version = 3
wasm_file = "plugin.wasm"
language = "java"
extensions = ["java", "class"]
"#,
        )
        .unwrap();
        fs::write(plugin_java.join("plugin.wasm"), "").unwrap();

        let registry = PluginRegistry::scan(&[], Some(&dir.path().to_path_buf()), &[]);
        let plugins = registry.list_plugins();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].language, "java");
        assert_eq!(plugins[0].extensions, vec!["class", "java"]);
    }

    #[test]
    fn test_manifest_aliases_parsed() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("mylang");
        fs::create_dir_all(&p).unwrap();
        fs::write(
            p.join("plugin.toml"),
            r#"
name = "mylang-plugin"
version = "0.1.0"
abi_version = 3
wasm_file = "plugin.wasm"
language = "mylang"
aliases = ["ml2", "mylanguage"]
extensions = ["ml2"]
"#,
        )
        .unwrap();
        fs::write(p.join("plugin.wasm"), "").unwrap();

        let infos = scan_ext_infos(&[], Some(&dir.path().to_path_buf()));
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].lang.as_deref(), Some("mylang"));
        assert_eq!(
            infos[0].aliases,
            vec!["ml2".to_string(), "mylanguage".to_string()]
        );
    }

    #[test]
    fn test_manifest_patterns_and_interpreters_parsed() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("mylang");
        fs::create_dir_all(&p).unwrap();
        fs::write(
            p.join("plugin.toml"),
            r#"
name = "mylang-plugin"
version = "0.1.0"
abi_version = 3
wasm_file = "plugin.wasm"
language = "mylang"
extensions = ["ml2"]
patterns = ["Mylangfile", "*.ml2x"]
interpreters = ["mylang", "ml2run"]
"#,
        )
        .unwrap();
        fs::write(p.join("plugin.wasm"), "").unwrap();

        let infos = scan_ext_infos(&[], Some(&dir.path().to_path_buf()));
        assert_eq!(infos.len(), 1);
        assert_eq!(
            infos[0].patterns,
            vec!["Mylangfile".to_string(), "*.ml2x".to_string()]
        );
        assert_eq!(
            infos[0].interpreters,
            vec!["mylang".to_string(), "ml2run".to_string()]
        );
    }

    #[test]
    fn test_manifest_aliases_default_empty() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("plugin.toml"),
            r#"
name = "noalias"
version = "0.1.0"
abi_version = 3
wasm_file = "plugin.wasm"
extensions = ["xyz"]
"#,
        )
        .unwrap();
        fs::write(dir.path().join("plugin.wasm"), "").unwrap();

        let infos = scan_ext_infos(&[dir.path().to_path_buf()], None);
        assert_eq!(infos.len(), 1);
        assert!(infos[0].aliases.is_empty());
        assert!(infos[0].patterns.is_empty());
        assert!(infos[0].interpreters.is_empty());
    }

    #[test]
    fn test_list_plugins_fallback_to_name() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(&dir).unwrap();

        fs::write(
            dir.path().join("plugin.toml"),
            r#"
name = "my-plugin"
version = "0.1.0"
abi_version = 3
wasm_file = "plugin.wasm"
extensions = ["xyz"]
"#,
        )
        .unwrap();
        fs::write(dir.path().join("plugin.wasm"), "").unwrap();

        let registry = PluginRegistry::scan(&[dir.path().to_path_buf()], None, &[]);
        let plugins = registry.list_plugins();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].language, "my-plugin");
    }
}
