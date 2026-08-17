wit_bindgen::generate!({
    world: "plugin-world",
    path: "../../wit",
});

use exports::treetags::plugin::plugin::{Guest, Request, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};
use treetags_plugin_common::{
    child_ident, for_each_child, line_of, node_text, walk_tree, ScopeKey, ScopeStack,
    TagKindConfig, WalkContext,
};

struct ObjcPlugin;

impl Guest for ObjcPlugin {
    fn generate(req: Request, source: Vec<u8>) -> Result<Vec<Tag>, String> {
        let mut parser = TsParser::new();
        let language: tree_sitter::Language = tree_sitter_objc::LANGUAGE.into();
        parser
            .set_language(&language)
            .map_err(|e| format!("set_language: {e}"))?;
        generate_tags(&mut parser, &req, &source)
    }
}

export!(ObjcPlugin);

// Objective-C is a strict superset of C, so constructs shared with C use the
// *exact* kind letters the C parser emits (`src/parser/cpp.rs` C_KIND_*), and
// Objective-C-only constructs use letters that do not clash with any C letter.
const OBJC_DEFAULT_KINDS: &[(&[&str], &str)] = &[
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
const OBJC_OPTIONAL_KINDS: &[(&[&str], &str)] = &[
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
    source: &'src [u8],
    scopes: ScopeStack<ScopeKind>,
    kinds: TagKindConfig,
    tags: Vec<Tag>,
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
        let source = self.source;
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

fn make_tag(name: String, line: u32, kind: &str, scope: Option<(&str, &str)>) -> Tag {
    let mut ext = vec![];
    if let Some((scope_key, scope_value)) = scope {
        ext.push((scope_key.to_string(), scope_value.to_string()));
    }
    Tag {
        name,
        line,
        kind: kind.to_string(),
        end_line: None,
        extension_fields: ext,
    }
}

/// Resolve the declared name of a (possibly nested) declarator, following the
/// `declarator` field so parameter names in function/function-pointer
/// declarators are skipped. Returns `(name, is_function_declarator)`.
fn declarator_name(node: Node, source: &[u8]) -> Option<(String, bool)> {
    declarator_name_inner(node, source, false)
}

fn declarator_name_inner(node: Node, source: &[u8], is_func: bool) -> Option<(String, bool)> {
    if matches!(
        node.kind(),
        "identifier" | "type_identifier" | "field_identifier"
    ) {
        return Some((node_text(node, source).to_string(), is_func));
    }
    let func = is_func || node.kind() == "function_declarator";
    if let Some(d) = node.child_by_field_name("declarator") {
        return declarator_name_inner(d, source, func);
    }
    // No `declarator` field: descend into an identifier-like child, or recurse
    // through a nested `*_declarator` that isn't attached via that field
    // (e.g. `parenthesized_declarator` -> `pointer_declarator`).
    let mut result = None;
    let mut cursor = node.walk();
    for_each_child!(cursor, {
        let child = cursor.node();
        let k = child.kind();
        if matches!(k, "identifier" | "field_identifier" | "type_identifier") {
            result = Some((node_text(child, source).to_string(), func));
            break;
        }
        if k.ends_with("_declarator") {
            if let Some(r) = declarator_name_inner(child, source, func) {
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
    kind: &str,
    scope: Option<(&str, &str)>,
    tags: &mut Vec<Tag>,
) {
    for_each_child!(cursor, {
        if cursor.node().kind() == "struct_declaration" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "struct_declarator" {
                    if let Some((name, _)) = declarator_name(cursor.node(), source) {
                        tags.push(make_tag(name, line, kind, scope));
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
    kind: &str,
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

    if walker.kinds.is_enabled(kind) {
        let scope = walker.scopes.current_field();
        let mut tag = make_tag(name.clone(), line, kind, scope);
        if let Some(cat) = &category {
            tag.extension_fields
                .push(("category".to_string(), cat.clone()));
        } else if let Some(p) = &protocols {
            tag.extension_fields
                .push(("protocols".to_string(), p.clone()));
        }
        walker.tags.push(tag);
    }

    let prev_cat = walker.current_category.clone();
    walker.scopes.push(scope_kind, &name);
    walker.opens.push(Open::ScopeWithCategory(prev_cat));

    if let Some(cat) = category {
        if walker.kinds.is_enabled("C") {
            let scope = walker.scopes.current_field();
            walker.tags.push(make_tag(cat.clone(), line, "C", scope));
        }
        walker.current_category = Some(cat);
    }
    true
}

/// The first `function_declarator` at or below `node` (declarators may be
/// wrapped, e.g. `pointer_declarator` -> `function_declarator`).
fn find_function_declarator(node: Node) -> Option<Node> {
    if node.kind() == "function_declarator" {
        return Some(node);
    }
    let mut cursor = node.walk();
    let mut found = None;
    for_each_child!(cursor, {
        if let Some(f) = find_function_declarator(cursor.node()) {
            found = Some(f);
            break;
        }
    });
    found
}

/// Emit a `z` (parameter) tag for each named parameter of the function/prototype
/// declarator `decl`, scoped under `function:fn_name` (matches the C parser).
fn emit_function_params(decl: Node, source: &[u8], fn_name: &str, tags: &mut Vec<Tag>) {
    let Some(func_decl) = find_function_declarator(decl) else {
        return;
    };
    let Some(params) = func_decl.child_by_field_name("parameters") else {
        return;
    };
    let mut cursor = params.walk();
    for_each_child!(cursor, {
        let param = cursor.node();
        if param.kind() == "parameter_declaration" {
            if let Some(d) = param.child_by_field_name("declarator") {
                if let Some((name, _)) = declarator_name(d, source) {
                    tags.push(make_tag(
                        name,
                        line_of(param),
                        "z",
                        Some(("function", fn_name)),
                    ));
                }
            }
        }
    });
}

/// Emit a `D` (macroparam) tag for each parameter of a function-like macro,
/// scoped under `macro:macro_name` (matches the C parser).
fn emit_macro_params(
    cursor: &mut TreeCursor,
    source: &[u8],
    macro_name: &str,
    tags: &mut Vec<Tag>,
) {
    for_each_child!(cursor, {
        if cursor.node().kind() == "preproc_params" {
            for_each_child!(cursor, {
                let p = cursor.node();
                if p.kind() == "identifier" {
                    let name = node_text(p, source).to_string();
                    tags.push(make_tag(name, line_of(p), "D", Some(("macro", macro_name))));
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

/// The first `enumerator_list` at or below `node`. For a plain `enum_specifier`
/// it is a direct child; for `typedef NS_ENUM(...) { ... }` it sits inside the
/// macro's expansion, so the search descends.
fn find_enumerator_list(node: Node) -> Option<Node> {
    if node.kind() == "enumerator_list" {
        return Some(node);
    }
    let mut cursor = node.walk();
    let mut found = None;
    for_each_child!(cursor, {
        if let Some(l) = find_enumerator_list(cursor.node()) {
            found = Some(l);
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
    tags: &mut Vec<Tag>,
) {
    for_each_child!(cursor, {
        let c = cursor.node();
        if c.kind() == "type_identifier" {
            let name = node_text(c, source).to_string();
            tags.push(make_tag(name, line_of(c), "e", Some(("enum", enum_name))));
        }
    });
}

/// Emit an `e` (enumerator) tag for each constant in `node`'s enumerator list,
/// scoped under `enum:enum_name` (matches the C parser).
fn emit_enumerators(node: Node, source: &[u8], enum_name: &str, tags: &mut Vec<Tag>) {
    let Some(list) = find_enumerator_list(node) else {
        return;
    };
    let mut cursor = list.walk();
    for_each_child!(cursor, {
        let e = cursor.node();
        if e.kind() == "enumerator" {
            if let Some((name, _)) = child_ident(&mut cursor, source, &["identifier"]) {
                tags.push(make_tag(name, line_of(e), "e", Some(("enum", enum_name))));
            }
        }
    });
}

fn process_node_inner(source: &[u8], cursor: &mut TreeCursor, walker: &mut ObjcWalker<'_>) -> bool {
    let node = cursor.node();
    let line = line_of(node);

    match node.kind() {
        "preproc_def" | "preproc_function_def" => {
            if let Some((name, _)) = child_ident(cursor, source, &["identifier"]) {
                if walker.kinds.is_enabled("d") {
                    walker.tags.push(make_tag(name.clone(), line, "d", None));
                }
                if node.kind() == "preproc_function_def" && walker.kinds.is_enabled("D") {
                    emit_macro_params(cursor, source, &name, &mut walker.tags);
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
            if walker.kinds.is_enabled("P") {
                let scope = walker.scopes.current_field();
                let mut tag = make_tag(name.clone(), line, "P", scope);
                if let Some(p) = &protocols {
                    tag.extension_fields
                        .push(("protocols".to_string(), p.clone()));
                }
                walker.tags.push(tag);
            }
            let prev_cat = walker.current_category.clone();
            walker.scopes.push(ScopeKind::Protocol, &name);
            walker.opens.push(Open::ScopeWithCategory(prev_cat));
            true
        }
        "method_declaration" | "method_definition" => {
            let is_class = node_text(node, source).trim_start().starts_with('+');
            let kind = if is_class { "c" } else { "M" };
            if walker.kinds.is_enabled(kind) {
                if let Some(name) = method_selector(cursor, source) {
                    let scope = walker.scopes.current_field();
                    let mut tag = make_tag(name, line, kind, scope);
                    if let Some(cat) = &walker.current_category {
                        tag.extension_fields
                            .push(("category".to_string(), cat.clone()));
                    }
                    walker.tags.push(tag);
                }
            }
            false
        }
        "property_declaration" => {
            if walker.kinds.is_enabled("A") {
                let scope = walker.scopes.current_field();
                emit_struct_declarators(cursor, source, line, "A", scope, &mut walker.tags);
            }
            false
        }
        "instance_variable" => {
            if walker.kinds.is_enabled("E") {
                let scope = walker.scopes.current_field();
                emit_struct_declarators(cursor, source, line, "E", scope, &mut walker.tags);
            }
            false
        }
        "field_declaration" => {
            if walker.kinds.is_enabled("m") {
                let scope = walker.scopes.current_field();
                for_each_child!(cursor, {
                    if cursor.field_name() == Some("declarator") {
                        if let Some((name, is_func)) = declarator_name(cursor.node(), source) {
                            if !is_func {
                                walker.tags.push(make_tag(name, line, "m", scope));
                            }
                        }
                    }
                });
            }
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
                    if walker.kinds.is_enabled(kind) {
                        let scope = walker.scopes.current_field();
                        walker.tags.push(make_tag(name.clone(), line, kind, scope));
                    }
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
                if walker.kinds.is_enabled("g") {
                    let scope = walker.scopes.current_field();
                    walker.tags.push(make_tag(name.clone(), line, "g", scope));
                }
                if walker.kinds.is_enabled("e") {
                    emit_enumerators(node, source, &name, &mut walker.tags);
                }
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
                                if walker.kinds.is_enabled("g") {
                                    let scope = walker.scopes.current_field();
                                    walker
                                        .tags
                                        .push(make_tag(enum_name.clone(), line, "g", scope));
                                }
                                if walker.kinds.is_enabled("e") {
                                    emit_nsenum_constants(
                                        cursor,
                                        source,
                                        &enum_name,
                                        &mut walker.tags,
                                    );
                                }
                            }
                        }
                        return false;
                    }
                }
            }

            let declarator = node.child_by_field_name("declarator");
            let name = declarator
                .and_then(|d| declarator_name(d, source))
                .map(|(n, _)| n);
            // Point the typedef at the line where its name is written (e.g. the
            // `} KlassCoordinate;` line of a typedef'd anonymous struct).
            let name_line = declarator.map(line_of).unwrap_or(line);
            if let Some(name) = name {
                if walker.kinds.is_enabled("t") {
                    let scope = walker.scopes.current_field();
                    walker
                        .tags
                        .push(make_tag(name.clone(), name_line, "t", scope));
                }
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
                if let Some(decl) = node.child_by_field_name("declarator") {
                    if let Some((name, _)) = declarator_name(decl, source) {
                        if walker.kinds.is_enabled("f") {
                            let scope = walker.scopes.current_field();
                            walker.tags.push(make_tag(name.clone(), line, "f", scope));
                        }
                        if walker.kinds.is_enabled("z") {
                            emit_function_params(decl, source, &name, &mut walker.tags);
                        }
                        // Open a function scope so body locals/labels get a
                        // `function:name` field, as the C parser does.
                        let prev_cat = walker.current_category.clone();
                        walker.scopes.push(ScopeKind::Function, &name);
                        walker.opens.push(Open::ScopeWithCategory(prev_cat));
                        return true;
                    }
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
                    let child = cursor.node();
                    if let Some((name, is_func)) = declarator_name(child, source) {
                        if is_func {
                            if walker.kinds.is_enabled("p") {
                                let scope = walker.scopes.current_field();
                                walker.tags.push(make_tag(name.clone(), line, "p", scope));
                            }
                            if walker.kinds.is_enabled("z") {
                                emit_function_params(child, source, &name, &mut walker.tags);
                            }
                        } else {
                            let (kind, emit) = if is_extern {
                                ("x", !in_body)
                            } else if in_body {
                                ("l", true)
                            } else {
                                ("v", at_file_scope)
                            };
                            if emit && walker.kinds.is_enabled(kind) {
                                let scope = walker.scopes.current_field();
                                walker.tags.push(make_tag(name, line, kind, scope));
                            }
                        }
                    }
                }
            });
            false
        }
        "preproc_include" | "preproc_import" => {
            if walker.kinds.is_enabled("h") {
                if let Some((path, _)) =
                    child_ident(cursor, source, &["string_literal", "system_lib_string"])
                {
                    let trimmed = path
                        .trim_matches(|c| c == '"' || c == '<' || c == '>')
                        .to_string();
                    walker.tags.push(make_tag(trimmed, line, "h", None));
                }
            }
            false
        }
        "labeled_statement" => {
            if walker.kinds.is_enabled("L") {
                if let Some((name, _)) = child_ident(cursor, source, &["statement_identifier"]) {
                    let scope = walker.scopes.current_field();
                    walker.tags.push(make_tag(name, line, "L", scope));
                }
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

fn generate_tags(parser: &mut TsParser, req: &Request, source: &[u8]) -> Result<Vec<Tag>, String> {
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "parse failed".to_string())?;

    let mut walker = ObjcWalker {
        source,
        scopes: ScopeStack::new(),
        kinds: TagKindConfig::parse(&req.kinds, OBJC_DEFAULT_KINDS, OBJC_OPTIONAL_KINDS),
        tags: Vec::new(),
        current_category: None,
        in_body: 0,
        opens: Vec::new(),
    };

    let mut cursor = tree.walk();
    if cursor.goto_first_child() {
        walk_tree(&mut cursor, &mut walker);
    }

    Ok(walker.tags)
}
