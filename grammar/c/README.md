# Pinned C grammar

`grammar.js` is the canonical source shipped by `tree-sitter-c` 0.23.4.
Its SHA-256 is recorded in the generated Rust file. Do not edit generated
scanner tables directly.

Regenerate with `tree-sitter 0.25.10` and Node 22.17.0:

```sh
TREETAGS_NODE=/absolute/path/to/node-v22.17.0 cargo run --bin generate-scanner -- --lang c
```

Generation inputs live in `scanner.toml`. Append `-- --check` for a
byte-for-byte freshness check. The temporary `grammar.json` is used only during
generation and is deleted afterward.
