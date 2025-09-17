use super::helper::{self, LanguageContext, TagKindConfig};
use super::Parser;
use indexmap::IndexMap;
use tree_sitter::{Node, TreeCursor};

use crate::tag;

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

/// Enhanced Context for Rust with scope tracking
struct RustContext<'a> {
    base: helper::Context<'a>,
    scope_stack: Vec<(ScopeType, String)>,
}

impl<'a> RustContext<'a> {
    fn new(
        source_code: &'a str,
        lines: Vec<Vec<u8>>,
        file_name: &'a str,
        tags: &'a mut Vec<tag::Tag>,
        tag_config: &'a TagKindConfig,
        user_config: &'a crate::config::Config,
    ) -> Self {
        Self {
            base: helper::Context {
                source_code,
                lines,
                file_name,
                tags,
                tag_config,
                user_config,
            },
            scope_stack: Vec::new(),
        }
    }

    // Build extension fields based on the current scope stack
    fn create_extension_fields(&self) -> IndexMap<String, String> {
        let mut fields = IndexMap::new();
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
                    fields.insert(String::from("interface"), name.clone());
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

impl<'a> LanguageContext for RustContext<'a> {
    type ScopeType = ScopeType;

    fn push_scope(&mut self, scope_type: Self::ScopeType, name: String) {
        self.scope_stack.push((scope_type, name));
    }

    fn pop_scope(&mut self) -> Option<(Self::ScopeType, String)> {
        self.scope_stack.pop()
    }

    fn process_node(&mut self, cursor: &mut TreeCursor) -> Option<(Self::ScopeType, String)> {
        process_node(cursor, self)
    }
}

impl Parser {
    pub fn generate_rust_tags_with_full_config(
        &mut self,
        code: &[u8], // Changed to slice for flexibility
        file_path_relative_to_tag_file: &str,
        tag_config: &helper::TagKindConfig,
        user_config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        helper::generate_tags_with_config(
            &mut self.ts_parser,
            tree_sitter_rust::LANGUAGE.into(),
            code,
            file_path_relative_to_tag_file,
            |source_code, lines, cursor, tags| {
                let mut context = RustContext::new(
                    source_code,
                    lines,
                    file_path_relative_to_tag_file,
                    tags,
                    tag_config,
                    user_config,
                );
                helper::walk_generic(cursor, &mut context);
            },
        )
    }
}

// Dispatches node processing based on kind, returns scope info if node defines one
fn process_node(cursor: &mut TreeCursor, context: &mut RustContext) -> Option<(ScopeType, String)> {
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
            // Determine if it's a method or a standalone function based on scope
            let kind_char = if let Some((top_scope, _)) = context.scope_stack.last() {
                match top_scope {
                    ScopeType::Implementation
                    | ScopeType::Struct
                    | ScopeType::Enum
                    | ScopeType::Union
                    | ScopeType::Trait => "P", // Method or default trait method
                    ScopeType::Module => "f", // Function within a module
                }
            } else {
                "f" // Top-level function
            };
            process_function(cursor, context, kind_char);
            None // Doesn't start a new named scope for context stack
        }
        "function_signature_item" => {
            // Function signatures within traits
            process_function(cursor, context, "m"); // Also tag as 'm'
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
            None // Macros don't typically form scopes in the way structs/traits
        }
        _ => None, // Ignore other node kinds for scope tracking / direct tagging
    }
}

// --- Tag Creation Helper ---

