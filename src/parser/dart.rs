//! Dart tags from the downloaded tree-sitter WASM grammar.
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

pub(crate) const LANG_NAME: &str = "dart";
pub(crate) const LANG_EXTENSIONS: &[&str] = &["dart"];

pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["c", "class"], "c"),
    (&["M", "mixin"], "M"),
    (&["g", "enum"], "g"),
    (&["e", "enumerator"], "e"),
    (&["x", "extension"], "x"),
    (&["t", "typedef"], "t"),
    (&["f", "function"], "f"),
    (&["m", "method"], "m"),
    (&["p", "property"], "p"),
    (&["F", "field"], "F"),
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
    "extension",
    "function",
    "inherits",
    "mixin",
    "signature",
    "typeref",
];

#[derive(Clone, Copy, PartialEq)]
enum ScopeKind {
    Class,
    Mixin,
    Enum,
    Extension,
    Function,
}

impl ScopeKey for ScopeKind {
    fn key(self) -> &'static str {
        match self {
            ScopeKind::Class => "class",
            ScopeKind::Mixin => "mixin",
            ScopeKind::Enum => "enum",
            ScopeKind::Extension => "extension",
            ScopeKind::Function => "function",
        }
    }
}

enum SigClass {
    /// a method or function.
    Callable,
    /// a property accessor.
    Property,
    /// any of the constructor signatures.
    Constructor,
}

fn classify_sig(kind: &str) -> Option<SigClass> {
    match kind {
        "function_signature" | "operator_signature" => Some(SigClass::Callable),
        "getter_signature" | "setter_signature" => Some(SigClass::Property),
        "constructor_signature"
        | "constant_constructor_signature"
        | "factory_constructor_signature"
        | "redirecting_factory_constructor_signature" => Some(SigClass::Constructor),
        _ => None,
    }
}

struct DartWalker<'src> {
    base: Context<'src>,
    scopes: ScopeStack<ScopeKind>,
}

impl WalkContext for DartWalker<'_> {
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

fn emit_tag(
    w: &mut DartWalker,
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
            "class" | "enum" | "extension" | "function" | "mixin" => {
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
        "dart emit_tag: fields missing from FIELD_ORDER: {raw:?}"
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

fn add_end_line(fields: &mut ExtensionFields, node: Node) {
    fields.insert("end", (node.end_position().row + 1).to_string());
}

fn access_of_name(name: &str) -> Option<String> {
    name.starts_with('_').then(|| "private".to_string())
}

fn type_base_name(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut result = None;
    for_each_child!(cursor, {
        let node = cursor.node();
        if node.is_named() {
            if node.kind() == "type_identifier" {
                result = Some(node_text(node, source).to_string());
            }
            break;
        }
    });
    result
}

fn named_child_of_kind<'a>(cursor: &mut TreeCursor<'a>, kind: &str) -> Option<Node<'a>> {
    let mut result = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == kind {
            result = Some(cursor.node());
            break;
        }
    });
    result
}

fn process_node_inner(source: &[u8], cursor: &mut TreeCursor, w: &mut DartWalker) -> bool {
    match cursor.node().kind() {
        "class_declaration" => emit_type(cursor, source, w, "c", ScopeKind::Class),
        "mixin_declaration" => emit_type(cursor, source, w, "M", ScopeKind::Mixin),
        "enum_declaration" => emit_type(cursor, source, w, "g", ScopeKind::Enum),
        "extension_declaration" => emit_extension(cursor, source, w),
        "extension_type_declaration" => emit_extension_type(cursor, source, w),
        "extension_type_representation" => {
            emit_representation(cursor, source, w);
            false
        }
        "enum_constant" => {
            emit_enum_constant(cursor, source, w);
            false
        }
        "type_alias" => {
            emit_typedef(cursor, source, w);
            false
        }
        "top_level_variable_declaration" => {
            emit_fields(cursor, source, w, "v");
            false
        }
        "function_declaration" => emit_signature_decl(cursor, source, w, "f"),
        "local_function_declaration" => emit_local_function(cursor, source, w),
        "getter_declaration" | "setter_declaration" => emit_signature_decl(cursor, source, w, "p"),
        "method_declaration" => emit_method(cursor, source, w),
        "declaration" => {
            emit_member_declaration(cursor, source, w);
            false
        }
        "local_variable_declaration" => {
            emit_locals(cursor, source, w);
            false
        }
        "formal_parameter" => {
            emit_parameter(cursor, source, w);
            false
        }
        _ => false,
    }
}

