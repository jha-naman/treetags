use super::helper::{self, LanguageContext, TagKindConfig};
use crate::tag::{self, ExtensionFields};
use tree_sitter::{Node, TreeCursor};

pub(crate) const LANG_NAME: &str = "c#";
pub(crate) const LANG_EXTENSIONS: &[&str] = &["cs"];

pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["E", "event"], "E"),
    (&["c", "class"], "c"),
    (&["d", "macro"], "d"),
    (&["e", "enumerator"], "e"),
    (&["f", "field"], "f"),
    (&["g", "enum"], "g"),
    (&["i", "interface"], "i"),
    (&["m", "method"], "m"),
    (&["n", "namespace"], "n"),
    (&["p", "property"], "p"),
    (&["r", "record"], "r"),
    (&["s", "struct"], "s"),
    (&["t", "typedef"], "t"),
];
pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] = &[(&["l", "local"], "l")];

pub(crate) fn generate(
    parser: &mut tree_sitter::Parser,
    language: tree_sitter::Language,
    code: &[u8],
    path: &str,
    tag_config: &TagKindConfig,
    config: &crate::config::Config,
) -> Option<Vec<tag::Tag>> {
    helper::generate_tags_with_config(
        parser,
        language,
        code,
        path,
        |source_code, lines, cursor, tags| {
            let mut context = CSharpContext {
                base: helper::Context {
                    source_code,
                    lines,
                    file_name: path.into(),
                    tags,
                    tag_config,
                    user_config: config,
                },
                scopes: Vec::new(),
            };
            helper::walk_generic(cursor, &mut context);
        },
    )
}

#[derive(Debug)]
enum ScopeType {
    Namespace,
    Class,
    Struct,
    Interface,
    Enum,
    Record,
    Method,
}

struct CSharpContext<'a> {
    base: helper::Context<'a>,
    /// Names are stored fully qualified because C# ctags scopes use dots.
    scopes: Vec<(ScopeType, String)>,
}

impl CSharpContext<'_> {
    fn enclosing_name(&self) -> Option<&str> {
        self.scopes.last().map(|(_, name)| name.as_str())
    }

    fn qualify(&self, name: &str) -> String {
        match self.enclosing_name() {
            Some(parent) => format!("{parent}.{name}"),
            None => name.to_string(),
        }
    }

    fn add_tag(&mut self, name: String, kind: &'static str, node: Node, file_scoped: bool) {
        if name.is_empty() || !self.base.tag_config.is_kind_enabled(kind) {
            return;
        }

        let row = node.start_position().row;
        let mut fields = ExtensionFields::new();
        if self.base.user_config.fields_config.is_field_enabled("kind") {
            fields.insert("kind", kind);
        }
        if self.base.user_config.fields_config.is_field_enabled("line") {
            fields.insert("line", (row + 1).to_string());
        }
        if self
            .base
            .user_config
            .fields_config
            .is_field_enabled("scope")
        {
            if let Some((scope_type, scope_name)) = self.scopes.last() {
                let scope = match scope_type {
                    ScopeType::Namespace => Some(("n", "namespace")),
                    ScopeType::Class => Some(("c", "class")),
                    ScopeType::Struct => Some(("s", "struct")),
                    ScopeType::Interface => Some(("i", "interface")),
                    ScopeType::Enum => Some(("g", "enum")),
                    ScopeType::Record => Some(("r", "record")),
                    // Universal ctags does not attach method scopes to C# locals.
                    ScopeType::Method => None,
                };
                if let Some((scope_kind, key)) = scope {
                    if self.base.tag_config.is_kind_enabled(scope_kind) {
                        fields.insert(key, scope_name.clone());
                    }
                }
            }
        }
        if file_scoped {
            fields.insert("file", "");
        }
        if self.base.user_config.fields_config.is_field_enabled("end")
            && node.end_position().row > row
        {
            fields.insert("end", (node.end_position().row + 1).to_string());
        }

        self.base.tags.push(tag::Tag {
            name,
            file_name: self.base.file_name.clone(),
            address: helper::address_string_from_line(row, &self.base),
            kind: Some(kind.into()),
            extension_fields: (!fields.is_empty()).then_some(fields),
        });
    }
}

