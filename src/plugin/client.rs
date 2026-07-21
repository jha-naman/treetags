//! Client-side plugin management: discovering, installing, and updating plugins
//! from the distribution index published for this build's `PLUGIN_ABI_VERSION`.
//!
//! The binary only ever talks to its own ABI bucket (`{base}/{abi}/index.json`),
//! so an older treetags keeps resolving the last plugins that were compatible
//! with it. Downloads are verified against the `wasm_sha256` in the index.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};

use super::index::{sha256_hex, IndexEntry, PluginIndex};
use super::manifest::PluginManifest;
use super::PLUGIN_ABI_VERSION;
use crate::config::paths::get_cache_dir;

/// Default site hosting the per-ABI plugin index. Overridable via
/// `--plugin-index-url` or the `TREETAGS_PLUGIN_INDEX` env var.
pub const DEFAULT_PLUGIN_INDEX_BASE: &str = "https://jha-naman.github.io/treetags";

/// How long a cached index is served before we re-fetch.
const INDEX_CACHE_TTL: Duration = Duration::from_secs(3600);

/// A plugin found in the local plugins directory.
struct InstalledPlugin {
    version: String,
    dir: PathBuf,
}

/// Resolves the index base URL: `--plugin-index-url` > `$TREETAGS_PLUGIN_INDEX`
/// > the compiled-in default.
pub fn resolve_base_url(cli_override: Option<&str>) -> String {
    if let Some(url) = cli_override {
        if !url.is_empty() {
            return url.to_string();
        }
    }
    if let Ok(url) = std::env::var("TREETAGS_PLUGIN_INDEX") {
        if !url.is_empty() {
            return url;
        }
    }
    DEFAULT_PLUGIN_INDEX_BASE.to_string()
}

fn index_url(base: &str) -> String {
    format!(
        "{}/{}/index.json",
        base.trim_end_matches('/'),
        PLUGIN_ABI_VERSION
    )
}

fn index_cache_path() -> PathBuf {
    get_cache_dir().join(format!("index-v{PLUGIN_ABI_VERSION}.json"))
}

// ---------------------------------------------------------------------------
// Subcommand handlers
// ---------------------------------------------------------------------------

/// `treetags plugin available` — list plugins downloadable for this ABI.
pub fn available(base: &str, refresh: bool, plugins_dir: &Path) -> Result<()> {
    let index = fetch_index(base, refresh)?;
    if index.plugins.is_empty() {
        println!("No plugins available for ABI {PLUGIN_ABI_VERSION}.");
        return Ok(());
    }
    let installed = installed_plugins(plugins_dir);

    let rows: Vec<[String; 5]> = index
        .plugins
        .iter()
        .map(|p| {
            let status = match installed.get(&p.name) {
                None => String::new(),
                Some(local) => status_label(&local.version, &p.version),
            };
            [
                p.name.clone(),
                p.version.clone(),
                p.language.clone().unwrap_or_default(),
                p.extensions.join(", "),
                status,
            ]
        })
        .collect();

    print_table(
        &["NAME", "VERSION", "LANGUAGE", "EXTENSIONS", "STATUS"],
        &rows,
    );
    Ok(())
}

