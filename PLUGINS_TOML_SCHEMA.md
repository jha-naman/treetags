# `plugin.toml` Schema

## Top-level fields

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `name` | string | yes | — | Plugin identifier (e.g. `"java"`) |
| `version` | string | yes | — | Semver version string (e.g. `"0.2.0"`) |
| `abi_version` | integer | no | — | ABI version the plugin targets; must match the `PLUGIN_ABI_VERSION` in `src/plugin/mod.rs` |
| `extensions` | array of strings | yes | — | File extensions this plugin handles (e.g. `["java"]`) |
| `language` | string | no | — | Language name used to match `--kinds-{lang}=fn` CLI argument |
| `wasm_file` | string | no | `"plugin.wasm"` | Path to the `.wasm` component file, relative to this manifest file. `treetags-build-plugin` sets it to `plugin.wasm` explicitly|
| `[[kinds]]` | array of `Kind` | no | — | Tag kinds the plugin can generate; used for `--list-kinds` output |

## `[[kinds]]` fields

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `letter` | string | yes | — | Single-character kind letter (e.g. `"m"` for method) |
| `name` | string | yes | — | Human readable kind name (e.g. `"method"`) |
| `default` | boolean | no | `true` | Whether this kind is emitted by default or only when explicitly requested |

## Notes

- `wasm_file` defaults to `"plugin.wasm"`. The `dist/` copy written by
`treetags-build-plugin` binary sets it explicitly.
- `language` is used to control the plugin's kinds via `---kinds-{lang}=...` CLI argument.
