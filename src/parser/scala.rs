//! Scala definitions from the bundled tree-sitter grammar.
use super::common::{
    cursor::{field_child, line_of, node_text},
    scope::{ScopeKey, ScopeStack},
    scope_walker::{walk_tree, WalkContext},
    tree_walker::{generate_tags_with_config, Context},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};

pub(crate) const LANG_NAME: &str = "scala";
pub(crate) const LANG_EXTENSIONS: &[&str] = &["scala"];
pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["C", "constant"], "C"),
    (&["c", "class"], "c"),
    (&["e", "enumerator"], "e"),
    (&["f", "function"], "f"),
    (&["g", "enum"], "g"),
    (&["i", "trait", "interface"], "i"),
    (&["m", "method"], "m"),
    (&["o", "object"], "o"),
    (&["p", "package"], "p"),
    (&["P", "property"], "P"),
    (&["t", "type", "typealias"], "t"),
    (&["v", "variable"], "v"),
];
pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] =
    &[(&["l", "local"], "l"), (&["z", "parameter"], "z")];

const FIELD_ORDER: &[&str] = &[
    "kind",
    "line",
    "end",
    "access",
    "class",
    "enum",
    "function",
    "inherits",
    "method",
    "object",
    "package",
    "signature",
    "trait",
    "typeref",
];

#[derive(Clone, Copy, PartialEq)]
enum ScopeKind {
    Package,
    Class,
    Trait,
    Object,
    Enum,
    Function,
    Method,
}

impl ScopeKey for ScopeKind {
    fn key(self) -> &'static str {
        match self {
            Self::Package => "package",
            Self::Class => "class",
            Self::Trait => "trait",
            Self::Object => "object",
            Self::Enum => "enum",
            Self::Function => "function",
            Self::Method => "method",
        }
    }
}

struct ScalaWalker<'a> {
    base: Context<'a>,
    scopes: ScopeStack<ScopeKind>,
}

impl WalkContext for ScalaWalker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        process_node(cursor, self)
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

fn field_text(cursor: &mut TreeCursor, source: &[u8], field: &str) -> Option<String> {
    field_child(cursor, field).map(|node| node_text(node, source).to_owned())
}

fn name(cursor: &mut TreeCursor, source: &[u8]) -> Option<(String, u32)> {
    field_child(cursor, "name").map(|node| (node_text(node, source).to_owned(), line_of(node)))
}

fn access(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut result = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "access_modifier" {
            result = Some(node_text(cursor.node(), source).to_owned());
            break;
        }
        if cursor.node().kind() == "modifiers" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "access_modifier" {
                    result = Some(node_text(cursor.node(), source).to_owned());
                    break;
                }
            });
        }
    });
    result
}

fn signature(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut parts = Vec::new();
    for_each_child!(cursor, {
        if matches!(cursor.field_name(), Some("parameters" | "class_parameters")) {
            parts.push(node_text(cursor.node(), source).to_owned());
        }
    });
    (!parts.is_empty()).then(|| parts.join(""))
}

fn emit(
    w: &mut ScalaWalker,
    name: String,
    line: u32,
    kind: &'static str,
    node: Node,
    extras: Vec<(&'static str, String)>,
) {
    if name.is_empty() || name == "_" || !w.base.tag_config.is_kind_enabled(kind) {
        return;
    }
    let mut fields = ExtensionFields::new();
    fields.insert("kind", kind);
    fields.insert("line", line.to_string());
    fields.insert("end", (node.end_position().row + 1).to_string());
    if let Some((key, value)) = w.scopes.current_field() {
        fields.insert(key, value.to_owned());
    }
    for (key, value) in extras {
        fields.insert(key, value);
    }
    let mut raw: Vec<_> = fields.into_iter().collect();
    let mut enabled = ExtensionFields::new();
    for &key in FIELD_ORDER {
        let Some(index) = raw.iter().position(|(field, _)| field.as_ref() == key) else {
            continue;
        };
        let include = match key {
            "class" | "enum" | "function" | "method" | "object" | "package" | "trait" => {
                w.base.user_config.fields_config.is_field_enabled("scope")
                    || w.base.user_config.extras_config.qualified
            }
            _ => w.base.user_config.fields_config.is_field_enabled(key),
        };
        let (key, value) = raw.swap_remove(index);
        if include {
            enabled.insert(key, value);
        }
    }
    debug_assert!(
        raw.is_empty(),
        "scala emit: fields missing from FIELD_ORDER: {raw:?}"
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
        extension_fields: (!enabled.is_empty()).then_some(enabled),
    });
}

// Only binding-pattern nodes are visited; type names and constructor names are
// deliberately excluded. The shared cursor is restored before returning.
fn bindings(cursor: &mut TreeCursor, source: &[u8], out: &mut Vec<(String, u32)>) {
    let node = cursor.node();
    match node.kind() {
        "identifier" | "operator_identifier" => {
            out.push((node_text(node, source).to_owned(), line_of(node)));
        }
        "identifiers"
        | "tuple_pattern"
        | "named_pattern"
        | "named_tuple_pattern"
        | "typed_pattern" => {
            for_each_child!(cursor, {
                if cursor.field_name() != Some("type") && cursor.node().is_named() {
                    bindings(cursor, source, out);
                }
            });
        }
        _ => {}
    }
}

