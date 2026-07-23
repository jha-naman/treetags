//! Schema for the plugin distribution index published to GitHub Pages.
//!
//! Each ABI version gets its own `index.json` (one [`IndexEntry`] per plugin,
//! latest compatible version only) plus a shared `abis.json` enumerating which
//! ABI buckets exist. These types are the single source of truth shared by the
//! `treetags-build-site` generator and the client that installs plugins, so
//! the published schema can never drift from what the CLI expects.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::manifest::{ManifestKind, PluginManifest};

/// One tag kind advertised by a plugin, mirrored from its manifest for display
/// on the human-facing site and in `treetags plugin available`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexKind {
    pub letter: String,
    pub name: String,
    pub default: bool,
}

impl From<&ManifestKind> for IndexKind {
    fn from(k: &ManifestKind) -> Self {
        IndexKind {
            letter: k.letter.clone(),
            name: k.name.clone(),
            default: k.default,
        }
    }
}

/// A single plugin's entry in an ABI bucket's `index.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexEntry {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interpreters: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<IndexKind>>,
    /// Absolute URL of the `.wasm` component (a GitHub Release asset).
    pub wasm_url: String,
    /// Absolute URL of the distribution `plugin.toml`.
    pub manifest_url: String,
    /// Lowercase hex SHA-256 of the `.wasm`, verified by the client on download.
    pub wasm_sha256: String,
    pub wasm_size: u64,
    /// Reserved for a future detached signature over the `.wasm` (e.g. if the
    /// project opens to third-party plugins). Absent today: integrity relies on
    /// a trusted local build plus HTTPS + `wasm_sha256`. Kept optional so it can
    /// be populated later without a breaking schema change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm_signature: Option<String>,
}

impl IndexEntry {
    /// Build an entry from a parsed manifest plus the hosting metadata computed
    /// for its `.wasm`. Asset URLs follow the `<base>/<name>.{wasm,toml}`
    /// convention used by the publish workflow's per-ABI release.
    pub fn from_manifest(
        manifest: &PluginManifest,
        asset_base_url: &str,
        wasm_sha256: String,
        wasm_size: u64,
    ) -> Self {
        let base = asset_base_url.trim_end_matches('/');
        IndexEntry {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            language: manifest.language.clone(),
            extensions: manifest.extensions.clone(),
            aliases: manifest.aliases.clone(),
            patterns: manifest.patterns.clone(),
            interpreters: manifest.interpreters.clone(),
            kinds: manifest
                .kinds
                .as_ref()
                .map(|ks| ks.iter().map(IndexKind::from).collect()),
            wasm_url: format!("{base}/{}.wasm", manifest.name),
            manifest_url: format!("{base}/{}.toml", manifest.name),
            wasm_sha256,
            wasm_size,
            wasm_signature: None,
        }
    }
}

/// The `index.json` for one ABI bucket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginIndex {
    pub abi_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    pub plugins: Vec<IndexEntry>,
}

/// The root `abis.json`, letting the human site and tooling enumerate buckets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AbisFile {
    /// All published ABI versions, sorted descending (newest first).
    pub abis: Vec<u32>,
    /// Highest published ABI version.
    pub latest: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
}

impl AbisFile {
    /// Merge `abi` into an existing (possibly absent) `abis.json`, keeping the
    /// list deduplicated and sorted newest-first. This is how a publish run for
    /// one ABI records itself without disturbing older buckets.
    pub fn merged(existing: Option<AbisFile>, abi: u32, generated_at: Option<String>) -> Self {
        let mut abis: Vec<u32> = existing.map(|e| e.abis).unwrap_or_default();
        if !abis.contains(&abi) {
            abis.push(abi);
        }
        abis.sort_unstable();
        abis.dedup();
        abis.reverse();
        let latest = abis.iter().copied().max().unwrap_or(abi);
        AbisFile {
            abis,
            latest,
            generated_at,
        }
    }
}

/// Lowercase hex SHA-256 of `bytes` — the integrity value stored in the index
/// and re-checked by the client after downloading a `.wasm`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> PluginManifest {
        toml::from_str(
            r#"
            name = "java"
            version = "0.2.0"
            abi_version = 3
            extensions = ["java"]
            language = "java"
            aliases = ["jvm"]

            [[kinds]]
            letter = "m"
            name = "method"

            [[kinds]]
            letter = "l"
            name = "local"
            default = false
            "#,
        )
        .unwrap()
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn entry_maps_manifest_and_builds_urls() {
        let m = manifest();
        let entry = IndexEntry::from_manifest(
            &m,
            "https://example.com/releases/download/plugin-store-v3/",
            "deadbeef".to_string(),
            42,
        );
        assert_eq!(entry.name, "java");
        assert_eq!(entry.version, "0.2.0");
        assert_eq!(entry.language.as_deref(), Some("java"));
        assert_eq!(entry.extensions, vec!["java"]);
        assert_eq!(entry.aliases, vec!["jvm"]);
        // Trailing slash on the base is trimmed exactly once.
        assert_eq!(
            entry.wasm_url,
            "https://example.com/releases/download/plugin-store-v3/java.wasm"
        );
        assert_eq!(
            entry.manifest_url,
            "https://example.com/releases/download/plugin-store-v3/java.toml"
        );
        assert_eq!(entry.wasm_sha256, "deadbeef");
        assert_eq!(entry.wasm_size, 42);
        let kinds = entry.kinds.unwrap();
        assert_eq!(kinds.len(), 2);
        assert!(kinds[0].default, "kinds default to true when omitted");
        assert!(!kinds[1].default);
    }

    #[test]
    fn index_round_trips_through_json() {
        let m = manifest();
        let index = PluginIndex {
            abi_version: 3,
            generated_at: Some("2026-07-21T00:00:00Z".to_string()),
            plugins: vec![IndexEntry::from_manifest(&m, "https://x/y", "ab".into(), 1)],
        };
        let json = serde_json::to_string(&index).unwrap();
        let back: PluginIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(index, back);
    }

    #[test]
    fn abis_merge_dedups_sorts_and_tracks_latest() {
        let a = AbisFile::merged(None, 3, None);
        assert_eq!(a.abis, vec![3]);
        assert_eq!(a.latest, 3);

        let b = AbisFile::merged(Some(a.clone()), 4, None);
        assert_eq!(b.abis, vec![4, 3]);
        assert_eq!(b.latest, 4);

        // Re-publishing an existing ABI is idempotent.
        let c = AbisFile::merged(Some(b.clone()), 3, None);
        assert_eq!(c.abis, vec![4, 3]);
        assert_eq!(c.latest, 4);
    }
}
