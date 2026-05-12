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
pub const PLUGIN_ABI_VERSION: u32 = 2;
