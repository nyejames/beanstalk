//! Compile-time template folding.
//!
//! WHAT: Converts finalized TIR-backed template trees and const control-flow
//! bodies into interned string IDs.
//!
//! WHY: Keeps compile-time folding inside AST template preparation and shares
//! the same finalized template semantics that later runtime handoff consumes,
//! without entangling parser or HIR code.

use crate::compiler_frontend::ast::const_eval::constant_fold;
use crate::compiler_frontend::ast::const_values::resolver::classify_template_from_effective_tir;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::expression_rpn::{
    ExpressionRpn, ExpressionRpnItem,
};
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateFoldBinding, TemplateLoopControlKind,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrRegistry, TemplateTirPhase, TirFoldCache, TirView, fold_tir_view,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::instrumentation::{AstCounter, add_ast_counter};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

// -------------------------
//  Folding Context
// -------------------------

/// Required context for compile-time template folding.
///
/// WHAT: carries all project-aware state that folding can require.
/// WHY: folding must not rely on ad-hoc inherited-style placeholders or
///       resolver-less fallback branches.
pub struct TemplateFoldContext<'a> {
    pub string_table: &'a mut StringTable,
    pub(crate) project_path_resolver: &'a ProjectPathResolver,
    pub path_format_config: &'a PathStringFormatConfig,
    pub source_file_scope: &'a InternedPath,
    pub template_const_loop_iteration_limit: usize,

    /// Module-local TIR registry used to resolve root and child view identity.
    ///
    /// WHAT: provides the registry authority needed to construct precise
    ///       [`TirView`](crate::compiler_frontend::ast::templates::tir::TirView)
    ///       instances for child-template references during recursive folding.
    /// WHY: top-level and child templates carry registry-qualified
    ///      root/phase/overlay identity. `Template::fold_to_emission` requires
    ///      this authority, while low-level store-local walkers may omit it only
    ///      when their caller already owns the exact store and root.
    pub(crate) template_ir_registry: Option<Rc<RefCell<TemplateIrRegistry>>>,

    pub(crate) bindings: Vec<TemplateFoldBinding>,

    /// AST-phase-local cache for TIR fold results.
    ///
    /// WHAT: stores results of folding specific TIR views so repeated folds of
    ///       the same effective view can reuse prior work.
    /// WHY: the cache is tied to one fold context and must not survive beyond it.
    ///      Keeping it on the context avoids global or static state.
    pub(crate) fold_cache: TirFoldCache,
}

/// Compile-time template folding must keep structural no-output distinct from
/// output that happens to be an empty string, because parent wrappers apply only
/// to structurally emitted children.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TemplateEmission {
    NoOutput,
    Output(StringId),
    Break(Option<StringId>),
    Continue(Option<StringId>),
}

/// Borrow-first expression resolution result for template folding.
///
/// WHAT: distinguishes expressions that were not modified during fold-binding
///       resolution (borrowed reference to the original) from expressions that
///       were actually rewritten (owned).
/// WHY: most template expressions pass through folding unchanged because they
///      contain no foldable bindings. Returning a borrowed reference avoids
///      cloning the entire expression tree on the common no-substitution path,
///      which is the majority of expressions in template-heavy modules.
pub(crate) enum FoldResolvedExpression<'a> {
    /// The expression was not changed; fold sites can use the original.
    Borrowed(&'a Expression),
    /// The expression was actually rewritten; this is the owned result.
    Owned(Box<Expression>),
}

impl FoldResolvedExpression<'_> {
    /// Consumes the resolved expression and returns an owned `Expression`.
    ///
    /// WHAT: clones only when the resolved expression is borrowed (no substitution
    ///       happened), so callers that genuinely need an owned value still work.
    /// WHY: a few call sites (like RPN operand vectors) need owned values, but
    ///      this method makes the clone explicit and only happens when the
    ///      borrow-first path determined a rewrite is required.
    pub(crate) fn into_owned(self) -> Expression {
        match self {
            FoldResolvedExpression::Borrowed(expr) => expr.clone(),
            FoldResolvedExpression::Owned(expr) => *expr,
        }
    }
}

