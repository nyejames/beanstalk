//! AST node template normalization for HIR preparation.
//!
//! WHAT: Recursively traverses AST nodes to normalize embedded templates by
//! folding compile-time constants, materializing runtime handoffs, and
//! completing template metadata. Mutates AST nodes in place to prepare them for
//! HIR.
//!
//! WHY: HIR assumes templates are semantically complete with folded constants,
//! no escaped helper artifacts, and owned runtime handoff shapes for runtime
//! templates. This normalization satisfies that AST→HIR boundary contract
//! before lowering.
//!
//! ## Normalization Strategy
//!
//! 1. **Constant Folding**: Templates with `RenderableString` const value kinds
//!    are folded into `StringSlice` expressions immediately.
//!
//! 2. **Runtime Handoff Construction**: Runtime templates receive owned runtime
//!    handoffs so HIR does not need to reconstruct template structure.
//!
//! 3. **Metadata Completion**: All templates have their kind refreshed from
//!    their final effective TIR view.
//!
//! 4. **Helper Rejection**: escaped `$insert(...)` helper templates are rejected
//!    if they reach finalization outside immediate wrapper-slot composition.
//!
//! ## AST→HIR Template Boundary
//!
//! AST owns:
//! - Template foldability decisions
//! - Constant template lowering
//! - Runtime template handoff materialization
//!
//! HIR receives:
//! - Folded constant templates as `StringSlice` expressions
//! - Runtime templates with owned runtime handoffs
//! - No escaped helper artifacts (`TemplateType::SlotInsert`)
//! - No templates requiring formatting

