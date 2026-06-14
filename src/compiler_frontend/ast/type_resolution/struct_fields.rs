//! Struct field type resolution and default-value constant inlining.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::eval_expression::ExpressionTypingError;
use crate::compiler_frontend::ast::expressions::eval_expression::evaluate_expression;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::type_resolution::{
    TypeResolutionContext, resolve_diagnostic_type_to_type_id_checked,
};
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationTable};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompileTimeEvaluationErrorReason, CompilerDiagnostic,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use rustc_hash::FxHashSet;
use std::rc::Rc;

use super::resolve_named_signature_type;

pub(crate) enum StructFieldResolutionError {
    Diagnostic(Box<CompilerDiagnostic>),
    Infrastructure(Box<CompilerError>),
}

impl From<CompilerDiagnostic> for StructFieldResolutionError {
    fn from(diagnostic: CompilerDiagnostic) -> Self {
        StructFieldResolutionError::Diagnostic(Box::new(diagnostic))
    }
}

impl From<Box<CompilerDiagnostic>> for StructFieldResolutionError {
    fn from(diagnostic: Box<CompilerDiagnostic>) -> Self {
        StructFieldResolutionError::Diagnostic(diagnostic)
    }
}

impl From<CompilerError> for StructFieldResolutionError {
    fn from(error: CompilerError) -> Self {
        StructFieldResolutionError::Infrastructure(Box::new(error))
    }
}

impl From<Box<CompilerError>> for StructFieldResolutionError {
    fn from(error: Box<CompilerError>) -> Self {
        StructFieldResolutionError::Infrastructure(error)
    }
}

impl From<ExpressionTypingError> for StructFieldResolutionError {
    fn from(error: ExpressionTypingError) -> Self {
        match error {
            ExpressionTypingError::Diagnostic(diagnostic) => {
                StructFieldResolutionError::Diagnostic(diagnostic)
            }
            ExpressionTypingError::Infrastructure(error) => {
                StructFieldResolutionError::Infrastructure(error)
            }
        }
    }
}

// ------------------------------
//  Struct field type resolution
// ------------------------------

/// Resolve all declared struct field types against visible declarations.
pub(crate) fn resolve_struct_field_types(
    struct_path: &InternedPath,
    fields: &[Declaration],
    type_resolution_context: &mut TypeResolutionContext<'_>,
    string_table: &mut StringTable,
) -> Result<Vec<Declaration>, StructFieldResolutionError> {
    let resolved_fields = resolve_struct_field_types_inner(
        fields,
        type_resolution_context,
        string_table,
        StructFieldResolutionMode::FinalDefinition,
    )?;

    validate_resolved_field_parent_paths(struct_path, &resolved_fields)?;

    Ok(resolved_fields)
}

/// Resolve struct field shell types for constant-time constructor parsing.
///
/// WHAT: turns parsed field type annotations into semantic `TypeId`s without inlining or
/// validating default expressions.
/// WHY: constants are resolved before final nominal member definitions are written to
/// `TypeEnvironment`; their constructors still need checked semantic field types, while
/// defaults may legitimately reference constants that are not resolved until this stage runs.
pub(crate) fn resolve_struct_constructor_shell_types(
    struct_path: &InternedPath,
    fields: &[Declaration],
    type_resolution_context: &mut TypeResolutionContext<'_>,
    string_table: &mut StringTable,
) -> Result<Vec<Declaration>, StructFieldResolutionError> {
    let resolved_fields = resolve_struct_field_types_inner(
        fields,
        type_resolution_context,
        string_table,
        StructFieldResolutionMode::ConstructorShell,
    )?;

    validate_resolved_field_parent_paths(struct_path, &resolved_fields)?;

    Ok(resolved_fields)
}