/// `treetags plugin install <name>` — download, verify, and place a plugin.
pub fn install(
    base: &str,
    name: &str,
    force: bool,
    refresh: bool,
    plugins_dir: &Path,
) -> Result<()> {
    let index = fetch_index(base, refresh)?;
    let entry = find_entry(&index, name)?;

    if !force {
        if let Some(local) = installed_plugins(plugins_dir).get(name) {
            match cmp_versions(&local.version, &entry.version) {
                Some(Ordering::Equal) => {
                    println!(
                        "`{name}` {} is already installed and up to date.",
                        local.version
                    );
                    return Ok(());
                }
                Some(Ordering::Greater) => {
                    println!(
                        "`{name}` {} is newer than the available {} — keeping it (use --force to override).",
                        local.version, entry.version
                    );
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    let dir = download_and_place(entry, plugins_dir)?;
    println!(
        "Installed `{}` {} to {}",
        entry.name,
        entry.version,
        dir.display()
    );
    Ok(())
}

/// `treetags plugin uninstall <name>` — remove a locally installed plugin.
pub fn uninstall(name: &str, plugins_dir: &Path) -> Result<()> {
    match installed_plugins(plugins_dir).get(name) {
        None => {
            println!("`{name}` is not installed under {}", plugins_dir.display());
            Ok(())
        }
        Some(local) => {
            std::fs::remove_dir_all(&local.dir)
                .with_context(|| format!("removing {}", local.dir.display()))?;
            println!("Uninstalled `{name}` from {}", local.dir.display());
            Ok(())
        }
    }
}

/// `treetags plugin update [name]` — reinstall installed plugins whose available
/// version is newer. With no name, updates all installed plugins.
pub fn update(base: &str, name: Option<&str>, refresh: bool, plugins_dir: &Path) -> Result<()> {
    let installed = installed_plugins(plugins_dir);
    if installed.is_empty() {
        println!("No plugins installed under {}.", plugins_dir.display());
        return Ok(());
    }
    if let Some(n) = name {
        if !installed.contains_key(n) {
            bail!("`{n}` is not installed under {}", plugins_dir.display());
        }
    }

    let index = fetch_index(base, refresh)?;
    let mut updated = 0;
    for (pname, local) in &installed {
        if let Some(filter) = name {
            if filter != pname {
                continue;
            }
        }
        let Some(entry) = index.plugins.iter().find(|p| &p.name == pname) else {
            continue;
        };
        if cmp_versions(&local.version, &entry.version) == Some(Ordering::Less) {
            download_and_place(entry, plugins_dir)?;
            println!("Updated `{pname}` {} -> {}", local.version, entry.version);
            updated += 1;
        }
    }
    if updated == 0 {
        println!("All plugins up to date.");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Index fetching + caching
// ---------------------------------------------------------------------------

/// Fetches this ABI's index, using a short-lived on-disk cache. On a network
/// failure it falls back to a stale cache if one exists.
fn fetch_index(base: &str, refresh: bool) -> Result<PluginIndex> {
    let cache = index_cache_path();
    if !refresh {
        if let Some(index) = read_cache(&cache, true) {
            return Ok(index);
        }
    }

    let url = index_url(base);
    match http_get_string(&url) {
        Ok(body) => {
            let index: PluginIndex = serde_json::from_str(&body)
                .with_context(|| format!("parsing plugin index from {url}"))?;
            if index.abi_version != PLUGIN_ABI_VERSION {
                bail!(
                    "index at {url} targets ABI {} but this treetags supports ABI {PLUGIN_ABI_VERSION}",
                    index.abi_version
                );
            }
            let _ = write_cache(&cache, &body);
            Ok(index)
        }
        Err(e) => match read_cache(&cache, false) {
            Some(index) => {
                eprintln!("warning: could not fetch {url} ({e}); using cached index");
                Ok(index)
            }
            None => Err(e.context(format!("fetching plugin index {url}"))),
        },
    }
}

/// Reads the cached index. When `require_fresh`, only returns it if younger than
/// [`INDEX_CACHE_TTL`]; otherwise returns it regardless of age (stale fallback).
fn read_cache(path: &Path, require_fresh: bool) -> Option<PluginIndex> {
    let meta = std::fs::metadata(path).ok()?;
    if require_fresh {
        let age = meta.modified().ok()?.elapsed().unwrap_or(INDEX_CACHE_TTL);
        if age >= INDEX_CACHE_TTL {
            return None;
        }
    }
    let body = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&body).ok()
}

fn write_cache(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, body)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Download + install
// ---------------------------------------------------------------------------

/// Downloads the plugin's `.wasm` (verifying its SHA-256) and manifest, then
/// installs into `plugins_dir/<name>/` via a temp dir + rename so a failed or
/// interrupted download never corrupts an existing install.
fn download_and_place(entry: &IndexEntry, plugins_dir: &Path) -> Result<PathBuf> {
    let wasm = http_get_bytes(&entry.wasm_url)
        .with_context(|| format!("downloading {}", entry.wasm_url))?;
    let got = sha256_hex(&wasm);
    if got != entry.wasm_sha256 {
        bail!(
            "checksum mismatch for `{}`: expected {}, got {}\n\
             The index and asset may be mid-update; retry with `--refresh`.",
            entry.name,
            entry.wasm_sha256,
            got
        );
    }

    let manifest_text = http_get_string(&entry.manifest_url)
        .with_context(|| format!("downloading {}", entry.manifest_url))?;
    let manifest: PluginManifest = toml::from_str(&manifest_text)
        .with_context(|| format!("parsing downloaded manifest for `{}`", entry.name))?;
    if manifest.name != entry.name {
        bail!(
            "downloaded manifest name `{}` does not match requested `{}`",
            manifest.name,
            entry.name
        );
    }
    if manifest.abi_version != PLUGIN_ABI_VERSION {
        bail!(
            "plugin `{}` targets ABI {} but this treetags supports ABI {PLUGIN_ABI_VERSION}",
            entry.name,
            manifest.abi_version
        );
    }

    std::fs::create_dir_all(plugins_dir)
        .with_context(|| format!("creating {}", plugins_dir.display()))?;
    let final_dir = plugins_dir.join(&entry.name);
    let tmp_dir = plugins_dir.join(format!(".{}.tmp", entry.name));

    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir)?;
    std::fs::write(tmp_dir.join("plugin.wasm"), &wasm)?;
    std::fs::write(tmp_dir.join("plugin.toml"), manifest_text.as_bytes())?;

    let _ = std::fs::remove_dir_all(&final_dir);
    std::fs::rename(&tmp_dir, &final_dir)
        .with_context(|| format!("installing into {}", final_dir.display()))?;
    Ok(final_dir)
}

// ---------------------------------------------------------------------------
// Local scanning + helpers
// ---------------------------------------------------------------------------

/// Scans `plugins_dir` (recursively) for installed plugin manifests, keyed by
/// plugin name. Malformed manifests are skipped.
fn installed_plugins(plugins_dir: &Path) -> HashMap<String, InstalledPlugin> {
    let mut out = HashMap::new();
    collect_installed(plugins_dir, &mut out);
    out
}

fn collect_installed(dir: &Path, out: &mut HashMap<String, InstalledPlugin>) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_installed(&path, out);
        } else if path
            .file_name()
            .map(|n| n == "plugin.toml")
            .unwrap_or(false)
        {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(m) = toml::from_str::<PluginManifest>(&text) {
                    out.insert(
                        m.name,
                        InstalledPlugin {
                            version: m.version,
                            dir: path.parent().unwrap_or(dir).to_path_buf(),
                        },
                    );
                }
            }
        }
    }
}

