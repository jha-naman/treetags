//! Objective-C tag generation, ported from the original Objective-C plugin.
use super::common::{
    cursor::{child_ident, line_of, node_text},
    scope::{ScopeKey, ScopeStack},
    scope_walker::{walk_tree, WalkContext},
    tree_walker::{generate_tags_with_config, Context},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use tree_sitter::{Parser as TsParser, TreeCursor};

pub(crate) const LANG_NAME: &str = "objc";
pub(crate) const LANG_ALIASES: &[&str] = &["objectivec", "objective-c"];
pub(crate) const LANG_EXTENSIONS: &[&str] = &["m", "h"];
pub(crate) const DISAMBIG_SIGNALS: &[&str] = &[
    "@interface",
    "@implementation",
    "@protocol",
    "@property",
    "@end",
    "@class",
    "@import",
    "@selector",
    "@autoreleasepool",
    "NS_ASSUME_NONNULL",
];

// Objective-C is a strict superset of C, so constructs shared with C use the
// *exact* kind letters the C parser emits (`src/parser/cpp.rs` C_KIND_*), and
// Objective-C-only constructs use letters that do not clash with any C letter.
pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    // Shared with C — identical letters to the C parser.
    (&["d", "macro"], "d"),
    (&["e", "enumerator"], "e"),
    (&["f", "function"], "f"),
    (&["g", "enum"], "g"),
    (&["h", "header"], "h"),
    (&["m", "member"], "m"),
    (&["s", "struct"], "s"),
    (&["t", "typedef"], "t"),
    (&["u", "union"], "u"),
    (&["v", "variable"], "v"),
    // Objective-C-only — non-clashing letters.
    (&["A", "property"], "A"),
    (&["C", "category"], "C"),
    (&["E", "ivar"], "E"),
    (&["I", "implementation"], "I"),
    (&["M", "method"], "M"),
    (&["P", "protocol"], "P"),
    (&["c", "class"], "c"),
    (&["i", "interface"], "i"),
];

// Off by default, matching the C parser's optional kinds (C_KIND_OPTIONALS).
pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] = &[
    (&["l", "local"], "l"),
    (&["p", "prototype"], "p"),
    (&["x", "externvar"], "x"),
    (&["z", "parameter"], "z"),
    (&["L", "label"], "L"),
    (&["D", "macroparam"], "D"),
];

#[derive(Clone, Copy)]
enum ScopeKind {
    Interface,
    Implementation,
    Protocol,
    Struct,
    Union,
    Function,
    Method,
}

impl ScopeKey for ScopeKind {
    fn key(self) -> &'static str {
        match self {
            ScopeKind::Interface => "interface",
            ScopeKind::Implementation => "implementation",
            ScopeKind::Protocol => "protocol",
            ScopeKind::Struct => "struct",
            ScopeKind::Union => "union",
            ScopeKind::Function => "function",
            ScopeKind::Method => "method",
        }
    }
}

/// One undo action per `process_node` that returned `true`, reversed in LIFO
/// order by `pop_scope`.
enum Open {
    /// Pop a scope frame and restore the previous `current_category`.
    ScopeWithCategory(Option<String>),
    /// Leave a `{ ... }` body: decrement the body-depth suppressor.
    Body,
}

struct ObjcWalker<'src> {
    base: Context<'src>,
    scopes: ScopeStack<ScopeKind>,
    /// Category name of the enclosing `@interface Foo (Cat)` /
    /// `@implementation Foo (Cat)`, added as a `category:` field to members.
    current_category: Option<String>,
    /// Depth inside function/method bodies; when > 0, in-body declarations are
    /// not tagged (locals are not ctags-visible for Objective-C).
    in_body: u32,
    opens: Vec<Open>,
}

impl WalkContext for ObjcWalker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        let source = self.base.source_code.as_bytes();
        process_node_inner(source, cursor, self)
    }

    fn pop_scope(&mut self) {
        match self.opens.pop() {
            Some(Open::ScopeWithCategory(prev)) => {
                self.scopes.pop();
                self.current_category = prev;
            }
            Some(Open::Body) => {
                self.in_body = self.in_body.saturating_sub(1);
            }
            None => self.scopes.pop(),
        }
    }
}

