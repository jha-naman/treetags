//! OCaml definitions from the official tree-sitter grammar.
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

pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["C", "Constructor"], "C"),
    (&["M", "module"], "M"),
    (&["c", "class"], "c"),
    (&["e", "Exception"], "e"),
    (&["f", "function"], "f"),
    (&["m", "method"], "m"),
    (&["p", "val"], "p"),
    (&["r", "RecordField"], "r"),
    (&["t", "type"], "t"),
    (&["v", "var"], "v"),
];

#[derive(Clone, Copy)]
enum Scope {
    Module,
    Class,
    Type,
}

impl ScopeKey for Scope {
    fn key(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Class => "class",
            Self::Type => "type",
        }
    }
}

struct Walker<'a> {
    base: Context<'a>,
    scopes: ScopeStack<Scope>,
}

fn first_child<'tree>(cursor: &mut TreeCursor<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    let mut found = None;
    for_each_child!(cursor, {
        let child = cursor.node();
        if kinds.contains(&child.kind()) {
            found = Some(child);
            break;
        }
    });
    found
}

fn collect_pattern_names(cursor: &mut TreeCursor, source: &[u8], names: &mut Vec<String>) {
    if cursor.node().kind() == "value_name" {
        names.push(node_text(cursor.node(), source).to_owned());
        return;
    }
    for_each_child!(cursor, {
        collect_pattern_names(cursor, source, names);
    });
}

impl Walker<'_> {
    fn emit(&mut self, name: &str, kind: &'static str, node: Node) {
        if name.is_empty() || name == "_" || !self.base.tag_config.is_kind_enabled(kind) {
            return;
        }
        let line = line_of(node) as usize;
        let config = &self.base.user_config.fields_config;
        let mut fields = ExtensionFields::new();
        if config.is_field_enabled("kind") {
            fields.insert("kind", kind);
        }
        if config.is_field_enabled("line") {
            fields.insert("line", line.to_string());
        }
        if config.is_field_enabled("language") {
            fields.insert("language", "OCaml");
        }
        if let Some((key, value)) = self.scopes.current_field() {
            if config.is_field_enabled("scope") || self.base.user_config.extras_config.qualified {
                fields.insert(key, value.replace('.', "/"));
            }
        }
        self.base.tags.push(Tag {
            name: name.to_owned(),
            file_name: self.base.file_name.clone(),
            address: Tag::address_from_line(self.base.lines.get(line - 1).copied().unwrap_or(b"")),
            kind: Some(kind.into()),
            extension_fields: (!fields.is_empty()).then_some(fields),
        });
    }

    fn named(
        &mut self,
        cursor: &mut TreeCursor,
        field: &str,
        kind: &'static str,
        scope: Option<Scope>,
    ) -> bool {
        let node = cursor.node();
        let Some(name_node) = field_child(cursor, field) else {
            return false;
        };
        let name = node_text(name_node, self.base.source_code.as_bytes()).to_owned();
        self.emit(&name, kind, node);
        if let Some(scope) = scope {
            self.scopes.push(scope, &name);
            return true;
        }
        false
    }

    fn direct(&mut self, cursor: &mut TreeCursor, names: &[&str], kind: &'static str) {
        let node = cursor.node();
        if let Some(name) = first_child(cursor, names) {
            let text = node_text(name, self.base.source_code.as_bytes()).to_owned();
            self.emit(&text, kind, node);
        }
    }

    fn binding(&mut self, cursor: &mut TreeCursor) {
        let node = cursor.node();
        // OCaml's `let ... in` uses the same binding node as a structure item.
        // Only structure bindings are global ctags functions and variables.
        let mut parent = node.parent();
        while let Some(ancestor) = parent {
            if matches!(ancestor.kind(), "let_binding" | "method_definition") {
                return;
            }
            parent = ancestor.parent();
        }
        let Some(pattern) = field_child(cursor, "pattern") else {
            return;
        };
        let mut names = Vec::new();
        for_each_child!(cursor, {
            if cursor.node() == pattern {
                if cursor.node().kind() == "parenthesized_operator" {
                    names.push(
                        node_text(cursor.node(), self.base.source_code.as_bytes())
                            .trim_start_matches('(')
                            .trim_end_matches(')')
                            .trim()
                            .to_owned(),
                    );
                } else {
                    collect_pattern_names(cursor, self.base.source_code.as_bytes(), &mut names);
                }
                break;
            }
        });
        let mut function = false;
        for_each_child!(cursor, {
            if cursor.node().kind().contains("parameter") {
                function = true;
            }
            if cursor.field_name() == Some("body")
                && matches!(
                    cursor.node().kind(),
                    "fun_expression" | "function_expression"
                )
            {
                function = true;
            }
        });
        for name in names {
            self.emit(&name, if function { "f" } else { "v" }, node);
        }
    }
}

impl WalkContext for Walker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        match cursor.node().kind() {
            "module_binding" => return self.named(cursor, "name", "M", Some(Scope::Module)),
            "module_type_definition" => {
                return self.named(cursor, "name", "M", Some(Scope::Module));
            }
            "class_binding" | "class_type_binding" => {
                return self.named(cursor, "name", "c", Some(Scope::Class));
            }
            "type_binding" => return self.named(cursor, "name", "t", Some(Scope::Type)),
            "let_binding" => self.binding(cursor),
            "value_specification" => {
                self.direct(cursor, &["value_name", "parenthesized_operator"], "p")
            }
            "external" => self.direct(cursor, &["value_name", "parenthesized_operator"], "f"),
            "constructor_declaration" => {
                if cursor
                    .node()
                    .parent()
                    .is_some_and(|parent| parent.kind() == "exception_definition")
                {
                    self.direct(cursor, &["constructor_name"], "e");
                } else {
                    self.direct(cursor, &["constructor_name"], "C");
                }
            }
            "tag_specification" => self.direct(cursor, &["tag"], "C"),
            "field_declaration" => self.direct(cursor, &["field_name"], "r"),
            "method_definition" => {
                self.named(cursor, "name", "m", None);
            }
            "method_specification" => self.direct(cursor, &["method_name"], "m"),
            _ => {}
        }
        false
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
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
                scopes: ScopeStack::new(),
            };
            if !source_code.trim().is_empty() {
                if let Some(stem) = std::path::Path::new(path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                {
                    let mut chars = stem.chars();
                    if let Some(first) = chars.next() {
                        let name = first.to_uppercase().collect::<String>() + chars.as_str();
                        walker.emit(&name, "M", cursor.node());
                    }
                }
            }
            walk_tree(cursor, &mut walker);
        },
    )
}