fn process_node(cursor: &mut TreeCursor, w: &mut ScalaWalker<'_>) -> bool {
    let node = cursor.node();
    let source = w.base.source_code.as_bytes();
    match node.kind() {
        "package_clause" => {
            if let Some((name, line)) = name(cursor, source) {
                emit(w, name.clone(), line, "p", node, vec![]);
                if field_child(cursor, "body").is_some() {
                    w.scopes.push(ScopeKind::Package, &name);
                    return true;
                }
                w.scopes.append_package(&name);
            }
        }
        "package_object" | "class_definition" | "trait_definition" | "object_definition"
        | "enum_definition" => {
            let (kind, scope) = match node.kind() {
                "package_object" => ("o", ScopeKind::Object),
                "class_definition" => ("c", ScopeKind::Class),
                "trait_definition" => ("i", ScopeKind::Trait),
                "object_definition" => ("o", ScopeKind::Object),
                _ => ("g", ScopeKind::Enum),
            };
            if let Some((name, line)) = name(cursor, source) {
                let mut extras = Vec::new();
                if let Some(value) = access(cursor, source) {
                    extras.push(("access", value));
                }
                if let Some(value) = field_text(cursor, source, "extend") {
                    let value = value.trim_start_matches("extends").trim().to_owned();
                    if !value.is_empty() {
                        extras.push(("inherits", value));
                    }
                }
                if let Some(value) = signature(cursor, source) {
                    extras.push(("signature", value));
                }
                emit(w, name.clone(), line, kind, node, extras);
                w.scopes.push(scope, &name);
                return true;
            }
        }
        "simple_enum_case" | "full_enum_case" => {
            if let Some((name, line)) = name(cursor, source) {
                let mut extras = Vec::new();
                if let Some(value) = signature(cursor, source) {
                    extras.push(("signature", value));
                }
                emit(w, name.clone(), line, "e", node, extras);
                if node.kind() == "full_enum_case" {
                    w.scopes.push(ScopeKind::Class, &name);
                    return true;
                }
            }
        }
        "function_definition" | "function_declaration" => {
            if let Some((name, line)) = name(cursor, source) {
                let method = matches!(
                    w.scopes.last_key(),
                    Some(ScopeKind::Class | ScopeKind::Trait | ScopeKind::Object | ScopeKind::Enum)
                );
                let (kind, scope) = if method {
                    ("m", ScopeKind::Method)
                } else {
                    ("f", ScopeKind::Function)
                };
                let mut extras = Vec::new();
                if let Some(value) = access(cursor, source) {
                    extras.push(("access", value));
                }
                if let Some(value) = signature(cursor, source) {
                    extras.push(("signature", value));
                }
                if let Some(value) = field_text(cursor, source, "return_type") {
                    extras.push(("typeref", format!("typename:{value}")));
                }
                emit(w, name.clone(), line, kind, node, extras);
                w.scopes.push(scope, &name);
                return true;
            }
        }
        "type_definition" => {
            if let Some((name, line)) = name(cursor, source) {
                let mut extras = Vec::new();
                if let Some(value) = field_text(cursor, source, "type") {
                    extras.push(("typeref", format!("typename:{value}")));
                }
                emit(w, name, line, "t", node, extras);
            }
        }
        "given_definition" => {
            if let Some((name, line)) = name(cursor, source) {
                let mut extras = Vec::new();
                if let Some(value) = field_text(cursor, source, "return_type") {
                    extras.push(("typeref", format!("typename:{value}")));
                }
                emit(w, name, line, "C", node, extras);
            }
        }
        "val_definition" | "var_definition" | "val_declaration" | "var_declaration" => {
            let is_val = node.kind().starts_with("val_");
            let kind = if matches!(
                w.scopes.last_key(),
                Some(ScopeKind::Function | ScopeKind::Method)
            ) {
                "l"
            } else if is_val {
                "C"
            } else {
                "v"
            };
            let mut names = Vec::new();
            for_each_child!(cursor, {
                if cursor.field_name() == Some("pattern") {
                    bindings(cursor, source, &mut names);
                }
                if cursor.field_name() == Some("name") {
                    names.push((
                        node_text(cursor.node(), source).to_owned(),
                        line_of(cursor.node()),
                    ));
                }
            });
            let mut extras = Vec::new();
            if let Some(value) = field_text(cursor, source, "type") {
                extras.push(("typeref", format!("typename:{value}")));
            }
            if let Some(value) = access(cursor, source) {
                extras.push(("access", value));
            }
            for (name, line) in names {
                emit(w, name, line, kind, node, extras.clone());
            }
        }
        "class_parameter" => {
            if let Some((name, line)) = name(cursor, source) {
                let mut extras = Vec::new();
                if let Some(value) = field_text(cursor, source, "type") {
                    extras.push(("typeref", format!("typename:{value}")));
                }
                emit(w, name, line, "P", node, extras);
            }
        }
        "parameter" => {
            if let Some((name, line)) = name(cursor, source) {
                let mut extras = Vec::new();
                if let Some(value) = field_text(cursor, source, "type") {
                    extras.push(("typeref", format!("typename:{value}")));
                }
                emit(w, name, line, "z", node, extras);
            }
        }
        _ => {}
    }
    false
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
            let mut walker = ScalaWalker {
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
