//! PHP definitions and ctags extension fields from the bundled grammar.
use super::common::{
    cursor::{line_of, node_text},
    scope_walker::{walk_tree, WalkContext},
    tree_walker::{generate_tags_with_config, Context},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};

pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["a", "alias"], "a"),
    (&["c", "class"], "c"),
    (&["d", "define"], "d"),
    (&["f", "function"], "f"),
    (&["i", "interface"], "i"),
    (&["n", "namespace"], "n"),
    (&["t", "trait"], "t"),
    (&["v", "variable"], "v"),
];
pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] = &[(&["l", "local"], "l")];

#[derive(Clone, Copy)]
enum ScopeKind {
    Class,
    Interface,
    Trait,
    Function,
}

impl ScopeKind {
    fn field(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Trait => "trait",
            Self::Function => "function",
        }
    }
}

enum Frame {
    Namespace(String),
    Symbol(ScopeKind, String),
}

struct Walker<'a> {
    base: Context<'a>,
    namespace: String,
    frames: Vec<Frame>,
}

fn named_child<'tree>(cursor: &mut TreeCursor<'tree>) -> Option<Node<'tree>> {
    let mut found = None;
    for_each_child!(cursor, {
        if cursor.node().is_named() {
            found = Some(cursor.node());
            break;
        }
    });
    found
}

fn text<'a>(node: Node<'a>, source: &'a str) -> &'a str {
    node_text(node, source.as_bytes())
}

fn identifier(node: Node, source: &str) -> String {
    text(node, source).trim_start_matches('$').to_owned()
}