/// Emit a class / mixin / enum tag and open its scope.
fn emit_type(
    cursor: &mut TreeCursor,
    source: &[u8],
    w: &mut DartWalker,
    letter: &'static str,
    scope_kind: ScopeKind,
) -> bool {
    let node = cursor.node();
    let (name, line) = type_name_line(cursor, source);

    let inherits = inherits_of(cursor, source);
    emit_tag(w, name.clone(), line, letter, |fields| {
        add_field(fields, "access", access_of_name(&name));
        add_field(fields, "inherits", inherits);
        add_end_line(fields, node);
    });
    w.scopes.push(scope_kind, &name);
    true
}

/// The declared name + line of a class/mixin/enum. Handles the
/// `class Bar = Foo with M;` form, whose name sits inside a
/// `mixin_application_class` rather than the `name` field.
fn type_name_line(cursor: &mut TreeCursor, source: &[u8]) -> (String, u32) {
    let node = cursor.node();
    if let Some(nm) = field_child(cursor, "name") {
        return (node_text(nm, source).to_string(), line_of(nm));
    }
    let mut result = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "mixin_application_class" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "identifier" {
                    let id = cursor.node();
                    result = Some((node_text(id, source).to_string(), line_of(id)));
                    break;
                }
            });
            break;
        }
    });
    result.unwrap_or_else(|| (String::new(), line_of(node)))
}

fn inherits_of(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut names = Vec::new();
    for_each_child!(cursor, {
        match cursor.node().kind() {
            // `type` direct child = a mixin's `on` constraint.
            "type" => {
                if let Some(n) = type_base_name(cursor, source) {
                    names.push(n);
                }
            }
            "superclass" => {
                for_each_child!(cursor, {
                    match cursor.node().kind() {
                        "type" => {
                            if let Some(n) = type_base_name(cursor, source) {
                                names.push(n);
                            }
                        }
                        "mixins" => collect_types(cursor, source, &mut names),
                        _ => {}
                    }
                });
            }
            "mixins" | "interfaces" => collect_types(cursor, source, &mut names),
            "mixin_application_class" => {
                for_each_child!(cursor, {
                    if cursor.node().kind() == "mixin_application" {
                        for_each_child!(cursor, {
                            match cursor.node().kind() {
                                "type" => {
                                    if let Some(n) = type_base_name(cursor, source) {
                                        names.push(n);
                                    }
                                }
                                "mixins" => collect_types(cursor, source, &mut names),
                                _ => {}
                            }
                        });
                    }
                });
            }
            _ => {}
        }
    });
    (!names.is_empty()).then(|| names.join(","))
}

fn collect_types(cursor: &mut TreeCursor, source: &[u8], names: &mut Vec<String>) {
    for_each_child!(cursor, {
        if cursor.node().kind() == "type" {
            if let Some(n) = type_base_name(cursor, source) {
                names.push(n);
            }
        }
    });
}

fn emit_extension(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) -> bool {
    let node = cursor.node();
    let mut on_type = None;
    for_each_child!(cursor, {
        if cursor.field_name() == Some("class") {
            on_type = type_base_name(cursor, source);
            break;
        }
    });
    let (name, line) = match field_child(cursor, "name") {
        Some(nm) => (node_text(nm, source).to_string(), line_of(nm)),
        None => (
            on_type.clone().unwrap_or_else(|| "extension".to_string()),
            line_of(node),
        ),
    };

    emit_tag(w, name.clone(), line, "x", |fields| {
        add_field(fields, "access", access_of_name(&name));
        add_field(fields, "typeref", on_type.map(|t| format!("typename:{t}")));
        add_end_line(fields, node);
    });
    w.scopes.push(ScopeKind::Extension, &name);
    true
}

