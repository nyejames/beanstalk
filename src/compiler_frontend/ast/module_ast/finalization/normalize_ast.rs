//! AST node template normalization for HIR preparation.
//!
//! WHAT: Recursively traverses AST nodes to normalize embedded templates by
//! folding compile-time constants, building render plans, and materializing
//! template metadata. Mutates AST nodes in place to prepare them for HIR.
//!
//! WHY: HIR assumes templates are semantically complete with folded constants
//! and render plans. This normalization ensures the AST→HIR boundary contract
//! is satisfied before lowering.
//!
//! ## Normalization Strategy
//!
//! 1. **Constant Folding**: Templates with `RenderableString` const value kinds
//!    are folded into `StringSlice` expressions immediately.
//!
//! 2. **Render Plan Construction**: Runtime templates receive complete render
//!    plans so HIR doesn't need to reconstruct them.
//!
//! 3. **Metadata Completion**: All templates have `content_needs_formatting`
//!    set to false and their kind refreshed from content.
//!
//! 4. **Helper Rejection**: escaped `$insert(...)` helper templates are rejected
//!    if they reach finalization outside immediate wrapper-slot composition.
//!
//! ## AST→HIR Template Boundary
//!
//! AST owns:
//! - Template foldability decisions
//! - Render plan construction
//! - Constant template lowering
//! - Runtime template planning
//!
//! HIR receives:
//! - Folded constant templates as `StringSlice` expressions
//! - Runtime templates with complete render plans
//! - No escaped helper artifacts (`TemplateType::SlotInsert`)
//! - No templates requiring formatting

use super::finalizer::AstFinalizer;
use super::template_helpers::try_fold_template_to_string;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, LoopBindings, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, FallibleHandling,
};
use crate::compiler_frontend::ast::expressions::expression_types::CastHandling;
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::statements::value_production::types::ValueBlock;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::ast::templates::template::{TemplateAtom, TemplateType};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::value_mode::ValueMode;

struct TemplateNormalizationContext<'a, 'strings> {
    source_file_scope: &'a InternedPath,
    path_format_config: &'a PathStringFormatConfig,
    project_path_resolver: &'a ProjectPathResolver,
    template_const_loop_iteration_limit: usize,
    string_table: &'strings mut StringTable,
}