/// Standard ctags fields first, then Objective-C-specific fields alphabetically.
const FIELD_ORDER: &[&str] = &[
    "kind",
    "line",
    "end",
    "category",
    "enum",
    "function",
    "implementation",
    "interface",
    "macro",
    "method",
    "protocol",
    "protocols",
    "struct",
    "union",
];

fn emit_tag(
    walker: &mut ObjcWalker,
    name: String,
    line: u32,
    kind: &'static str,
    extra_fields: impl FnOnce(&mut ExtensionFields),
) {
    if name.is_empty() || name == "_" || !walker.base.tag_config.is_kind_enabled(kind) {
        return;
    }

    let mut fields = ExtensionFields::new();
    fields.insert("kind", kind);
    fields.insert("line", line.to_string());
    if let Some((key, value)) = walker.scopes.current_field() {
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
            "kind" | "line" | "end" => walker.base.user_config.fields_config.is_field_enabled(key),
            "enum" | "function" | "implementation" | "interface" | "macro" | "method"
            | "protocol" | "struct" | "union" => {
                walker
                    .base
                    .user_config
                    .fields_config
                    .is_field_enabled("scope")
                    || walker.base.user_config.extras_config.qualified
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
        "objective_c emit_tag: field(s) missing from FIELD_ORDER: {raw:?}"
    );

    walker.base.tags.push(Tag {
        name,
        file_name: walker.base.file_name.clone(),
        address: Tag::address_from_line(
            walker
                .base
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

/// Resolve the declared name of a (possibly nested) declarator, following the
/// `declarator` field so parameter names in function/function-pointer
/// declarators are skipped. Returns `(name, is_function_declarator)`.
fn declarator_name(cursor: &mut TreeCursor, source: &[u8]) -> Option<(String, bool)> {
    declarator_name_inner(cursor, source, false)
}

fn declarator_name_inner(
    cursor: &mut TreeCursor,
    source: &[u8],
    is_func: bool,
) -> Option<(String, bool)> {
    let node = cursor.node();
    if matches!(
        node.kind(),
        "identifier" | "type_identifier" | "field_identifier"
    ) {
        return Some((node_text(node, source).to_string(), is_func));
    }
    let func = is_func || node.kind() == "function_declarator";
    let mut result = None;
    for_each_child!(cursor, {
        if cursor.field_name() == Some("declarator") {
            result = declarator_name_inner(cursor, source, func);
            break;
        }
    });
    if result.is_some() {
        return result;
    }

    // No `declarator` field: descend into an identifier-like child, or recurse
    // through a nested `*_declarator` that isn't attached via that field
    // (e.g. `parenthesized_declarator` -> `pointer_declarator`).
    for_each_child!(cursor, {
        let child = cursor.node();
        let k = child.kind();
        if matches!(k, "identifier" | "field_identifier" | "type_identifier") {
            result = Some((node_text(child, source).to_string(), func));
            break;
        }
        if k.ends_with("_declarator") {
            if let Some(r) = declarator_name_inner(cursor, source, func) {
                result = Some(r);
                break;
            }
        }
    });
    result
}

/// Comma-joined adopted protocols of an `@interface`. On a class interface the
/// `<Proto1, Proto2>` list is a `parameterized_arguments` child whose
/// `type_name` grandchildren name the protocols.
fn interface_protocols(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut names = Vec::new();
    for_each_child!(cursor, {
        if cursor.node().kind() == "parameterized_arguments" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "type_name" {
                    names.push(node_text(cursor.node(), source).trim().to_string());
                }
            });
            break;
        }
    });
    (!names.is_empty()).then(|| names.join(","))
}

/// Comma-joined adopted protocols of an `@protocol`, taken from its
/// `protocol_reference_list` (`<NSObject>`).
fn protocol_reference_list(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut names = Vec::new();
    for_each_child!(cursor, {
        if cursor.node().kind() == "protocol_reference_list" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "identifier" {
                    names.push(node_text(cursor.node(), source).to_string());
                }
            });
            break;
        }
    });
    (!names.is_empty()).then(|| names.join(","))
}

