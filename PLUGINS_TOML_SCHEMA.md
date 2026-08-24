# `plugin.toml` Schema

## Top-level fields

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `name` | string | yes | — | Plugin identifier (e.g. `"java"`) |
| `version` | string | yes | — | Semver version string (e.g. `"0.2.0"`) |
| `abi_version` | integer | no | — | ABI version the plugin targets; must match the `PLUGIN_ABI_VERSION` in `src/plugin/mod.rs` |
| `extensions` | array of strings | yes | — | Every file extension this plugin handles (e.g. `["java"]`), including any content-disambiguated ones listed under `[disambiguation]` |
| `language` | string | no | — | Language name used to match `--kinds-{lang}=fn` CLI argument and `--language-force` |
| `aliases` | array of strings | no | `[]` | Additional names accepted by `--language-force` for this plugin's language |
| `patterns` | array of strings | no | `[]` | `fnmatch`-style filename globs (matched against the basename) that select this plugin, e.g. `Dockerfile` or `*.bzl` |
| `interpreters` | array of strings | no | `[]` | Interpreter names matched against a `#!` shebang line, e.g. `node` (used only when the file name gives no match and shebang guessing is enabled) |
| `wasm_file` | string | no | `"plugin.wasm"` | Path to the `.wasm` component file, relative to this manifest file. `treetags-build-plugin` sets it to `plugin.wasm` explicitly|
| `[[kinds]]` | array of `Kind` | no | — | Tag kinds the plugin can generate; used for `--list-kinds` output |
| `[disambiguation]` | table of `ext -> signals` | no | — | Content-disambiguation signals for extensions shared with other languages (e.g. `h`). See below |

## `[[kinds]]` fields

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `letter` | string | yes | — | Single-character kind letter (e.g. `"m"` for method) |
| `name` | string | yes | — | Human readable kind name (e.g. `"method"`) |
| `default` | boolean | no | `true` | Whether this kind is emitted by default or only when explicitly requested |

## `[disambiguation]` table

A table mapping an **extension** (which must also appear in top-level
`extensions`) to an **array of signal strings** — marker substrings that identify
this language. The plugin is selected for that shared-extension file only when one
of the signals appears in the file's content prefix.

Every extension in `extensions` is claimed **outright** (the plugin is the sole
owner) *unless* it also appears as a key under `[disambiguation]`, in which case
it is claimed **only when the content matches**. Such extensions can have a *base
owner* (a language that claims them outright via its own `extensions`, which is
also the default when no signals match) plus one or more *signal-gated
candidates*.

For example `.h` is shared by C, C++, and Objective-C. C owns it outright (its
base owner / default); C++ and an Objective-C plugin declare it under
disambiguation with their own signals. Treetags reads an 8 KB content prefix and
picks the first candidate whose signals appear; if none do, it falls back to the
base owner (C). Plugin candidates are tried before built-in ones. Built-in
languages declare the equivalent signals via the `disambiguation` field of their
`BuiltinLangDesc` (`src/builtin_langs.rs`), so plugins and native parsers are
resolved identically.

```toml
# Objective-C: handles `.m` and `.h`; `.m` outright, `.h` only on ObjC markers.
extensions = ["m", "h"]

[disambiguation]
h = ["@interface", "@implementation", "@protocol", "@import"]
```

## Notes

- `wasm_file` defaults to `"plugin.wasm"`. The `dist/` copy written by
`treetags-build-plugin` binary sets it explicitly.
- `language` is used to control the plugin's kinds via `---kinds-{lang}=...` CLI argument.
  It (and any `aliases`) also lets `--language-force=<language>` route every input
  file through this plugin, matching how built-in languages work.
