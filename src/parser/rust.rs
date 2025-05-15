use super::Parser;
use std::collections::HashMap;
use tree_sitter::{Node, TreeCursor};

use crate::{split_by_newlines, tag};

// Represents the type of scope for context tracking
#[derive(Debug)]
enum ScopeType {
    Module,
    Struct,
    Enum,
    Union,
    Trait,
    Implementation, // Can store both trait and type info if needed
}

// Stores context during traversal
struct Context<'a> {
    source_code: &'a str,
    lines: Vec<Vec<u8>>,
    file_name: &'a str,
    tags: &'a mut Vec<tag::Tag>,
    // Use a stack to keep track of nested scopes (module, struct, trait, etc.)
    scope_stack: Vec<(ScopeType, String)>,
}

impl<'a> Context<'a> {
    // Helper to get the text content of a node
    fn node_text(&self, node: &Node) -> &'a str {
        node.utf8_text(self.source_code.as_bytes())
            .unwrap_or_else(|_| {
                eprintln!(
                    "Warning: Failed to get UTF-8 text for node at range {:?}-{:?}",
                    node.start_position(),
                    node.end_position()
                );
                "" // Return empty string on error
            })
    }

    // Build extension fields based on the current scope stack
    fn create_extension_fields(&self) -> HashMap<String, String> {
        let mut fields = HashMap::new();
        let mut module_path = Vec::new();

        for (scope_type, name) in &self.scope_stack {
            match scope_type {
                ScopeType::Module => module_path.push(name.clone()),
                ScopeType::Struct => {
                    fields.insert(String::from("struct"), name.clone());
                }
                ScopeType::Enum => {
                    fields.insert(String::from("enum"), name.clone());
                }
                ScopeType::Union => {
                    fields.insert(String::from("union"), name.clone());
                }
                ScopeType::Trait => {
                    fields.insert(String::from("trait"), name.clone());
                }
                ScopeType::Implementation => {
                    // For impls, store the type being implemented.
                    // If it's a trait impl, the trait name might already be present
                    // from a Trait scope, or we can add it explicitly here if needed.
                    // Let's store the impl type under 'implementation' or 'impl_for'.
                    // Example `impl MyTrait for MyType` might have trait:MyTrait, implementation:MyType
                    // Example `impl MyType` would have implementation:MyType
                    fields.insert(String::from("implementation"), name.clone());
                    // Could also store the trait specifically if parsed:
                    // if let Some(trait_name) = find_trait_in_impl_scope(...) {
                    //     fields.insert(String::from("trait"), trait_name);
                    // }
                }
            }
        }

        if !module_path.is_empty() {
            fields.insert(String::from("module"), module_path.join("::"));
        }

        fields
    }
}

impl Parser {
    pub fn generate_rust_tags(
        &mut self,
        code: &[u8], // Changed to slice for flexibility
        file_path_relative_to_tag_file: &str,
    ) -> Option<Vec<tag::Tag>> {
        // Ensure the code is valid UTF-8
        let source_code = match std::str::from_utf8(code) {
            Ok(s) => s,
            Err(_) => {
                eprintln!(
                    "Warning: Input for {} is not valid UTF-8, skipping.",
                    file_path_relative_to_tag_file
                );
                return None;
            }
        };

        // Split the source code into lines for address generation
        let lines = split_by_newlines::split_by_newlines(code);

        self.ts_parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("Error loading Rust grammar");

        // Parse the source code using the parser from self
        let tree = self.ts_parser.parse(source_code, None)?; // Use ? for concise error handling

        let mut tags = Vec::new();
        {
            // New scope for cursor and context to avoid lifetime issues when returning tags
            let mut cursor = tree.walk();

            // Need to move the cursor to the root node's first child to start traversal
            if !cursor.goto_first_child() {
                return Some(tags); // Empty file or parse error
            }

            let mut context = Context {
                file_name: file_path_relative_to_tag_file,
                lines,
                source_code,
                tags: &mut tags,
                scope_stack: vec![], // Initialize empty scope stack
            };

            walk(&mut cursor, &mut context);
        }

        Some(tags)
    }
}

// Depth-First Tree Traversal with Scope Management
fn walk(cursor: &mut TreeCursor, context: &mut Context) {
    loop {
        let node = cursor.node();

        // --- 1. Process the current node ---
        // Returns Option<(ScopeType, Name)> if a new scope is entered
        let scope_info = process_node(cursor, context);

        // --- 2. Manage Scope Stack ---
        let mut scope_pushed = false;
        if let Some((scope_type, scope_name)) = scope_info {
            // Avoid pushing empty names which can happen for anonymous blocks
            if !scope_name.is_empty() {
                context.scope_stack.push((scope_type, scope_name));
                scope_pushed = true;
            }
        }

        // --- 3. Recurse into children ---
        if cursor.goto_first_child() {
            walk(cursor, context);
        }

        // --- 4. Move to the next sibling ---
        if !cursor.goto_next_sibling() {
            cursor.goto_parent(); // Return cursor to current node after visiting children
            break; // No more siblings, return to parent level
        }

        // --- 5. Pop Scope if necessary ---
        if scope_pushed && context.scope_stack.pop().is_none() {
            // This should ideally not happen if push/pop logic is correct
            eprintln!(
                "Warning: Popped from empty scope stack! Node: {:?}",
                node.kind()
            );
        }
    }
}