fn create_tag(
    name: String,
    kind_char: &str,
    node: Node, // Pass the node for position info
    context: &mut RustContext,
    // Allow passing extra fields specific to the item being tagged
    extra_fields: Option<IndexMap<String, String>>,
) {
    if name.is_empty() || name == "_" {
        return; // Don't tag empty or placeholder names
    }

    // Check if this tag kind is enabled in the configuration
    if !context.base.tag_config.is_kind_enabled(kind_char) {
        return; // Skip creating this tag if the kind is disabled
    }

    let row = node.start_position().row;
    let address = helper::address_string_from_line(row, &context.base);
    let mut extension_fields = IndexMap::new();

    // Insert fields in ctags order (alphabetical by field letter, with special cases first):

    // 1. Kind field (k) - if enabled as extension field
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("kind")
    {
        extension_fields.insert(String::from("kind"), kind_char.to_string());
    }

    // 2. Line number (n) - typically second
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("line")
    {
        extension_fields.insert(String::from("line"), (row + 1).to_string());
    }

    // 3. Access field (a) - access modifier
    if let Some(extras) = &extra_fields {
        if let Some(access) = extras.get("access") {
            if context
                .base
                .user_config
                .fields_config
                .is_field_enabled("access")
            {
                extension_fields.insert("access".to_string(), access.clone());
            }
        }
    }

    // 4. File field (f) - file-restricted scoping
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("file")
    {
        extension_fields.insert(String::from("file"), context.base.file_name.to_string());
    }

    // 5. Signature field (S) - function signature
    if let Some(extras) = &extra_fields {
        if let Some(signature) = extras.get("signature") {
            if context
                .base
                .user_config
                .fields_config
                .is_field_enabled("signature")
            {
                extension_fields.insert("signature".to_string(), signature.clone());
            }
        }
    }

    // 6. Scope information (s) - scope of tag definition
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("scope")
        || context.base.user_config.extras_config.qualified
    {
        let scope_fields = context.create_extension_fields();
        extension_fields.extend(scope_fields);
    }

    // 7. Typeref field (t) - type reference
    if let Some(extras) = &extra_fields {
        if let Some(typeref) = extras.get("typeref") {
            if context
                .base
                .user_config
                .fields_config
                .is_field_enabled("typeref")
            {
                extension_fields.insert("typeref".to_string(), typeref.clone());
            }
        }
    }

    // 8. End position (e) - end line number
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("end")
    {
        extension_fields.insert(
            String::from("end"),
            (node.end_position().row + 1).to_string(),
        );
    }

    // Handle remaining extra fields that weren't processed above
    if let Some(extras) = extra_fields {
        for (key, value) in extras {
            // Skip fields we've already processed
            if matches!(key.as_str(), "access" | "signature" | "typeref") {
                continue;
            }

            match key.as_str() {
                "implementation" | "trait" | "struct" | "enum" | "union" => {
                    if context
                        .base
                        .user_config
                        .fields_config
                        .is_field_enabled("implementation")
                        || context
                            .base
                            .user_config
                            .fields_config
                            .is_field_enabled("scope")
                        || context.base.user_config.extras_config.qualified
                    {
                        extension_fields.insert(key, value);
                    }
                }
                _ => {
                    // For other fields, include them if scope/qualified is enabled
                    if context
                        .base
                        .user_config
                        .fields_config
                        .is_field_enabled("scope")
                        || context.base.user_config.extras_config.qualified
                    {
                        extension_fields.insert(key, value);
                    }
                }
            }
        }
    }

    context.base.tags.push(tag::Tag {
        name,
        file_name: context.base.file_name.to_string(),
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

fn process_module(
    cursor: &mut TreeCursor,
    context: &mut RustContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
        create_tag(name.clone(), "n", node, context, None); // 'n' for module
        Some((ScopeType::Module, name))
    } else {
        None
    }
}

fn process_struct(
    cursor: &mut TreeCursor,
    context: &mut RustContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["type_identifier"]) {
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
    context: &mut RustContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["type_identifier"]) {
        create_tag(name.clone(), "u", node, context, None); // 'u' for union
        Some((ScopeType::Union, name))
    } else {
        None
    }
}

fn process_enum(cursor: &mut TreeCursor, context: &mut RustContext) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let enum_name = helper::get_node_name(cursor, &context.base, &["type_identifier"]);

    match &enum_name {
        None => None,
        Some(name) => {
            create_tag(name.clone(), "g", node, context, None); // 'g' for enum
            process_identifiers_list(cursor, context, name, "e");

            cursor.goto_parent(); // Back to enum_item node
                                  // --- End Process Enum Variants ---

            Some((ScopeType::Enum, name.clone()))
        }
    }
}

