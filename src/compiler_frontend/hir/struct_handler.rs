//! Struct Handler Component
//!
//! This module implements the StructHandler component for the HIR builder.
//! It handles the transformation of struct definitions, creation, field access,
//! and assignments from AST to HIR representation.
//!
//! ## Responsibilities
//! - Transform struct definitions into HIR struct declarations
//! - Transform struct creation into HIR allocation instructions
//! - Handle field access with proper offset calculations
//! - Manage struct assignments and ownership semantics
//!
//! ## Key Design Principles
//! - Field offsets are calculated based on struct layout
//! - Ownership semantics are preserved for struct fields
//! - All struct operations integrate with Beanstalk's memory management

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::build_hir::HirBuilderContext;
use crate::compiler_frontend::hir::nodes::{
    HirExpr, HirExprKind, HirKind, HirNode, HirPlace, HirStmt,
};
use crate::compiler_frontend::host_functions::registry::{CallTarget, HostFunctionId};
use crate::compiler_frontend::parsers::tokenizer::tokens::TextLocation;
use crate::compiler_frontend::string_interning::InternedString;
use crate::return_hir_transformation_error;
use std::collections::HashMap;
use crate::compiler_frontend::ast::ast_nodes::Var;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};

/// Layout information for a struct type
#[derive(Debug, Clone)]
pub struct StructLayout {
    /// Total size of the struct in bytes
    pub total_size: u32,
    /// Alignment requirement for the struct
    pub alignment: u32,
    /// Offset of each field within the struct
    pub field_offsets: HashMap<InternedString, u32>,
    /// Types of each field
    pub field_types: HashMap<InternedString, DataType>,
    /// Order of fields (for iteration)
    pub field_order: Vec<InternedString>,
}

impl StructLayout {
    /// Creates a new empty struct layout
    pub fn new() -> Self {
        StructLayout {
            total_size: 0,
            alignment: 1,
            field_offsets: HashMap::new(),
            field_types: HashMap::new(),
            field_order: Vec::new(),
        }
    }

    /// Gets the offset for a field
    pub fn get_field_offset(&self, field: &InternedString) -> Option<u32> {
        self.field_offsets.get(field).copied()
    }

    /// Gets the type for a field
    pub fn get_field_type(&self, field: &InternedString) -> Option<&DataType> {
        self.field_types.get(field)
    }

    /// Checks if a field exists in this layout
    pub fn has_field(&self, field: &InternedString) -> bool {
        self.field_offsets.contains_key(field)
    }
}

impl Default for StructLayout {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculator for struct memory layouts
#[derive(Debug, Default)]
pub struct StructLayoutCalculator {
    /// Cached layouts for struct types
    layouts: HashMap<InternedString, StructLayout>,
}

impl StructLayoutCalculator {
    /// Creates a new layout calculator
    pub fn new() -> Self {
        StructLayoutCalculator {
            layouts: HashMap::new(),
        }
    }

    /// Calculates and caches the layout for a struct type
    pub fn calculate_layout(
        &mut self,
        struct_name: InternedString,
        fields: &[Var],
    ) -> StructLayout {
        // Check if we already have this layout cached
        if let Some(layout) = self.layouts.get(&struct_name) {
            return layout.clone();
        }

        let mut layout = StructLayout::new();
        let mut current_offset: u32 = 0;
        let mut max_alignment: u32 = 1;

        for field in fields {
            let field_size = self.get_type_size(&field.value.data_type);
            let field_alignment = self.get_type_alignment(&field.value.data_type);

            // Align the current offset
            current_offset = self.align_to(current_offset, field_alignment);

            // Record the field offset and type
            layout.field_offsets.insert(field.id, current_offset);
            layout
                .field_types
                .insert(field.id, field.value.data_type.clone());
            layout.field_order.push(field.id);

            // Move to the next position
            current_offset += field_size;

            // Track maximum alignment
            if field_alignment > max_alignment {
                max_alignment = field_alignment;
            }
        }

        // Align the total size to the struct's alignment
        layout.total_size = self.align_to(current_offset, max_alignment);
        layout.alignment = max_alignment;

        // Cache the layout
        self.layouts.insert(struct_name, layout.clone());

        layout
    }