impl Walker<'_> {
    fn scope(&self) -> Option<(&'static str, String)> {
        let mut path = self.namespace.clone();
        let mut kind = if path.is_empty() {
            None
        } else {
            Some("namespace")
        };
        for frame in &self.frames {
            if let Frame::Symbol(next_kind, name) = frame {
                if !path.is_empty() {
                    path.push_str(
                        if matches!(next_kind, ScopeKind::Function)
                            && !matches!(kind, Some("namespace"))
                        {
                            "::"
                        } else {
                            "\\"
                        },
                    );
                }
                path.push_str(name);
                kind = Some(next_kind.field());
            }
        }
        kind.map(|kind| (kind, path))
    }

    fn emit(
        &mut self,
        name: String,
        kind: &'static str,
        node: Node,
        end: Option<Node>,
        extra: &[(&'static str, String)],
    ) {
        if name.is_empty() || !self.base.tag_config.is_kind_enabled(kind) {
            return;
        }
        let config = &self.base.user_config.fields_config;
        let line = line_of(node) as usize;
        let mut fields = ExtensionFields::new();
        if config.is_field_enabled("kind") {
            fields.insert("kind", kind);
        }
        if config.is_field_enabled("line") {
            fields.insert("line", line.to_string());
        }
        if config.is_field_enabled("language") {
            fields.insert("language", "PHP");
        }
        if config.is_field_enabled("end") {
            if let Some(end) = end {
                fields.insert("end", (end.end_position().row + 1).to_string());
            }
        }
        if config.is_field_enabled("scope") || self.base.user_config.extras_config.qualified {
            if let Some((key, value)) = self.scope() {
                fields.insert(key, value);
            }
        }
        for (key, value) in extra {
            if config.is_field_enabled(key) {
                fields.insert(*key, value.clone());
            }
        }
        self.base.tags.push(Tag {
            name,
            file_name: self.base.file_name.clone(),
            address: Tag::address_from_line(self.base.lines.get(line - 1).copied().unwrap_or(b"")),
            kind: Some(kind.into()),
            extension_fields: (!fields.is_empty()).then_some(fields),
        });
    }

    fn declaration(
        &mut self,
        cursor: &mut TreeCursor,
        kind: &'static str,
        scope: ScopeKind,
    ) -> bool {
        let node = cursor.node();
        let Some(name_node) = node.child_by_field_name("name") else {
            return false;
        };
        let name = text(name_node, self.base.source_code).to_owned();
        let mut extra = Vec::new();
        if kind == "c" || kind == "i" {
            let mut parents = Vec::new();
            for_each_child!(cursor, {
                if matches!(
                    cursor.node().kind(),
                    "base_clause" | "class_interface_clause"
                ) {
                    for_each_child!(cursor, {
                        if matches!(cursor.node().kind(), "name" | "qualified_name") {
                            parents.push(text(cursor.node(), self.base.source_code).to_owned());
                        }
                    });
                }
            });
            if !parents.is_empty() {
                extra.push(("inherits", parents.join(",")));
            }
        }
        if kind == "c"
            && (0..node.child_count())
                .filter_map(|i| node.child(i))
                .any(|child| child.kind() == "abstract_modifier")
        {
            extra.push(("implementation", "abstract".to_owned()));
        }
        self.emit(name.clone(), kind, node, Some(node), &extra);
        self.frames.push(Frame::Symbol(scope, name));
        true
    }

    fn routine(&mut self, node: Node) -> bool {
        let Some(name_node) = node.child_by_field_name("name") else {
            return false;
        };
        let name = text(name_node, self.base.source_code).to_owned();
        let mut extra = Vec::new();
        if let Some(return_type) = node.child_by_field_name("return_type") {
            extra.push((
                "typeref",
                format!("unknown:{}", text(return_type, self.base.source_code)),
            ));
        }
        if node.kind() == "method_declaration" {
            if let Some(access) = (0..node.child_count())
                .filter_map(|i| node.child(i))
                .find(|child| child.kind() == "visibility_modifier")
            {
                extra.push(("access", text(access, self.base.source_code).to_owned()));
            }
            if (0..node.child_count())
                .filter_map(|i| node.child(i))
                .any(|child| child.kind() == "abstract_modifier")
            {
                extra.push(("implementation", "abstract".to_owned()));
            }
        }
        if let Some(params) = node.child_by_field_name("parameters") {
            extra.push(("signature", text(params, self.base.source_code).to_owned()));
        }
        self.emit(name.clone(), "f", node, Some(node), &extra);
        self.frames.push(Frame::Symbol(ScopeKind::Function, name));
        true
    }

    fn namespace_use(&mut self, cursor: &mut TreeCursor) {
        let node = cursor.node();
        let alias = node.child_by_field_name("alias");
        let Some(target) = named_child(cursor) else {
            return;
        };
        let mut target_text = text(target, self.base.source_code).to_owned();
        if let Some(group) = node.parent().filter(|n| n.kind() == "namespace_use_group") {
            if let Some(prefix) = group.parent().and_then(|n| {
                (0..n.child_count())
                    .filter_map(|i| n.child(i))
                    .find(|c| c.kind() == "namespace_name")
            }) {
                target_text = format!("{}\\{}", text(prefix, self.base.source_code), target_text);
            }
        }
        let name = alias
            .map(|n| text(n, self.base.source_code).to_owned())
            .unwrap_or_else(|| target_text.rsplit('\\').next().unwrap_or("").to_owned());
        self.emit(
            name,
            "a",
            node.parent().unwrap_or(node),
            None,
            &[("typeref", format!("unknown:{target_text}"))],
        );
    }

    fn define_call(&mut self, node: Node) {
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if text(function, self.base.source_code).trim_start_matches('\\') != "define" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let Some(first) = (0..args.child_count())
            .filter_map(|i| args.child(i))
            .find(|child| child.is_named())
        else {
            return;
        };
        let name = text(first, self.base.source_code)
            .trim_matches(['\'', '"'])
            .to_owned();
        self.emit(name, "d", node, None, &[]);
    }
}

impl WalkContext for Walker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        let node = cursor.node();
        match node.kind() {
            "namespace_definition" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.base.source_code).to_owned())
                    .unwrap_or_default();
                self.emit(name.clone(), "n", node, Some(node), &[]);
                let previous = std::mem::replace(&mut self.namespace, name);
                if node.child_by_field_name("body").is_some() {
                    self.frames.push(Frame::Namespace(previous));
                    true
                } else {
                    false
                }
            }
            "class_declaration" => self.declaration(cursor, "c", ScopeKind::Class),
            "interface_declaration" => self.declaration(cursor, "i", ScopeKind::Interface),
            "trait_declaration" => self.declaration(cursor, "t", ScopeKind::Trait),
            "function_definition" | "method_declaration" => self.routine(node),
            "namespace_use_clause" => {
                self.namespace_use(cursor);
                false
            }
            "const_element" => {
                if let Some(name) = named_child(cursor) {
                    self.emit(
                        text(name, self.base.source_code).to_owned(),
                        "d",
                        node.parent().unwrap_or(node),
                        None,
                        &[],
                    );
                }
                false
            }
            "property_element" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let mut extra = Vec::new();
                    if let Some(parent) = node.parent() {
                        if let Some(access) = (0..parent.child_count())
                            .filter_map(|i| parent.child(i))
                            .find(|child| child.kind() == "visibility_modifier")
                        {
                            extra.push(("access", text(access, self.base.source_code).to_owned()));
                        }
                    }
                    self.emit(
                        identifier(name, self.base.source_code),
                        "v",
                        node.parent().unwrap_or(node),
                        None,
                        &extra,
                    );
                }
                false
            }
            "assignment_expression" => {
                if let Some(left) = node.child_by_field_name("left") {
                    if left.kind() == "variable_name" {
                        let local = self
                            .frames
                            .iter()
                            .any(|frame| matches!(frame, Frame::Symbol(ScopeKind::Function, _)));
                        self.emit(
                            identifier(left, self.base.source_code),
                            if local { "l" } else { "v" },
                            node,
                            None,
                            &[],
                        );
                    }
                }
                false
            }
            "static_variable_declaration" => {
                if let Some(name) = node.child_by_field_name("name") {
                    if self
                        .frames
                        .iter()
                        .any(|frame| matches!(frame, Frame::Symbol(ScopeKind::Function, _)))
                    {
                        self.emit(
                            identifier(name, self.base.source_code),
                            "l",
                            node,
                            None,
                            &[],
                        );
                    }
                }
                false
            }
            "function_call_expression" => {
                self.define_call(node);
                false
            }
            _ => false,
        }
    }

    fn pop_scope(&mut self) {
        if let Some(Frame::Namespace(previous)) = self.frames.pop() {
            self.namespace = previous;
        }
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
            let mut walker = Walker {
                base: Context {
                    source_code,
                    lines,
                    file_name: path.into(),
                    tags,
                    tag_config: kinds,
                    user_config: config,
                },
                namespace: String::new(),
                frames: Vec::new(),
            };
            walk_tree(cursor, &mut walker);
        },
    )
}
