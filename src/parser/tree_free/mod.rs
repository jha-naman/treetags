//! Tree-free tag backends: forward-only, token-stream hooks over generated
//! lexers, sharing the runtime in [`common`]. Each language lives in its own
//! directory (`c/`, `go/`, …) holding its hook, syntax helpers, and generated
//! lexer; `common/` holds the cursor/emitter/scanner primitives they share.
pub(crate) mod common;

pub(crate) mod c;
pub(crate) mod go;
