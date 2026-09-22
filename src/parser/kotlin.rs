//! Native Kotlin tag generation using the external WASM grammar.
use super::common::{
    cursor::{child_ident, line_of},
    scope::{ScopeKey, ScopeStack},
    scope_walker::{walk_tree, WalkContext},
    tree_walker::{generate_tags_with_config, Context},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use tree_sitter::{Parser as TsParser, TreeCursor};

pub(crate) const LANG_NAME: &str = "kotlin";
pub(crate) const LANG_EXTENSIONS: &[&str] = &["kt", "kts"];
pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["C", "constant"], "C"),
    (&["T", "typealias"], "T"),
    (&["c", "class"], "c"),
    (&["i", "interface"], "i"),
    (&["m", "method"], "m"),
    (&["o", "object"], "o"),
    (&["p", "package"], "p"),
    (&["v", "variable"], "v"),
];
pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] = &[];

#[derive(Clone, Copy)]
enum ScopeKind {
    Class,
    Interface,
    Object,
    Method,
    PendingLambda,
}

impl ScopeKey for ScopeKind {
    fn key(self) -> &'static str {
        match self {
            ScopeKind::Class => "class",
            ScopeKind::Interface => "interface",
            ScopeKind::Object => "object",
            ScopeKind::Method | ScopeKind::PendingLambda => "method",
        }
    }
}

struct KotlinWalker<'src> {
    base: Context<'src>,
    scopes: ScopeStack<ScopeKind>,
}

impl WalkContext for KotlinWalker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        let source = self.base.source_code.as_bytes();
        process_node_inner(source, cursor, self)
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

fn has_child(cursor: &mut TreeCursor, kinds: &[&str]) -> bool {
    let mut found = false;
    for_each_child!(cursor, {
        if kinds.contains(&cursor.node().kind()) {
            found = true;
            break;
        }
    });
    found
}

/// `"C"` for `val` / `"v"` for `var`, read from a `binding_pattern_kind` child.
/// Cursor restored.
fn binding_kind(cursor: &mut TreeCursor, context: &Context<'_>) -> Option<&'static str> {
    let mut result = None;
    for_each_child!(cursor, {
        let child = cursor.node();
        if child.kind() == "binding_pattern_kind" {
            result = match context.node_text(&child).trim() {
                "val" => Some("C"),
                "var" => Some("v"),
                _ => None,
            };
            break;
        }
    });
    result
}

/// Collect `(name, line)` for every identifier bound by the first
/// `variable_declaration`/`multi_variable_declaration` child of the current node,
/// along with whether it was a destructuring (`multi_*`). Cursor restored.
fn collect_bindings(cursor: &mut TreeCursor, source: &[u8]) -> Option<(bool, Vec<(String, u32)>)> {
    let mut result = None;
    for_each_child!(cursor, {
        match cursor.node().kind() {
            "variable_declaration" => {
                let mut names = Vec::new();
                if let Some(id) = child_ident(cursor, source, &["simple_identifier"]) {
                    names.push(id);
                }
                result = Some((false, names));
                break;
            }
            "multi_variable_declaration" => {
                let mut names = Vec::new();
                for_each_child!(cursor, {
                    if cursor.node().kind() == "variable_declaration" {
                        if let Some(id) = child_ident(cursor, source, &["simple_identifier"]) {
                            names.push(id);
                        }
                    }
                });
                result = Some((true, names));
                break;
            }
            _ => {}
        }
    });
    result
}

/// Collect `(name, line)` for the parameters of the lambda the cursor is on.
/// Cursor restored.
fn collect_lambda_params(cursor: &mut TreeCursor, source: &[u8]) -> Vec<(String, u32)> {
    let mut params = Vec::new();
    for_each_child!(cursor, {
        if cursor.node().kind() == "lambda_parameters" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "variable_declaration" {
                    if let Some(id) = child_ident(cursor, source, &["simple_identifier"]) {
                        params.push(id);
                    }
                }
            });
            break;
        }
    });
    params
}

/// Name + line of a declaration: its first `type_identifier`/`simple_identifier`
/// direct child. Cursor restored.
fn decl_name(cursor: &mut TreeCursor, source: &[u8]) -> Option<(String, u32)> {
    child_ident(cursor, source, &["type_identifier", "simple_identifier"])
}

const FIELD_ORDER: &[&str] = &[
    "kind",
    "line",
    "class",
    "interface",
    "method",
    "object",
    "package",
];

