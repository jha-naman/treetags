pub mod client;
// Consumed by the treetags-build-site bin and the plugin-install client; a few
// items (AbisFile/merged) are only used by the bin, so they read as dead from
// the main treetags binary's copy of this module tree.
#[allow(dead_code)]
pub mod index;
pub(crate) mod instance;
pub mod manifest;
pub mod registry;
mod shared;

pub use registry::print_plugin_list;
#[allow(unused_imports)]
pub use registry::PluginRegistry;

/// ABI version this build of treetags accepts from WASM plugins.
/// Bump this whenever the WIT interface (wit/treetags-plugin.wit) changes
/// in a backwards-incompatible way, and update the constant in plugins/common.
pub const PLUGIN_ABI_VERSION: u32 = 3;
