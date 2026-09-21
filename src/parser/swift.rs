//! Swift tag generation using the external Swift WASM grammar.
use super::common::{
    cursor::{child_ident, line_of, node_text},
    scope::{ScopeKey, ScopeStack},
    scope_walker::{walk_tree, WalkContext},
    tree_walker::{generate_tags_with_config, Context},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};

pub(crate) const LANG_NAME: &str = "swift";
pub(crate) const LANG_EXTENSIONS: &[&str] = &["swift"];

pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["c", "class"], "c"),
    (&["s", "struct"], "s"),
    (&["P", "protocol"], "P"),
    (&["g", "enum"], "g"),
    (&["e", "enumerator"], "e"),
    (&["x", "extension"], "x"),
    (&["a", "actor"], "a"),
    (&["f", "function"], "f"),
    (&["m", "method"], "m"),
    (&["p", "property"], "p"),
    (&["v", "variable"], "v"),
    (&["t", "typealias"], "t"),
    (&["A", "associatedtype"], "A"),
    (&["o", "operator"], "o"),
];

pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] =
    &[(&["l", "local"], "l"), (&["z", "parameter"], "z")];

/// Standard ctags fields first, then Swift-specific fields alphabetically.
const FIELD_ORDER: &[&str] = &[
    "kind",
    "line",
    "end",
    "access",
    "actor",
    "class",
    "enum",
    "extension",
    "function",
    "inherits",
    "protocol",
    "signature",
    "struct",
    "typeref",
];

#[derive(Clone, Copy, PartialEq)]
enum ScopeKind {
    Class,
    Struct,
    Enum,
    Actor,
    Extension,
    Protocol,
    Function,
}

impl ScopeKey for ScopeKind {
    fn key(self) -> &'static str {
        match self {
            ScopeKind::Class => "class",
            ScopeKind::Struct => "struct",
            ScopeKind::Enum => "enum",
            ScopeKind::Actor => "actor",
            ScopeKind::Extension => "extension",
            ScopeKind::Protocol => "protocol",
            ScopeKind::Function => "function",
        }
    }
}

impl ScopeKind {
    fn is_type(self) -> bool {
        !matches!(self, ScopeKind::Function)
    }
}

struct SwiftWalker<'src> {
    base: Context<'src>,
    scopes: ScopeStack<ScopeKind>,
}

impl WalkContext for SwiftWalker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        process_node_inner(self.base.source_code.as_bytes(), cursor, self)
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

fn add_field(fields: &mut ExtensionFields, key: &'static str, value: Option<String>) {
    if let Some(value) = value {
        fields.insert(key, value);
    }
}

fn add_end_line(fields: &mut ExtensionFields, node: Node) {
    add_field(
        fields,
        "end",
        Some((node.end_position().row + 1).to_string()),
    );
}

fn emit_tag(
    w: &mut SwiftWalker,
    name: String,
    line: u32,
    kind: &'static str,
    extra_fields: impl FnOnce(&mut ExtensionFields),
) {
    if name.is_empty() || name == "_" || !w.base.tag_config.is_kind_enabled(kind) {
        return;
    }
    let mut fields = ExtensionFields::new();
    fields.insert("kind", kind);
    fields.insert("line", line.to_string());
    if let Some((key, value)) = w.scopes.current_field() {
        fields.insert(key, value.to_string());
    }
    extra_fields(&mut fields);

    let mut raw: Vec<_> = fields.into_iter().collect();
    let mut enabled_fields = ExtensionFields::new();
    for &key in FIELD_ORDER {
        let Some(pos) = raw.iter().position(|(field, _)| field.as_ref() == key) else {
            continue;
        };
        let enabled = match key {
            "kind" | "line" | "end" | "access" | "signature" | "typeref" => {
                w.base.user_config.fields_config.is_field_enabled(key)
            }
            "actor" | "class" | "enum" | "extension" | "function" | "protocol" | "struct" => {
                w.base.user_config.fields_config.is_field_enabled("scope")
                    || w.base.user_config.extras_config.qualified
            }
            _ => true,
        };
        let (key, value) = raw.swap_remove(pos);
        if enabled {
            enabled_fields.insert(key, value);
        }
    }
    debug_assert!(
        raw.is_empty(),
        "swift emit: fields missing from FIELD_ORDER: {raw:?}"
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
        extension_fields: (!enabled_fields.is_empty()).then_some(enabled_fields),
    });
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

/// Text and line of a named field. For generic user types, use their base
/// identifier (`Foo` for `Foo<Bar>`). The cursor returns to its starting node.
fn field_ident(cursor: &mut TreeCursor, source: &[u8], field: &str) -> Option<(String, u32)> {
    let mut result = None;
    for_each_child!(cursor, {
        if cursor.field_name() == Some(field) {
            let node = cursor.node();
            let line = line_of(node);
            let mut name = None;
            if node.kind() == "user_type" {
                for_each_child!(cursor, {
                    if cursor.node().is_named() {
                        name = Some(node_text(cursor.node(), source).to_string());
                        break;
                    }
                });
            }
            result = Some((
                name.unwrap_or_else(|| node_text(node, source).to_string()),
                line,
            ));
            break;
        }
    });
    result
}