/// Build an Objective-C selector from a method_declaration/method_definition.
/// Selector labels are the direct `identifier` children; each labelled part of
/// a keyword selector is followed by a `method_parameter`. A no-argument method
/// has a single `identifier` and no `method_parameter`.
fn method_selector(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut labels: Vec<String> = Vec::new();
    let mut has_param = false;
    for_each_child!(cursor, {
        match cursor.node().kind() {
            "identifier" => labels.push(node_text(cursor.node(), source).to_string()),
            "method_parameter" => has_param = true,
            "keyword_declarator" => {
                if let Some((label, _)) = child_ident(cursor, source, &["identifier"]) {
                    labels.push(label);
                }
                has_param = true;
            }
            _ => {}
        }
    });
    if labels.is_empty() {
        return None;
    }
    if has_param {
        Some(labels.iter().map(|l| format!("{l}:")).collect())
    } else {
        Some(labels.join(""))
    }
}

/// Emit a tag of `kind` for each `struct_declarator` inside the current node's
/// `struct_declaration` child (used for instance variables and `@property`s).
fn emit_struct_declarators(
    cursor: &mut TreeCursor,
    source: &[u8],
    line: u32,
    kind: &'static str,
    walker: &mut ObjcWalker,
) {
    for_each_child!(cursor, {
        if cursor.node().kind() == "struct_declaration" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "struct_declarator" {
                    if let Some((name, _)) = declarator_name(cursor, source) {
                        emit_tag(walker, name, line, kind, |_| {});
                    }
                }
            });
        }
    });
}

/// Handle `@interface`/`@implementation` (with optional `(Category)`): emit the
/// container tag, an optional category tag, and push the container scope.
fn handle_container(
    cursor: &mut TreeCursor,
    source: &[u8],
    walker: &mut ObjcWalker<'_>,
    kind: &'static str,
    scope_kind: ScopeKind,
) -> bool {
    let node = cursor.node();
    let line = line_of(node);
    let name = match child_ident(cursor, source, &["identifier"]) {
        Some((n, _)) => n,
        None => return false,
    };
    let category = node
        .child_by_field_name("category")
        .map(|n| node_text(n, source).to_string());
    let protocols = if category.is_none() {
        interface_protocols(cursor, source)
    } else {
        None
    };

    emit_tag(walker, name.clone(), line, kind, |fields| {
        if let Some(cat) = &category {
            fields.insert("category", cat.clone());
        } else if let Some(p) = &protocols {
            fields.insert("protocols", p.clone());
        }
    });

    let prev_cat = walker.current_category.clone();
    walker.scopes.push(scope_kind, &name);
    walker.opens.push(Open::ScopeWithCategory(prev_cat));

    if let Some(cat) = category {
        emit_tag(walker, cat.clone(), line, "C", |_| {});
        walker.current_category = Some(cat);
    }
    true
}

/// Emit a `z` (parameter) tag for each named parameter of the function/prototype
/// under `function:fn_name` (matches the C parser). The supplied cursor starts
/// on a declarator and is restored before returning.
fn emit_function_params(
    cursor: &mut TreeCursor,
    source: &[u8],
    fn_name: &str,
    walker: &mut ObjcWalker,
) -> bool {
    if cursor.node().kind() != "function_declarator" {
        let mut found = false;
        for_each_child!(cursor, {
            if !found && emit_function_params(cursor, source, fn_name, walker) {
                found = true;
                break;
            }
        });
        return found;
    }

    for_each_child!(cursor, {
        if cursor.field_name() == Some("parameters") {
            for_each_child!(cursor, {
                let param = cursor.node();
                if param.kind() == "parameter_declaration" {
                    for_each_child!(cursor, {
                        if cursor.field_name() == Some("declarator") {
                            if let Some((name, _)) = declarator_name(cursor, source) {
                                emit_tag(walker, name, line_of(param), "z", |fields| {
                                    fields.insert("function", fn_name.to_string());
                                });
                            }
                            break;
                        }
                    });
                }
            });
            break;
        }
    });
    true
}

