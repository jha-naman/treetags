use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::Store;

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

pub fn new_store(engine: &wasmtime::Engine) -> Store<PluginState> {
    let state = PluginState {
        ctx: wasmtime_wasi::WasiCtxBuilder::new()
            .inherit_stderr()
            .build(),
        table: ResourceTable::new(),
    };
    Store::new(engine, state)
}