/// Explicit access level (`public`, `private`, …) if one is written.
fn access_of(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut access = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "modifiers" {
            access = child_ident(cursor, source, &["visibility_modifier"]).map(|(text, _)| text);
            break;
        }
    });
    access
}

/// Comma-joined superclass/protocol conformances from `inheritance_specifier`
/// children (also covers an enum's raw-value type, e.g. `enum E: Int`)
fn inherits_of(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut names = Vec::new();
    for_each_child!(cursor, {
        if cursor.node().kind() == "inheritance_specifier" {
            let spec = cursor.node();
            let ty = spec.child_by_field_name("inherits_from").unwrap_or(spec);
            names.push(node_text(ty, source).to_string());
        }
    });
    (!names.is_empty()).then(|| names.join(","))
}

fn signature_of(cursor: &mut TreeCursor, source: &[u8]) -> String {
    let mut parts = Vec::new();
    for_each_child!(cursor, {
        if cursor.node().kind() == "parameter" {
            parts.push(node_text(cursor.node(), source).to_string());
        }
    });
    format!("({})", parts.join(", "))
}

fn return_typeref(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("return_type")
        .map(|rt| format!("typename:{}", node_text(rt, source)))
}

fn annotation_typeref(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut typeref = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "type_annotation" {
            if let Some(ty) = cursor.node().child_by_field_name("type") {
                typeref = Some(format!("typename:{}", node_text(ty, source)));
            }
            break;
        }
    });
    typeref
}

fn process_node_inner(source: &[u8], cursor: &mut TreeCursor, w: &mut SwiftWalker) -> bool {
    match cursor.node().kind() {
        "class_declaration" => emit_type(cursor, source, w),
        "protocol_declaration" => emit_protocol(cursor, source, w),
        "function_declaration" | "protocol_function_declaration" => {
            emit_function(cursor, source, w)
        }
        "init_declaration" => emit_named_method(cursor, source, w, "init", true),
        "deinit_declaration" => emit_named_method(cursor, source, w, "deinit", false),
        "subscript_declaration" => emit_subscript(cursor, source, w),
        "property_declaration" => emit_property(cursor, source, w),
        "protocol_property_declaration" => {
            emit_protocol_property(cursor, source, w);
            false
        }
        "enum_entry" => {
            emit_enum_entry(cursor, source, w);
            false
        }
        "typealias_declaration" => {
            emit_typealias(cursor, source, w);
            false
        }
        "associatedtype_declaration" => {
            emit_associatedtype(cursor, source, w);
            false
        }
        "operator_declaration" => {
            emit_operator(cursor, source, w);
            false
        }
        "parameter" => {
            emit_parameter(cursor, source, w);
            false
        }
        "lambda_literal" => {
            w.scopes.push(ScopeKind::Function, "__closure");
            true
        }
        _ => false,
    }
}

fn emit_type(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) -> bool {
    let node = cursor.node();
    let dk = node
        .child_by_field_name("declaration_kind")
        .map(|n| node_text(n, source))
        .unwrap_or("");
    let (scope_kind, letter) = match dk {
        "struct" => (ScopeKind::Struct, "s"),
        "enum" => (ScopeKind::Enum, "g"),
        "actor" => (ScopeKind::Actor, "a"),
        "extension" => (ScopeKind::Extension, "x"),
        _ => (ScopeKind::Class, "c"),
    };
    let (name, line) =
        field_ident(cursor, source, "name").unwrap_or_else(|| (String::new(), line_of(node)));

    if w.base.tag_config.is_kind_enabled(letter) {
        let access = access_of(cursor, source);
        let inherits = inherits_of(cursor, source);
        emit_tag(w, name.clone(), line, letter, |fields| {
            add_field(fields, "access", access);
            add_field(fields, "inherits", inherits);
            add_end_line(fields, node);
        });
    }
    w.scopes.push(scope_kind, &name);
    true
}

fn emit_protocol(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) -> bool {
    let node = cursor.node();
    let (name, line) =
        field_ident(cursor, source, "name").unwrap_or_else(|| (String::new(), line_of(node)));

    if w.base.tag_config.is_kind_enabled("P") {
        let access = access_of(cursor, source);
        let inherits = inherits_of(cursor, source);
        emit_tag(w, name.clone(), line, "P", |fields| {
            add_field(fields, "access", access);
            add_field(fields, "inherits", inherits);
            add_end_line(fields, node);
        });
    }
    w.scopes.push(ScopeKind::Protocol, &name);
    true
}

fn emit_function(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) -> bool {
    let node = cursor.node();
    let (name, line) =
        field_ident(cursor, source, "name").unwrap_or_else(|| (String::new(), line_of(node)));

    let letter = if w.scopes.last_key().is_some_and(ScopeKind::is_type) {
        "m"
    } else {
        "f"
    };

    if w.base.tag_config.is_kind_enabled(letter) {
        let access = access_of(cursor, source);
        let signature = signature_of(cursor, source);
        let typeref = return_typeref(node, source);
        emit_tag(w, name.clone(), line, letter, |fields| {
            add_field(fields, "access", access);
            fields.insert("signature", signature);
            add_field(fields, "typeref", typeref);
            add_end_line(fields, node);
        });
    }
    w.scopes.push(ScopeKind::Function, &name);
    true
}

