//! Shared template folding helpers for AST finalization.
//!
//! WHAT: Provides common template folding utilities used by both AST node
//! normalization and module constant normalization.
//!
//! WHY: Consolidates duplicated template folding logic to ensure consistent
//! behavior across all normalization contexts.

use crate::compiler_frontend::ast::module_ast::finalization::normalize_ast::TemplateNormalizationError;
use crate::compiler_frontend::ast::templates::template::Template;
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext,
};
use crate::compiler_frontend::ast::templates::tir::{
    PreparedTirViewFold, TemplateIrStore, TemplateTirPhase, TirFoldCache, TirView,
    classify_effective_tir_view_template, fold_tir_view_prepared, prepare_tir_view_fold,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use std::cell::RefCell;
use std::rc::Rc;

/// Folds a compile-time template into a `StringSlice` expression.
///
/// WHAT: Checks if the template is foldable (RenderableString or WrapperTemplate),
/// folds it using `TemplateFoldContext`, and returns a `StringId`.
///
/// WHY: This pattern is repeated in both AST node and module constant
/// normalization. Consolidating it ensures consistent folding behavior.
///
/// The result explicitly records when preparation found a semantic runtime
/// fallback. Callers must route that disposition through the owned handoff
/// materializer instead of inferring it from `TemplateConstValueKind`.
pub(super) fn try_fold_template_to_string(
    template: &Template,
    mut fold_inputs: TemplateFinalizationFoldInputs<'_, '_>,
) -> Result<TemplateFinalizationFoldResult, TemplateNormalizationError> {
    fold_template_view_to_string(template, &mut fold_inputs)
}

/// Folds through the authoritative module-store `TirView`.
///
/// WHAT: validates the template's module-local root and overlay identity, classifies
///       the effective view and folds that exact root when it is const-renderable.
/// WHY: AST finalization always owns the module store. Missing or mismatched
///      identity is an internal invariant failure, not permission to reconstruct
///      template semantics outside the exact view.
fn fold_template_view_to_string(
    template: &Template,
    fold_inputs: &mut TemplateFinalizationFoldInputs<'_, '_>,
) -> Result<TemplateFinalizationFoldResult, TemplateNormalizationError> {
    let reference = &template.tir_reference;

    if !reference.phase.is_at_least(TemplateTirPhase::Composed) {
        return Err(CompilerError::compiler_error(format!(
            "AST finalization template folding requires Composed-or-later TIR, but root {} is at phase {}.",
            reference.root, reference.phase
        ))
        .into());
    }

    let store_handle = Rc::clone(fold_inputs.template_ir_store);

    increment_ast_counter(AstCounter::TirFinalizationFoldAttempts);

    let registry_borrow = store_handle.borrow();
    let view = TirView::with_minimum_phase(
        &registry_borrow,
        reference.root,
        reference.phase,
        TemplateTirPhase::Composed,
        reference.overlay_set_id,
    )?;

    // Complete authority validation and view-native preparation before making
    // any semantic fallback decision. The preparation proof is tied to this
    // exact root, phase, overlay set, and module store.
    increment_ast_counter(AstCounter::TirReadOnlyFoldAttempts);
    let fold_preparation: PreparedTirViewFold = {
        let store_borrow = fold_inputs.template_ir_store.borrow();
        prepare_tir_view_fold(&view, &store_borrow, fold_inputs.string_table)?
    };

    // Classification and folding must observe the same effective overlays.
    // Non-const and helper-only shapes stop here without entering the fold
    // walker. Renderable slots with no contribution remain valid and fold to
    // empty output at their structural position.
    let template_const_kind = {
        let store = fold_inputs.template_ir_store.borrow();
        classify_effective_tir_view_template(&view, &store)?.const_value_kind
    };

    match template_const_kind {
        TemplateConstValueKind::LoopControlSignal
        | TemplateConstValueKind::SlotInsertHelper
        | TemplateConstValueKind::NonConst => {
            increment_ast_counter(AstCounter::TirFinalizationFoldSuccesses);
            return Ok(TemplateFinalizationFoldResult {
                folded: None,
                const_value_kind: template_const_kind,
                disposition: TemplateFinalizationFoldDisposition::NotFoldable,
            });
        }

        TemplateConstValueKind::RenderableString | TemplateConstValueKind::WrapperTemplate => {}
    }

    // --- Prepared fold path ---
    //
    // Attempt to fold without cloning the store. The preparation proof covers
    // the exact structural and overlay shape, rejecting runtime plans,
    // reactive content, and unsupported wrapper/slot paths. A read-only proof
    // additionally guarantees that the fold walker only reads the live store
    // and never pushes synthetic wrapper nodes.
    //
    // The fold context retains the store handle so module-local child
    // references keep their exact overlay identity through the view path.
    let read_only_safe = fold_preparation.read_only_safe();

    if !fold_preparation.fold_eligible() {
        increment_ast_counter(AstCounter::TirReadOnlyFoldFallbacks);
        return Ok(TemplateFinalizationFoldResult {
            folded: None,
            const_value_kind: template_const_kind,
            disposition: TemplateFinalizationFoldDisposition::RuntimeHandoffRequired,
        });
    }

    if read_only_safe {
        let store = fold_inputs.template_ir_store.borrow();

        let mut fold_context = make_fold_context(
            fold_inputs.source_file_scope,
            fold_inputs.path_format_config,
            fold_inputs.project_path_resolver,
            fold_inputs.string_table,
            fold_inputs.template_const_loop_iteration_limit,
            Some(Rc::clone(&store_handle)),
        );

        let result = fold_tir_view_prepared(&view, &store, &mut fold_context, fold_preparation)?;
        let folded = template_emission_to_string_id(result, &mut fold_context)?;
        increment_ast_counter(AstCounter::TemplatesFoldedDuringFinalization);
        increment_ast_counter(AstCounter::TirReadOnlyFoldSuccesses);
        increment_ast_counter(AstCounter::TirFinalizationFoldSuccesses);
        return Ok(TemplateFinalizationFoldResult {
            folded: Some(folded),
            const_value_kind: template_const_kind,
            disposition: TemplateFinalizationFoldDisposition::Folded,
        });
    }

    increment_ast_counter(AstCounter::TirReadOnlyFoldFallbacks);

    let store = fold_inputs.template_ir_store.borrow();
    let mut fold_context = make_fold_context(
        fold_inputs.source_file_scope,
        fold_inputs.path_format_config,
        fold_inputs.project_path_resolver,
        fold_inputs.string_table,
        fold_inputs.template_const_loop_iteration_limit,
        Some(Rc::clone(&store_handle)),
    );
    let result = fold_tir_view_prepared(&view, &store, &mut fold_context, fold_preparation)?;
    let folded = template_emission_to_string_id(result, &mut fold_context)?;
    increment_ast_counter(AstCounter::TemplatesFoldedDuringFinalization);
    increment_ast_counter(AstCounter::TirFinalizationFoldSuccesses);
    Ok(TemplateFinalizationFoldResult {
        folded: Some(folded),
        const_value_kind: template_const_kind,
        disposition: TemplateFinalizationFoldDisposition::Folded,
    })
}

/// Semantic outcome of finalization-time folding preparation.
///
/// WHAT: distinguishes a folded string from a valid runtime template that
///       needs owned HIR handoff materialization and from non-foldable helper
///       or runtime shapes.
/// WHY: `TemplateConstValueKind` describes template shape, while preparation
///      owns the decision about whether the fold proof is semantically usable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TemplateFinalizationFoldDisposition {
    Folded,
    RuntimeHandoffRequired,
    NotFoldable,
}