impl TemplateFoldContext<'_> {
    fn lookup_binding(&self, path: &InternedPath) -> Option<&Expression> {
        self.bindings
            .iter()
            .rev()
            .find(|binding| &binding.path == path)
            .map(|binding| &binding.value)
    }

    pub(crate) fn push_bindings(
        &mut self,
        bindings: impl IntoIterator<Item = TemplateFoldBinding>,
    ) -> usize {
        let previous_len = self.bindings.len();
        self.bindings.extend(bindings);
        previous_len
    }

    pub(crate) fn restore_bindings(&mut self, previous_len: usize) {
        self.bindings.truncate(previous_len);
    }
}

// -------------------------
//  Folding Implementation
// -------------------------

impl Template {
    /// Folds a fully-resolved template into an interned string ID.
    ///
    /// WHAT: folds the template's authoritative registry-backed TIR view through
    /// the TIR-native folder.
    /// WHY: compile-time folding should consume the same TIR shape that runtime
    /// handoff materialization uses.
    pub(crate) fn fold_into_stringid(
        &self,
        fold_context: &mut TemplateFoldContext<'_>,
    ) -> Result<StringId, TemplateError> {
        // Keep resolver/path/scope in the fold contract even when a specific template
        // only needs string interning today. Callers must propagate full project context.
        let _required_project_context = (
            fold_context.project_path_resolver,
            fold_context.path_format_config,
            fold_context.source_file_scope,
        );

        match self.fold_to_emission(fold_context)? {
            TemplateEmission::NoOutput => {
                let empty_id = fold_context.string_table.intern("");
                record_fold_output_intern(0);
                Ok(empty_id)
            }
            TemplateEmission::Output(output) => Ok(output),
            TemplateEmission::Break(_) | TemplateEmission::Continue(_) => Err(
                CompilerError::compiler_error(
                    "Template loop-control signal escaped the nearest template loop during folding.",
                )
                .into(),
            ),
        }
    }

    /// Folds a fully-resolved template into a `TemplateEmission`.
    ///
    /// WHAT: resolves the template's authoritative registry-backed TIR view and
    ///       folds it against the store that owns the root.
    /// WHY: compile-time folding should consume the same final TIR authority as
    ///      finalization and HIR-handoff paths.
    pub(crate) fn fold_to_emission(
        &self,
        fold_context: &mut TemplateFoldContext<'_>,
    ) -> Result<TemplateEmission, TemplateError> {
        fold_to_emission_from_view(self, fold_context)
    }
}

/// Folds a template through its stable registry-backed `TirView`.
///
/// WHAT: when the template owns a `Composed`-or-later TIR reference (same-store
///       or foreign), resolve its owning store through the module registry and
///       fold the view directly through `fold_tir_view`.
/// WHY: compile-time folding should consume the same final TIR authority as
///      finalization and HIR-handoff paths whenever registry identity is
///      available. A missing reference, registry entry, owner match, overlay or
///      minimum phase is an AST invariant failure rather than permission to
///      reconstruct the template from compatibility content.
fn fold_to_emission_from_view(
    template: &Template,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError> {
    let reference = &template.tir_reference;

    let registry_rc = fold_context
        .template_ir_registry
        .as_ref()
        .map(Rc::clone)
        .ok_or_else(|| {
            CompilerError::compiler_error(
                "Template folding requires the module-local TIR registry.",
            )
        })?;

    let registry_borrow = registry_rc.borrow();
    let store_handle = registry_borrow
        .store_handle(reference.root.store_id)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "Template folding store {} is not registered.",
                reference.root.store_id
            ))
        })?;
    let store = store_handle.borrow();

    if !Arc::ptr_eq(&store.owner(), &reference.store_owner) {
        return Err(CompilerError::compiler_error(format!(
            "Template folding root {} does not belong to its registered store.",
            reference.root
        ))
        .into());
    }

    let view = TirView::with_minimum_phase(
        &registry_borrow,
        reference.root,
        reference.phase,
        TemplateTirPhase::Composed,
        reference.overlay_set_id,
    )?;

    fold_tir_view(&view, &store, fold_context)
}