/// Emit a `D` (macroparam) tag for each parameter of a function-like macro,
/// scoped under `macro:macro_name` (matches the C parser).
fn emit_macro_params(
    cursor: &mut TreeCursor,
    source: &[u8],
    macro_name: &str,
    walker: &mut ObjcWalker,
) {
    for_each_child!(cursor, {
        if cursor.node().kind() == "preproc_params" {
            for_each_child!(cursor, {
                let p = cursor.node();
                if p.kind() == "identifier" {
                    emit_tag(
                        walker,
                        node_text(p, source).to_string(),
                        line_of(p),
                        "D",
                        |fields| {
                            fields.insert("macro", macro_name.to_string());
                        },
                    );
                }
            });
            break;
        }
    });
}

/// Whether a `declaration` node carries an `extern` storage-class specifier.
fn has_extern_specifier(cursor: &mut TreeCursor, source: &[u8]) -> bool {
    let mut found = false;
    for_each_child!(cursor, {
        let c = cursor.node();
        if c.kind() == "storage_class_specifier" && node_text(c, source) == "extern" {
            found = true;
            break;
        }
    });
    found
}

/// Emit an `e` tag for each enumerator below the current node. The same cursor
/// is moved through descendants and restored on return.
fn emit_enumerators(
    cursor: &mut TreeCursor,
    source: &[u8],
    enum_name: &str,
    walker: &mut ObjcWalker,
) -> bool {
    if cursor.node().kind() == "enumerator_list" {
        for_each_child!(cursor, {
            let e = cursor.node();
            if e.kind() == "enumerator" {
                if let Some((name, _)) = child_ident(cursor, source, &["identifier"]) {
                    emit_tag(walker, name, line_of(e), "e", |fields| {
                        fields.insert("enum", enum_name.to_string());
                    });
                }
            }
        });
        return true;
    }

    let mut found = false;
    for_each_child!(cursor, {
        if !found && emit_enumerators(cursor, source, enum_name, walker) {
            found = true;
            break;
        }
    });
    found
}

/// Emit an `e` (enumerator) tag for each constant of a `typedef NS_ENUM/NS_OPTIONS`.
/// tree-sitter-objc cannot parse the macro body, so the constants surface as
/// `type_identifier` nodes directly under the `type_definition` (wrapped by
/// `ERROR` `{`/`}` siblings); the enum name lives inside the `macro_type_specifier`
/// and is therefore not a direct child, so it is not picked up here.
fn emit_nsenum_constants(
    cursor: &mut TreeCursor,
    source: &[u8],
    enum_name: &str,
    walker: &mut ObjcWalker,
) {
    for_each_child!(cursor, {
        let c = cursor.node();
        if c.kind() == "type_identifier" {
            emit_tag(
                walker,
                node_text(c, source).to_string(),
                line_of(c),
                "e",
                |fields| {
                    fields.insert("enum", enum_name.to_string());
                },
            );
        }
    });
}