fn validate_resolved_field_parent_paths(
    struct_path: &InternedPath,
    resolved_fields: &[Declaration],
) -> Result<(), StructFieldResolutionError> {
    if resolved_fields.is_empty() {
        return Ok(());
    }

    for field in resolved_fields {
        let Some(parent) = field.id.parent() else {
            return Err(CompilerError::compiler_error(
                "Resolved struct field is missing its parent struct path.",
            )
            .into());
        };

        if parent != *struct_path {
            return Err(CompilerError::compiler_error(
                "Resolved struct field parent does not match the enclosing struct declaration.",
            )
            .into());
        }
    }

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StructFieldResolutionMode {
    ConstructorShell,
    FinalDefinition,
}

fn resolve_struct_field_types_inner(
    fields: &[Declaration],
    type_resolution_context: &mut TypeResolutionContext<'_>,
    string_table: &mut StringTable,
    mode: StructFieldResolutionMode,
) -> Result<Vec<Declaration>, StructFieldResolutionError> {
    // WHY: Struct fields must enter AST/HIR in fully resolved nominal form so later
    // phases do not carry unresolved `NamedType` placeholders.
    let mut resolved_fields = Vec::with_capacity(fields.len());

    for field in fields {
        let mut resolved_field = field.to_owned();

        resolved_field.value.diagnostic_type = resolve_named_signature_type(
            &field.value.diagnostic_type,
            &field.value.location,
            type_resolution_context,
            string_table,
        )?;

        let type_environment = &mut *type_resolution_context.type_environment;

        resolved_field.value.type_id = resolve_diagnostic_type_to_type_id_checked(
            &resolved_field.value.diagnostic_type,
            type_environment,
            &resolved_field.value.location,
        )?;

        if mode == StructFieldResolutionMode::FinalDefinition {
            resolved_field.value = inline_visible_constant_references(
                &resolved_field.value,
                type_resolution_context.declaration_table,
                type_resolution_context.visible_declaration_ids,
                type_environment,
                string_table,
            )?;

            if !matches!(resolved_field.value.kind, ExpressionKind::NoValue)
                && !resolved_field.value.is_compile_time_constant()
            {
                return Err(CompilerDiagnostic::invalid_struct_default_value(
                    resolved_field.value.location.clone(),
                )
                .into());
            }
        }

        resolved_fields.push(resolved_field);
    }

    Ok(resolved_fields)
}

// ----------------------------------
//  Constant inlining for field defaults
// ----------------------------------

fn inline_visible_constant_references(
    expression: &Expression,
    declaration_table: &Rc<TopLevelDeclarationTable>,
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    type_environment: &mut TypeEnvironment,
    string_table: &mut StringTable,
) -> Result<Expression, StructFieldResolutionError> {
    inline_visible_constant_references_impl(
        expression,
        declaration_table,
        visible_declaration_ids,
        type_environment,
        string_table,
    )
}

fn inline_visible_constant_references_impl(
    expression: &Expression,
    declaration_table: &Rc<TopLevelDeclarationTable>,
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    type_environment: &mut TypeEnvironment,
    string_table: &mut StringTable,
) -> Result<Expression, StructFieldResolutionError> {
    match &expression.kind {
        // Direct reference — try to resolve to a visible compile-time constant.
        ExpressionKind::Reference(path) => Ok(declaration_table
            .get_visible_resolved_by_path(path, visible_declaration_ids)
            .filter(|declaration| declaration.value.is_compile_time_constant())
            .or_else(|| {
                path.name().and_then(|name| {
                    declaration_table
                        .get_visible_resolved_by_name(name, visible_declaration_ids)
                        .filter(|declaration| declaration.value.is_compile_time_constant())
                })
            })
            .map(|declaration| {
                let mut resolved = declaration.value.to_owned();
                resolved.location = expression.location.clone();
                resolved
            })
            .unwrap_or_else(|| expression.to_owned())),

        // Runtime expression — inline constants inside nested nodes, then re-evaluate.
        ExpressionKind::Runtime(runtime_nodes) => {
            let mut rewritten_nodes = Vec::with_capacity(runtime_nodes.len());

            for node in runtime_nodes {
                rewritten_nodes.push(inline_visible_constant_references_in_node(
                    node,
                    declaration_table,
                    visible_declaration_ids,
                    type_environment,
                    string_table,
                )?);
            }

            let mut current_type = ExpectedType::Known(expression.type_id);

            let mut evaluation_context = ScopeContext::new(
                ContextKind::ConstantHeader,
                expression.location.scope.to_owned(),
                Rc::clone(declaration_table),
                ExternalPackageRegistry::new(),
                Vec::new(),
            );

            if let Some(visible) = visible_declaration_ids {
                evaluation_context.visible_declaration_ids = Some(visible.to_owned());
            }

            let mut compatibility_cache = TypeCompatibilityCache::new();
            let mut type_interner =
                AstTypeInterner::new(type_environment, &mut compatibility_cache);
            evaluate_expression(
                &evaluation_context,
                rewritten_nodes,
                &mut type_interner,
                &mut current_type,
                &expression.value_mode,
                string_table,
            )
            .map_err(|_| {
                CompilerDiagnostic::compile_time_evaluation_error(
                    CompileTimeEvaluationErrorReason::StructFieldDefaultNotFoldable,
                    None,
                    expression.location.clone(),
                )
            })
            .map_err(StructFieldResolutionError::from)
        }

        // Collection — inline each element.
        ExpressionKind::Collection(elements) => {
            let mut resolved_elements = Vec::with_capacity(elements.len());

            for element in elements {
                resolved_elements.push(inline_visible_constant_references_impl(
                    element,
                    declaration_table,
                    visible_declaration_ids,
                    type_environment,
                    string_table,
                )?);
            }

            Ok(expression_with_inlined_kind(
                expression,
                ExpressionKind::Collection(resolved_elements),
            ))
        }

        // Struct instance — inline each field value.
        ExpressionKind::StructInstance(fields) => {
            let mut resolved_fields = Vec::with_capacity(fields.len());

            for field in fields {
                resolved_fields.push(Declaration {
                    id: field.id.to_owned(),
                    value: inline_visible_constant_references_impl(
                        &field.value,
                        declaration_table,
                        visible_declaration_ids,
                        type_environment,
                        string_table,
                    )?,
                });
            }

            Ok(expression_with_inlined_kind(
                expression,
                ExpressionKind::StructInstance(resolved_fields),
            ))
        }

        // Range — inline start and end.
        ExpressionKind::Range(start, end) => Ok(expression_with_inlined_kind(
            expression,
            ExpressionKind::Range(
                Box::new(inline_visible_constant_references(
                    start,
                    declaration_table,
                    visible_declaration_ids,
                    type_environment,
                    string_table,
                )?),
                Box::new(inline_visible_constant_references(
                    end,
                    declaration_table,
                    visible_declaration_ids,
                    type_environment,
                    string_table,
                )?),
            ),
        )),

        // Result construct — inline the wrapped value.
        ExpressionKind::FallibleCarrierConstruct { variant, value } => {
            Ok(expression_with_inlined_kind(
                expression,
                ExpressionKind::FallibleCarrierConstruct {
                    variant: *variant,
                    value: Box::new(inline_visible_constant_references(
                        value,
                        declaration_table,
                        visible_declaration_ids,
                        type_environment,
                        string_table,
                    )?),
                },
            ))
        }

        // Coercion — inline the inner value.
        ExpressionKind::Coerced { value, to_type } => Ok(expression_with_inlined_kind(
            expression,
            ExpressionKind::Coerced {
                value: Box::new(inline_visible_constant_references(
                    value,
                    declaration_table,
                    visible_declaration_ids,
                    type_environment,
                    string_table,
                )?),
                to_type: *to_type,
            },
        )),

        // Everything else — no inlining needed.
        _ => Ok(expression.to_owned()),
    }
}

fn expression_with_inlined_kind(expression: &Expression, kind: ExpressionKind) -> Expression {
    let mut rewritten = Expression::new(
        kind,
        expression.location.clone(),
        expression.type_id,
        expression.diagnostic_type.to_owned(),
        expression.value_mode.to_owned(),
    );

    // Constant-reference inlining replaces only the structural children. The surrounding value
    // keeps its previously resolved metadata so later expression policy still sees the same
    // const-record, reactive, division-provenance, and string/template/path shape facts.
    rewritten.const_record_state = expression.const_record_state;
    rewritten.reactive_source = expression.reactive_source.clone();
    rewritten.reactive_template = expression.reactive_template.clone();
    rewritten.contains_regular_division = expression.contains_regular_division;
    rewritten.value_shape = expression.value_shape;
    rewritten
}

fn inline_visible_constant_references_in_node(
    node: &AstNode,
    declaration_table: &Rc<TopLevelDeclarationTable>,
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    type_environment: &mut TypeEnvironment,
    string_table: &mut StringTable,
) -> Result<AstNode, StructFieldResolutionError> {
    let mut rewritten_node = node.to_owned();

    rewritten_node.kind = match &node.kind {
        NodeKind::Rvalue(expression) => NodeKind::Rvalue(inline_visible_constant_references_impl(
            expression,
            declaration_table,
            visible_declaration_ids,
            type_environment,
            string_table,
        )?),

        NodeKind::VariableDeclaration(declaration) => NodeKind::VariableDeclaration(Declaration {
            id: declaration.id.to_owned(),
            value: inline_visible_constant_references_impl(
                &declaration.value,
                declaration_table,
                visible_declaration_ids,
                type_environment,
                string_table,
            )?,
        }),

        _ => node.kind.to_owned(),
    };

    Ok(rewritten_node)
}