// Dispatches node processing based on kind, returns scope info if node defines one
fn process_node(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    // Returns (ScopeType, Name) if scope changes
    let node = cursor.node();
    // println!("Node: {} @ {:?} Scope: {:?}", node.kind(), String::from_utf8_lossy(&context.lines[node.start_position().row]), context.scope_stack); // Debug print

    match node.kind() {
        "mod_item" => process_module(cursor, context),
        "struct_item" => process_struct(cursor, context),
        "enum_item" => process_enum(cursor, context),
        "union_item" => process_union(cursor, context),
        "trait_item" => process_trait(cursor, context),
        "impl_item" => process_impl(cursor, context),

        // Items often found inside traits or impls
        "function_item" => {
            // Standalone functions or methods in impls
            process_function(cursor, context, "P"); // 'f' for function/method
            None // Doesn't start a new named scope for context stack
        }
        "function_signature_item" => {
            // Function signatures within traits
            process_function(cursor, context, "P"); // Also tag as 'P'
            None
        }
        "associated_type" => {
            process_associated_type(cursor, context);
            None
        }
        "const_item" => {
            process_constant(cursor, context);
            None
        }

        // Other top-level items
        "static_item" => {
            process_variable(cursor, context);
            None
        }
        "type_item" => {
            // Type alias
            process_typedef(cursor, context);
            None
        }
        "macro_definition" => {
            process_macro(cursor, context);
            None // Macros don't typically form scopes in the way structs/traits [48;52;236;1976;3776tdo
        }
        _ => None, // Ignore other node kinds for scope tracking / direct tagging
    }
}

// --- Tag Creation Helper ---

fn create_tag(
    name: String,
    kind_char: &str,
    node: Node, // Pass the node for position info
    context: &mut Context,
    // Allow passing extra fields specific to the item being tagged
    extra_fields: Option<HashMap<String, String>>,
) {
    if name.is_empty() || name == "_" {
        return; // Don't tag empty or placeholder names
    }
    let row = node.start_position().row;
    let address = address_string_from_line(row, context);
    let mut extension_fields = context.create_extension_fields();

    // Merge extra fields if provided
    if let Some(extras) = extra_fields {
        extension_fields.extend(extras);
    }

    context.tags.push(tag::Tag {
        name,
        file_name: context.file_name.to_string(),
        address,
        kind: Some(String::from(kind_char)),
        extension_fields: if extension_fields.is_empty() {
            None
        } else {
            Some(extension_fields)
        },
    });
}

// --- Specific Node Processors (returning Scope Info) ---

fn process_module(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["identifier"]) {
        create_tag(name.clone(), "n", node, context, None); // 'n' for module
        Some((ScopeType::Module, name))
    } else {
        None
    }
}

fn process_struct(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["type_identifier"]) {
        create_tag(name.clone(), "s", node, context, None); // 's' for struct
        process_identifiers_list(cursor, context, &name, "m");
        cursor.goto_parent();
        Some((ScopeType::Struct, name))
    } else {
        None
    }
}

fn process_union(
    cursor: &mut TreeCursor,
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["type_identifier"]) {
        create_tag(name.clone(), "u", node, context, None); // 'u' for union
        Some((ScopeType::Union, name))
    } else {
        None
    }
}

fn process_enum(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let enum_name = get_node_name(cursor, context, &["type_identifier"]);

    match &enum_name {
        None => None,
        Some(name) => {
            create_tag(name.clone(), "e", node, context, None); // 'e' for enum
            process_identifiers_list(cursor, context, name, "g");

            cursor.goto_parent(); // Back to enum_item node
                                  // --- End Process Enum Variants ---

            Some((ScopeType::Enum, name.clone()))
        }
    }
}