/// Classification and optional folded output from one authoritative TIR view.
///
/// Returning both facts keeps module-constant finalization from reclassifying
/// the template through the compatibility-content materialization API after a
/// non-foldable result.
pub(super) struct TemplateFinalizationFoldResult {
    pub(super) folded: Option<StringId>,
    pub(super) const_value_kind: TemplateConstValueKind,
    pub(super) disposition: TemplateFinalizationFoldDisposition,
}

fn template_emission_to_string_id(
    emission: TemplateEmission,
    fold_context: &mut TemplateFoldContext<'_>,
) -> Result<StringId, TemplateNormalizationError> {
    match emission {
        TemplateEmission::NoOutput => Ok(fold_context.string_table.intern("")),
        TemplateEmission::Output(output) => Ok(output),
        TemplateEmission::Break(_) | TemplateEmission::Continue(_) => {
            Err(CompilerError::compiler_error(
                "Template loop-control signal escaped the nearest template loop during folding.",
            )
            .into())
        }
    }
}

/// Project-aware inputs for finalization-time template folding.
///
/// WHAT: bundles the stable services and TIR ownership handles needed to build
/// a `TemplateFoldContext`.
/// WHY: finalization folds need the shared module-store handle. Keeping these
/// values together avoids long signatures as TIR authority is passed into the
/// fold path.
pub(super) struct TemplateFinalizationFoldInputs<'a, 'strings> {
    pub(super) source_file_scope: &'a InternedPath,
    pub(super) path_format_config: &'a PathStringFormatConfig,
    pub(super) project_path_resolver: &'a ProjectPathResolver,
    pub(super) string_table: &'strings mut StringTable,
    pub(super) template_const_loop_iteration_limit: usize,
    pub(super) template_ir_store: &'a Rc<RefCell<TemplateIrStore>>,
}

/// Creates a `TemplateFoldContext` from finalization parameters.
///
/// WHAT: bundles project-aware folding services and the module-store authority.
/// WHY: folding receives the exact store at each TIR entry point, so the context
///      no longer carries a duplicate snapshot or borrowed-store access model.
pub(super) fn make_fold_context<'a>(
    source_file_scope: &'a InternedPath,
    path_format_config: &'a PathStringFormatConfig,
    project_path_resolver: &'a ProjectPathResolver,
    string_table: &'a mut StringTable,
    template_const_loop_iteration_limit: usize,
    template_ir_store: Option<Rc<RefCell<TemplateIrStore>>>,
) -> TemplateFoldContext<'a> {
    TemplateFoldContext {
        string_table,
        project_path_resolver,
        path_format_config,
        source_file_scope,
        template_const_loop_iteration_limit,
        template_ir_store,
        bindings: Vec::new(),
        fold_cache: TirFoldCache::new(),
    }
}