fn process_node_inner(source: &[u8], cursor: &mut TreeCursor, walker: &mut ObjcWalker<'_>) -> bool {
    let node = cursor.node();
    let line = line_of(node);

    match node.kind() {
        "preproc_def" | "preproc_function_def" => {
            if let Some((name, _)) = child_ident(cursor, source, &["identifier"]) {
                emit_tag(walker, name.clone(), line, "d", |_| {});
                if node.kind() == "preproc_function_def" {
                    emit_macro_params(cursor, source, &name, walker);
                }
            }
            false
        }
        "class_interface" => handle_container(cursor, source, walker, "i", ScopeKind::Interface),
        "class_implementation" => {
            handle_container(cursor, source, walker, "I", ScopeKind::Implementation)
        }
        "protocol_declaration" => {
            let name = match child_ident(cursor, source, &["identifier"]) {
                Some((n, _)) => n,
                None => return false,
            };
            let protocols = protocol_reference_list(cursor, source);
            emit_tag(walker, name.clone(), line, "P", |fields| {
                if let Some(p) = &protocols {
                    fields.insert("protocols", p.clone());
                }
            });
            let prev_cat = walker.current_category.clone();
            walker.scopes.push(ScopeKind::Protocol, &name);
            walker.opens.push(Open::ScopeWithCategory(prev_cat));
            true
        }
        "method_declaration" | "method_definition" => {
            let mut is_class = false;
            for_each_child!(cursor, {
                match cursor.node().kind() {
                    "+" => {
                        is_class = true;
                        break;
                    }
                    "-" => break,
                    _ => {}
                }
            });
            let kind = if is_class { "c" } else { "M" };
            if let Some(name) = method_selector(cursor, source) {
                let category = walker.current_category.clone();
                emit_tag(walker, name.clone(), line, kind, |fields| {
                    if let Some(cat) = category {
                        fields.insert("category", cat.clone());
                    }
                });
                if node.kind() == "method_definition" {
                    let prev_cat = walker.current_category.clone();
                    walker.scopes.push(ScopeKind::Method, &name);
                    walker.opens.push(Open::ScopeWithCategory(prev_cat));
                    return true;
                }
            }
            false
        }
        "property_declaration" => {
            emit_struct_declarators(cursor, source, line, "A", walker);
            false
        }
        "instance_variable" => {
            emit_struct_declarators(cursor, source, line, "E", walker);
            false
        }
        "field_declaration" => {
            for_each_child!(cursor, {
                if cursor.field_name() == Some("declarator") {
                    if let Some((name, is_func)) = declarator_name(cursor, source) {
                        if !is_func {
                            emit_tag(walker, name, line, "m", |_| {});
                        }
                    }
                }
            });
            false
        }
        "struct_specifier" | "union_specifier" => {
            let (kind, scope_kind) = if node.kind() == "union_specifier" {
                ("u", ScopeKind::Union)
            } else {
                ("s", ScopeKind::Struct)
            };
            if let Some(nm) = node.child_by_field_name("name") {
                if node.child_by_field_name("body").is_some() {
                    let name = node_text(nm, source).to_string();
                    emit_tag(walker, name.clone(), line, kind, |_| {});
                    let prev_cat = walker.current_category.clone();
                    walker.scopes.push(scope_kind, &name);
                    walker.opens.push(Open::ScopeWithCategory(prev_cat));
                    return true;
                }
            }
            false
        }
        "enum_specifier" => {
            // A named enum matches C: the name is `g`, each constant is `e`
            // scoped `enum:Name`. Anonymous enums emit nothing (as in C).
            if let Some(nm) = node.child_by_field_name("name") {
                let name = node_text(nm, source).to_string();
                emit_tag(walker, name.clone(), line, "g", |_| {});
                emit_enumerators(cursor, source, &name, walker);
            }
            false
        }
        "type_definition" => {
            // `typedef NS_ENUM(BaseType, EnumName) { ... }` — the macro carries
            // the enum name; emit it as an enum rather than a bogus typedef.
            if let Some(tn) = node.child_by_field_name("type") {
                if tn.kind() == "macro_type_specifier" {
                    let macro_name = tn
                        .child_by_field_name("name")
                        .map(|n| node_text(n, source))
                        .unwrap_or_default();
                    if macro_name == "NS_ENUM" || macro_name == "NS_OPTIONS" {
                        if let Some(ty) = tn.child_by_field_name("type") {
                            let enum_name = node_text(ty, source).trim().to_string();
                            if !enum_name.is_empty() {
                                emit_tag(walker, enum_name.clone(), line, "g", |_| {});
                                emit_nsenum_constants(cursor, source, &enum_name, walker);
                            }
                        }
                        return false;
                    }
                }
            }

            let mut name = None;
            let mut name_line = line;
            for_each_child!(cursor, {
                if cursor.field_name() == Some("declarator") {
                    name_line = line_of(cursor.node());
                    name = declarator_name(cursor, source).map(|(name, _)| name);
                    break;
                }
            });
            // Point the typedef at the line where its name is written (e.g. the
            // `} KlassCoordinate;` line of a typedef'd anonymous struct).
            if let Some(name) = name {
                emit_tag(walker, name.clone(), name_line, "t", |_| {});
                // Anonymous struct/union body: scope its members under the alias.
                if let Some(tn) = node.child_by_field_name("type") {
                    if matches!(tn.kind(), "struct_specifier" | "union_specifier")
                        && tn.child_by_field_name("name").is_none()
                        && tn.child_by_field_name("body").is_some()
                    {
                        let prev_cat = walker.current_category.clone();
                        walker.scopes.push(ScopeKind::Struct, &name);
                        walker.opens.push(Open::ScopeWithCategory(prev_cat));
                        return true;
                    }
                }
            }
            false
        }
        "function_definition" => {
            if walker.in_body == 0 {
                let mut function_name = None;
                for_each_child!(cursor, {
                    if cursor.field_name() == Some("declarator") {
                        if let Some((name, _)) = declarator_name(cursor, source) {
                            emit_tag(walker, name.clone(), line, "f", |_| {});
                            emit_function_params(cursor, source, &name, walker);
                            function_name = Some(name);
                        }
                        break;
                    }
                });
                if let Some(name) = function_name {
                    // Open a function scope so body locals/labels get a
                    // `function:name` field, as the C parser does.
                    let prev_cat = walker.current_category.clone();
                    walker.scopes.push(ScopeKind::Function, &name);
                    walker.opens.push(Open::ScopeWithCategory(prev_cat));
                    return true;
                }
            }
            false
        }
        "declaration" => {
            // Mirror the C parser's classification: function declarators are
            // prototypes (`p`); variables are `x` (extern), `l` (inside a body)
            // or `v` (file-scope global).
            let is_extern = has_extern_specifier(cursor, source);
            let in_body = walker.in_body > 0;
            let at_file_scope = !in_body && walker.scopes.current_field().is_none();
            for_each_child!(cursor, {
                if cursor.field_name() == Some("declarator") {
                    if let Some((name, is_func)) = declarator_name(cursor, source) {
                        if is_func {
                            emit_tag(walker, name.clone(), line, "p", |_| {});
                            emit_function_params(cursor, source, &name, walker);
                        } else {
                            let (kind, emit) = if is_extern {
                                ("x", !in_body)
                            } else if in_body {
                                ("l", true)
                            } else {
                                ("v", at_file_scope)
                            };
                            if emit {
                                emit_tag(walker, name, line, kind, |_| {});
                            }
                        }
                    }
                }
            });
            false
        }
        "preproc_include" | "preproc_import" => {
            if let Some((path, _)) =
                child_ident(cursor, source, &["string_literal", "system_lib_string"])
            {
                let trimmed = path
                    .trim_matches(|c| c == '"' || c == '<' || c == '>')
                    .to_string();
                emit_tag(walker, trimmed, line, "h", |_| {});
            }
            false
        }
        "labeled_statement" => {
            if let Some((name, _)) = child_ident(cursor, source, &["statement_identifier"]) {
                emit_tag(walker, name, line, "L", |_| {});
            }
            false
        }
        "compound_statement" => {
            walker.in_body += 1;
            walker.opens.push(Open::Body);
            true
        }
        _ => false,
    }
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
            let mut walker = ObjcWalker {
                base: Context {
                    source_code,
                    lines,
                    file_name: path.into(),
                    tags,
                    tag_config: kinds,
                    user_config: config,
                },
                scopes: ScopeStack::new(),
                current_category: None,
                in_body: 0,
                opens: Vec::new(),
            };
            walk_tree(cursor, &mut walker);
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::cpp::{C_KIND_DEFAULTS, C_KIND_OPTIONALS};
    use clap::Parser;

    fn objective_c_language() -> tree_sitter::Language {
        let engine = tree_sitter::wasmtime::Engine::default();
        let mut store = tree_sitter::WasmStore::new(&engine).unwrap();
        store
            .load_language(
                "objc",
                include_bytes!("../../tests/grammars/wasm/14/tree-sitter-objc.wasm"),
            )
            .unwrap()
    }

    #[test]
    fn c_kind_letters_do_not_drift() {
        assert_eq!(
            &KIND_DEFAULTS[..C_KIND_DEFAULTS.len()],
            C_KIND_DEFAULTS,
            "Objective-C's shared default kinds must match C"
        );
        assert_eq!(
            KIND_OPTIONALS, C_KIND_OPTIONALS,
            "Objective-C's optional kinds must match C"
        );
    }

    #[test]
    #[should_panic(expected = "Error loading grammar")]
    fn grammar_setup_failure_uses_shared_policy() {
        let mut parser = TsParser::new();
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        generate(
            &mut parser,
            objective_c_language(),
            b"@interface Example\n@end\n",
            "valid.m",
            &kinds,
            &crate::config::Config::parse_from(["treetags"]),
        );
    }
}