    /// Gets a cached layout for a struct type
    pub fn get_layout(&self, struct_name: &InternedString) -> Option<&StructLayout> {
        self.layouts.get(struct_name)
    }

    /// Gets the size of a data type in bytes
    fn get_type_size(&self, data_type: &DataType) -> u32 {
        match data_type {
            DataType::Bool | DataType::True | DataType::False => 1,
            DataType::Char => 4,     // UTF-8 char can be up to 4 bytes
            DataType::Int => 8,      // i64
            DataType::Float => 8,    // f64
            DataType::Decimal => 16, // 128-bit decimal
            DataType::String | DataType::CoerceToString => 8, // Pointer to string data
            DataType::Struct(fields, _) => {
                // Calculate size based on fields
                let mut size: u32 = 0;
                for field in fields {
                    size += self.get_type_size(&field.value.data_type);
                }
                size
            }
            DataType::Collection(_, _) => 12, // Pointer + length + capacity
            DataType::Parameters(_) => 8,     // Pointer to tuple
            DataType::Returns(_) => 8,        // Pointer to tuple
            DataType::Option(_) => 9,         // Tag + value
            DataType::None => 0,
            DataType::Inferred => 8,       // Default to pointer size
            DataType::Template => 8,       // Pointer
            DataType::Choices(_) => 16,    // Tag + largest variant
            DataType::Range => 16,         // Start + end
            DataType::Reference(_) => 8,   // Pointer
            DataType::Function(_, _) => 8, // Function pointer
        }
    }

    /// Gets the alignment requirement for a data type
    fn get_type_alignment(&self, data_type: &DataType) -> u32 {
        match data_type {
            DataType::Bool | DataType::True | DataType::False => 1,
            DataType::Char => 4,
            DataType::Int => 8,
            DataType::Float => 8,
            DataType::Decimal => 8,
            DataType::String | DataType::CoerceToString => 8,
            DataType::Struct(fields, _) => {
                // Alignment is the maximum alignment of any field
                let mut max_align: u32 = 1;
                for field in fields {
                    let align = self.get_type_alignment(&field.value.data_type);
                    if align > max_align {
                        max_align = align;
                    }
                }
                max_align
            }
            DataType::Collection(_, _) => 8,
            DataType::Parameters(_) => 8,
            DataType::Returns(_) => 8,
            DataType::Option(_) => 8,
            DataType::None => 1,
            DataType::Inferred => 8,
            DataType::Template => 8,
            DataType::Choices(_) => 8,
            DataType::Range => 8,
            DataType::Reference(_) => 8,
            DataType::Function(_, _) => 8,
        }
    }

    /// Aligns a value to the given alignment
    fn align_to(&self, value: u32, alignment: u32) -> u32 {
        if alignment == 0 {
            return value;
        }
        let remainder = value % alignment;
        if remainder == 0 {
            value
        } else {
            value + (alignment - remainder)
        }
    }
}

/// The StructHandler component handles transformation of structs from AST to HIR.
///
/// This component operates on borrowed HirBuilderContext and does not maintain
/// independent state beyond the layout calculator. All transformations are
/// coordinated through the context.
#[derive(Debug, Default)]
pub struct StructHandler {
    /// Calculator for struct memory layouts
    layout_calculator: StructLayoutCalculator,
}

impl StructHandler {
    /// Creates a new StructHandler
    pub fn new() -> Self {
        StructHandler {
            layout_calculator: StructLayoutCalculator::new(),
        }
    }

    /// Transforms an AST struct definition into HIR representation.
    ///
    /// This creates a HIR struct definition node and registers the struct
    /// in the context for later use.
    ///
    /// # Arguments
    /// * `name` - The struct name
    /// * `fields` - The struct fields
    /// * `ctx` - The HIR builder context
    /// * `location` - Source location for error reporting
    ///
    /// # Returns
    /// A HIR node representing the struct definition
    pub fn transform_struct_definition(
        &mut self,
        name: InternedString,
        fields: &[Var],
        ctx: &mut HirBuilderContext,
        location: TextLocation,
    ) -> Result<HirNode, CompilerError> {
        // Calculate and cache the struct layout
        let _layout = self.layout_calculator.calculate_layout(name, fields);

        // Register the struct in the context
        ctx.register_struct(name, fields.to_vec());

        // Create the struct definition HIR node
        let node_id = ctx.allocate_node_id();
        let struct_node = HirNode {
            kind: HirKind::Stmt(HirStmt::StructDef {
                name,
                fields: fields.to_vec(),
            }),
            location,
            id: node_id,
        };

        Ok(struct_node)
    }

