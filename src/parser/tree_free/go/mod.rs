//! Tree-free Go backend: the token-stream hook ([`hooks`]) and its declaration
//! helpers ([`syntax`]) over the generated Go lexer ([`generated`]).
#[allow(dead_code)] // Generated lexicon exposes every grammar token.
pub(crate) mod generated;
mod hooks;
mod syntax;

pub(crate) use hooks::generate;