use super::finalizer::AstFinalizer;
use super::template_helpers::{
    TemplateFinalizationFoldDisposition, TemplateFinalizationFoldInputs, make_fold_context,
    try_fold_template_to_string,
};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, LoopBindings, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, FallibleHandling, ReactiveTemplateMetadata,
};
use crate::compiler_frontend::ast::expressions::expression_rpn::{
    ExpressionRpnItem, PlaceExpression, PlaceExpressionKind,
};
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::statements::value_production::types::ValueBlock;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::reactive_template_metadata;
use crate::compiler_frontend::ast::templates::runtime_handoff;
use crate::compiler_frontend::ast::templates::runtime_handoff::{
    OwnedRuntimeSlotApplicationHandoff, OwnedRuntimeTemplateHandoff, OwnedRuntimeTemplateNode,
};
use crate::compiler_frontend::ast::templates::template::Template;
use crate::compiler_frontend::ast::templates::template::{TemplateConstValueKind, TemplateType};
use crate::compiler_frontend::ast::templates::tir::{
    ExpressionSiteId, TemplateIrStore, TemplateTirPhase, TemplateTirReference, TemplateViewContext,
    TirExpressionOverlay, TirTemplateClassification, TirView, classify_effective_tir_view_template,
    collect_effective_tir_expression_overlay_payloads, finalized_tir_view_for_template,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateSlotReason, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::value_mode::ValueMode;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

struct TemplateNormalizationContext<'a, 'strings> {
    source_file_scope: &'a InternedPath,
    path_format_config: &'a PathStringFormatConfig,
    project_path_resolver: &'a ProjectPathResolver,
    template_const_loop_iteration_limit: usize,
    string_table: &'strings mut StringTable,
    template_ir_store: Rc<RefCell<TemplateIrStore>>,
}

impl AstFinalizer<'_, '_> {
    /// Normalizes all templates in the AST for HIR consumption.
    ///
    /// WHAT: Traverses all AST nodes and normalizes embedded templates by
    /// folding constants and materializing runtime handoffs.
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
                template_ir_store: Rc::clone(&self.context.template_ir_store),
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

        NodeKind::MultiBind { value, .. } | NodeKind::ExpressionStatement(value) => {
            normalize_expression_templates(value, context)
        }

        NodeKind::Function(_, _, body) => normalize_nodes(body, context),

        NodeKind::Return(values) => normalize_expressions(values, context),

        NodeKind::ReturnError(value) => normalize_expression_templates(value, context),

        // Runtime fragment push — normalize the template expression it carries.
        NodeKind::PushStartRuntimeFragment(expression) => {
            normalize_expression_templates(expression, context)
        }

        NodeKind::Assert { condition, .. } => normalize_expression_templates(condition, context),

        // Terminal nodes (no templates to normalize)
        NodeKind::Break | NodeKind::Continue => Ok(()),
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

        NodeKind::Assignment { value, .. } => normalize_expression_templates(value, context),

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

#[derive(Debug)]
pub(super) enum TemplateNormalizationError {
    Diagnostic(Box<CompilerDiagnostic>),
    Infrastructure(Box<CompilerError>),
}

impl From<CompilerDiagnostic> for TemplateNormalizationError {
    fn from(diagnostic: CompilerDiagnostic) -> Self {
        TemplateNormalizationError::Diagnostic(Box::new(diagnostic))
    }
}

impl From<CompilerError> for TemplateNormalizationError {
    fn from(error: CompilerError) -> Self {
        TemplateNormalizationError::Infrastructure(Box::new(error))
    }
}

impl From<TemplateError> for TemplateNormalizationError {
    fn from(error: TemplateError) -> Self {
        match error {
            TemplateError::Diagnostic(diagnostic) => {
                TemplateNormalizationError::Diagnostic(diagnostic)
            }
            TemplateError::Infrastructure(error) => {
                TemplateNormalizationError::Infrastructure(error)
            }
        }
    }
}

/// Normalizes templates in expressions.
///
/// WHAT: Recursively normalizes templates embedded in expressions by folding
/// compile-time constants and materializing runtime handoffs where needed.
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

fn normalize_place_expression_templates(
    place: &mut PlaceExpression,
) -> Result<(), TemplateNormalizationError> {
    match &mut place.kind {
        PlaceExpressionKind::Local(_) => Ok(()),
        PlaceExpressionKind::Field { base, .. } => normalize_place_expression_templates(base),
    }
}

fn normalize_expression_templates_with_context(
    expression: &mut Expression,
    context: &mut TemplateNormalizationContext<'_, '_>,
    helper_artifact_policy: HelperArtifactPolicy,
) -> Result<(), TemplateNormalizationError> {
    let template_replacement = match &mut expression.kind {
        ExpressionKind::Copy(place) => {
            normalize_place_expression_templates(place)?;
            None
        }

        ExpressionKind::Runtime(rpn) => {
            for item in &mut rpn.items {
                match item {
                    ExpressionRpnItem::Operand(expression) => {
                        normalize_expression_templates_with_context(
                            expression,
                            context,
                            helper_artifact_policy,
                        )?;
                    }
                    ExpressionRpnItem::Operator { .. } => {}
                }
            }
            None
        }

        ExpressionKind::FieldAccess { base, .. } => {
            normalize_expression_templates_with_context(base, context, helper_artifact_policy)?;
            None
        }

        ExpressionKind::MethodCall { receiver, args, .. }
        | ExpressionKind::CollectionBuiltinCall { receiver, args, .. }
        | ExpressionKind::MapBuiltinCall { receiver, args, .. } => {
            normalize_expression_templates_with_context(receiver, context, helper_artifact_policy)?;
            normalize_call_argument_values(args, context)?;
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

        ExpressionKind::HandledFallibleHostFunctionCall { args, .. }
        | ExpressionKind::HandledFallibleFunctionCall { args, .. } => {
            for argument in args {
                normalize_expression_templates_with_context(
                    &mut argument.value,
                    context,
                    helper_artifact_policy,
                )?;
            }
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
            None
        }

        #[cfg(test)]
        ExpressionKind::FallibleCarrierConstruct { value, .. } => {
            normalize_expression_templates_with_context(value, context, helper_artifact_policy)?;
            None
        }

        ExpressionKind::OptionPropagation { value } | ExpressionKind::Coerced { value, .. } => {
            normalize_expression_templates_with_context(value, context, helper_artifact_policy)?;
            None
        }

        ExpressionKind::HandledFallibleExpression { value, .. } => {
            normalize_expression_templates_with_context(value, context, helper_artifact_policy)?;
            None
        }

        ExpressionKind::Template(template) => {
            normalize_template_for_hir(template, context)?;

            let final_classification = classify_final_effective_template_view(template, context)?;
            let template_const_kind = final_classification.const_value_kind;

            // Fold renderable values through the preparation owner. A renderable
            // shape can still require runtime lowering when its prepared proof
            // finds a runtime slot plan, so that disposition must reach the
            // existing owned handoff materializer.
            if matches!(
                template_const_kind,
                TemplateConstValueKind::RenderableString
            ) {
                let fold_result = try_fold_template_to_string(
                    template,
                    TemplateFinalizationFoldInputs {
                        source_file_scope: context.source_file_scope,
                        path_format_config: context.path_format_config,
                        project_path_resolver: context.project_path_resolver,
                        string_table: context.string_table,
                        template_const_loop_iteration_limit: context
                            .template_const_loop_iteration_limit,
                        template_ir_store: &context.template_ir_store,
                    },
                )?;

                match fold_result.disposition {
                    TemplateFinalizationFoldDisposition::Folded => {
                        let folded = fold_result.folded.ok_or_else(|| {
                            CompilerError::compiler_error(
                                "Renderable template folding completed without folded output.",
                            )
                        })?;
                        Some(NormalizedTemplateExpression::Folded(folded))
                    }

                    TemplateFinalizationFoldDisposition::RuntimeHandoffRequired => {
                        materialize_runtime_template_handoff_for_hir(
                            template,
                            context,
                            &final_classification,
                            reactive_template_metadata_from_current_store(template, context)?,
                        )?
                    }

                    TemplateFinalizationFoldDisposition::NotFoldable => None,
                }
            } else {
                // Nested helper-owned contribution structure can be legal inside wrapper
                // templates. Reject only when this expression's final value itself is a
                // standalone helper artifact after composition.
                if helper_artifact_policy == HelperArtifactPolicy::RejectFinalHelperValue
                    && is_illegal_final_template_helper_value(
                        effective_template_kind(template, context)?,
                        template_const_kind,
                    )
                {
                    return Err(CompilerDiagnostic::invalid_template_structure(
                        InvalidTemplateStructureReason::HelperOutsideWrapperSlot,
                        template.location.to_owned(),
                    )
                    .into());
                }

                materialize_runtime_template_handoff_for_hir(
                    template,
                    context,
                    &final_classification,
                    reactive_template_metadata_from_current_store(template, context)?,
                )?
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
                    normalize_fallible_handling_templates(
                        &mut value_catch.handler,
                        context,
                        helper_artifact_policy,
                    )?;
                }
            }
            None
        }

        ExpressionKind::RuntimeTemplateHandoff(handoff) => {
            normalize_runtime_template_handoff_for_hir(handoff, context)?;
            increment_ast_counter(AstCounter::RuntimeTemplateHandoffsRefreshedForHir);
            None
        }

        ExpressionKind::RuntimeSlotApplicationHandoff(handoff) => {
            normalize_runtime_slot_handoff_for_hir(handoff, context)?;
            increment_ast_counter(AstCounter::RuntimeTemplateHandoffsRefreshedForHir);
            None
        }

        ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Function(_)
        | ExpressionKind::Reference(_) => None,

        #[cfg(test)]
        ExpressionKind::Path(_) => None,

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

    match template_replacement {
        Some(NormalizedTemplateExpression::Folded(folded_template)) => {
            expression.kind = ExpressionKind::StringSlice(folded_template);
            expression.diagnostic_type = DataType::StringSlice;
            expression.value_mode = ValueMode::ImmutableOwned;
            expression.reactive_template = None;
        }

        Some(NormalizedTemplateExpression::RuntimeSlotApplication(handoff, reactive_template)) => {
            let value_mode = expression.value_mode.clone();
            *expression = Expression::runtime_slot_application_handoff(handoff, value_mode);
            expression.reactive_template = reactive_template;
        }

        Some(NormalizedTemplateExpression::RuntimeTemplate(handoff, reactive_template)) => {
            let value_mode = expression.value_mode.clone();
            *expression = Expression::runtime_template_handoff(handoff, value_mode);
            expression.reactive_template = reactive_template;
        }

        None => {
            if let ExpressionKind::Template(template) = &expression.kind {
                expression.reactive_template =
                    reactive_template_metadata_from_current_store(template, context)?;
            }
        }
    }

    Ok(())
}

enum NormalizedTemplateExpression {
    Folded(StringId),
    RuntimeTemplate(
        OwnedRuntimeTemplateHandoff,
        Option<ReactiveTemplateMetadata>,
    ),
    RuntimeSlotApplication(
        OwnedRuntimeSlotApplicationHandoff,
        Option<ReactiveTemplateMetadata>,
    ),
}

fn reactive_template_metadata_from_current_store(
    template: &Template,
    context: &TemplateNormalizationContext<'_, '_>,
) -> Result<Option<ReactiveTemplateMetadata>, CompilerError> {
    // Normalization has the module store, so it should refresh metadata from
    // the same finalized TIR roots that HIR handoff materialization consumes.
    // Use the final effective `TirView` so expression overlays are honored.
    let store = context.template_ir_store.borrow();
    reactive_template_metadata_from_store(template, &store)
}

fn reactive_template_metadata_from_store(
    template: &Template,
    store: &TemplateIrStore,
) -> Result<Option<ReactiveTemplateMetadata>, CompilerError> {
    let mut metadata = ReactiveTemplateMetadata::template_backed();
    reactive_template_metadata::merge_reactive_template_metadata_with_store(
        template,
        store,
        &mut metadata,
        &mut |expression| expression_reactive_template_metadata_from_store(expression, store),
    )?;

    Ok(Some(metadata))
}

/// Classifies the finalized effective `TirView` of `template`.
///
/// WHAT: requires the module-owned finalized root and classifies its effective
///       expression, slot-resolution and wrapper-context overlays against the
///       live module store.
/// WHY: finalization must make fold and runtime-handoff decisions from the same
///      authoritative TIR identity. Reconstructing compatibility content here
///      would ignore overlay semantics immediately before the AST/HIR boundary.
fn classify_final_effective_template_view(
    template: &mut Template,
    context: &TemplateNormalizationContext<'_, '_>,
) -> Result<TirTemplateClassification, TemplateNormalizationError> {
    let reference = template.tir_reference;

    if !reference.phase.is_at_least(TemplateTirPhase::Finalized) {
        return Err(CompilerError::compiler_error(format!(
            "Template HIR normalization requires Finalized TIR, but root {} is at phase {}.",
            reference.root, reference.phase
        ))
        .into());
    }

    let initial_classification = {
        let store = context.template_ir_store.borrow();
        let view = TirView::with_minimum_phase(
            &store,
            reference.root,
            reference.phase,
            TemplateTirPhase::Finalized,
            reference.context,
        )?;
        classify_effective_tir_view_template(&view, &store)?
    };

    let mut store = context.template_ir_store.borrow_mut();

    // The authoritative kind lives in `TemplateIr.kind`. The first classification
    // may refresh the generic String/StringFunction classification; the single
    // synchronization owner writes both `TemplateIr.kind` and the durable
    // `Template.kind` cache so they cannot drift.
    template
        .synchronize_kind_from_classification(&mut store, &initial_classification)
        .map_err(TemplateNormalizationError::from)?;

    drop(store);
    let store = context.template_ir_store.borrow();
    let view = TirView::with_minimum_phase(
        &store,
        reference.root,
        reference.phase,
        TemplateTirPhase::Finalized,
        reference.context,
    )?;
    classify_effective_tir_view_template(&view, &store).map_err(Into::into)
}

fn expression_reactive_template_metadata_from_store(
    expression: &Expression,
    store: &TemplateIrStore,
) -> Result<Option<ReactiveTemplateMetadata>, CompilerError> {
    if let Some(metadata) = &expression.reactive_template {
        return Ok(Some(metadata.clone()));
    }

    if let ExpressionKind::Template(template) = &expression.kind {
        return reactive_template_metadata_from_store(template, store);
    }

    Ok(None)
}

/// Normalizes a template for HIR consumption.
///
/// WHAT: normalizes every expression payload reachable from the template's root
///       TIR reference, including control-flow selectors and loop headers.
///
/// WHY:
/// - Runtime templates may contain compile-time child templates after wrapper/head
///   composition. We fold those pieces now so HIR sees finalized chunks.
/// - AST may fold compile-time subtemplates inside a runtime template, but must preserve
///   the enclosing runtime template whenever any runtime chunk remains.
/// - Only escaped helper artifacts are invalid after AST composition.
/// - The enclosing expression replacement builds the owned runtime handoff from
///   the normalized template so HIR receives a neutral payload without depending
///   on AST template internals.
fn normalize_template_for_hir(
    template: &mut Template,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    normalize_expression_overlays_for_template_reference(template, context)?;

    Ok(())
}

fn normalize_expression_overlays_for_template_reference(
    template: &mut Template,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    // Keep normalized payloads in the shared view context consumed
    // by the finalized effective view and runtime handoff materializer. This
    // preserves shared TIR nodes while covering dynamic expressions, selectors,
    // loop headers, and every reachable control-flow body from one root pass.
    let reference = template.tir_reference;
    // Same-store is now the only path: every TIR reference is local to this
    // module store, so expression-overlay payloads are always collected. Phase
    // promotion to Finalized is gated separately below, so parsed references
    // can receive normalized overlays without becoming finalized views.
    let should_mark_finalized = reference.phase.is_at_least(TemplateTirPhase::Composed);
    let expression_payloads = collect_expression_overlay_payloads(&reference, context)?;
    if expression_payloads.is_empty() {
        if should_mark_finalized {
            template.tir_reference.phase = TemplateTirPhase::Finalized;
        }
        return Ok(());
    }

    let mut normalized_overrides = Vec::with_capacity(expression_payloads.len());
    for (site_id, mut expression) in expression_payloads {
        normalize_expression_templates_with_context(
            &mut expression,
            context,
            HelperArtifactPolicy::AllowNestedHelperContent,
        )?;
        normalized_overrides.push((site_id, Box::new(expression)));
    }

    let mut store = context.template_ir_store.borrow_mut();
    let normalized_site_ids = normalized_overrides
        .iter()
        .map(|(site_id, _)| *site_id)
        .collect::<HashSet<_>>();

    let mut overrides = if let Some(existing_overlay_id) = reference.context.expression_overlay {
        let existing_overlay = store
            .expression_overlay(existing_overlay_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "expression overlay normalization referenced missing expression overlay {}",
                    existing_overlay_id
                ))
            })?;
        existing_overlay
            .overrides
            .iter()
            .filter(|(site_id, _)| !normalized_site_ids.contains(site_id))
            .map(|(site_id, expression)| (*site_id, expression.clone()))
            .collect()
    } else {
        Vec::new()
    };
    overrides.extend(normalized_overrides);

    let expression_overlay_id =
        store.allocate_expression_overlay(TirExpressionOverlay { overrides });
    let expression_context = TemplateViewContext {
        expression_overlay: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    };

    template.tir_reference.context = reference.context.merge(expression_context);
    if should_mark_finalized {
        template.tir_reference.phase = TemplateTirPhase::Finalized;
    }

    Ok(())
}