impl AstFinalizer<'_, '_> {
    /// Normalizes all templates in the AST for HIR consumption.
    ///
    /// WHAT: Traverses all AST nodes and normalizes embedded templates by
    /// folding constants and building render plans.
    ///
    /// WHY: Ensures HIR receives semantically complete templates without
    /// needing to understand template composition or folding rules.
    pub(super) fn normalize_ast_templates_for_hir(
        &self,
        ast: &mut [AstNode],
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<(), TemplateNormalizationError> {
        let canonical_source_by_symbol_path = &self
            .environment
            .lookups
            .module_symbols
            .canonical_source_by_symbol_path;
        let path_format_config = &self.context.path_format_config;

        for node in ast {
            let source_file_scope = canonical_source_by_symbol_path
                .get(&node.scope)
                .unwrap_or(&node.location.scope)
                .to_owned();

            let mut normalization_context = TemplateNormalizationContext {
                source_file_scope: &source_file_scope,
                path_format_config,
                project_path_resolver,
                template_const_loop_iteration_limit: self
                    .context
                    .template_const_loop_iteration_limit,
                string_table,
            };
            normalize_ast_node_templates(node, &mut normalization_context)?;
        }

        Ok(())
    }
}

/// Normalizes templates in an AST node by routing to category-specific handlers.
///
/// WHAT: Dispatcher function that routes AST nodes to specialized normalization
/// functions based on node category (control flow, declarations, calls, etc.).
///
/// WHY: Keeps the main normalization logic organized by node category while
/// providing a single entry point for recursive traversal.
fn normalize_ast_node_templates(
    node: &mut AstNode,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    increment_ast_counter(AstCounter::TemplateNormalizationNodesVisited);

    match &mut node.kind {
        NodeKind::If(_, _, _)
        | NodeKind::Match { .. }
        | NodeKind::ScopedBlock { .. }
        | NodeKind::RangeLoop { .. }
        | NodeKind::CollectionLoop { .. }
        | NodeKind::WhileLoop(_, _) => normalize_control_flow_templates(node, context),

        NodeKind::VariableDeclaration(_)
        | NodeKind::Assignment { .. }
        | NodeKind::StructDefinition(_, _) => normalize_declaration_templates(node, context),

        NodeKind::FunctionCall { .. }
        | NodeKind::HostFunctionCall { .. }
        | NodeKind::MethodCall { .. }
        | NodeKind::CollectionBuiltinCall { .. }
        | NodeKind::MapBuiltinCall { .. }
        | NodeKind::HandledFallibleHostFunctionCall { .. }
        | NodeKind::HandledFallibleFunctionCall { .. } => normalize_call_templates(node, context),

        NodeKind::MultiBind { value, .. } | NodeKind::Rvalue(value) => {
            normalize_expression_templates(value, context)
        }

        NodeKind::Function(_, _, body) => normalize_nodes(body, context),

        NodeKind::Return(values) => normalize_expressions(values, context),

        NodeKind::ReturnError(value) => normalize_expression_templates(value, context),

        NodeKind::FieldAccess { base, .. } => normalize_ast_node_templates(base, context),

        // Runtime fragment push — normalize the template expression it carries.
        NodeKind::PushStartRuntimeFragment(expression) => {
            normalize_expression_templates(expression, context)
        }

        NodeKind::Assert { condition, .. } => normalize_expression_templates(condition, context),

        // Terminal nodes (no templates to normalize)
        NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => Ok(()),
        NodeKind::ThenValue(produced_values) => {
            for expression in &mut produced_values.expressions {
                normalize_expression_templates_with_context(
                    expression,
                    context,
                    HelperArtifactPolicy::RejectFinalHelperValue,
                )?;
            }
            Ok(())
        }
    }
}

/// Normalizes templates in control flow nodes (if, match, loops).
///
/// WHAT: Handles normalization for if statements, match expressions, and all
/// loop types (range, collection, while) by recursively normalizing conditions
/// and body statements.
///
/// WHY: Control flow nodes have similar structure (condition + body) and can
/// be handled together to avoid duplication.
fn normalize_control_flow_templates(
    node: &mut AstNode,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    match &mut node.kind {
        NodeKind::If(condition, then_body, else_body) => {
            normalize_expression_templates(condition, context)?;
            normalize_nodes(then_body, context)?;

            if let Some(else_body) = else_body {
                normalize_nodes(else_body, context)?;
            }

            Ok(())
        }

        NodeKind::Match {
            scrutinee,
            arms,
            default,
            exhaustiveness: _,
        } => {
            normalize_expression_templates(scrutinee, context)?;

            for arm in arms {
                match &mut arm.pattern {
                    MatchPattern::Literal(expression)
                    | MatchPattern::OptionValue {
                        value: expression, ..
                    }
                    | MatchPattern::Relational {
                        value: expression, ..
                    } => normalize_expression_templates(expression, context)?,
                    MatchPattern::OptionNone { .. }
                    | MatchPattern::Wildcard { .. }
                    | MatchPattern::ChoiceVariant { .. }
                    | MatchPattern::Capture { .. }
                    | MatchPattern::OptionPresentCapture { .. } => {}
                }

                if let Some(guard) = &mut arm.guard {
                    normalize_expression_templates(guard, context)?;
                }

                normalize_nodes(&mut arm.body, context)?;
            }

            if let Some(default_body) = default {
                normalize_nodes(default_body, context)?;
            }

            Ok(())
        }

        NodeKind::ScopedBlock { body } => normalize_nodes(body, context),

        NodeKind::RangeLoop {
            bindings,
            range,
            body,
        } => {
            normalize_loop_bindings(bindings, context)?;
            normalize_expression_templates(&mut range.start, context)?;
            normalize_expression_templates(&mut range.end, context)?;

            if let Some(step) = &mut range.step {
                normalize_expression_templates(step, context)?;
            }

            normalize_nodes(body, context)
        }

        NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        } => {
            normalize_loop_bindings(bindings, context)?;
            normalize_expression_templates(iterable, context)?;
            normalize_nodes(body, context)
        }

        NodeKind::WhileLoop(condition, body) => {
            normalize_expression_templates(condition, context)?;
            normalize_nodes(body, context)
        }

        _ => unreachable!("normalize_control_flow_templates called with non-control-flow node"),
    }
}

