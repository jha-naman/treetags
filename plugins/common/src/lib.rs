/// ABI version implemented by this SDK version.
/// Must match `PLUGIN_ABI_VERSION` in the treetags host (`src/plugin/mod.rs`).
/// Bump this (and the host constant) whenever the WIT interface changes.
pub const ABI_VERSION: u32 = 3;

pub mod tag_config;
pub use tag_config::TagKindConfig;

pub mod scope;
pub use scope::{ScopeKey, ScopeStack};

#[cfg(feature = "tree-walker")]
pub mod cursor;
#[cfg(feature = "tree-walker")]
pub use cursor::{child_ident, has_child, line_of, node_text};

#[cfg(feature = "tree-walker")]
pub mod tree_walker;
#[cfg(feature = "tree-walker")]
pub use tree_walker::{walk_tree, WalkContext};