pub(crate) fn selected_option_capture_payload(
    scrutinee: &Expression,
    pattern: &MatchPattern,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<Option<TemplateFoldBinding>, TemplateError> {
    match const_option_presence(scrutinee, fold_context)? {
        ConstOptionPresence::Present(value) => Ok(Some(TemplateFoldBinding {
            path: option_capture_binding_path(pattern)?,
            value: *value,
        })),

        ConstOptionPresence::Absent => Ok(None),
    }
}

enum ConstOptionPresence {
    Present(Box<Expression>),
    Absent,
}

fn const_option_presence(
    scrutinee: &Expression,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<ConstOptionPresence, TemplateError> {
    let resolved = resolve_fold_bindings_in_expression(scrutinee, fold_context)?;

    // Work with the resolved expression by reference to avoid an extra clone
    // when the resolver returned a borrowed reference (no binding was substituted).
    let resolved_ref: &Expression = match &resolved {
        FoldResolvedExpression::Borrowed(expr) => expr,
        FoldResolvedExpression::Owned(expr) => expr,
    };

    match &resolved_ref.kind {
        ExpressionKind::OptionNone => Ok(ConstOptionPresence::Absent),

        ExpressionKind::Coerced { value, .. } => {
            let payload = (**value).clone();
            let template_ir_registry = fold_context.template_ir_registry.as_ref();
            let string_table = &*fold_context.string_table;

            // Scalar and other non-template payloads keep their ordinary const rules.
            // Registry authority is required only when expression recursion reaches a
            // nested template, whether that template belongs to the active or a foreign store.
            let payload_is_compile_time_constant = payload
                .const_value_kind_with_template_classifier(&mut |template| {
                    let registry = template_ir_registry.ok_or_else(|| {
                        CompilerError::compiler_error(
                            "Template option-capture folding requires the module-local TIR registry.",
                        )
                    })?;

                    classify_template_from_effective_tir(template, registry, string_table)
                })?
                .is_compile_time_value();

            if payload_is_compile_time_constant {
                Ok(ConstOptionPresence::Present(Box::new(payload)))
            } else {
                Err(option_capture_const_deferred_error(resolved_ref).into())
            }
        }

        _ => Err(option_capture_const_deferred_error(resolved_ref).into()),
    }
}

fn option_capture_binding_path(pattern: &MatchPattern) -> Result<InternedPath, TemplateError> {
    let MatchPattern::OptionPresentCapture { binding_path, .. } = pattern else {
        return Err(CompilerError::compiler_error(
            "Template option-capture folding received a non-capture pattern.",
        )
        .into());
    };

    Ok(binding_path.clone())
}

fn option_capture_const_deferred_error(expression: &Expression) -> CompilerDiagnostic {
    CompilerDiagnostic::invalid_template_structure(
        InvalidTemplateStructureReason::TemplateOptionCaptureConstDeferred,
        expression.location.clone(),
    )
}

pub(crate) fn fold_conditional_loop_const_condition(
    condition: &Expression,
    location: &SourceLocation,
) -> Result<bool, TemplateError> {
    match &condition.kind {
        ExpressionKind::Bool(value) => Ok(*value),

        ExpressionKind::Coerced { value, .. } => {
            fold_conditional_loop_const_condition(value, location)
        }

        _ => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopConditionNotConst,
            condition_location_or_loop_location(condition, location),
        )
        .into()),
    }
}

pub(crate) fn condition_location_or_loop_location(
    condition: &Expression,
    loop_location: &SourceLocation,
) -> SourceLocation {
    if condition.location == Default::default() {
        loop_location.clone()
    } else {
        condition.location.clone()
    }
}

