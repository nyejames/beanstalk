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

use crate::ast_log;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ReactiveSource,
};
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_renderability::is_template_renderable_type;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::paths::rendered_path_usage::resolve_compile_time_paths_for_rendered_output;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation};
use crate::compiler_frontend::value_mode::ValueMode;

fn is_unresolved_constant_placeholder_reference(
    expression: &Expression,
    context: &ScopeContext,
) -> bool {
    let ExpressionKind::Reference(path) = &expression.kind else {
        return false;
    };

    path.name()
        .and_then(|name| context.get_reference(&name))
        .is_some_and(Declaration::is_unresolved_constant_placeholder)
}

fn validate_template_head_value_type(
    expression: &Expression,
    location: &SourceLocation,
    type_environment: &TypeEnvironment,
) -> Result<(), CompilerDiagnostic> {
    if type_environment.is_fallible_carrier(expression.type_id) {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::ResultInTemplateHead,
            location.to_owned(),
        ));
    }

    // Template head values must be simple scalar types that can render as
    // text. Templates and paths are handled by separate code paths, so this
    // function only sees non-template, non-path expression values.
    //
    // Use the shared renderability classifier so the policy lives in one
    // AST-template-owned place and uses semantic TypeId identity.
    if is_template_renderable_type(expression.type_id, type_environment) {
        return Ok(());
    }

    Err(CompilerDiagnostic::invalid_template_structure(
        InvalidTemplateStructureReason::UnsupportedTypeInTemplateHead {
            type_id: expression.type_id,
        },
        location.to_owned(),
    ))
}

/// Handles a template-typed value found in the template head.
/// Wrapper templates preserve slot semantics; runtime templates mark unfoldable.
pub(super) fn handle_template_value_in_template_head(
    value: &Template,
    context: &ScopeContext,
    parent_template: &mut Template,
    foldable: &mut bool,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    if context.kind.is_constant_context() && matches!(value.kind, TemplateType::StringFunction) {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::RuntimeTemplateInConst,
            location.to_owned(),
        ));
    }

    if matches!(value.kind, TemplateType::Comment(_)) {
        return Ok(());
    }

    if matches!(value.kind, TemplateType::SlotDefinition(_)) {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::SlotInHead,
            location.to_owned(),
        ));
    }

    if matches!(value.kind, TemplateType::StringFunction) {
        *foldable = false;
    }

    parent_template.content.add_with_origin(
        Expression::template(value.to_owned(), ValueMode::ImmutableOwned),
        TemplateSegmentOrigin::Head,
    );

    Ok(())
}

/// Pushes a non-template expression into the head content after validation.
pub(super) fn push_template_head_expression(
    expression: Expression,
    context: &ScopeContext,
    type_environment: &TypeEnvironment,
    parent_template: &mut Template,
    foldable: &mut bool,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    if let ExpressionKind::Template(template_value) = &expression.kind {
        return handle_template_value_in_template_head(
            template_value,
            context,
            parent_template,
            foldable,
            location,
        );
    }

    let defer_inferred_type_validation =
        is_unresolved_constant_placeholder_reference(&expression, context);

    if !defer_inferred_type_validation {
        validate_template_head_value_type(&expression, location, type_environment)?;
    }

    if context.kind.is_constant_context()
        && !expression.is_compile_time_constant()
        && !is_unresolved_constant_placeholder_reference(&expression, context)
    {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::RuntimeValueInConstTemplateHead,
            location.to_owned(),
        ));
    }

    if !expression.kind.is_foldable() && !expression.is_compile_time_constant() {
        ast_log!("Template is no longer foldable due to reference");
        *foldable = false;
    }

    // Ordinary `[source]` insertion is a snapshot read. Keep reactive source identity only for
    // the explicit `$(source)` subscription path.
    let mut snapshot_expression = expression;
    snapshot_expression.clear_reactive_source();
    parent_template
        .content
        .add_with_origin(snapshot_expression, TemplateSegmentOrigin::Head);
    Ok(())
}

/// Pushes an explicit `$(source)` subscription into the head content.
///
/// WHAT: reuses the ordinary reference expression for current rendering while attaching V1
/// subscription metadata to the segment.
/// WHY: the language type stays `String`/underlying scalar rendering; the reactive dependency is
/// a template fact for later HIR/backend phases, not a value type or borrow.
pub(super) fn push_template_head_reactive_subscription(
    expression: Expression,
    source: ReactiveSource,
    context: &ScopeContext,
    type_environment: &TypeEnvironment,
    parent_template: &mut Template,
    foldable: &mut bool,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    if context.kind.is_constant_context() {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::ReactiveSubscriptionInConstTemplate,
            location.to_owned(),
        ));
    }

    validate_template_head_value_type(&expression, location, type_environment)?;

    *foldable = false;

    let subscription = ReactiveSubscription {
        source,
        type_id: expression.type_id,
        location: location.to_owned(),
    };
    parent_template.content.add_reactive_subscription(
        expression,
        TemplateSegmentOrigin::Head,
        subscription,
    );

    Ok(())
}

/// Coerces a compile-time path token in template head context to string output.
/// Emits source-file warnings when `.bst` files are inserted into rendered output.
pub(super) fn push_template_head_path_expression(
    paths: &[InternedPath],
    token_stream: &FileTokens,
    context: &ScopeContext,
    parent_template: &mut Template,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    if paths.is_empty() {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::EmptyPathInTemplateHead,
            token_stream.current_location(),
        ));
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
            let path_str = path.source_path.to_portable_string(string_table);
            context.emit_warning(CompilerDiagnostic::bst_file_path_in_template_output(
                string_table.get_or_intern(path_str),
                location,
            ));
        }
    }

    // Templates always fold to strings, so path text is eagerly formatted here.
    context.record_rendered_path_usages(recorded.usages);
    let interned = string_table.get_or_intern(recorded.rendered_text);
    parent_template.content.add_with_origin(
        Expression::string_slice(
            interned,
            token_stream.current_location(),
            ValueMode::ImmutableOwned,
        ),
        TemplateSegmentOrigin::Head,
    );

    Ok(())
}