fn emit_extension_type(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) -> bool {
    let node = cursor.node();
    let mut name_node = None;
    let mut repr_type = None;
    for_each_child!(cursor, {
        match cursor.field_name() {
            Some("name") => name_node = named_child_of_kind(cursor, "identifier"),
            Some("representation") => {
                repr_type = field_child(cursor, "type")
                    .map(|t| format!("typename:{}", node_text(t, source)));
            }
            _ => {}
        }
    });
    let (name, line) = match name_node {
        Some(id) => (node_text(id, source).to_string(), line_of(id)),
        None => (String::new(), line_of(node)),
    };
    let inherits = inherits_of(cursor, source);
    emit_tag(w, name.clone(), line, "x", |fields| {
        add_field(fields, "access", access_of_name(&name));
        add_field(fields, "typeref", repr_type);
        add_field(fields, "inherits", inherits);
        add_end_line(fields, node);
    });
    w.scopes.push(ScopeKind::Extension, &name);
    true
}

fn emit_representation(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    let Some(nm) = field_child(cursor, "name") else {
        return;
    };
    let name = node_text(nm, source).to_string();
    let typeref = field_child(cursor, "type").map(|t| format!("typename:{}", w.base.node_text(&t)));
    emit_tag(w, name.clone(), line_of(nm), "F", |fields| {
        add_field(fields, "access", access_of_name(&name));
        add_field(fields, "typeref", typeref);
    });
}

fn emit_enum_constant(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    if let Some(nm) = field_child(cursor, "name") {
        emit_tag(
            w,
            node_text(nm, source).to_string(),
            line_of(nm),
            "e",
            |_| {},
        );
    }
}

fn emit_typedef(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    let mut name_line = None;
    let mut value = None;
    for_each_child!(cursor, {
        match cursor.node().kind() {
            "type_identifier" if name_line.is_none() => {
                let id = cursor.node();
                name_line = Some((node_text(id, source).to_string(), line_of(id)));
            }
            "type" => value = Some(node_text(cursor.node(), source).to_string()),
            _ => {}
        }
    });
    if let Some((name, line)) = name_line {
        emit_tag(w, name.clone(), line, "t", |fields| {
            add_field(fields, "access", access_of_name(&name));
            add_field(fields, "typeref", value.map(|v| format!("typename:{v}")));
        });
    }
}

fn emit_fields(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker, letter: &'static str) {
    let mut typeref = None;
    for_each_child!(cursor, {
        match cursor.node().kind() {
            "type" => typeref = Some(format!("typename:{}", node_text(cursor.node(), source))),
            "initialized_identifier_list" | "static_final_declaration_list" => {
                for_each_child!(cursor, {
                    let entry = cursor.node();
                    if matches!(
                        entry.kind(),
                        "initialized_identifier" | "static_final_declaration"
                    ) {
                        if let Some(nm) = field_child(cursor, "name") {
                            let name = node_text(nm, source).to_string();
                            emit_tag(w, name.clone(), line_of(nm), letter, |fields| {
                                add_field(fields, "access", access_of_name(&name));
                                add_field(fields, "typeref", typeref.clone());
                            });
                        }
                    }
                });
            }
            _ => {}
        }
    });
}

fn emit_locals(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    for_each_child!(cursor, {
        if cursor.node().kind() == "initialized_variable_definition" {
            if let Some(nm) = field_child(cursor, "name") {
                emit_tag(
                    w,
                    node_text(nm, source).to_string(),
                    line_of(nm),
                    "l",
                    |_| {},
                );
            }
        }
    });
}

fn emit_parameter(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    if let Some(nm) = field_child(cursor, "name") {
        let name = node_text(nm, source).to_string();
        let mut typeref = None;
        for_each_child!(cursor, {
            if cursor.node().kind() == "type" {
                typeref = Some(format!("typename:{}", node_text(cursor.node(), source)));
                break;
            }
        });
        emit_tag(w, name, line_of(nm), "z", |fields| {
            add_field(fields, "typeref", typeref);
        });
    }
}

