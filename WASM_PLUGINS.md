# WASM PLUGINS

## Installing WASM plugins

Build the plugin using the provided helper binary from the treetags root directory.
You need to fulfill the requirements for building treetags documented in
[README.md](README.md#installation)

```
cargo run --bin treetags-build-plugin -- plugins/java
```

Copy the plugins to the default location treetags looks for plugins and then
your plugin will be picked up by treetags on next runs.

```
cp -r plugins/java/dist/java ~/.config/treetags/plugins/
```

Use `--plugin-dir` or `--plugins-dir` if you want plugins to be located in
another location.

## Plugin Interface

A treetags plugin is a __directory__ containing two files, each serving a specific
purpose.

### Plugin WASM file

This is a WebAssembly component that satisfies the WIT interface defined
[here](wit/treetags-plugin.wit). This being a WASM file can be written in any
language that supports creating WebAssembly Components. The Java plugin
implementation in `plugins/java` should be considered as a reference implementation
of what treetags expects from a plugin.

The `generate` function exposed by the component is passed the information it
requires for generating tags for a single source code file. Refer to the `request`
record in the wit file for details. Treetags passes in the the file path of the
source code file relative to project root, the value of `--kinds-{lang}` cli
argument passed by the user (`lang` is picked up from the TOML file supplied as
part of the plugin), value of `extras` cli argument and value of `fields` cli
argument passed by the user. The plugin is expected to return an array of the
`tag` record described in the wit file as well as a string containing description
of any errors. The WASM plugin gets access to the systems `stderr` stream and
nothing else on the system.

### `plugin.toml` file

This is a TOML file containing data about the plugin that treetags uses for
parsing command line flags and for deciding whether the WASM plugin needs to
be compiled for any of the files being processed by treetags. The WASM plugin is
compiled lazily only once it is needed. See [plugins_toml_schema.md](PLUGINS_TOML_SCHEMA.md)
for details on this file.

## Plugin Host Implementation

Treetags looks for plugins at three places in following order or priority:

- It will try to load directory passed via `--plugin-dir` as a plugin
- It will look for all directories which are treetags plugins under directory
  passed via `--plugin-dirs`
- It will look for all directories which are treetags plugins under
  `~/.config/treetags/plugins/` by default, unless the `--plugin-dirs` option
  is passed by the user.

All the plugins found by treetags are loaded into the `PluginRegistry`. The
registry JIT does not initially compile the WASM plugin. The compilation
happens only once on the first call to `try_generate`. Each worker thread that
processes a file handled by the plugin creates a `WasmInstance` from the
`SharedPlugin`.

