use std::path::Path;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::Store;
use wasmtime_wasi::{DirPerms, FilePerms};

wasmtime::component::bindgen!({
    world: "plugin-world",
    path: "wit",
});

pub use exports::treetags::plugin::plugin::{Request, Tag};

pub struct PluginState {
    ctx: wasmtime_wasi::WasiCtx,
    table: ResourceTable,
}

impl wasmtime_wasi::WasiView for PluginState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

pub struct WasmInstance {
    store: Store<PluginState>,
    plugin: PluginWorld,
}

impl WasmInstance {
    pub fn from_component(
        mut store: Store<PluginState>,
        component: &Component,
        linker: &Linker<PluginState>,
    ) -> anyhow::Result<Self> {
        let plugin = PluginWorld::instantiate(&mut store, component, linker)?;
        Ok(Self { store, plugin })
    }

    /// Calls the plugin's `generate` export.
    pub fn generate(
        &mut self,
        req: &Request,
        source: &[u8],
    ) -> anyhow::Result<Result<Vec<Tag>, String>> {
        self.plugin
            .treetags_plugin_plugin()
            .call_generate(&mut self.store, req, source)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

pub fn new_store(engine: &wasmtime::Engine, cache_dir: Option<&Path>) -> Store<PluginState> {
    let mut builder = wasmtime_wasi::WasiCtxBuilder::new();
    builder.inherit_stderr();
    if let Some(dir) = cache_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("treetags: cannot create cache dir {}: {e}", dir.display());
        } else if let Err(e) = builder.preopened_dir(dir, ".", DirPerms::all(), FilePerms::all()) {
            eprintln!("treetags: cannot preopen cache dir {}: {e}", dir.display());
        }
    }
    let state = PluginState {
        ctx: builder.build(),
        table: ResourceTable::new(),
    };
    Store::new(engine, state)
}
