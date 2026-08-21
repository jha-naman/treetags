//! Table definitions and Tag rendering for generated C-family parsers. The
//! parsing itself is the single-pass engine in [`super::island`]; this module
//! defines the data-table types those parsers embed and turns the engine's
//! candidates into [`Tag`]s. It has no dependency on Tree-sitter.
//!
//! Some table types (patterns/actions/matchers/…) are the compiled-query
//! representation the generator derives `SPECIFIERS`/`ROLES` from; they are
//! embedded for provenance but not all fields are read at runtime.
#![allow(dead_code)]
use crate::tag::{ExtensionFields, Tag};
use std::{borrow::Cow, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub(crate) struct KindSpec {
    pub letter: &'static str,
    pub name: &'static str,
    pub default_enabled: bool,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct CaptureSpec {
    pub name: &'static str,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct RootIndex {
    pub kind: &'static str,
    pub pattern_start: usize,
    pub pattern_len: usize,
}
/// Keyword-introduced construct dispatch, emitted from grammar + tags + parse.json.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) struct SpecifierSpec {
    /// Leading keyword (grammar FIRST terminal).
    pub keyword: &'static str,
    /// Structural handler category (from parse.json `dispatch`).
    pub category: &'static str,
    /// ctags kind letter (from the tag action).
    pub letter: &'static str,
    /// Scope kind pushed by this construct, if any (from `#tt-enter-scope!`).
    pub scope: Option<&'static str>,
    /// Field format for an enum base type, if any.
    pub base_format: Option<&'static str>,
    /// Kind id used when synthesizing anonymous names (from `#tt-anonymous!`).
    pub anon_id: u8,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct PatternSpec {
    pub root_kind: &'static str,
    pub name_capture: &'static str,
    pub match_start: usize,
    pub match_len: usize,
    pub action_start: usize,
    pub action_len: usize,
    pub predicate_start: usize,
    pub predicate_len: usize,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct MatchSpec {
    pub field: &'static str,
    pub kind: Option<&'static str>,
    pub absent: bool,
}
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) enum PredicateSpec {
    Eq {
        capture: &'static str,
        value: &'static str,
        positive: bool,
    },
    Match {
        capture: &'static str,
        regex: &'static str,
        positive: bool,
    },
    AnyOf {
        capture: &'static str,
        values: &'static [&'static str],
        positive: bool,
    },
}
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) enum ActionSpec {
    SelectKind(&'static str),
    ResolveName {
        capture: &'static str,
        resolver: &'static str,
    },
    Transform {
        capture: &'static str,
        transform: &'static str,
    },
    EnterScope {
        capture: &'static str,
        scope: &'static str,
    },
    Field {
        field: &'static str,
        capture: &'static str,
        format: &'static str,
    },
    ConditionalKind {
        condition: &'static str,
        then_kind: &'static str,
        else_kind: &'static str,
    },
    SkipIf {
        capture: &'static str,
        condition: &'static str,
    },
    Anonymous {
        target: &'static str,
        kind_id: u8,
        prefix: &'static str,
    },
    Emit {
        each: bool,
    },
}
#[derive(Debug)]
pub(crate) struct LanguageTables {
    pub name: &'static str,
    pub kinds: &'static [KindSpec],
    pub roots: &'static [RootIndex],
    pub patterns: &'static [PatternSpec],
    pub matchers: &'static [MatchSpec],
    pub captures: &'static [CaptureSpec],
    pub predicates: &'static [PredicateSpec],
    pub actions: &'static [ActionSpec],
    pub syntax_nodes: &'static [&'static str],
    pub grammar_rules: &'static [&'static str],
    pub specifiers: &'static [SpecifierSpec],
    pub roles: &'static [(&'static str, &'static str)],
    pub storage_roles: &'static [(&'static str, &'static str)],
}

pub(crate) fn generate(
    t: &'static LanguageTables,
    code: &[u8],
    path: &str,
    kinds: &crate::parser::TagKindConfig,
    config: &crate::config::Config,
) -> Option<Vec<Tag>> {
    let source = std::str::from_utf8(code).ok()?;
    let parsed = super::island::scan(t, source, path, kinds);
    Some(render(parsed, source, path, kinds, config))
}

pub(crate) struct Candidate {
    pub name: String,
    pub kind: &'static str,
    pub row: usize,
    pub fields: Vec<(&'static str, String)>,
}


pub(crate) fn render(
    candidates: Vec<Candidate>,
    source: &str,
    path: &str,
    kinds: &crate::parser::TagKindConfig,
    config: &crate::config::Config,
) -> Vec<Tag> {
    let lines: Vec<_> = source.lines().collect();
    let file_name: Arc<str> = Arc::from(path);
    let mut tags = Vec::new();
    for candidate in candidates {
        if !kinds.is_kind_enabled(candidate.kind) {
            continue;
        }
        let mut address = "/^".to_owned();
        Tag::escape_address_into(
            lines.get(candidate.row).copied().unwrap_or(""),
            &mut address,
        );
        address.push_str("$/;\"");
        let mut fields = ExtensionFields::new();
        if config.fields_config.is_field_enabled("kind") {
            fields.insert("kind", candidate.kind)
        }
        if config.fields_config.is_field_enabled("line") {
            fields.insert("line", (candidate.row + 1).to_string())
        }
        for (key, value) in candidate.fields {
            if key == "typeref"
                || config.fields_config.is_field_enabled("scope")
                || config.extras_config.qualified
            {
                fields.insert(key, value)
            }
        }
        tags.push(Tag {
            name: candidate.name,
            file_name: file_name.clone(),
            address,
            kind: Some(Cow::Borrowed(candidate.kind)),
            extension_fields: (!fields.is_empty()).then_some(fields),
        })
    }
    tags
}