fn process_identifiers_list(
    cursor: &mut TreeCursor,
    context: &mut RustContext,
    name: &str,
    tag_kind: &str,
) {
    if !cursor.goto_first_child() {
        return;
    }

    let variant_type = if tag_kind == "e" { "enum" } else { "struct" };

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
                    if let Some(variant_name) = helper::get_node_name(
                        cursor,
                        &context.base,
                        &["identifier", "field_identifier"],
                    ) {
                        // Add enum/struct name context specifically for the variant tag
                        let mut variant_fields = IndexMap::new();
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

fn process_trait(
    cursor: &mut TreeCursor,
    context: &mut RustContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["type_identifier"]) {
        create_tag(name.clone(), "i", node, context, None); // 'i' for trait
        Some((ScopeType::Trait, name))
    } else {
        None
    }
}

// Process 'impl_item' -> impl Foo { ... } or impl Bar for Foo { ... }
fn process_impl(cursor: &mut TreeCursor, context: &mut RustContext) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let (trait_name, type_name) = find_impl_names(cursor, context)?;

    let mut extra_fields = IndexMap::new();
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
    context: &RustContext,
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
                let name = context.base.node_text(&child_node).to_string();
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
    context: &mut RustContext,
    kind_char: &str, // e.g., "f"
) {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
        let mut extra_fields = IndexMap::new();

        // Only get the signature string if signature field is enabled
        if context
            .base
            .user_config
            .fields_config
            .is_field_enabled("signature")
        {
            if let Some(signature_str) = get_function_signature_string(node, cursor, &context.base)
            {
                extra_fields.insert(String::from("signature"), signature_str);
            }
        }

        create_tag(
            name,
            kind_char,
            node,
            context,
            if extra_fields.is_empty() {
                None
            } else {
                Some(extra_fields)
            },
        );
    }
}

// Processes 'associated_type' -> type Item;
fn process_associated_type(cursor: &mut TreeCursor, context: &mut RustContext) {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["type_identifier"]) {
        // Using 'T' like type alias, context provides trait scope
        create_tag(name, "T", node, context, None);
    }
}

// Processes 'const_item'
fn process_constant(cursor: &mut TreeCursor, context: &mut RustContext) {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
        create_tag(name, "C", node, context, None); // 'c' for constant
    }
}

// Processes 'static_item'
fn process_variable(cursor: &mut TreeCursor, context: &mut RustContext) {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
        create_tag(name, "v", node, context, None); // 'v' for variable (ctags uses this for static)
    }
}

// Processes 'type_item' -> type MyType = ...;
fn process_typedef(cursor: &mut TreeCursor, context: &mut RustContext) {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["type_identifier"]) {
        create_tag(name, "t", node, context, None); // 'T' for type alias
    }
}

// Processes 'macro_definition' -> macro_rules! my_macro { ... }
fn process_macro(cursor: &mut TreeCursor, context: &mut RustContext) {
    let node = cursor.node();
    if let Some(name) =
        helper::get_node_name(cursor, &context.base, &["identifier", "metavariable"])
    {
        let clean_name = name.strip_suffix('!').unwrap_or(&name).to_string();
        create_tag(clean_name, "M", node, context, None); // 'M' for macro definition
    }
}

// --- Helper Functions ---

// oo

// Constructs the signature string for a function/method node.
// Example: "(param1: Type1, param2: Type2) -> ReturnType"
fn get_function_signature_string(
    func_node: Node,
    cursor: &mut TreeCursor,
    context: &helper::Context,
) -> Option<String> {
    // The `parameters` node in tree-sitter-rust typically includes the parentheses.
    // Its text would be like "(param1: Type1, param2: Type2)" or "()".
    let params_text = helper::get_node_name(cursor, context, &["parameters"])?;

    // For Return Type: "return_type" is a FIELD NAME on the function_item node.
    // The actual child node will have a KIND corresponding to the specific type (e.g., type_identifier).
    // We fetch the child by its field name, then get its text.
    let return_type_text_opt = func_node
        .child_by_field_name("return_type")
        .and_then(|rt_node| {
            let text = context.node_text(&rt_node).to_string();
            if text.is_empty() {
                // If the node's text is empty, treat as None
                None
            } else {
                Some(text)
            }
        });

    let raw_signature_str = if let Some(rt_text) = return_type_text_opt {
        // Only add "-> ReturnType" if rt_text is non-empty.
        if !rt_text.is_empty() {
            // Some(format!("{} -> {}", params_text, rt_text))
            format!("{} -> {}", params_text, rt_text)
        } else {
            params_text // Return type node exists but its text is empty.
        }
    } else {
        params_text // No return type node.
    };

    // Replace newlines and normalize whitespace to single spaces
    let cleaned_signature = raw_signature_str
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");

    Some(cleaned_signature)
}
