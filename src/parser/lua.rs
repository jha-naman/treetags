//! Lua function definitions and optional call references.
use super::common::{
    cursor::{line_of, node_text},
    scope::{ScopeKey, ScopeStack},
    scope_walker::{walk_tree, WalkContext},
    tree_walker::{generate_tags_with_config, Context},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};

pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[(&["f", "function"], "f")];
pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] = &[
    (&["Y", "unknown"], "Y"),
    (&["m", "method"], "m"),
    (&["l", "local"], "l"),
    (&["v", "global"], "v"),
    (&["F", "field"], "F"),
    (&["L", "label"], "L"),
];

#[derive(Clone, Copy)]
enum Scope {
    Function,
    Table,
}
impl ScopeKey for Scope {
    fn key(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Table => "table",
        }
    }
}

struct Walker<'a> {
    base: Context<'a>,
    scopes: ScopeStack<Scope>,
    bindings: Vec<HashSet<String>>,
    opens: Vec<Open>,
    local_assignment: Option<usize>,
    table_owners: HashMap<usize, String>,
}

enum Open {
    Function,
    Table,
    Lexical,
}

enum AssignmentValue {
    Function(Option<String>, u32),
    Table(usize),
    Other,
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(value)
}

// Build a static table path from identifiers and index nodes. Dynamic indexes
// cannot provide a reliable scope name.
fn table_path(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let node_kind = cursor.node().kind();
    match node_kind {
        "identifier" => Some(node_text(cursor.node(), source).to_owned()),
        "variable" | "parenthesized_expression" => {
            let mut result = None;
            for_each_child!(cursor, {
                if cursor.node().is_named() {
                    result = table_path(cursor, source);
                    break;
                }
            });
            result
        }
        "dot_index_expression" | "method_index_expression" | "bracket_index_expression" => {
            let mut table = None;
            let mut field = None;
            for_each_child!(cursor, {
                match cursor.field_name() {
                    Some("table") => table = table_path(cursor, source),
                    Some("field" | "method") => {
                        let child = cursor.node();
                        if child.kind() == "string"
                            || (child.kind() == "identifier"
                                && node_kind != "bracket_index_expression")
                        {
                            field = Some(unquote(node_text(child, source)).to_owned());
                        }
                    }
                    _ => {}
                }
            });
            Some(format!("{}.{}", table?, field?))
        }
        _ => None,
    }
}

// The last field or method is the tag name; the table expression is its scope.
fn name_and_table(cursor: &mut TreeCursor, source: &[u8]) -> Option<(String, Option<String>)> {
    let node = cursor.node();
    match node.kind() {
        "identifier" => Some((node_text(node, source).to_owned(), None)),
        "variable" => {
            let mut result = None;
            for_each_child!(cursor, {
                if cursor.node().is_named() {
                    result = name_and_table(cursor, source);
                    break;
                }
            });
            result
        }
        "dot_index_expression" | "method_index_expression" | "bracket_index_expression" => {
            let mut table = None;
            let mut name = None;
            for_each_child!(cursor, {
                match cursor.field_name() {
                    Some("table") => table = table_path(cursor, source),
                    Some("field" | "method") => {
                        let child = cursor.node();
                        if child.kind() == "string"
                            || (child.kind() == "identifier"
                                && node.kind() != "bracket_index_expression")
                        {
                            name = Some(unquote(node_text(child, source)).to_owned());
                        }
                    }
                    _ => {}
                }
            });
            Some((name?, table))
        }
        _ => None,
    }
}

fn parameters(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut result = None;
    for_each_child!(cursor, {
        if cursor.field_name() == Some("parameters") {
            result = Some(node_text(cursor.node(), source).to_owned());
            break;
        }
    });
    result
}

