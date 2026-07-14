/// ABI version implemented by this SDK version.
/// Must match `PLUGIN_ABI_VERSION` in the treetags host (`src/plugin/mod.rs`).
/// Bump this (and the host constant) whenever the WIT interface changes.
pub const ABI_VERSION: u32 = 3;

pub mod tag_config;
pub use tag_config::TagKindConfig;

#[cfg(feature = "tree-walker")]
pub mod tree_walker;
#[cfg(feature = "tree-walker")]
pub use tree_walker::{walk_tree, WalkContext};