fn collect_expression_overlay_payloads(
    reference: &TemplateTirReference,
    context: &TemplateNormalizationContext<'_, '_>,
) -> Result<Vec<(ExpressionSiteId, Expression)>, TemplateNormalizationError> {
    let store = context.template_ir_store.borrow();
    let expression_payloads = collect_effective_tir_expression_overlay_payloads(
        &store,
        reference.root,
        reference.context,
    )?;

    Ok(expression_payloads)
}

fn normalize_runtime_slot_template_expression_for_hir(
    expression: &mut Expression,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    normalize_expression_templates_with_context(
        expression,
        context,
        HelperArtifactPolicy::AllowNestedHelperContent,
    )
}

/// Materializes the neutral AST-to-HIR payload from the final effective TIR view.
///
/// WHAT: consumes the classification already derived from the store-backed
///       final view, rejects escaped insert helpers and then builds either the
///       specialized slot-application handoff or the general owned runtime
///       handoff from that same view.
/// WHY: normalization has already finalized the module-owned TIR reference.
///      Rebuilding a fresh tree here could discard effective overlays and
///      revive stale compatibility data at the AST/HIR boundary.
fn materialize_runtime_template_handoff_for_hir(
    template: &Template,
    context: &mut TemplateNormalizationContext<'_, '_>,
    classification: &TirTemplateClassification,
    reactive_template: Option<ReactiveTemplateMetadata>,
) -> Result<Option<NormalizedTemplateExpression>, TemplateNormalizationError> {
    let store_handle = Rc::clone(&context.template_ir_store);
    let store = store_handle.borrow();
    let view = finalized_tir_view_for_template(template, &store)?;

    // Const-foldable templates and helper artifacts are lowered by AST folding,
    // not by the HIR runtime-template path.
    if matches!(
        classification.const_value_kind,
        TemplateConstValueKind::LoopControlSignal | TemplateConstValueKind::SlotInsertHelper
    ) {
        return Ok(None);
    }

    // Slot placeholders that survived composition are now represented in the
    // owned handoff as no-output structural nodes. Escaped `$insert(...)`
    // helpers are invalid outside wrapper-slot composition and must be rejected
    // at the AST/HIR boundary instead of reaching HIR as ordinary content.
    if classification.has_slot_insertions {
        return Err(CompilerDiagnostic::invalid_template_slot(
            InvalidTemplateSlotReason::InsertOutsideParentSlot,
            None,
            template.location.to_owned(),
        )
        .into());
    }

    if let Some(handoff) = store.owned_runtime_slot_handoff_for_tir_view(&view)? {
        increment_ast_counter(AstCounter::RuntimeTemplateHandoffsMaterialized);
        return Ok(Some(NormalizedTemplateExpression::RuntimeSlotApplication(
            handoff,
            reactive_template,
        )));
    }

    let handoff = {
        let mut fold_context = make_fold_context(
            context.source_file_scope,
            context.path_format_config,
            context.project_path_resolver,
            context.string_table,
            context.template_const_loop_iteration_limit,
            Some(Rc::clone(&context.template_ir_store)),
        );
        store.owned_runtime_template_handoff_for_tir_view_with_fold_context(
            &view,
            &mut fold_context,
        )?
    };

    increment_ast_counter(AstCounter::RuntimeTemplateHandoffsMaterialized);
    Ok(Some(NormalizedTemplateExpression::RuntimeTemplate(
        handoff,
        reactive_template,
    )))
}