    /// Transforms struct creation into HIR representation.
    ///
    /// This creates HIR instructions for allocating and initializing a struct instance.
    /// Field values are linearized and assigned to the appropriate offsets.
    ///
    /// # Arguments
    /// * `type_name` - The struct type name
    /// * `field_values` - The field names and their initialization expressions
    /// * `ctx` - The HIR builder context
    /// * `location` - Source location for error reporting
    ///
    /// # Returns
    /// A tuple of HIR nodes for setup and the final struct expression
    pub fn transform_struct_creation(
        &mut self,
        type_name: InternedString,
        field_values: &[(InternedString, Expression)],
        ctx: &mut HirBuilderContext,
        location: &TextLocation,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        let nodes = Vec::new();
        let mut hir_fields = Vec::new();

        // Get the struct definition to validate fields
        let struct_def = ctx.get_struct_definition(&type_name).cloned();

        // Linearize each field value
        for (field_name, field_expr) in field_values {
            // Validate that the field exists in the struct definition
            if let Some(ref def) = struct_def {
                let field_exists = def.iter().any(|f| f.id == *field_name);
                if !field_exists {
                    return_hir_transformation_error!(
                        format!("Field '{}' does not exist in struct", field_name),
                        location.to_error_location_without_table(),
                        {
                            CompilationStage => "HIR Generation - Struct Creation",
                            PrimarySuggestion => "Check the struct definition for valid field names"
                        }
                    );
                }
            }

            // Transform the field expression
            let hir_expr = self.transform_field_value(field_expr, ctx)?;
            hir_fields.push((*field_name, hir_expr));
        }

        // Create the struct construction expression
        let struct_expr = HirExpr {
            kind: HirExprKind::StructConstruct {
                type_name,
                fields: hir_fields,
            },
            location: location.clone(),
        };

        // Mark the struct as potentially owned (it's a new allocation)
        // The struct instance will need to be assigned to a variable
        // which will be tracked for ownership

        Ok((nodes, struct_expr))
    }

    /// Transforms field access into HIR representation.
    ///
    /// This creates HIR instructions for accessing a field of a struct.
    /// The field offset is calculated based on the struct layout.
    ///
    /// # Arguments
    /// * `base` - The base expression (the struct being accessed)
    /// * `field` - The field name being accessed
    /// * `field_type` - The type of the field
    /// * `_ctx` - The HIR builder context (unused, reserved for future use)
    /// * `location` - Source location for error reporting
    ///
    /// # Returns
    /// A tuple of HIR nodes for setup and the field access expression
    pub fn transform_field_access(
        &mut self,
        base: &Expression,
        field: InternedString,
        _ctx: &mut HirBuilderContext,
        location: &TextLocation,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        let nodes = Vec::new();

        // Extract the base variable name
        let base_var = self.extract_base_variable(base)?;

        // Create the field access expression
        let field_expr = HirExpr {
            kind: HirExprKind::Field {
                base: base_var,
                field,
            },
            location: location.clone(),
        };

        Ok((nodes, field_expr))
    }

    /// Transforms field assignment into HIR representation.
    ///
    /// This creates HIR instructions for assigning a value to a struct field.
    ///
    /// # Arguments
    /// * `base` - The base expression (the struct being modified)
    /// * `field` - The field name being assigned
    /// * `value` - The value being assigned
    /// * `ctx` - The HIR builder context
    /// * `location` - Source location for error reporting
    ///
    /// # Returns
    /// HIR nodes for the field assignment
    pub fn transform_field_assignment(
        &mut self,
        base: &Expression,
        field: InternedString,
        value: &Expression,
        ctx: &mut HirBuilderContext,
        location: &TextLocation,
    ) -> Result<Vec<HirNode>, CompilerError> {
        // Extract the base variable name
        let base_var = self.extract_base_variable(base)?;

        // Transform the value expression
        let hir_value = self.transform_field_value(value, ctx)?;

        // Create the field place
        let field_place = HirPlace::Field {
            base: Box::new(HirPlace::Var(base_var)),
            field,
        };

        // Create the assignment node
        let node_id = ctx.allocate_node_id();
        let assign_node = HirNode {
            kind: HirKind::Stmt(HirStmt::Assign {
                target: field_place,
                value: hir_value,
                is_mutable: true, // Field assignments require mutable access
            }),
            location: location.clone(),
            id: node_id,
        };

        Ok(vec![assign_node])
    }