fn emit_signature_decl(
    cursor: &mut TreeCursor,
    source: &[u8],
    w: &mut DartWalker,
    letter: &'static str,
) -> bool {
    let wrapper = cursor.node();
    let mut name = None;
    for_each_child!(cursor, {
        if cursor.field_name() == Some("signature") {
            name = Some(emit_callable(cursor, source, w, wrapper, letter));
            break;
        }
    });
    name.is_some_and(|name| push_body_scope(cursor, w, &name))
}

fn emit_local_function(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) -> bool {
    let wrapper = cursor.node();
    let mut name = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "function_signature" {
            name = Some(emit_callable(cursor, source, w, wrapper, "f"));
            break;
        }
    });
    name.is_some_and(|name| push_body_scope(cursor, w, &name))
}

fn emit_method(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) -> bool {
    let wrapper = cursor.node();
    let mut name = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "method_signature" {
            for_each_child!(cursor, {
                if classify_sig(cursor.node().kind()).is_some() {
                    let letter = member_letter(cursor.node());
                    name = Some(emit_callable(cursor, source, w, wrapper, letter));
                    break;
                }
            });
            break;
        }
    });
    name.is_some_and(|name| push_body_scope(cursor, w, &name))
}

fn emit_member_declaration(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    let wrapper = cursor.node();
    let mut found = false;
    for_each_child!(cursor, {
        if classify_sig(cursor.node().kind()).is_some() {
            let letter = member_letter(cursor.node());
            emit_callable(cursor, source, w, wrapper, letter);
            found = true;
            break;
        }
    });
    if !found {
        emit_fields(cursor, source, w, "F");
    }
}

fn member_letter(sig: Node) -> &'static str {
    match classify_sig(sig.kind()) {
        Some(SigClass::Property) => "p",
        _ => "m",
    }
}

fn push_body_scope(cursor: &mut TreeCursor, w: &mut DartWalker, name: &str) -> bool {
    if field_child(cursor, "body").is_some() {
        w.scopes.push(ScopeKind::Function, name);
        true
    } else {
        false
    }
}

fn callable_name_line(cursor: &mut TreeCursor, source: &[u8]) -> (String, u32) {
    let sig = cursor.node();
    match sig.kind() {
        "operator_signature" => match field_child(cursor, "operator") {
            Some(op) => (format!("operator {}", node_text(op, source)), line_of(op)),
            None => ("operator".to_string(), line_of(sig)),
        },
        "constructor_signature"
        | "constant_constructor_signature"
        | "factory_constructor_signature"
        | "redirecting_factory_constructor_signature" => {
            (constructor_name(sig, source), line_of(sig))
        }
        _ => match field_child(cursor, "name") {
            Some(nm) => (node_text(nm, source).to_string(), line_of(nm)),
            None => (String::new(), line_of(sig)),
        },
    }
}

fn constructor_name(sig: Node, source: &[u8]) -> String {
    let text = node_text(sig, source);
    let head = text.split('(').next().unwrap_or(text);
    head.split_whitespace().last().unwrap_or("").to_string()
}

fn emit_callable(
    cursor: &mut TreeCursor,
    source: &[u8],
    w: &mut DartWalker,
    wrapper: Node,
    letter: &'static str,
) -> String {
    let (name, line) = callable_name_line(cursor, source);
    let signature = named_child_of_kind(cursor, "formal_parameter_list")
        .map(|params| w.base.node_text(&params).to_string());
    let typeref =
        field_child(cursor, "return_type").map(|rt| format!("typename:{}", w.base.node_text(&rt)));
    emit_tag(w, name.clone(), line, letter, |fields| {
        add_field(fields, "access", access_of_name(&name));
        add_field(fields, "signature", signature);
        add_field(fields, "typeref", typeref);
        add_end_line(fields, wrapper);
    });
    name
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
            let mut walker = DartWalker {
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
