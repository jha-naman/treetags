use std::path::Path;
use wasmtime::component::{Component, Linker};
use wasmtime::Engine;

use super::instance::{new_store, PluginState, WasmInstance};

/// The compiled component for one plugin, loaded exactly once.
/// All worker threads share one `SharedPlugin` and each creates a
/// per-thread `WasmInstance` from it via `create_instance()`.
pub struct SharedPlugin {
    pub engine: Engine,
    pub component: Component,
    pub linker: Linker<PluginState>,
}

impl SharedPlugin {
    /// Loads and JIT-compiles the `.wasm` component file (done once).
    pub fn from_file(engine: &Engine, path: &Path) -> anyhow::Result<Self> {
        let engine = engine.clone();
        let component = Component::from_file(&engine, path)
            .map_err(|e| anyhow::anyhow!("load component {}: {e}", path.display()))?;

        let mut linker: Linker<PluginState> = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;

        Ok(Self {
            engine,
            component,
            linker,
        })
    }

    /// Creates a new per-thread execution context (own linear memory, own call stack).
    /// The compiled component code is shared — no re-JIT per thread.
    pub fn create_instance(&self) -> anyhow::Result<WasmInstance> {
        let store = new_store(&self.engine);
        WasmInstance::from_component(store, &self.component, &self.linker)
    }
}