    /// Calculates the field offset for a struct field.
    ///
    /// # Arguments
    /// * `struct_name` - The struct type name
    /// * `field` - The field name
    ///
    /// # Returns
    /// The byte offset of the field within the struct
    pub fn calculate_field_offset(
        &self,
        struct_name: InternedString,
        field: InternedString,
    ) -> Result<u32, CompilerError> {
        if let Some(layout) = self.layout_calculator.get_layout(&struct_name) {
            if let Some(offset) = layout.get_field_offset(&field) {
                return Ok(offset);
            }
            return_hir_transformation_error!(
                format!("Field not found in struct layout"),
                crate::compiler_frontend::compiler_errors::ErrorLocation::default(),
                {
                    CompilationStage => "HIR Generation - Struct Field Offset",
                    PrimarySuggestion => "Ensure the field exists in the struct definition"
                }
            );
        }
        return_hir_transformation_error!(
            format!("Struct layout not found"),
            crate::compiler_frontend::compiler_errors::ErrorLocation::default(),
            {
                CompilationStage => "HIR Generation - Struct Field Offset",
                PrimarySuggestion => "Ensure the struct is defined before accessing fields"
            }
        );
    }

    /// Gets the layout for a struct type
    pub fn get_struct_layout(&self, struct_name: &InternedString) -> Option<&StructLayout> {
        self.layout_calculator.get_layout(struct_name)
    }

    /// Registers a struct layout (used when processing struct definitions)
    pub fn register_struct_layout(&mut self, name: InternedString, fields: &[Var]) -> StructLayout {
        self.layout_calculator.calculate_layout(name, fields)
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Transforms a field value expression into HIR.
    fn transform_field_value(
        &self,
        expr: &Expression,
        _ctx: &mut HirBuilderContext,
    ) -> Result<HirExpr, CompilerError> {
        let hir_expr_kind = match &expr.kind {
            ExpressionKind::Int(val) => HirExprKind::Int(*val),
            ExpressionKind::Float(val) => HirExprKind::Float(*val),
            ExpressionKind::Bool(val) => HirExprKind::Bool(*val),
            ExpressionKind::StringSlice(s) => HirExprKind::StringLiteral(*s),
            ExpressionKind::Char(c) => HirExprKind::Char(*c),
            ExpressionKind::Reference(name) => HirExprKind::Load(HirPlace::Var(*name)),
            ExpressionKind::None => HirExprKind::Int(0), // Placeholder for None
            _ => {
                // For complex expressions, we would need to use the expression linearizer
                // For now, return an error for unsupported expressions
                return_hir_transformation_error!(
                    format!(
                        "Complex expression in struct field not yet supported: {:?}",
                        expr.kind
                    ),
                    expr.location.to_error_location_without_table(),
                    {
                        CompilationStage => "HIR Generation - Struct Field Value",
                        PrimarySuggestion => "Simplify the field value expression"
                    }
                );
            }
        };

        Ok(HirExpr {
            kind: hir_expr_kind,
            location: expr.location.clone(),
        })
    }

    /// Extracts the base variable name from an expression.
    fn extract_base_variable(&self, expr: &Expression) -> Result<InternedString, CompilerError> {
        match &expr.kind {
            ExpressionKind::Reference(name) => Ok(*name),
            _ => {
                return_hir_transformation_error!(
                    format!("Cannot extract base variable from expression: {:?}", expr.kind),
                    expr.location.to_error_location_without_table(),
                    {
                        CompilationStage => "HIR Generation - Struct Field Access",
                        PrimarySuggestion => "Use a variable reference as the base for field access"
                    }
                );
            }
        }
    }
}

// ============================================================================
// Heap Allocation and Memory Management
// ============================================================================

impl StructHandler {
    /// Handles heap allocation for struct instances.
    ///
    /// This creates HIR instructions for allocating memory on the heap
    /// and integrates with Beanstalk's memory management system.
    ///
    /// # Arguments
    /// * `size` - The size in bytes to allocate
    /// * `alignment` - The alignment requirement for the allocation
    /// * `ctx` - The HIR builder context
    /// * `location` - Source location for error reporting
    ///
    /// # Returns
    /// A tuple of HIR nodes for the allocation and the resulting pointer expression
    pub fn handle_heap_allocation(
        &mut self,
        size: u32,
        alignment: u32,
        ctx: &mut HirBuilderContext,
        location: &TextLocation,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        // Create a host call to the memory allocation function
        // In Beanstalk, heap allocation is handled by the runtime

        // Create arguments for the allocation call
        let size_expr = HirExpr {
            kind: HirExprKind::Int(size as i64),
            location: location.clone(),
        };

        let align_expr = HirExpr {
            kind: HirExprKind::Int(alignment as i64),
            location: location.clone(),
        };

        // Create the host call node for allocation
        let node_id = ctx.allocate_node_id();
        let alloc_node = HirNode {
            kind: HirKind::Stmt(HirStmt::Call {
                target: CallTarget::HostFunction(HostFunctionId::Alloc),
                args: vec![size_expr, align_expr],
            }),
            location: location.clone(),
            id: node_id,
        };

        // The result is a pointer (represented as Int in WASM)
        let result_expr = HirExpr {
            kind: HirExprKind::Call {
                target: CallTarget::HostFunction(HostFunctionId::Alloc),
                args: vec![],
            },
            location: location.clone(),
        };

        Ok((vec![alloc_node], result_expr))
    }

