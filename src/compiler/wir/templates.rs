//! # Template Transformation Module
//!
//! This module contains functions for handling template-to-string conversion,
//! managing variable interpolation in templates, processing struct literal
//! transformations, and handling string concatenation operations.

// Import context types from context module
use crate::compiler::wir::context::WirTransformContext;

// Import WIR types
use crate::compiler::wir::place::{Place, ProjectionElem, FieldOffset, FieldSize};
use crate::compiler::wir::wir_nodes::{Constant, Operand, Rvalue, Statement};

// Import utility functions
use crate::compiler::wir::utilities::lookup_variable_or_error;

// Import expression transformation
use crate::compiler::wir::expressions::expression_to_rvalue_with_context;

// Core compiler imports
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::{
    compiler_errors::CompileError, datatypes::DataType,
    parsers::statements::create_template_node::Template, string_interning::StringTable,
};

/// Transform a Beanstalk template expression to WIR rvalue
///
/// Converts Beanstalk's template syntax into WIR statements and rvalues for
/// string generation. Templates can be either compile-time (folded to constants)
/// or runtime (generating function calls for dynamic content).
///
/// # Parameters
///
/// - `template`: Template AST node with head and content
/// - `location`: Source location for error reporting
/// - `string_table`: String interning table for string resolution
/// - `context`: Transformation context for variable lookup and temporary allocation
///
/// # Returns
///
/// - `Ok((statements, rvalue))`: Statements to build template and resulting string rvalue
/// - `Err(CompileError)`: Template transformation error with source location
///
/// # Template Processing
///
/// 1. **Content Analysis**: Examine template content for variables and expressions
/// 2. **String Building**: Generate statements to construct the final string
/// 3. **Variable Interpolation**: Handle embedded variables and expressions
/// 4. **Result Generation**: Create rvalue representing the final template string
///
/// # Implementation Status
///
/// Currently implements basic template transformation. Full template support includes
/// string concatenation, variable interpolation, and template compilation.
pub fn transform_template_to_rvalue(
    template: &Template,
    location: &TextLocation,
    string_table: &mut StringTable,
    context: &mut WirTransformContext,
) -> Result<(Vec<Statement>, Rvalue), CompileError> {
    // TODO
    // This should bascically create a function that returns the constructed string at runtime

    // Create a temporary place for the template result
    let result_place = context.create_temporary_place(&DataType::Template);

    // Transform template content to string operations
    let mut statements = Vec::new();

    // Handle template content transformation
    let content_statements =
        transform_template_content(&template.content, location, string_table, context)?;
    statements.extend(content_statements);
    
    // Temporary placeholder for template result
    let template_rvalue = Rvalue::Use(Operand::Copy(result_place.clone()));

    // TODO
    statements.push(Statement::Assign {
        place: result_place.clone(),
        rvalue: template_rvalue,
    });

    Ok((statements, Rvalue::Use(Operand::Copy(result_place))))
}

/// Transform runtime template to rvalue
pub fn transform_runtime_template_to_rvalue(
    _template: &Template,
    location: &TextLocation,
    string_table: &mut StringTable,
    context: &mut WirTransformContext,
) -> Result<(Vec<Statement>, Rvalue), CompileError> {
    // Runtime templates need to be evaluated at runtime
    // This involves creating function calls for template evaluation

    let mut statements = Vec::new();
    let result_place = context.create_temporary_place(&DataType::Template);

    // Transform template for runtime evaluation
    let runtime_statements =
        transform_template_for_runtime(_template, location, string_table, context)?;
    statements.extend(runtime_statements);

    // Create runtime template evaluation call
    // This is a placeholder - actual implementation would call template runtime functions
    let placeholder_id = string_table.intern("runtime_template_placeholder");
    let runtime_rvalue = Rvalue::Use(Operand::Constant(Constant::String(placeholder_id)));

    statements.push(Statement::Assign {
        place: result_place.clone(),
        rvalue: runtime_rvalue,
    });

    Ok((statements, Rvalue::Use(Operand::Copy(result_place))))
}

/// Transform template with variable interpolation
pub fn transform_template_with_variable_interpolation(
    template: &Template,
    variables: &[String],
    location: &TextLocation,
    string_table: &mut StringTable,
    context: &mut WirTransformContext,
) -> Result<(Vec<Statement>, Rvalue), CompileError> {
    let mut statements = Vec::new();
    let result_place = context.create_temporary_place(&DataType::String);

    // Handle variable interpolation in templates
    for variable_name in variables {
        // Use consolidated helper for variable lookup
        let variable_place = lookup_variable_or_error(
            context,
            variable_name,
            location,
            string_table,
            "template interpolation",
        )?;

        // Create string coercion for the variable
        let coercion_statements =
            create_string_coercion(&variable_place, location, string_table, context)?;
        statements.extend(coercion_statements);
    }

    // Create interpolated template result
    let placeholder_id = string_table.intern("interpolated_template_placeholder");
    let interpolated_rvalue = Rvalue::Use(Operand::Constant(Constant::String(placeholder_id)));

    statements.push(Statement::Assign {
        place: result_place.clone(),
        rvalue: interpolated_rvalue,
    });

    Ok((statements, Rvalue::Use(Operand::Copy(result_place))))
}

