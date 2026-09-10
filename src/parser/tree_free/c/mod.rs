//! Tree-free C backend: the token-stream hook ([`hooks`]) over the generated C
//! lexer ([`generated`]), including the preprocessor external lexer.
#[allow(dead_code)] // Generated lexicon exposes every grammar token.
pub(crate) mod generated;
mod hooks;

// `generate` is used by the corpus tests; `generate_builtin` under `native-c`.
#[allow(unused_imports)]
pub(crate) use hooks::{generate, generate_builtin};