/// Normalizes templates in declaration and assignment nodes.
///
/// WHAT: Handles normalization for variable declarations, assignments, and
/// struct definitions by recursively normalizing value expressions and fields.
///
/// WHY: Declaration nodes have similar structure (identifier + value) and can
/// be handled together to avoid duplication.
fn normalize_declaration_templates(
    node: &mut AstNode,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    match &mut node.kind {
        NodeKind::VariableDeclaration(declaration) => {
            normalize_expression_templates(&mut declaration.value, context)
        }

        NodeKind::Assignment { target, value } => {
            normalize_ast_node_templates(target, context)?;
            normalize_expression_templates(value, context)
        }

        NodeKind::StructDefinition(_, fields) => {
            for field in fields {
                normalize_expression_templates_with_context(
                    &mut field.value,
                    context,
                    HelperArtifactPolicy::AllowNestedHelperContent,
                )?;
            }
            Ok(())
        }

        _ => unreachable!("normalize_declaration_templates called with non-declaration node"),
    }
}

/// Normalizes templates in call-shaped AST nodes.
///
/// WHAT: Handles normalization for function calls, host function calls, method
/// calls, collection builtin calls, and fallible-handled function calls by
/// recursively normalizing arguments and fallible handling.
///
/// WHY: Call nodes have similar structure (receiver/target + arguments) and can
/// be handled together to avoid duplication.
fn normalize_call_templates(
    node: &mut AstNode,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    match &mut node.kind {
        NodeKind::MethodCall { receiver, args, .. }
        | NodeKind::CollectionBuiltinCall { receiver, args, .. }
        | NodeKind::MapBuiltinCall { receiver, args, .. } => {
            normalize_ast_node_templates(receiver, context)?;
            normalize_call_argument_values(args, context)?;
            Ok(())
        }

        NodeKind::FunctionCall { args, .. } | NodeKind::HostFunctionCall { args, .. } => {
            normalize_call_argument_values(args, context)?;
            Ok(())
        }

        NodeKind::HandledFallibleFunctionCall { args, handling, .. }
        | NodeKind::HandledFallibleHostFunctionCall { args, handling, .. } => {
            normalize_call_argument_values(args, context)?;
            normalize_fallible_handling_templates(
                handling,
                context,
                HelperArtifactPolicy::RejectFinalHelperValue,
            )
        }

        _ => unreachable!("normalize_call_templates called with non-call node"),
    }
}