fn emit_tag(w: &mut KotlinWalker, name: String, line: u32, kind: &'static str) {
    if name.is_empty() || name == "_" || !w.base.tag_config.is_kind_enabled(kind) {
        return;
    }
    let mut fields = ExtensionFields::new();
    fields.insert("kind", kind);
    fields.insert("line", line.to_string());
    if let Some((key, value)) = w.scopes.current_field() {
        fields.insert(key, value.to_string());
    }
    let mut raw: Vec<_> = fields.into_iter().collect();
    let mut enabled_fields = ExtensionFields::new();
    for &key in FIELD_ORDER {
        let Some(pos) = raw.iter().position(|(field, _)| field.as_ref() == key) else {
            continue;
        };
        let enabled = match key {
            "kind" | "line" => w.base.user_config.fields_config.is_field_enabled(key),
            _ => {
                w.base.user_config.fields_config.is_field_enabled("scope")
                    || w.base.user_config.extras_config.qualified
            }
        };
        let (key, value) = raw.swap_remove(pos);
        if enabled {
            enabled_fields.insert(key, value);
        }
    }
    debug_assert!(
        raw.is_empty(),
        "kotlin emit_tag: field missing from FIELD_ORDER: {raw:?}"
    );
    w.base.tags.push(Tag {
        name,
        file_name: w.base.file_name.clone(),
        address: Tag::address_from_line(
            w.base
                .lines
                .get(line.saturating_sub(1) as usize)
                .copied()
                .unwrap_or(b""),
        ),
        kind: Some(kind.into()),
        extension_fields: if enabled_fields.is_empty() {
            None
        } else {
            Some(enabled_fields)
        },
    });
}

fn process_node_inner(source: &[u8], cursor: &mut TreeCursor, w: &mut KotlinWalker<'_>) -> bool {
    let line = line_of(cursor.node());
    match cursor.node().kind() {
        "package_header" => {
            if let Some((name, _)) = child_ident(cursor, source, &["identifier"]) {
                if !name.is_empty() {
                    emit_tag(w, name.clone(), line, "p");
                    w.scopes.set_package(&name);
                }
            }
            false
        }
        "class_declaration" => {
            if let Some((name, _)) = decl_name(cursor, source) {
                let (kind, scope_kind) = if has_child(cursor, &["interface"]) {
                    ("i", ScopeKind::Interface)
                } else {
                    ("c", ScopeKind::Class)
                };
                emit_tag(w, name.clone(), line, kind);
                w.scopes.push(scope_kind, &name);
                return true;
            }
            false
        }
        "object_declaration" => {
            if let Some((name, _)) = decl_name(cursor, source) {
                emit_tag(w, name.clone(), line, "o");
                w.scopes.push(ScopeKind::Object, &name);
                return true;
            }
            false
        }
        "function_declaration" => {
            if let Some((name, name_line)) = decl_name(cursor, source) {
                emit_tag(w, name.clone(), name_line, "m");
                w.scopes.push(ScopeKind::Method, &name);
                return true;
            }
            false
        }
        "property_declaration" => {
            let kind = binding_kind(cursor, &w.base).unwrap_or("C");
            let bindings = collect_bindings(cursor, source);
            if let Some((_, names)) = &bindings {
                for (name, name_line) in names {
                    emit_tag(w, name.clone(), *name_line, kind);
                }
            }
            if has_child(cursor, &["lambda_literal", "anonymous_function"]) {
                if let Some((_, names)) = &bindings {
                    if let Some((name, _)) = names.first() {
                        w.scopes.push(ScopeKind::PendingLambda, name);
                        return true;
                    }
                }
            }
            false
        }
        "class_parameter" => {
            if let Some(kind) = binding_kind(cursor, &w.base) {
                if let Some((name, name_line)) = child_ident(cursor, source, &["simple_identifier"])
                {
                    emit_tag(w, name, name_line, kind);
                }
            }
            false
        }
        "for_statement" => {
            if let Some((is_multi, names)) = collect_bindings(cursor, source) {
                let kind = if is_multi { "C" } else { "m" };
                for (name, name_line) in names {
                    emit_tag(w, name, name_line, kind);
                }
            }
            false
        }
        "lambda_literal" | "anonymous_function" => {
            let claimed = matches!(w.scopes.last_key(), Some(ScopeKind::PendingLambda));
            let pushed = if claimed {
                w.scopes.set_last_key(ScopeKind::Method);
                false
            } else {
                emit_tag(w, "<lambda>".to_string(), line, "m");
                w.scopes.push(ScopeKind::Method, "<lambda>");
                true
            };
            for (name, name_line) in collect_lambda_params(cursor, source) {
                emit_tag(w, name, name_line, "m");
            }
            pushed
        }
        "type_alias" => {
            if let Some((name, name_line)) = decl_name(cursor, source) {
                emit_tag(w, name, name_line, "T");
            }
            false
        }
        _ => false,
    }
}

pub(crate) fn generate(
    parser: &mut TsParser,
    language: tree_sitter::Language,
    code: &[u8],
    path: &str,
    kinds: &TagKindConfig,
    config: &crate::config::Config,
) -> Option<Vec<Tag>> {
    generate_tags_with_config(
        parser,
        language,
        code,
        path,
        |source_code, lines, cursor, tags| {
            let mut walker = KotlinWalker {
                base: Context {
                    source_code,
                    lines,
                    file_name: path.into(),
                    tags,
                    tag_config: kinds,
                    user_config: config,
                },
                scopes: ScopeStack::new(),
            };
            walk_tree(cursor, &mut walker);
        },
    )
}