fn normalize_runtime_slot_handoff_for_hir(
    handoff: &mut OwnedRuntimeSlotApplicationHandoff,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    runtime_handoff::walk_owned_runtime_slot_application_handoff_mut(handoff, &mut |event| {
        normalize_owned_runtime_template_handoff_event_for_hir(event, context)
    })
}

fn normalize_runtime_template_handoff_for_hir(
    handoff: &mut OwnedRuntimeTemplateHandoff,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    runtime_handoff::walk_owned_runtime_template_handoff_mut(handoff, &mut |event| {
        normalize_owned_runtime_template_handoff_event_for_hir(event, context)
    })
}

fn normalize_owned_runtime_template_handoff_event_for_hir(
    event: runtime_handoff::OwnedRuntimeTemplateWalkMutEvent<'_>,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    match event {
        runtime_handoff::OwnedRuntimeTemplateWalkMutEvent::Node(node) => {
            normalize_owned_runtime_template_node_for_hir(node, context)?;
        }

        runtime_handoff::OwnedRuntimeTemplateWalkMutEvent::HandoffAfterBody(_handoff) => {
            // `Style` no longer carries recursive wrapper templates, so there is
            // nothing to normalize at the handoff boundary. Nested child templates
            // are visited through `OwnedRuntimeTemplateNode::ChildTemplate` nodes.
        }
    }

    Ok(())
}