/// Normalizes templates in fallible handling constructs.
///
/// WHAT: Handles normalization for fallible handling by recursively normalizing handler bodies.
///
/// WHY: Fallible handlers can contain templates that must be normalized for HIR.
fn normalize_fallible_handling_templates(
    handling: &mut FallibleHandling,
    context: &mut TemplateNormalizationContext<'_, '_>,
    _helper_artifact_policy: HelperArtifactPolicy,
) -> Result<(), TemplateNormalizationError> {
    match handling {
        FallibleHandling::Handler { body, .. } => normalize_nodes(body, context),
        FallibleHandling::Propagate => Ok(()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HelperArtifactPolicy {
    RejectFinalHelperValue,
    AllowNestedHelperContent,
}

fn normalize_nodes(
    nodes: &mut [AstNode],
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    for node in nodes {
        normalize_ast_node_templates(node, context)?;
    }

    Ok(())
}

fn normalize_expressions(
    expressions: &mut [Expression],
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    for expression in expressions {
        normalize_expression_templates(expression, context)?;
    }

    Ok(())
}

fn normalize_loop_bindings(
    bindings: &mut LoopBindings,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    if let Some(item_binding) = &mut bindings.item {
        normalize_expression_templates(&mut item_binding.value, context)?;
    }

    if let Some(index_binding) = &mut bindings.index {
        normalize_expression_templates(&mut index_binding.value, context)?;
    }

    Ok(())
}

fn normalize_call_argument_values(
    arguments: &mut [CallArgument],
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    for argument in arguments {
        normalize_expression_templates(&mut argument.value, context)?;
    }

    Ok(())
}

pub(super) enum TemplateNormalizationError {
    Diagnostic(CompilerDiagnostic),
    Infrastructure(CompilerError),
}

impl From<CompilerDiagnostic> for TemplateNormalizationError {
    fn from(diagnostic: CompilerDiagnostic) -> Self {
        TemplateNormalizationError::Diagnostic(diagnostic)
    }
}

impl From<CompilerError> for TemplateNormalizationError {
    fn from(error: CompilerError) -> Self {
        TemplateNormalizationError::Infrastructure(error)
    }
}

impl From<TemplateError> for TemplateNormalizationError {
    fn from(error: TemplateError) -> Self {
        match error {
            TemplateError::Diagnostic(diagnostic) => {
                TemplateNormalizationError::Diagnostic(*diagnostic)
            }
            TemplateError::Infrastructure(error) => {
                TemplateNormalizationError::Infrastructure(*error)
            }
        }
    }
}

/// Normalizes templates in expressions.
///
/// WHAT: Recursively normalizes templates embedded in expressions by folding
/// compile-time constants and building render plans for runtime templates.
///
/// WHY: Expressions can contain templates at any level of nesting, so we need
/// to recursively traverse the expression tree to normalize all templates.
fn normalize_expression_templates(
    expression: &mut Expression,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    normalize_expression_templates_with_context(
        expression,
        context,
        HelperArtifactPolicy::RejectFinalHelperValue,
    )?;

    Ok(())
}

fn normalize_expression_templates_with_context(
    expression: &mut Expression,
    context: &mut TemplateNormalizationContext<'_, '_>,
    helper_artifact_policy: HelperArtifactPolicy,
) -> Result<(), TemplateNormalizationError> {
    let folded_template = match &mut expression.kind {
        ExpressionKind::Copy(place) => {
            normalize_ast_node_templates(place, context)?;
            None
        }

        ExpressionKind::Runtime(nodes) => {
            normalize_nodes(nodes, context)?;
            None
        }

        ExpressionKind::Function(_, body) => {
            normalize_nodes(body, context)?;
            None
        }

        ExpressionKind::FunctionCall { args, .. }
        | ExpressionKind::HostFunctionCall { args, .. } => {
            for argument in args {
                normalize_expression_templates_with_context(
                    &mut argument.value,
                    context,
                    helper_artifact_policy,
                )?;
            }
            None
        }

        ExpressionKind::HandledFallibleHostFunctionCall { args, handling, .. }
        | ExpressionKind::HandledFallibleFunctionCall { args, handling, .. } => {
            for argument in args {
                normalize_expression_templates_with_context(
                    &mut argument.value,
                    context,
                    helper_artifact_policy,
                )?;
            }
            normalize_fallible_handling_templates(handling, context, helper_artifact_policy)?;
            None
        }

        ExpressionKind::Collection(args) => {
            for argument in args {
                normalize_expression_templates_with_context(
                    argument,
                    context,
                    helper_artifact_policy,
                )?;
            }
            None
        }

        ExpressionKind::Cast(cast) => {
            normalize_expression_templates_with_context(
                &mut cast.source,
                context,
                helper_artifact_policy,
            )?;
            if let CastHandling::Recover(handling) = &mut cast.handling {
                normalize_fallible_handling_templates(handling, context, helper_artifact_policy)?;
            }
            None
        }

        ExpressionKind::FallibleCarrierConstruct { value, .. }
        | ExpressionKind::OptionPropagation { value }
        | ExpressionKind::Coerced { value, .. } => {
            normalize_expression_templates_with_context(value, context, helper_artifact_policy)?;
            None
        }

        ExpressionKind::HandledFallibleExpression { value, handling } => {
            normalize_expression_templates_with_context(value, context, helper_artifact_policy)?;
            normalize_fallible_handling_templates(handling, context, helper_artifact_policy)?;
            None
        }

        ExpressionKind::Template(template) => {
            normalize_template_for_hir(template, context)?;

            let template_const_kind = template.const_value_kind();

            // Fold only fully renderable final template values.
            // Wrapper-shaped values may still represent runtime templates in this path.
            if matches!(
                template_const_kind,
                TemplateConstValueKind::RenderableString
            ) {
                try_fold_template_to_string(
                    template,
                    context.source_file_scope,
                    context.path_format_config,
                    context.project_path_resolver,
                    context.string_table,
                    context.template_const_loop_iteration_limit,
                )?
            } else {
                // Nested helper-owned contribution structure can be legal inside wrapper
                // templates. Reject only when this expression's final value itself is a
                // standalone helper artifact after composition.
                if helper_artifact_policy == HelperArtifactPolicy::RejectFinalHelperValue
                    && is_illegal_final_template_helper_value(template, template_const_kind)
                {
                    return Err(CompilerDiagnostic::invalid_template_structure(
                        InvalidTemplateStructureReason::HelperOutsideWrapperSlot,
                        template.location.to_owned(),
                    )
                    .into());
                }

                None
            }
        }

        ExpressionKind::StructDefinition(arguments) | ExpressionKind::StructInstance(arguments) => {
            for argument in arguments {
                normalize_expression_templates_with_context(
                    &mut argument.value,
                    context,
                    helper_artifact_policy,
                )?;
            }
            None
        }

        ExpressionKind::Range(lower, upper) => {
            normalize_expression_templates_with_context(lower, context, helper_artifact_policy)?;
            normalize_expression_templates_with_context(upper, context, helper_artifact_policy)?;
            None
        }

        ExpressionKind::ValueBlock { block } => {
            match block.as_mut() {
                ValueBlock::If(value_if) => {
                    normalize_expression_templates_with_context(
                        &mut value_if.condition,
                        context,
                        helper_artifact_policy,
                    )?;
                    normalize_nodes(&mut value_if.then_body, context)?;
                    normalize_nodes(&mut value_if.else_body, context)?;
                }
                ValueBlock::Match(value_match) => {
                    normalize_expression_templates_with_context(
                        &mut value_match.scrutinee,
                        context,
                        helper_artifact_policy,
                    )?;
                    for arm in &mut value_match.arms {
                        if let Some(guard) = &mut arm.guard {
                            normalize_expression_templates_with_context(
                                guard,
                                context,
                                helper_artifact_policy,
                            )?;
                        }
                        normalize_nodes(&mut arm.body, context)?;
                    }
                    if let Some(default_body) = &mut value_match.default {
                        normalize_nodes(default_body, context)?;
                    }
                }
                ValueBlock::Catch(value_catch) => {
                    normalize_expression_templates_with_context(
                        &mut value_catch.handled_value,
                        context,
                        helper_artifact_policy,
                    )?;
                }
            }
            None
        }

        ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Path(_)
        | ExpressionKind::Reference(_) => None,

        ExpressionKind::ChoiceConstruct { fields, .. } => {
            for field in fields {
                normalize_expression_templates(&mut field.value, context)?;
            }
            None
        }
        ExpressionKind::MapLiteral(entries) => {
            for entry in entries {
                normalize_expression_templates_with_context(
                    &mut entry.key,
                    context,
                    helper_artifact_policy,
                )?;
                normalize_expression_templates_with_context(
                    &mut entry.value,
                    context,
                    helper_artifact_policy,
                )?;
            }
            None
        }
    };

    // If we folded a template, replace the expression with a StringSlice
    if let Some(folded_template) = folded_template {
        expression.kind = ExpressionKind::StringSlice(folded_template);
        expression.diagnostic_type = DataType::StringSlice;
        expression.value_mode = ValueMode::ImmutableOwned;
        expression.reactive_template = None;
    } else if let ExpressionKind::Template(template) = &expression.kind {
        expression.reactive_template = template.reactive_template_metadata();
    }

    Ok(())
}

/// Normalizes a template for HIR consumption.
///
/// WHAT: Normalizes child templates in template content, sets content_needs_formatting
/// to false, refreshes the template kind, and builds a render plan.
///
/// WHY:
/// - Runtime templates may contain compile-time child templates after wrapper/head
///   composition. We fold those pieces now so HIR sees finalized chunks.
/// - AST may fold compile-time subtemplates inside a runtime template, but must preserve
///   the enclosing runtime template whenever any runtime chunk remains.
/// - Only escaped helper artifacts are invalid after AST composition.
/// - The render plan is built here so HIR doesn't need to reconstruct it.
fn normalize_template_for_hir(
    template: &mut Template,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    for atom in &mut template.content.atoms {
        let TemplateAtom::Content(segment) = atom else {
            continue;
        };

        // Runtime templates may still contain compile-time child templates after
        // wrapper/head composition. Fold those now so HIR only sees real runtime
        // chunks plus finalized text pieces. Nested helper-owned contribution
        // structure is allowed at this stage for reusable wrapper templates.
        normalize_expression_templates_with_context(
            &mut segment.expression,
            context,
            HelperArtifactPolicy::AllowNestedHelperContent,
        )?;
    }

    // Rebuild final runtime metadata so HIR sees an authoritative post-normalization plan.
    template.resync_runtime_metadata();
    increment_ast_counter(AstCounter::RuntimeRenderPlansRebuilt);
    Ok(())
}

/// Checks whether a template's final value is an illegal helper artifact.
///
/// WHAT: `$insert(...)` helpers and `SlotInsert` template types are only valid
/// during wrapper composition; they must not survive as standalone values.
fn is_illegal_final_template_helper_value(
    template: &Template,
    const_kind: TemplateConstValueKind,
) -> bool {
    matches!(template.kind, TemplateType::SlotInsert(_))
        || matches!(const_kind, TemplateConstValueKind::SlotInsertHelper)
}
