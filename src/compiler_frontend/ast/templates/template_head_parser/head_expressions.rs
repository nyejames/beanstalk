//! Head expression insertion helpers.
//!
//! WHAT:
//! - Normalizes values inserted from template heads into template content.
//! - Handles template-valued expressions, non-template expressions, and compile-time
//!   path coercion.
//!
//! WHY:
//! - Head parsing needs one place for foldability and const-context checks so the
//!   orchestration loop remains readable and consistent.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::{TemplateSegmentOrigin, TemplateType};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::rendered_path_usage::resolve_compile_time_paths_for_rendered_output;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation};
use crate::{ast_log, return_syntax_error};

fn is_unresolved_constant_placeholder_reference(expr: &Expression, context: &ScopeContext) -> bool {
    let ExpressionKind::Reference(path) = &expr.kind else {
        return false;
    };

    path.name()
        .and_then(|name| context.get_reference(&name))
        .is_some_and(Declaration::is_unresolved_constant_placeholder)
}

fn validate_template_head_value_type(
    expr: &Expression,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if expr.data_type.is_result() {
        return_syntax_error!(
            "Template head expressions do not implicitly unwrap Result values.",
            location.to_owned(),
            {
                PrimarySuggestion => "Handle the Result before the template boundary, for example with 'expr! fallback'",
            }
        );
    }

    if matches!(
        expr.data_type,
        DataType::StringSlice
            | DataType::Template
            | DataType::TemplateWrapper
            | DataType::Int
            | DataType::Float
            | DataType::Bool
            | DataType::Char
            | DataType::Path(_)
    ) {
        return Ok(());
    }

    return_syntax_error!(
        format!(
            "Template head expressions only accept final scalar or textual values, found '{}'.",
            expr.data_type.display_with_table(string_table)
        ),
        location.to_owned(),
        {
            PrimarySuggestion => "Convert the value before the template boundary or insert a template/scalar instead",
        }
    )
}

/// Handles a template-typed value found in the template head.
/// Wrapper templates preserve slot semantics; runtime templates mark unfoldable.
pub(super) fn handle_template_value_in_template_head(
    value: &Template,
    context: &ScopeContext,
    template: &mut Template,
    foldable: &mut bool,
    location: &SourceLocation,
    _string_table: &StringTable,
) -> Result<(), CompilerError> {
    if context.kind.is_constant_context() && matches!(value.kind, TemplateType::StringFunction) {
        return_syntax_error!(
            "Const templates can only capture compile-time templates.",
            location.to_owned()
        );
    }

    if matches!(value.kind, TemplateType::Comment(_)) {
        return Ok(());
    }

    if matches!(value.kind, TemplateType::SlotDefinition(_)) {
        return_syntax_error!(
            "'$slot' markers are only valid as direct nested templates inside template bodies.",
            location.to_owned()
        );
    }

    if matches!(value.kind, TemplateType::StringFunction) {
        *foldable = false;
    }

    template.content.add_with_origin(
        Expression::template(value.to_owned(), Ownership::ImmutableOwned),
        TemplateSegmentOrigin::Head,
    );

    Ok(())
}

/// Pushes a non-template expression into the head content after validation.
pub(super) fn push_template_head_expression(
    expr: Expression,
    context: &ScopeContext,
    template: &mut Template,
    foldable: &mut bool,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if let ExpressionKind::Template(template_value) = &expr.kind {
        return handle_template_value_in_template_head(
            template_value,
            context,
            template,
            foldable,
            location,
            string_table,
        );
    }

    let defer_inferred_type_validation = matches!(expr.data_type, DataType::Inferred)
        && context
            .top_level_declarations
            .declarations()
            .iter()
            .any(Declaration::is_unresolved_constant_placeholder);

    if !defer_inferred_type_validation {
        validate_template_head_value_type(&expr, location, string_table)?;
    }

    if context.kind.is_constant_context()
        && !expr.is_compile_time_constant()
        && !is_unresolved_constant_placeholder_reference(&expr, context)
    {
        return_syntax_error!(
            "Const templates can only capture compile-time values in the template head.",
            location.to_owned()
        );
    }

    if !expr.kind.is_foldable() && !expr.is_compile_time_constant() {
        ast_log!("Template is no longer foldable due to reference");
        *foldable = false;
    }

    template
        .content
        .add_with_origin(expr, TemplateSegmentOrigin::Head);
    Ok(())
}

/// Coerces a compile-time path token in template head context to string output.
/// Emits source-file warnings when `.bst` files are inserted into rendered output.
pub(super) fn push_template_head_path_expression(
    paths: &[InternedPath],
    token_stream: &FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    if paths.is_empty() {
        return_syntax_error!(
            "Path token in template head cannot be empty.",
            token_stream.current_location()
        );
    }

    let source_scope = context.required_source_file_scope("template head path coercion")?;
    let importer_file = source_scope.to_path_buf(string_table);
    let (resolved, recorded) = resolve_compile_time_paths_for_rendered_output(
        paths,
        context.required_project_path_resolver("template head path coercion")?,
        &importer_file,
        source_scope,
        &token_stream.current_location(),
        &context.path_format_config,
        string_table,
    )?;

    // Warn when a .bst source file path is coerced into template output.
    for path in &resolved.paths {
        if path
            .filesystem_path
            .extension()
            .is_some_and(|extension| extension == "bst")
        {
            let location = token_stream.current_location();
            context.emit_warning(CompilerWarning::new(
                &format!(
                    "Path to Beanstalk source file is being inserted into template output: '{}'",
                    path.source_path.to_portable_string(string_table)
                ),
                location,
                WarningKind::BstFilePathInTemplateOutput,
            ));
        }
    }

    // Templates always fold to strings, so path text is eagerly formatted here.
    context.record_rendered_path_usages(recorded.usages);
    let interned = string_table.get_or_intern(recorded.rendered_text);
    template.content.add_with_origin(
        Expression::string_slice(
            interned,
            token_stream.current_location(),
            Ownership::ImmutableOwned,
        ),
        TemplateSegmentOrigin::Head,
    );

    Ok(())
}
