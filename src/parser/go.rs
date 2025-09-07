use super::helper::{self, Context, LanguageContext, TagKindConfig};
use super::Parser;
use indexmap::IndexMap;
use tree_sitter::TreeCursor;

use crate::tag;

/// Get the preferred field ordering for Go
fn get_field_order_for_go() -> Vec<&'static str> {
    vec![
        "line",
        "package",
        "struct",
        "interface",
        "typeref",
        "signature",
        "access",
        "end",
    ]
}

/// Create extension fields with Go-specific ordering
fn create_extension_fields_with_language(
    context: &Context,
    kind_char: &str,
    row: usize,
    node: tree_sitter::Node,
    extra_fields: Option<IndexMap<String, String>>,
) -> Option<IndexMap<String, String>> {
    let field_order = get_field_order_for_go();
    let mut extension_fields = IndexMap::new();

    // Process fields in the preferred order
    for &field_name in &field_order {
        match field_name {
            "kind" if context.user_config.fields_config.is_field_enabled("kind") => {
                extension_fields.insert(String::from("kind"), kind_char.to_string());
            }
            "line" if context.user_config.fields_config.is_field_enabled("line") => {
                extension_fields.insert(String::from("line"), (row + 1).to_string());
            }
            "file" if context.user_config.fields_config.is_field_enabled("file") => {
                extension_fields.insert(String::from("file"), context.file_name.to_string());
            }
            "end" if context.user_config.fields_config.is_field_enabled("end") => {
                // Only add end field if the tag spans multiple lines
                let start_line = node.start_position().row;
                let end_line = node.end_position().row;
                if end_line > start_line {
                    extension_fields.insert(String::from("end"), (end_line + 1).to_string());
                }
            }
            "access" => {
                if let Some(extras) = &extra_fields {
                    if let Some(access) = extras.get("access") {
                        if context.user_config.fields_config.is_field_enabled("access") {
                            extension_fields.insert("access".to_string(), access.clone());
                        }
                    }
                }
            }
            "signature" => {
                if let Some(extras) = &extra_fields {
                    if let Some(signature) = extras.get("signature") {
                        if context
                            .user_config
                            .fields_config
                            .is_field_enabled("signature")
                        {
                            extension_fields.insert("signature".to_string(), signature.clone());
                        }
                    }
                }
            }
            "typeref" => {
                if let Some(extras) = &extra_fields {
                    if let Some(typeref) = extras.get("typeref") {
                        if context
                            .user_config
                            .fields_config
                            .is_field_enabled("typeref")
                        {
                            extension_fields.insert("typeref".to_string(), typeref.clone());
                        }
                    }
                }
            }
            // Scope-related fields
            field_name
                if matches!(
                    field_name,
                    "struct" | "enum" | "union" | "interface" | "implementation" | "package"
                ) =>
            {
                if context.user_config.fields_config.is_field_enabled("scope")
                    || context.user_config.extras_config.qualified
                {
                    if let Some(extras) = &extra_fields {
                        if let Some(value) = extras.get(field_name) {
                            extension_fields.insert(field_name.to_string(), value.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Handle remaining extra fields that weren't processed above
    if let Some(extras) = extra_fields {
        for (key, value) in extras {
            // Skip fields we've already processed
            if extension_fields.contains_key(&key) {
                continue;
            }

            // For other scope-related fields, include them if scope/qualified is enabled
            if context.user_config.fields_config.is_field_enabled("scope")
                || context.user_config.extras_config.qualified
            {
                extension_fields.insert(key, value);
            }
        }
    }

    if extension_fields.is_empty() {
        None
    } else {
        Some(extension_fields)
    }
}

// Represents the type of scope for context tracking
#[derive(Debug)]
enum ScopeType {
    Package,
    Struct,
    Interface,
}

// Enhanced Context for Go with scope tracking
struct GoContext<'a> {
    base: Context<'a>,
    // Use a stack to keep track of nested scopes
    scope_stack: Vec<(ScopeType, String)>,
}

impl<'a> GoContext<'a> {
    fn new(
        source_code: &'a str,
        lines: Vec<Vec<u8>>,
        file_name: &'a str,
        tags: &'a mut Vec<tag::Tag>,
        tag_config: &'a TagKindConfig,
        user_config: &'a crate::config::Config,
    ) -> Self {
        Self {
            base: Context {
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

        for (scope_type, name) in &self.scope_stack {
            match scope_type {
                ScopeType::Package => {
                    fields.insert(String::from("package"), name.clone());
                }
                ScopeType::Struct => {
                    fields.insert(
                        String::from("struct"),
                        format!("{}.{}", self.get_package_name(), name),
                    );
                }
                ScopeType::Interface => {
                    fields.insert(
                        String::from("interface"),
                        format!("{}.{}", self.get_package_name(), name),
                    );
                }
            }
        }

        fields
    }

    fn get_package_name(&self) -> String {
        for (scope_type, name) in &self.scope_stack {
            if matches!(scope_type, ScopeType::Package) {
                return name.clone();
            }
        }
        "".to_string()
    }

    /// Creates a tag with Go-specific extension field handling
    fn create_go_tag(
        &mut self,
        name: String,
        kind_char: &str,
        node: tree_sitter::Node,
        extra_fields: Option<IndexMap<String, String>>,
    ) {
        if name.is_empty() || name == "_" {
            return; // Don't tag empty or placeholder names
        }

        // Check if this tag kind is enabled in the configuration
        if !self.base.tag_config.is_kind_enabled(kind_char) {
            return; // Skip creating this tag if the kind is disabled
        }

        let row = node.start_position().row;
        let address = helper::address_string_from_line(row, &self.base);

        // Create extension fields with Go-specific ordering
        let extension_fields =
            create_extension_fields_with_language(&self.base, kind_char, row, node, extra_fields);

        self.base.tags.push(tag::Tag {
            name,
            file_name: self.base.file_name.to_string(),
            address,
            kind: Some(String::from(kind_char)),
            extension_fields,
        });
    }
}

impl<'a> LanguageContext for GoContext<'a> {
    type ScopeType = ScopeType;

    fn push_scope(&mut self, scope_type: Self::ScopeType, name: String) {
        self.scope_stack.push((scope_type, name));
    }

    fn pop_scope(&mut self) -> Option<(Self::ScopeType, String)> {
        self.scope_stack.pop()
    }

    fn process_node(&mut self, cursor: &mut TreeCursor) -> Option<(Self::ScopeType, String)> {
        process_go_node(cursor, self)
    }
}

impl Parser {
    pub fn generate_go_tags_with_full_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        tag_config: &TagKindConfig,
        user_config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        helper::generate_tags_with_config(
            &mut self.ts_parser,
            tree_sitter_go::LANGUAGE.into(),
            code,
            file_path_relative_to_tag_file,
            |source_code, lines, cursor, tags| {
                let mut context = GoContext::new(
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

// Dispatches Go node processing based on kind
fn process_go_node(
    cursor: &mut TreeCursor,
    context: &mut GoContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();

    match node.kind() {
        "package_clause" => process_package(cursor, context),
        "import_declaration" => {
            process_imports(cursor, context);
            None
        }
        "function_declaration" => {
            process_function(cursor, context);
            None
        }
        "method_declaration" => {
            process_method(cursor, context);
            None
        }
        "const_declaration" => {
            process_constants(cursor, context);
            None
        }
        "var_declaration" => {
            process_variables(cursor, context);
            None
        }
        "short_var_declaration" => {
            process_short_var_declaration(cursor, context);
            None
        }
        "type_declaration" => process_type_declaration(cursor, context),
        "method_elem" => {
            process_method_spec_if_in_interface(cursor, context);
            None
        }
        _ => None,
    }
}

fn process_package(
    cursor: &mut TreeCursor,
    context: &mut GoContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["package_identifier"]) {
        context.create_go_tag(name.clone(), "p", node, None);
        // Push package scope directly to the stack since it should persist for the entire file
        context.scope_stack.push((ScopeType::Package, name));
        None // Don't return scope info to prevent automatic popping
    } else {
        None
    }
}

fn process_imports(cursor: &mut TreeCursor, context: &mut GoContext) {
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        let node = cursor.node();
        match node.kind() {
            "import_spec" => {
                if let Some((alias_name, import_path)) = get_import_name(cursor, context) {
                    let mut extra_fields = IndexMap::new();
                    extra_fields.insert("package".to_string(), import_path);
                    context.create_go_tag(alias_name, "P", node, Some(extra_fields));
                }
            }
            "import_spec_list" => {
                if cursor.goto_first_child() {
                    loop {
                        let spec_node = cursor.node();
                        if spec_node.kind() == "import_spec" {
                            if let Some((alias_name, import_path)) =
                                get_import_name(cursor, context)
                            {
                                let mut extra_fields = IndexMap::new();
                                extra_fields.insert("package".to_string(), import_path);
                                context.create_go_tag(
                                    alias_name,
                                    "P",
                                    spec_node,
                                    Some(extra_fields),
                                );
                            }
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                    cursor.goto_parent();
                }
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();
}

fn get_import_name(cursor: &mut TreeCursor, context: &mut GoContext) -> Option<(String, String)> {
    if !cursor.goto_first_child() {
        return None;
    }

    let mut import_path = None;
    let mut alias = None;

    loop {
        let node = cursor.node();
        match node.kind() {
            "interpreted_string_literal" => {
                let path_text = context.base.node_text(&node);
                // Remove quotes
                let clean_path = path_text.trim_matches('"');
                import_path = Some(clean_path.to_string());
            }
            "package_identifier" => {
                alias = Some(context.base.node_text(&node).to_string());
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    // Only return if there's an alias (aliased import)
    if let (Some(alias_name), Some(path)) = (alias, import_path) {
        Some((alias_name, path))
    } else {
        None
    }
}

fn process_function(cursor: &mut TreeCursor, context: &mut GoContext) {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
        let mut extra_fields = context.create_extension_fields();

        // Get function signature
        if context
            .base
            .user_config
            .fields_config
            .is_field_enabled("signature")
        {
            if let Some(signature) = get_function_signature(cursor, context) {
                extra_fields.insert("signature".to_string(), signature);
            }
        }

        // Get return type
        if let Some(return_type) = get_function_return_type(cursor, context) {
            extra_fields.insert("typeref".to_string(), format!("typename:{}", return_type));
        }

        let final_fields = if extra_fields.is_empty() {
            None
        } else {
            Some(extra_fields)
        };
        context.create_go_tag(name, "f", node, final_fields);
    }
}

fn process_method(cursor: &mut TreeCursor, context: &mut GoContext) {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["field_identifier"]) {
        let mut extra_fields = IndexMap::new();

        // Get receiver type - methods should only have struct field, not package
        if let Some(receiver_type) = get_method_receiver_type(cursor, context) {
            let package_name = context.get_package_name();
            if !package_name.is_empty() {
                extra_fields.insert(
                    "struct".to_string(),
                    format!("{}.{}", package_name, receiver_type),
                );
            } else {
                extra_fields.insert("struct".to_string(), format!(".{}", receiver_type));
            }
        }

        // Get function signature
        if context
            .base
            .user_config
            .fields_config
            .is_field_enabled("signature")
        {
            if let Some(signature) = get_function_signature(cursor, context) {
                extra_fields.insert("signature".to_string(), signature);
            }
        }

        // Get return type
        if let Some(return_type) = get_function_return_type(cursor, context) {
            extra_fields.insert("typeref".to_string(), format!("typename:{}", return_type));
        }

        let final_fields = if extra_fields.is_empty() {
            None
        } else {
            Some(extra_fields)
        };
        context.create_go_tag(name, "f", node, final_fields);
    }
}

fn get_method_receiver_type(cursor: &mut TreeCursor, context: &mut GoContext) -> Option<String> {
    if !cursor.goto_first_child() {
        return None;
    }

    let mut receiver_type = None;
    loop {
        let node = cursor.node();
        if node.kind() == "parameter_list" {
            // This is the receiver
            if cursor.goto_first_child() {
                loop {
                    let param_node = cursor.node();
                    if param_node.kind() == "parameter_declaration" {
                        if let Some(type_name) = helper::get_node_name(
                            cursor,
                            &context.base,
                            &["type_identifier", "pointer_type"],
                        ) {
                            receiver_type = Some(type_name.trim_start_matches('*').to_string());
                            break;
                        }
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
                cursor.goto_parent();
            }
            break;
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();
    receiver_type
}

fn get_function_signature(cursor: &mut TreeCursor, context: &mut GoContext) -> Option<String> {
    match cursor.node().child_by_field_name("parameters") {
        None => Some("()".to_string()),
        Some(signature_node) => Some(context.base.node_text(&signature_node).to_string()),
    }
}

fn get_function_return_type(cursor: &mut TreeCursor, context: &mut GoContext) -> Option<String> {
    if !cursor.goto_first_child() {
        return None;
    }

    let mut return_type = None;
    loop {
        let node = cursor.node();
        match node.kind() {
            "type_identifier" | "pointer_type" | "slice_type" | "map_type" | "channel_type"
            | "function_type" => {
                // Skip parameter lists, only get return types
                let mut is_return_type = true;
                if let Some(prev_sibling) = node.prev_sibling() {
                    if prev_sibling.kind() == "parameter_list" {
                        is_return_type = true;
                    }
                }
                if is_return_type {
                    return_type = Some(context.base.node_text(&node).to_string());
                }
            }
            "parameter_list" => {
                // Check if this is followed by a return type
                if let Some(next_sibling) = node.next_sibling() {
                    match next_sibling.kind() {
                        "type_identifier" | "pointer_type" | "slice_type" | "map_type"
                        | "channel_type" | "function_type" => {
                            return_type = Some(context.base.node_text(&next_sibling).to_string());
                        }
                        "parameter_list" => {
                            // Multiple return values
                            return_type = Some(context.base.node_text(&next_sibling).to_string());
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();
    return_type
}

fn process_constants(cursor: &mut TreeCursor, context: &mut GoContext) {
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        let node = cursor.node();
        match node.kind() {
            "const_spec" => {
                process_const_spec(cursor, context);
            }
            "const_spec_list" => {
                // Handle grouped const specs
                if cursor.goto_first_child() {
                    loop {
                        let spec_node = cursor.node();
                        if spec_node.kind() == "const_spec" {
                            process_const_spec(cursor, context);
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                    cursor.goto_parent();
                }
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();
}

fn process_const_spec(cursor: &mut TreeCursor, context: &mut GoContext) {
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        let node = cursor.node();
        if node.kind() == "identifier" {
            let name = context.base.node_text(&node).to_string();
            let extra_fields = context.create_extension_fields();
            let final_fields = if extra_fields.is_empty() {
                None
            } else {
                Some(extra_fields)
            };
            context.create_go_tag(name, "c", node, final_fields);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();
}

fn process_variables(cursor: &mut TreeCursor, context: &mut GoContext) {
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        let node = cursor.node();
        match node.kind() {
            "var_spec" => {
                process_var_spec(cursor, context);
            }
            "var_spec_list" => {
                // Handle grouped var specs
                if cursor.goto_first_child() {
                    loop {
                        let spec_node = cursor.node();
                        if spec_node.kind() == "var_spec" {
                            process_var_spec(cursor, context);
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                    cursor.goto_parent();
                }
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();
}

fn process_var_spec(cursor: &mut TreeCursor, context: &mut GoContext) {
    if !cursor.goto_first_child() {
        return;
    }

    let mut identifiers = Vec::new();
    let mut type_info = None;

    loop {
        let node = cursor.node();
        match node.kind() {
            "identifier" => {
                identifiers.push((context.base.node_text(&node).to_string(), node));
            }
            "type_identifier" | "pointer_type" | "slice_type" | "map_type" | "channel_type"
            | "interface_type" => {
                type_info = Some(context.base.node_text(&node).to_string());
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    // Create tags for all identifiers
    for (name, node) in identifiers {
        let mut extra_fields = context.create_extension_fields();
        if let Some(ref type_name) = type_info {
            extra_fields.insert("typeref".to_string(), format!("typename:{}", type_name));
        }
        let final_fields = if extra_fields.is_empty() {
            None
        } else {
            Some(extra_fields)
        };
        context.create_go_tag(name, "v", node, final_fields);
    }
}

fn process_short_var_declaration(cursor: &mut TreeCursor, context: &mut GoContext) {
    if !cursor.goto_first_child() {
        return;
    }

    let mut identifiers = Vec::new();

    loop {
        let node = cursor.node();
        if node.kind() == "identifier" {
            identifiers.push((context.base.node_text(&node).to_string(), node));
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    // Create tags for all identifiers
    for (name, node) in identifiers {
        let extra_fields = context.create_extension_fields();
        let final_fields = if extra_fields.is_empty() {
            None
        } else {
            Some(extra_fields)
        };
        context.create_go_tag(name, "v", node, final_fields);
    }
}

fn process_type_declaration(
    cursor: &mut TreeCursor,
    context: &mut GoContext,
) -> Option<(ScopeType, String)> {
    if !cursor.goto_first_child() {
        return None;
    }

    let mut result = None;
    loop {
        let node = cursor.node();
        if node.kind() == "type_spec" {
            result = process_type_spec(cursor, context);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();
    result
}

fn process_type_spec(
    cursor: &mut TreeCursor,
    context: &mut GoContext,
) -> Option<(ScopeType, String)> {
    if !cursor.goto_first_child() {
        return None;
    }

    let mut type_name = None;
    let mut type_kind = None;
    let mut scope_info = None;
    let mut type_node = None;

    loop {
        let node = cursor.node();
        match node.kind() {
            "type_identifier" if type_name.is_none() => {
                type_name = Some(context.base.node_text(&node).to_string());
                type_node = Some(node);
            }
            "struct_type" => {
                type_kind = Some("s");
                if let Some(ref name) = type_name {
                    let extra_fields = context.create_extension_fields();
                    let final_fields = if extra_fields.is_empty() {
                        None
                    } else {
                        Some(extra_fields)
                    };
                    context.create_go_tag(name.clone(), "s", node, final_fields);
                    process_struct_fields(cursor, context, name);
                    scope_info = Some((ScopeType::Struct, name.clone()));
                }
            }
            "interface_type" => {
                type_kind = Some("i");
                if let Some(ref name) = type_name {
                    let extra_fields = context.create_extension_fields();
                    let final_fields = if extra_fields.is_empty() {
                        None
                    } else {
                        Some(extra_fields)
                    };
                    context.create_go_tag(name.clone(), "i", node, final_fields);
                    scope_info = Some((ScopeType::Interface, name.clone()));
                }
            }
            _ => {
                // All other types should be tagged as 't' (type alias)
                if type_kind.is_none() && type_name.is_some() {
                    type_kind = Some("t");
                    if let (Some(ref name), Some(type_node)) = (&type_name, type_node) {
                        let mut extra_fields = context.create_extension_fields();
                        extra_fields.insert(
                            "typeref".to_string(),
                            format!("typename:{}", context.base.node_text(&node)),
                        );
                        context.create_go_tag(name.clone(), "t", type_node, Some(extra_fields));
                    }
                }
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();
    scope_info
}

fn process_struct_fields(cursor: &mut TreeCursor, context: &mut GoContext, struct_name: &str) {
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        let node = cursor.node();
        if node.kind() == "field_declaration_list" {
            if cursor.goto_first_child() {
                loop {
                    let field_node = cursor.node();
                    if field_node.kind() == "field_declaration" {
                        process_field_declaration(cursor, context, struct_name);
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
                cursor.goto_parent();
            }
            break;
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();
}

fn process_field_declaration(cursor: &mut TreeCursor, context: &mut GoContext, struct_name: &str) {
    if !cursor.goto_first_child() {
        return;
    }

    let mut field_names = Vec::new();
    let mut field_type = None;

    loop {
        let node = cursor.node();
        match node.kind() {
            "field_identifier" => {
                field_names.push((context.base.node_text(&node).to_string(), node));
            }
            "type_identifier" | "pointer_type" | "slice_type" | "map_type" | "channel_type"
            | "interface_type" => {
                field_type = Some(context.base.node_text(&node).to_string());
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    // Create tags for all field names
    for (name, node) in field_names {
        let mut extra_fields = IndexMap::new();
        let package_name = context.get_package_name();
        if !package_name.is_empty() {
            extra_fields.insert(
                "struct".to_string(),
                format!("{}.{}", package_name, struct_name),
            );
        } else {
            extra_fields.insert("struct".to_string(), format!(".{}", struct_name));
        }
        if let Some(ref type_name) = field_type {
            extra_fields.insert("typeref".to_string(), format!("typename:{}", type_name));
        }
        context.create_go_tag(name, "m", node, Some(extra_fields));
    }
}

fn process_method_spec_if_in_interface(cursor: &mut TreeCursor, context: &mut GoContext) {
    // Check if we're inside an interface by looking at the scope stack
    let interface_name = context
        .scope_stack
        .iter()
        .rev()
        .find_map(|(scope_type, scope_name)| {
            if matches!(scope_type, ScopeType::Interface) {
                Some(scope_name.clone())
            } else {
                None
            }
        });

    if let Some(name) = interface_name {
        process_method_spec(cursor, context, &name);
    }
}

fn process_method_spec(cursor: &mut TreeCursor, context: &mut GoContext, interface_name: &str) {
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["field_identifier"]) {
        let node = cursor.node();
        let mut extra_fields = IndexMap::new();
        let package_name = context.get_package_name();
        if !package_name.is_empty() {
            extra_fields.insert(
                "interface".to_string(),
                format!("{}.{}", package_name, interface_name),
            );
        } else {
            extra_fields.insert("interface".to_string(), format!(".{}", interface_name));
        }

        // Get return type if available
        if let Some(return_type) = get_method_spec_return_type(cursor, context) {
            extra_fields.insert("typeref".to_string(), format!("typename:{}", return_type));
        }

        context.create_go_tag(name, "n", node, Some(extra_fields));
    }
}

fn get_method_spec_return_type(cursor: &mut TreeCursor, context: &mut GoContext) -> Option<String> {
    if !cursor.goto_first_child() {
        return None;
    }

    let mut return_type = None;
    loop {
        let node = cursor.node();
        match node.kind() {
            "type_identifier" | "pointer_type" | "slice_type" | "map_type" | "channel_type"
            | "function_type" => {
                // Check if this comes after parameters
                if let Some(prev_sibling) = node.prev_sibling() {
                    if prev_sibling.kind() == "parameter_list" {
                        return_type = Some(context.base.node_text(&node).to_string());
                    }
                }
            }
            "parameter_list" => {
                // Check if this is followed by a return type
                if let Some(next_sibling) = node.next_sibling() {
                    match next_sibling.kind() {
                        "type_identifier" | "pointer_type" | "slice_type" | "map_type"
                        | "channel_type" | "function_type" => {
                            return_type = Some(context.base.node_text(&next_sibling).to_string());
                        }
                        "parameter_list" => {
                            // Multiple return values
                            return_type = Some(context.base.node_text(&next_sibling).to_string());
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();
    return_type
}