    /// Handles complex field access patterns with proper offset calculations.
    ///
    /// This method handles nested field access like `a.b.c` by calculating
    /// the cumulative offset through the struct hierarchy.
    ///
    /// # Arguments
    /// * `base_expr` - The base expression (the outermost struct)
    /// * `field_path` - The path of field names to access
    /// * `_ctx` - The HIR builder context (unused, reserved for future use)
    /// * `location` - Source location for error reporting
    ///
    /// # Returns
    /// A tuple of HIR nodes for setup and the final field access expression
    pub fn handle_complex_field_access(
        &mut self,
        base_expr: &Expression,
        field_path: &[InternedString],
        _ctx: &mut HirBuilderContext,
        location: &TextLocation,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        if field_path.is_empty() {
            return_hir_transformation_error!(
                "Empty field path in complex field access",
                location.to_error_location_without_table(),
                {
                    CompilationStage => "HIR Generation - Complex Field Access",
                    PrimarySuggestion => "Provide at least one field name"
                }
            );
        }

        // Start with the base variable
        let base_var = self.extract_base_variable(base_expr)?;

        // Build the nested field access
        let mut current_place = HirPlace::Var(base_var);
        let mut current_type = base_expr.data_type.clone();

        for field_name in field_path {
            // Get the field type from the current struct type
            let field_type = self.get_field_type_from_struct(&current_type, *field_name)?;

            // Create nested field place
            current_place = HirPlace::Field {
                base: Box::new(current_place),
                field: *field_name,
            };

            current_type = field_type;
        }

        // Create the final load expression
        let result_expr = HirExpr {
            kind: HirExprKind::Load(current_place),
            location: location.clone(),
        };

        Ok((Vec::new(), result_expr))
    }

    /// Gets the type of a field from a struct type.
    fn get_field_type_from_struct(
        &self,
        struct_type: &DataType,
        field_name: InternedString,
    ) -> Result<DataType, CompilerError> {
        match struct_type {
            DataType::Struct(fields, _) => {
                for field in fields {
                    if field.id == field_name {
                        return Ok(field.value.data_type.clone());
                    }
                }
                return_hir_transformation_error!(
                    "Field not found in struct type".to_string(),
                    crate::compiler_frontend::compiler_errors::ErrorLocation::default(),
                    {
                        CompilationStage => "HIR Generation - Field Type Lookup",
                        PrimarySuggestion => "Ensure the field exists in the struct definition"
                    }
                );
            }
            _ => {
                return_hir_transformation_error!(
                    format!("Cannot access field on non-struct type: {:?}", struct_type),
                    crate::compiler_frontend::compiler_errors::ErrorLocation::default(),
                    {
                        CompilationStage => "HIR Generation - Field Type Lookup",
                        PrimarySuggestion => "Field access is only valid on struct types"
                    }
                );
            }
        }
    }