impl LanguageContext for CSharpContext<'_> {
    type ScopeType = ScopeType;

    fn push_scope(&mut self, scope_type: ScopeType, name: String) {
        self.scopes.push((scope_type, name));
    }

    fn pop_scope(&mut self) -> Option<(ScopeType, String)> {
        self.scopes.pop()
    }

    fn process_node(&mut self, cursor: &mut TreeCursor) -> Option<(ScopeType, String)> {
        process_node(cursor, self)
    }
}

fn declaration(
    cursor: &mut TreeCursor,
    context: &mut CSharpContext,
    kind: &'static str,
    scope_type: ScopeType,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let name = helper::get_node_name(cursor, &context.base, &["identifier"])?;
    context.add_tag(name.clone(), kind, node, false);
    Some((scope_type, context.qualify(&name)))
}

fn process_node(
    cursor: &mut TreeCursor,
    context: &mut CSharpContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    match node.kind() {
        "namespace_declaration" | "file_scoped_namespace_declaration" => {
            let name =
                helper::get_node_name(cursor, &context.base, &["identifier", "qualified_name"])?;
            context.add_tag(name.clone(), "n", node, false);
            Some((ScopeType::Namespace, context.qualify(&name)))
        }
        "class_declaration" => declaration(cursor, context, "c", ScopeType::Class),
        "struct_declaration" => declaration(cursor, context, "s", ScopeType::Struct),
        "interface_declaration" => declaration(cursor, context, "i", ScopeType::Interface),
        "enum_declaration" => declaration(cursor, context, "g", ScopeType::Enum),
        "record_declaration" => declaration(cursor, context, "r", ScopeType::Record),
        "method_declaration" | "local_function_statement" => {
            let name = helper::get_node_name(cursor, &context.base, &["identifier"])?;
            context.add_tag(name.clone(), "m", node, false);
            Some((ScopeType::Method, context.qualify(&name)))
        }
        "constructor_declaration" => {
            let name = helper::get_node_name(cursor, &context.base, &["identifier"])?;
            // A constructor without an accessibility modifier is private.
            let text = context.base.node_text(&node).trim_start();
            let file_scoped = !["public ", "protected ", "internal "]
                .iter()
                .any(|modifier| text.starts_with(modifier));
            context.add_tag(name.clone(), "m", node, file_scoped);
            Some((ScopeType::Method, context.qualify(&name)))
        }
        "property_declaration" => {
            if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
                context.add_tag(name, "p", node, false);
            }
            None
        }
        "delegate_declaration" => {
            if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
                context.add_tag(name, "m", node, false);
            }
            None
        }
        "enum_member_declaration" => {
            if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
                context.add_tag(name, "e", node, true);
            }
            None
        }
        "variable_declarator" => {
            let name = helper::get_node_name(cursor, &context.base, &["identifier"]);
            if matches!(context.scopes.last(), Some((ScopeType::Method, _))) {
                if let Some(name) = name {
                    context.add_tag(name, "l", node, false);
                }
                return None;
            }
            let mut ancestor = node.parent();
            let mut kind = None;
            while let Some(parent) = ancestor {
                match parent.kind() {
                    "field_declaration" => {
                        kind = Some("f");
                        break;
                    }
                    "event_field_declaration" => {
                        kind = Some("E");
                        break;
                    }
                    "local_declaration_statement" => {
                        kind = Some("l");
                        break;
                    }
                    "method_declaration"
                    | "constructor_declaration"
                    | "local_function_statement" => break,
                    _ => ancestor = parent.parent(),
                }
            }
            if let (Some(name), Some(kind)) = (name, kind) {
                let address_node = ancestor.unwrap_or(node);
                context.add_tag(name, kind, address_node, false);
            }
            None
        }
        // Error recovery for an incomplete record declaration. When a record has
        // no body, the grammar can attach a following method's body as the
        // record's declaration list.
        // A well-formed record body is walked normally (this branch is
        // skipped), so its members are scoped to the record instead.
        "declaration_list"
            if node.parent().is_some_and(|parent| {
                parent.kind() == "record_declaration" && parent.has_error()
            }) =>
        {
            let line = context.base.lines[node.start_position().row];
            let line = String::from_utf8_lossy(line);
            let name = line
                .split_once('(')
                .and_then(|(prefix, _)| prefix.split_whitespace().last())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                None
            } else {
                context.add_tag(name.clone(), "m", node, false);
                Some((ScopeType::Method, context.qualify(&name)))
            }
        }
        // C# preprocessor symbols are represented by define directives.
        "define_directive" => {
            if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
                context.add_tag(name, "d", node, true);
            }
            None
        }
        _ => None,
    }
}