/// Transform struct literal to statements and rvalue
pub fn transform_struct_literal_to_statements_and_rvalue(
    fields: &[crate::compiler::parsers::ast_nodes::Arg],
    location: &TextLocation,
    string_table: &mut StringTable,
    context: &mut WirTransformContext,
) -> Result<(Vec<Statement>, Rvalue), CompileError> {
    let mut statements = Vec::new();

    // Create a temporary place for the struct
    let struct_place = context.create_temporary_place(&DataType::Struct(
        fields.to_vec(),
        crate::compiler::datatypes::Ownership::ImmutableOwned,
    ));

    // Transform each field assignment
    for field in fields {
        let field_statements = transform_struct_field_assignment(
            field,
            &struct_place,
            location,
            string_table,
            context,
        )?;
        statements.extend(field_statements);
    }

    Ok((statements, Rvalue::Use(Operand::Copy(struct_place))))
}

/// Transform struct literal to rvalue (simplified version)
pub fn transform_struct_literal_to_rvalue(
    fields: &[crate::compiler::parsers::ast_nodes::Arg],
    location: &TextLocation,
    string_table: &mut StringTable,
    context: &mut WirTransformContext,
) -> Result<Rvalue, CompileError> {
    let (_, rvalue) =
        transform_struct_literal_to_statements_and_rvalue(fields, location, string_table, context)?;
    Ok(rvalue)
}

/// Create string coercion for a place
fn create_string_coercion(
    place: &Place,
    _location: &TextLocation,
    _string_table: &mut StringTable,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    let mut statements = Vec::new();

    // Create a temporary place for the string result
    let string_place = context.create_temporary_place(&DataType::String);

    // Create coercion operation (placeholder implementation)
    let coercion_rvalue = Rvalue::Use(Operand::Copy(place.clone()));

    statements.push(Statement::Assign {
        place: string_place,
        rvalue: coercion_rvalue,
    });

    Ok(statements)
}

/// Transform template content
fn transform_template_content(
    content: &crate::compiler::parsers::statements::template::TemplateContent,
    location: &TextLocation,
    string_table: &mut StringTable,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    let mut statements = Vec::new();

    // Transform before expressions
    for expr in &content.before {
        let (expr_statements, _) =
            expression_to_rvalue_with_context(
                expr,
                location,
                context,
                string_table,
            )?;
        statements.extend(expr_statements);
    }

    // Transform after expressions
    for expr in &content.after {
        let (expr_statements, _) =
            expression_to_rvalue_with_context(
                expr,
                location,
                context,
                string_table,
            )?;
        statements.extend(expr_statements);
    }

    Ok(statements)
}

/// Transform template for runtime evaluation
fn transform_template_for_runtime(
    template: &Template,
    location: &TextLocation,
    string_table: &mut StringTable,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    let mut statements = Vec::new();

    // Transform template content for runtime
    let content_statements =
        transform_template_content(&template.content, location, string_table, context)?;
    statements.extend(content_statements);

    // Add runtime template evaluation setup
    // This is a placeholder for actual runtime template handling

    Ok(statements)
}

/// Transform struct field assignment
fn transform_struct_field_assignment(
    field: &crate::compiler::parsers::ast_nodes::Arg,
    struct_place: &Place,
    location: &TextLocation,
    string_table: &mut StringTable,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    let mut statements = Vec::new();

    // Create field projection
    // TODO: Proper field index resolution based on struct definition
    let field_place = Place::Projection {
        base: Box::new(struct_place.clone()),
        elem: ProjectionElem::Field {
            index: 0, // Placeholder - should be resolved from struct definition
            offset: FieldOffset(0),
            size: FieldSize::Fixed(4), // Placeholder - 4 bytes
        },
    };

    // Transform field value
    let (value_statements, value_rvalue) =
        expression_to_rvalue_with_context(
            &field.value,
            location,
            context,
            string_table,
        )?;
    statements.extend(value_statements);

    statements.push(Statement::Assign {
        place: field_place,
        rvalue: value_rvalue,
    });

    Ok(statements)
}

/// Extract string from template or expression
pub fn extract_string_from_template(
    template: &Template,
    location: &TextLocation,
    string_table: &mut StringTable,
    context: &mut WirTransformContext,
) -> Result<(Vec<Statement>, String), CompileError> {
    // Extract compile-time string from template if possible
    // This is a placeholder implementation

    let statements =
        transform_template_content(&template.content, location, string_table, context)?;
    let extracted_string = "extracted_template_string".to_string();

    Ok((statements, extracted_string))
}