pub(crate) fn loop_body_not_const_error(
    error: TemplateError,
    diagnostic_location: &SourceLocation,
) -> TemplateError {
    match error {
        TemplateError::Diagnostic(diagnostic) => TemplateError::Diagnostic(diagnostic),
        TemplateError::Infrastructure(_) => CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateLoopBodyNotConst,
            diagnostic_location.clone(),
        )
        .into(),
    }
}

pub(crate) fn fold_bool_condition(
    condition: &Expression,
    fallback_location: &SourceLocation,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<bool, TemplateError> {
    let resolved = resolve_fold_bindings_in_expression(condition, fold_context)?;

    // Borrow the resolved expression by reference to avoid cloning when no
    // binding was substituted (the common path for const template conditions).
    let resolved_ref: &Expression = match &resolved {
        FoldResolvedExpression::Borrowed(expr) => expr,
        FoldResolvedExpression::Owned(expr) => expr,
    };

    fold_resolved_bool_condition(resolved_ref, fallback_location)
}

fn fold_resolved_bool_condition(
    condition: &Expression,
    fallback_location: &SourceLocation,
) -> Result<bool, TemplateError> {
    match &condition.kind {
        ExpressionKind::Bool(value) => Ok(*value),
        ExpressionKind::Coerced { value, .. } => {
            fold_resolved_bool_condition(value, fallback_location)
        }
        _ => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::TemplateIfConditionNotConst,
            if condition.location == Default::default() {
                fallback_location.clone()
            } else {
                condition.location.clone()
            },
        )
        .into()),
    }
}

pub(crate) fn template_emission_from_output_and_signal(
    output: StringId,
    signal_kind: Option<TemplateLoopControlKind>,
) -> TemplateEmission {
    match signal_kind {
        None => TemplateEmission::Output(output),
        Some(TemplateLoopControlKind::Break) => TemplateEmission::Break(Some(output)),
        Some(TemplateLoopControlKind::Continue) => TemplateEmission::Continue(Some(output)),
    }
}

/// Resolves fold bindings in an expression using a borrow-first strategy.
///
/// WHAT: examines an expression and returns either a borrowed reference to the
///       original (when no substitution was needed) or an owned rewritten expression.
/// WHY: most template expressions contain no foldable bindings. Cloning the
///      entire expression tree on every fold call is wasted work when the common
///      path simply passes the expression through unchanged. The borrow-first
///      approach avoids allocation on the no-substitution path entirely.
pub(crate) fn resolve_fold_bindings_in_expression<'a>(
    expression: &'a Expression,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<FoldResolvedExpression<'a>, TemplateError> {
    match &expression.kind {
        ExpressionKind::Reference(path) => {
            if let Some(bound_value) = fold_context.lookup_binding(path) {
                // Binding found: produce an owned clone of the bound value.
                // This is the actual substitution that justifies an allocation.
                add_ast_counter(AstCounter::TemplateFoldBindingSubstitutions, 1);
                add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
                add_ast_counter(AstCounter::TemplateFoldExpressionOwnedRewrites, 1);
                Ok(FoldResolvedExpression::Owned(Box::new(bound_value.clone())))
            } else {
                // No binding: borrow the original expression unchanged.
                Ok(FoldResolvedExpression::Borrowed(expression))
            }
        }

        ExpressionKind::Coerced { value, to_type } => {
            let resolved = resolve_fold_bindings_in_expression(value, fold_context)?;

            // If the inner value was not substituted, the coerced wrapper is
            // also unchanged — borrow the original expression.
            if matches!(resolved, FoldResolvedExpression::Borrowed(_)) {
                // A coercion wrapper around a template expression is transparent
                // for template string rendering: the nested template is rendered
                // as string content. Returning the inner template directly lets
                // downstream fold paths (including the parser-TIR-backed route)
                // handle it as a nested template rather than failing on the
                // Coerced wrapper.
                if matches!(value.kind, ExpressionKind::Template(_)) {
                    return Ok(FoldResolvedExpression::Borrowed(value));
                }
                return Ok(FoldResolvedExpression::Borrowed(expression));
            }

            // Inner value was rewritten: rebuild the coerced wrapper with the
            // resolved inner value. Only allocate because the inner actually changed.
            let resolved_owned = resolved.into_owned();
            add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
            add_ast_counter(AstCounter::TemplateFoldExpressionOwnedRewrites, 1);
            Ok(FoldResolvedExpression::Owned(Box::new(Expression {
                kind: ExpressionKind::Coerced {
                    value: Box::new(resolved_owned),
                    to_type: *to_type,
                },
                ..expression.clone()
            })))
        }

        ExpressionKind::Runtime(rpn) => {
            fold_runtime_expression_with_bindings(expression, rpn, fold_context)
        }

        // All other expression kinds have no foldable bindings — borrow unchanged.
        _ => Ok(FoldResolvedExpression::Borrowed(expression)),
    }
}

