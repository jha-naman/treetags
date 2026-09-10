//! Shared runtime for the tree-free backends: the forward-only token cursor and
//! delimiter map ([`linear`]), the generated-lexicon scan loop ([`scanner`]),
//! and the tag/extension-field emitter ([`tag_emitter`]).
pub(crate) mod linear;
pub(crate) mod scanner;
pub(crate) mod tag_emitter;