fn normalize_owned_runtime_template_node_for_hir(
    node: &mut OwnedRuntimeTemplateNode,
    context: &mut TemplateNormalizationContext<'_, '_>,
) -> Result<(), TemplateNormalizationError> {
    match node {
        OwnedRuntimeTemplateNode::DynamicExpression { expression, .. } => {
            normalize_runtime_slot_template_expression_for_hir(expression, context)?;
        }

        OwnedRuntimeTemplateNode::Sequence { .. }
        | OwnedRuntimeTemplateNode::ChildTemplate { .. }
        | OwnedRuntimeTemplateNode::ConditionalWrapper { .. }
        | OwnedRuntimeTemplateNode::BranchChain { .. }
        | OwnedRuntimeTemplateNode::Loop { .. }
        | OwnedRuntimeTemplateNode::Text { .. }
        | OwnedRuntimeTemplateNode::AggregateOutput
        | OwnedRuntimeTemplateNode::LoopControl { .. }
        | OwnedRuntimeTemplateNode::RuntimeSlotSite { .. }
        | OwnedRuntimeTemplateNode::Slot { .. } => {}
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/normalize_ast_tests.rs"]
mod normalize_ast_tests;

/// Checks whether a template's final value is an illegal helper artifact.
///
/// WHAT: `$insert(...)` helpers and `SlotInsert` template types are only valid
/// during wrapper composition; they must not survive as standalone values.
fn is_illegal_final_template_helper_value(
    template_kind: TemplateType,
    const_kind: TemplateConstValueKind,
) -> bool {
    matches!(template_kind, TemplateType::SlotInsert(_))
        || matches!(const_kind, TemplateConstValueKind::SlotInsertHelper)
}

/// Reads the authoritative template kind from the owning TIR store entry.
///
/// WHAT: resolves the template's TIR reference through the module store and
///       returns `TemplateIr.kind`.
/// WHY: `TemplateIr.kind` is the sole post-construction kind owner.
fn effective_template_kind(
    template: &Template,
    context: &TemplateNormalizationContext<'_, '_>,
) -> Result<TemplateType, TemplateNormalizationError> {
    let store = context.template_ir_store.borrow();
    template.tir_kind_from_store(&store).ok_or_else(|| {
        CompilerError::compiler_error(
            "AST finalization template kind was not found in the module TIR store.",
        )
        .into()
    })
}