    /// Calculates the total offset for a nested field path.
    ///
    /// This is useful for generating efficient memory access instructions
    /// when the full path is known at compile time.
    pub fn calculate_nested_field_offset(
        &mut self,
        struct_name: InternedString,
        field_path: &[InternedString],
        ctx: &HirBuilderContext,
    ) -> Result<u32, CompilerError> {
        if field_path.is_empty() {
            return Ok(0);
        }

        let mut total_offset = 0u32;
        let mut current_struct = struct_name;

        for (i, field_name) in field_path.iter().enumerate() {
            // Get the offset for this field
            let offset = self.calculate_field_offset(current_struct, *field_name)?;
            total_offset += offset;

            // If not the last field, get the struct type for the next iteration
            if i < field_path.len() - 1 {
                // Look up the field's type to continue traversal
                if let Some(struct_def) = ctx.get_struct_definition(&current_struct) {
                    for field in struct_def {
                        if field.id == *field_name {
                            // Check if this field is a struct type
                            if let DataType::Struct(_, _) = &field.value.data_type {
                                // We need to find the struct name for this nested struct
                                // For now, we'll use a placeholder approach
                                // In a full implementation, we'd track struct names properly
                                current_struct = *field_name;
                            } else {
                                return_hir_transformation_error!(
                                    format!("Cannot access nested field on non-struct type"),
                                    crate::compiler_frontend::compiler_errors::ErrorLocation::default(),
                                    {
                                        CompilationStage => "HIR Generation - Nested Field Offset",
                                        PrimarySuggestion => "Intermediate fields must be struct types"
                                    }
                                );
                            }
                            break;
                        }
                    }
                }
            }
        }

        Ok(total_offset)
    }

    /// Handles struct destructuring into individual field bindings.
    ///
    /// This generates HIR instructions for extracting fields from a struct
    /// with proper ownership semantics.
    ///
    /// # Arguments
    /// * `struct_expr` - The struct expression to destructure
    /// * `bindings` - The field names and their target variable names
    /// * `ctx` - The HIR builder context
    /// * `location` - Source location for error reporting
    ///
    /// # Returns
    /// HIR nodes for the destructuring assignments
    pub fn handle_struct_destructuring(
        &mut self,
        struct_expr: &Expression,
        bindings: &[(InternedString, InternedString)], // (field_name, target_var)
        ctx: &mut HirBuilderContext,
        location: &TextLocation,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();
        let base_var = self.extract_base_variable(struct_expr)?;

        for (field_name, target_var) in bindings {
            // Create the field access expression
            let field_expr = HirExpr {
                kind: HirExprKind::Field {
                    base: base_var,
                    field: *field_name,
                },
                location: location.clone(),
            };

            // Create the assignment to the target variable
            let node_id = ctx.allocate_node_id();
            let assign_node = HirNode {
                kind: HirKind::Stmt(HirStmt::Assign {
                    target: HirPlace::Var(*target_var),
                    value: field_expr,
                    is_mutable: false, // Destructuring creates immutable bindings by default
                }),
                location: location.clone(),
                id: node_id,
            };

            nodes.push(assign_node);

            // Mark the target variable as potentially owned
            // (ownership depends on whether the struct was owned)
            ctx.mark_potentially_owned(*target_var);
        }

        Ok(nodes)
    }

    /// Gets the size of a struct type.
    pub fn get_struct_size(&mut self, struct_name: InternedString, fields: &[Var]) -> u32 {
        let layout = self.layout_calculator.calculate_layout(struct_name, fields);
        layout.total_size
    }

    /// Gets the alignment of a struct type.
    pub fn get_struct_alignment(&mut self, struct_name: InternedString, fields: &[Var]) -> u32 {
        let layout = self.layout_calculator.calculate_layout(struct_name, fields);
        layout.alignment
    }
}