fn find_entry<'a>(index: &'a PluginIndex, name: &str) -> Result<&'a IndexEntry> {
    index
        .plugins
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| {
            anyhow!(
                "no plugin named `{name}` available for ABI {PLUGIN_ABI_VERSION}\n\
             Run `treetags plugin available` to see what's installable."
            )
        })
}

/// Compares two version strings as semver. Returns `None` if either is unparseable.
fn cmp_versions(local: &str, remote: &str) -> Option<Ordering> {
    let l = semver::Version::parse(local).ok()?;
    let r = semver::Version::parse(remote).ok()?;
    Some(l.cmp(&r))
}

fn status_label(local: &str, remote: &str) -> String {
    match cmp_versions(local, remote) {
        Some(Ordering::Less) => format!("update: {local} -> {remote}"),
        Some(Ordering::Greater) => format!("installed ({local}, newer)"),
        Some(Ordering::Equal) => "installed".to_string(),
        None => format!("installed ({local})"),
    }
}

fn print_table(headers: &[&str; 5], rows: &[[String; 5]]) {
    let mut widths = [0usize; 5];
    for (i, h) in headers.iter().enumerate() {
        widths[i] = h.len();
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }
    // Last column needs no trailing padding.
    let render = |cells: &[String; 5]| {
        let mut line = String::new();
        for (i, cell) in cells.iter().enumerate() {
            if i + 1 == cells.len() {
                line.push_str(cell);
            } else {
                line.push_str(&format!("{:<width$}    ", cell, width = widths[i]));
            }
        }
        line.trim_end().to_string()
    };
    let header_row: [String; 5] = std::array::from_fn(|i| headers[i].to_string());
    println!("{}", render(&header_row));
    for row in rows {
        println!("{}", render(row));
    }
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

fn http_get_string(url: &str) -> Result<String> {
    let resp = ureq::get(url).call().map_err(map_ureq_err)?;
    resp.into_string()
        .map_err(|e| anyhow!("reading response body from {url}: {e}"))
}

fn http_get_bytes(url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url).call().map_err(map_ureq_err)?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| anyhow!("reading response body from {url}: {e}"))?;
    Ok(buf)
}

fn map_ureq_err(e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::Status(404, resp) => {
            anyhow!("not found (HTTP 404) at {}", resp.get_url())
        }
        ureq::Error::Status(code, resp) => anyhow!("HTTP {code} at {}", resp.get_url()),
        ureq::Error::Transport(t) => anyhow!("network error: {t}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_precedence_prefers_override() {
        assert_eq!(resolve_base_url(Some("https://x/y")), "https://x/y");
        // Empty override is ignored (falls through).
        let resolved = resolve_base_url(Some(""));
        assert!(resolved == DEFAULT_PLUGIN_INDEX_BASE || resolved.starts_with("http"));
    }

    #[test]
    fn index_url_includes_abi_bucket() {
        assert_eq!(
            index_url("https://example.com/treetags/"),
            format!("https://example.com/treetags/{PLUGIN_ABI_VERSION}/index.json")
        );
    }

    #[test]
    fn status_label_reflects_version_relationship() {
        assert_eq!(status_label("0.1.0", "0.2.0"), "update: 0.1.0 -> 0.2.0");
        assert_eq!(status_label("0.2.0", "0.2.0"), "installed");
        assert_eq!(status_label("0.3.0", "0.2.0"), "installed (0.3.0, newer)");
        assert_eq!(status_label("weird", "0.2.0"), "installed (weird)");
    }
}