fn parameter_names(cursor: &mut TreeCursor, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for_each_child!(cursor, {
        if cursor.field_name() == Some("parameters") {
            for_each_child!(cursor, {
                if cursor.node().kind() == "identifier" {
                    names.push(node_text(cursor.node(), source).to_owned());
                }
            });
            break;
        }
    });
    names
}

fn variable_names(cursor: &mut TreeCursor, source: &[u8]) -> Vec<Option<(String, Option<String>)>> {
    let mut names = Vec::new();
    for_each_child!(cursor, {
        if cursor.field_name() == Some("name") {
            names.push(name_and_table(cursor, source));
        }
    });
    names
}

impl Walker<'_> {
    fn bind_local(&mut self, name: &str) {
        if let Some(frame) = self.bindings.last_mut() {
            frame.insert(name.to_owned());
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.bindings.iter().rev().any(|frame| frame.contains(name))
    }

    fn emit(
        &mut self,
        name: String,
        kind: &'static str,
        node: Node,
        signature: Option<String>,
        end: Option<u32>,
        table: Option<String>,
        local: bool,
        role: &'static str,
    ) {
        if name.is_empty() || !self.base.tag_config.is_kind_enabled(kind) {
            return;
        }
        if kind == "Y" && !self.base.user_config.extras_config.reference {
            return;
        }
        let config = &self.base.user_config.fields_config;
        let mut fields = ExtensionFields::new();
        if config.is_field_enabled("kind") {
            fields.insert("kind", kind);
        }
        if config.is_field_enabled("line") {
            fields.insert("line", line_of(node).to_string());
        }
        if config.is_field_enabled("language") {
            fields.insert("language", "Lua");
        }
        if let Some(end) = end {
            if config.is_field_enabled("end") {
                fields.insert("end", end.to_string());
            }
        }
        if config.is_field_enabled("scope") || self.base.user_config.extras_config.qualified {
            if let Some(table) = table {
                fields.insert("table", table);
            } else if let Some((key, value)) = self.scopes.current_field() {
                fields.insert(key, value.to_owned());
            }
        }
        if let Some(signature) = signature {
            if config.is_field_enabled("signature") {
                fields.insert("signature", signature);
            }
        }
        if local && config.is_field_enabled("file") {
            fields.insert("file", "");
        }
        if config.is_field_enabled("roles") {
            fields.insert("roles", role);
        }
        let line = line_of(node) as usize;
        self.base.tags.push(Tag {
            name,
            file_name: self.base.file_name.clone(),
            address: Tag::address_from_line(self.base.lines.get(line - 1).copied().unwrap_or(b"")),
            kind: Some(kind.into()),
            extension_fields: (!fields.is_empty()).then_some(fields),
        });
    }

    fn declaration(&mut self, cursor: &mut TreeCursor) -> bool {
        let node = cursor.node();
        let source = self.base.source_code.as_bytes();
        let local = cursor.field_name() == Some("local_declaration");
        let mut name = None;
        let mut method = false;
        for_each_child!(cursor, {
            if cursor.field_name() == Some("name") {
                method = cursor.node().kind() == "method_index_expression";
                name = name_and_table(cursor, source);
                break;
            }
        });
        let Some((name, table)) = name else {
            return false;
        };
        if local {
            self.bind_local(&name);
        }
        let signature = parameters(cursor, source);
        let mut params = parameter_names(cursor, source);
        if method {
            params.push("self".to_owned());
        }
        self.emit(
            name.clone(),
            if method && self.base.tag_config.is_kind_enabled("m") {
                "m"
            } else {
                "f"
            },
            node,
            signature,
            Some(node.end_position().row as u32 + 1),
            table.clone(),
            local,
            "def",
        );
        let path = table.map(|table| format!("{table}.{name}")).unwrap_or(name);
        self.scopes.push(Scope::Function, &path);
        self.bindings.push(params.into_iter().collect());
        self.opens.push(Open::Function);
        true
    }

    fn variable_declaration(&mut self, cursor: &mut TreeCursor) {
        let source = self.base.source_code.as_bytes();
        for_each_child!(cursor, {
            match cursor.node().kind() {
                "assignment_statement" => {
                    self.local_assignment = Some(cursor.node().id());
                    for_each_child!(cursor, {
                        if cursor.node().kind() == "variable_list" {
                            for name in variable_names(cursor, source).into_iter().flatten() {
                                self.bind_local(&name.0);
                            }
                            break;
                        }
                    });
                }
                "variable_list" => {
                    for name in variable_names(cursor, source).into_iter().flatten() {
                        self.bind_local(&name.0);
                        self.emit(name.0, "l", cursor.node(), None, None, None, true, "def");
                    }
                }
                _ => {}
            }
        });
    }

    fn for_statement(&mut self, cursor: &mut TreeCursor) {
        let node = cursor.node();
        let source = self.base.source_code.as_bytes();
        let mut names = Vec::new();
        for_each_child!(cursor, {
            if cursor.field_name() == Some("clause") {
                match cursor.node().kind() {
                    "for_numeric_clause" => for_each_child!(cursor, {
                        if cursor.field_name() == Some("name") {
                            names.push(node_text(cursor.node(), source).to_owned());
                        }
                    }),
                    "for_generic_clause" => for_each_child!(cursor, {
                        if cursor.node().kind() == "variable_list" {
                            names.extend(
                                variable_names(cursor, source)
                                    .into_iter()
                                    .flatten()
                                    .map(|name| name.0),
                            );
                        }
                    }),
                    _ => {}
                }
            }
        });
        self.bindings.push(names.iter().cloned().collect());
        self.opens.push(Open::Lexical);
        for name in names {
            self.emit(name, "l", node, None, None, None, true, "def");
        }
    }

    fn assignment(&mut self, cursor: &mut TreeCursor) {
        let node = cursor.node();
        let source = self.base.source_code.as_bytes();
        let local = self.local_assignment == Some(node.id());
        if local {
            self.local_assignment = None;
        }
        let mut names = Vec::new();
        let mut values = Vec::new();
        for_each_child!(cursor, {
            match cursor.node().kind() {
                "variable_list" => names = variable_names(cursor, source),
                "expression_list" => for_each_child!(cursor, {
                    if cursor.node().is_named() {
                        let value = match cursor.node().kind() {
                            "function_definition" => AssignmentValue::Function(
                                parameters(cursor, source),
                                cursor.node().end_position().row as u32 + 1,
                            ),
                            "table_constructor" => AssignmentValue::Table(cursor.node().id()),
                            _ => AssignmentValue::Other,
                        };
                        values.push(value);
                    }
                }),
                _ => {}
            }
        });
        let mut values = values.into_iter();
        for name in names {
            let value = values.next().unwrap_or(AssignmentValue::Other);
            if let Some((name, table)) = name {
                match value {
                    AssignmentValue::Function(signature, end) => {
                        self.emit(name, "f", node, signature, Some(end), table, local, "def")
                    }
                    AssignmentValue::Table(id) => {
                        let path = table
                            .as_ref()
                            .map(|table| format!("{table}.{name}"))
                            .unwrap_or_else(|| name.clone());
                        self.table_owners.insert(id, path);
                        if local {
                            self.emit(name, "l", node, None, None, None, true, "def");
                        } else if let Some(table) = table {
                            self.emit(name, "F", node, None, None, Some(table), false, "def");
                        } else if !self.is_local(&name) {
                            self.emit(name, "v", node, None, None, None, false, "def");
                        }
                    }
                    AssignmentValue::Other => {
                        if local {
                            self.emit(name, "l", node, None, None, None, true, "def");
                        } else if let Some(table) = table {
                            self.emit(name, "F", node, None, None, Some(table), false, "def");
                        } else if !self.is_local(&name) {
                            self.emit(name, "v", node, None, None, None, false, "def");
                        }
                    }
                }
            }
        }
    }

    fn field(&mut self, cursor: &mut TreeCursor) {
        let node = cursor.node();
        let source = self.base.source_code.as_bytes();
        let mut name = None;
        let mut function = None;
        let mut nested_table = None;
        for_each_child!(cursor, {
            match cursor.field_name() {
                Some("name") => {
                    let child = cursor.node();
                    if child.kind() == "identifier" || child.kind() == "string" {
                        name = Some(unquote(node_text(child, source)).to_owned());
                    }
                }
                Some("value") if cursor.node().kind() == "function_definition" => {
                    function = Some((
                        parameters(cursor, source),
                        cursor.node().end_position().row as u32 + 1,
                    ));
                }
                Some("value") if cursor.node().kind() == "table_constructor" => {
                    nested_table = Some(cursor.node().id());
                }
                _ => {}
            }
        });
        if let Some(name) = name {
            if let Some((signature, end)) = function {
                self.emit(name, "f", node, signature, Some(end), None, false, "def");
            } else {
                if let Some(id) = nested_table {
                    self.table_owners.insert(id, name.clone());
                }
                self.emit(name, "F", node, None, None, None, false, "def");
            }
        }
    }

    fn label(&mut self, cursor: &mut TreeCursor) {
        let node = cursor.node();
        let source = self.base.source_code.as_bytes();
        for_each_child!(cursor, {
            if cursor.node().kind() == "identifier" {
                self.emit(
                    node_text(cursor.node(), source).to_owned(),
                    "L",
                    node,
                    None,
                    None,
                    None,
                    false,
                    "def",
                );
                break;
            }
        });
    }

    fn call(&mut self, cursor: &mut TreeCursor) {
        if !self.base.user_config.extras_config.reference {
            return;
        }
        let node = cursor.node();
        let source = self.base.source_code.as_bytes();
        let mut name = None;
        for_each_child!(cursor, {
            if cursor.field_name() == Some("name") {
                name = name_and_table(cursor, source);
                break;
            }
        });
        if let Some((name, table)) = name {
            self.emit(name, "Y", node, None, None, table, false, "referenced");
        }
    }
}

