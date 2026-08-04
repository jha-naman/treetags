//! Echo plugin — an internal dev/test fixture, not an end-user plugin.
//!
//! It exists because it exercises plugin plumbing that the real Java plugin
//! cannot:
//!   * the `--plugin-cache` capability — it writes to `req.cache_file`, and is
//!     the only end-to-end test of the WASI preopened cache-file path;
//!   * the no-C-toolchain build path — it is pure Rust with no tree-sitter, so
//!     it builds with just the `wasm32-wasip2` target and needs no WASI SDK
//!     (the Java plugin's C code does), giving contributors without the SDK a
//!     working plugin to test against;
//!   * the raw `Guest` trait surface — it does not use `common`'s `tree-walker`
//!     feature, unlike Java.
//!
//! It emits a fixed `echo_tag` regardless of input, which keeps its integration
//! test deterministic. Because it does nothing useful for real source files, its
//! `plugin.toml` sets `internal = true`, hiding it from `--list-plugins` and
//! excluding it from the published distribution index.

wit_bindgen::generate!({
    world: "plugin-world",
    path: "../../wit",
});

use exports::treetags::plugin::plugin::{Guest, Request, Tag};

struct EchoPlugin;

impl Guest for EchoPlugin {
    fn generate(req: Request, _source: Vec<u8>) -> Result<Vec<Tag>, String> {
        if let Some(cache_name) = req.cache_file {
            std::fs::write(&cache_name, "echo_cache_written\n")
                .map_err(|e| format!("cache write error: {e}"))?;
        }
        Ok(vec![Tag {
            name: "echo_tag".into(),
            line: 1,
            kind: "f".into(),
            end_line: None,
            extension_fields: vec![],
        }])
    }
}

export!(EchoPlugin);