fn emit_named_method(
    cursor: &mut TreeCursor,
    source: &[u8],
    w: &mut SwiftWalker,
    name: &str,
    with_signature: bool,
) -> bool {
    let node = cursor.node();
    if w.base.tag_config.is_kind_enabled("m") {
        let access = access_of(cursor, source);
        let signature = with_signature.then(|| signature_of(cursor, source));
        emit_tag(w, name.to_string(), line_of(node), "m", |fields| {
            add_field(fields, "access", access);
            add_field(fields, "signature", signature);
            add_end_line(fields, node);
        });
    }
    w.scopes.push(ScopeKind::Function, name);
    true
}

fn emit_subscript(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) -> bool {
    let node = cursor.node();
    if w.base.tag_config.is_kind_enabled("m") {
        let access = access_of(cursor, source);
        let signature = signature_of(cursor, source);
        let typeref = return_typeref(node, source);
        emit_tag(w, "subscript".to_string(), line_of(node), "m", |fields| {
            add_field(fields, "access", access);
            fields.insert("signature", signature);
            add_field(fields, "typeref", typeref);
            add_end_line(fields, node);
        });
    }
    w.scopes.push(ScopeKind::Function, "subscript");
    true
}

fn emit_property(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) -> bool {
    let letter = match w.scopes.last_key() {
        Some(s) if s.is_type() => "p",
        Some(_) => "l",
        None => "v",
    };

    let access = access_of(cursor, source);
    let typeref = annotation_typeref(cursor, source);

    let mut first_name = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "pattern" {
            if let Some((name, line)) = child_ident(cursor, source, &["simple_identifier"]) {
                if first_name.is_none() {
                    first_name = Some(name.clone());
                }
                if w.base.tag_config.is_kind_enabled(letter) {
                    emit_tag(w, name, line, letter, |fields| {
                        add_field(fields, "access", access.clone());
                        add_field(fields, "typeref", typeref.clone());
                    });
                }
            }
        }
    });

    if has_child(cursor, &["computed_property", "willset_didset_block"]) {
        w.scopes
            .push(ScopeKind::Function, first_name.as_deref().unwrap_or("_"));
        true
    } else {
        false
    }
}

fn emit_protocol_property(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.base.tag_config.is_kind_enabled("p") {
        return;
    }
    let access = access_of(cursor, source);
    let typeref = annotation_typeref(cursor, source);

    let mut name_line = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "pattern" {
            name_line = child_ident(cursor, source, &["simple_identifier"]);
            break;
        }
    });

    if let Some((name, line)) = name_line {
        emit_tag(w, name, line, "p", |fields| {
            add_field(fields, "access", access);
            add_field(fields, "typeref", typeref);
        });
    }
}

fn emit_enum_entry(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.base.tag_config.is_kind_enabled("e") {
        return;
    }
    for_each_child!(cursor, {
        if cursor.node().kind() == "simple_identifier" {
            let n = cursor.node();
            emit_tag(w, node_text(n, source).to_string(), line_of(n), "e", |_| {});
        }
    });
}

fn emit_typealias(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.base.tag_config.is_kind_enabled("t") {
        return;
    }
    let node = cursor.node();
    let Some((name, line)) = field_ident(cursor, source, "name") else {
        return;
    };
    let access = access_of(cursor, source);
    let typeref = node
        .child_by_field_name("value")
        .map(|value| format!("typename:{}", node_text(value, source)));
    emit_tag(w, name, line, "t", |fields| {
        add_field(fields, "access", access);
        add_field(fields, "typeref", typeref);
    });
}

fn emit_associatedtype(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.base.tag_config.is_kind_enabled("A") {
        return;
    }
    let Some((name, line)) = field_ident(cursor, source, "name") else {
        return;
    };
    emit_tag(w, name, line, "A", |_| {});
}

fn emit_operator(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.base.tag_config.is_kind_enabled("o") {
        return;
    }
    if let Some((name, line)) = child_ident(cursor, source, &["custom_operator", "bang"]) {
        emit_tag(w, name, line, "o", |_| {});
    }
}

fn emit_parameter(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.base.tag_config.is_kind_enabled("z") {
        return;
    }
    let node = cursor.node();
    let Some((name, line)) = field_ident(cursor, source, "name") else {
        return;
    };
    let typeref = node
        .child_by_field_name("type")
        .map(|ty| format!("typename:{}", node_text(ty, source)));
    emit_tag(w, name, line, "z", |fields| {
        add_field(fields, "typeref", typeref);
    });
}

pub(crate) fn generate(
    parser: &mut TsParser,
    language: tree_sitter::Language,
    source: &[u8],
    path: &str,
    kinds: &TagKindConfig,
    config: &crate::config::Config,
) -> Option<Vec<Tag>> {
    generate_tags_with_config(
        parser,
        language,
        source,
        path,
        |source_code, lines, cursor, tags| {
            let mut walker = SwiftWalker {
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