impl WalkContext for Walker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        match cursor.node().kind() {
            "variable_declaration" => {
                self.variable_declaration(cursor);
                false
            }
            "function_declaration" => self.declaration(cursor),
            "for_statement" => {
                self.for_statement(cursor);
                true
            }
            "function_definition" => {
                let params = parameter_names(cursor, self.base.source_code.as_bytes());
                self.bindings.push(params.into_iter().collect());
                self.opens.push(Open::Lexical);
                true
            }
            "block" => {
                self.bindings.push(HashSet::new());
                self.opens.push(Open::Lexical);
                true
            }
            "table_constructor" => {
                if let Some(owner) = self.table_owners.remove(&cursor.node().id()) {
                    self.scopes.push(Scope::Table, &owner);
                    self.opens.push(Open::Table);
                    true
                } else {
                    false
                }
            }
            "assignment_statement" => {
                self.assignment(cursor);
                false
            }
            "field" => {
                self.field(cursor);
                false
            }
            "label_statement" => {
                self.label(cursor);
                false
            }
            "function_call" => {
                self.call(cursor);
                false
            }
            _ => false,
        }
    }
    fn pop_scope(&mut self) {
        match self.opens.pop() {
            Some(Open::Function) => {
                self.scopes.pop();
                self.bindings.pop();
            }
            Some(Open::Table) => self.scopes.pop(),
            Some(Open::Lexical) => {
                self.bindings.pop();
            }
            None => unreachable!("Lua walker scope stack out of sync"),
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
                scopes: ScopeStack::new(),
                bindings: vec![HashSet::new()],
                opens: Vec::new(),
                local_assignment: None,
                table_owners: HashMap::new(),
            };
            walk_tree(cursor, &mut walker);
        },
    )
}