fn process_identifiers_list(
    cursor: &mut TreeCursor,
    context: &mut Context,
    name: &str,
    tag_kind: &str,
) {
    if !cursor.goto_first_child() {
        return;
    }

    let variant_type = if tag_kind == "g" { "enum" } else { "struct" };

    // --- Process Enum Variants ---
    loop {
        // Look for the list containing variants
        if cursor.node().kind() == "field_declaration_list"
            || cursor.node().kind() == "enum_variant_list"
        {
            if !cursor.goto_first_child() {
                return;
            }

            // Process each variant node
            loop {
                let kind = cursor.node().kind();
                if kind == "enum_variant" || kind == "field_declaration" {
                    let variant_node = cursor.node();
                    if let Some(variant_name) =
                        get_node_name(cursor, context, &["identifier", "field_identifier"])
                    {
                        // Add enum/struct name context specifically for the variant tag
                        let mut variant_fields = HashMap::new();
                        variant_fields.insert(variant_type.to_string(), name.to_owned());
                        create_tag(
                            variant_name,
                            tag_kind,
                            variant_node,
                            context,
                            Some(variant_fields),
                        );
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent(); // Back to field_declaration_list/enum_variant_list
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn process_trait(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["type_identifier"]) {
        create_tag(name.clone(), "t", node, context, None); // 't' for trait
        Some((ScopeType::Trait, name))
    } else {
        None
    }
}

// Process 'impl_item' -> impl Foo { ... } or impl Bar for Foo { ... }
fn process_impl(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let (trait_name, type_name) = find_impl_names(cursor, context)?;

    let mut extra_fields = HashMap::new();
    let tag_name = type_name?.clone();
    let kind_char = "c";

    if let Some(tr_name) = &trait_name {
        extra_fields.insert("trait".to_string(), tr_name.clone());
    }

    create_tag(
        tag_name.clone(),
        kind_char,
        node,
        context,
        Some(extra_fields),
    );
    Some((ScopeType::Implementation, tag_name))
}

fn find_impl_names(
    cursor: &mut TreeCursor,
    context: &Context,
) -> Option<(Option<String>, Option<String>)> {
    let mut trait_name = None;
    let mut type_name = None;
    let mut found_for = false;

    if !cursor.goto_first_child() {
        return Some((trait_name, type_name));
    }

    loop {
        let child_node = cursor.node();
        match child_node.kind() {
            "type_identifier" | "scoped_type_identifier" | "generic_type" => {
                let name = context.node_text(&child_node).to_string();
                if found_for {
                    if type_name.is_none() {
                        type_name = Some(name);
                    }
                } else if trait_name.is_none() {
                    trait_name = Some(name);
                } else if type_name.is_none() {
                    type_name = Some(name);
                }
            }
            "for" => found_for = true,
            "declaration_list" | "{" => break,
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    if !found_for {
        type_name = trait_name.take();
    }

    Some((trait_name, type_name))
}
// Processes 'function_item' and 'function_signature_item'
fn process_function(
    cursor: &mut TreeCursor,
    context: &mut Context,
    kind_char: &str, // e.g., "f"
) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["identifier"]) {
        // No extra fields needed here, context provides scope (struct/trait/impl/module)
        create_tag(name, kind_char, node, context, None);
    }
}

// Processes 'associated_type' -> type Item;
fn process_associated_type(cursor: &mut TreeCursor, context: &mut Context) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["type_identifier"]) {
        // Using 'T' like type alias, context provides trait scope
        create_tag(name, "T", node, context, None);
    }
}

// Processes 'const_item'
fn process_constant(cursor: &mut TreeCursor, context: &mut Context) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["identifier"]) {
        create_tag(name, "C", node, context, None); // 'c' for constant
    }
}

// Processes 'static_item'
fn process_variable(cursor: &mut TreeCursor, context: &mut Context) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["identifier"]) {
        create_tag(name, "v", node, context, None); // 'v' for variable (ctags uses this for static)
    }
}

// Processes 'type_item' -> type MyType = ...;
fn process_typedef(cursor: &mut TreeCursor, context: &mut Context) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["type_identifier"]) {
        create_tag(name, "T", node, context, None); // 'T' for type alias
    }
}

// Processes 'macro_definition' -> macro_rules! my_macro { ... }
fn process_macro(cursor: &mut TreeCursor, context: &mut Context) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["identifier", "metavariable"]) {
        let clean_name = name.strip_suffix('!').unwrap_or(&name).to_string();
        create_tag(clean_name, "d", node, context, None); // 'd' for macro definition
    }
}

// --- Helper Functions ---

// Finds the first child node matching any of the specified kinds and returns its text content.
// IMPORTANT: Temporarily modifies the cursor but restores it.
fn get_node_name(
    cursor: &mut TreeCursor, // Needs to be mutable to move
    context: &Context,
    kinds: &[&str],
) -> Option<String> {
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let current_node = cursor.node();
        if kinds.contains(&current_node.kind()) {
            cursor.goto_parent(); // Restore cursor
            return Some(String::from(context.node_text(&current_node))); // Found the first match
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent(); // Restore cursor

    None
}

// Generates the ctags address string
fn address_string_from_line(row: usize, context: &Context) -> String {
    if row >= context.lines.len() {
        return format!("/^{}$/;", row + 1);
    }
    let line_bytes = &context.lines[row];
    let escaped = String::from_utf8_lossy(line_bytes)
        .replace('\\', "\\\\")
        .replace('/', "\\/")
        .replace('^', "\\^")
        .replace('$', "\\$");
    format!("/^{}$/;\"", escaped)
}