/// Resolves fold bindings in a runtime RPN expression.
///
/// WHAT: substitutes foldable bindings inside RPN operand expressions and
///       attempts constant folding on the substituted result. Returns a borrowed
///       reference when no operand was substituted and folding did not produce
///       a new value.
/// WHY: RPN expressions in const template loops are the other main allocation
///      hot spot. When all operands are non-binding references or literals,
///      the expression passes through unchanged and should not be cloned.
fn fold_runtime_expression_with_bindings<'a>(
    expression: &'a Expression,
    rpn: &ExpressionRpn,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<FoldResolvedExpression<'a>, TemplateError> {
    let mut substituted = Vec::with_capacity(rpn.items.len());
    let mut any_substituted = false;

    for item in &rpn.items {
        let new_item = match item {
            ExpressionRpnItem::Operand(value) => {
                let resolved = resolve_fold_bindings_in_expression(value, fold_context)?;
                match resolved {
                    FoldResolvedExpression::Borrowed(_) => {
                        // Operand unchanged — push the original clone (operator
                        // nodes need owned items in the substituted Vec).
                        item.clone()
                    }
                    FoldResolvedExpression::Owned(owned) => {
                        any_substituted = true;
                        add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
                        ExpressionRpnItem::Operand(*owned)
                    }
                }
            }
            ExpressionRpnItem::Operator { .. } => item.clone(),
        };
        substituted.push(new_item);
    }

    // No operand was substituted and constant folding has nothing new to
    // evaluate — borrow the original expression unchanged.
    if !any_substituted {
        return Ok(FoldResolvedExpression::Borrowed(expression));
    }

    // At least one operand was substituted; attempt constant folding on the
    // updated RPN to see if the expression can be simplified further.
    match constant_fold(&substituted, fold_context.string_table) {
        Ok(stack) => {
            if stack.len() == 1
                && let ExpressionRpnItem::Operand(folded) = &stack[0]
            {
                add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
                add_ast_counter(AstCounter::TemplateFoldExpressionOwnedRewrites, 1);
                return Ok(FoldResolvedExpression::Owned(Box::new(folded.to_owned())));
            }
            // Folding did not simplify to a single value; build a new Runtime
            // expression from the substituted RPN.
            add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
            add_ast_counter(AstCounter::TemplateFoldExpressionOwnedRewrites, 1);
            Ok(FoldResolvedExpression::Owned(Box::new(Expression {
                kind: ExpressionKind::Runtime(ExpressionRpn { items: substituted }),
                ..expression.clone()
            })))
        }

        Err(_) => {
            // Constant folding failed; build a new Runtime expression from the
            // substituted RPN so downstream sees the substituted operands.
            add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 1);
            add_ast_counter(AstCounter::TemplateFoldExpressionOwnedRewrites, 1);
            Ok(FoldResolvedExpression::Owned(Box::new(Expression {
                kind: ExpressionKind::Runtime(ExpressionRpn { items: substituted }),
                ..expression.clone()
            })))
        }
    }
}

fn record_fold_output_intern(byte_len: usize) {
    add_ast_counter(AstCounter::TemplateFoldStringInternCalls, 1);
    add_ast_counter(AstCounter::TemplateFoldOutputBytes, byte_len);
}
