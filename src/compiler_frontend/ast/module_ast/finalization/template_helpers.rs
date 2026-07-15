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
    TemplateIrRegistry, TemplateIrStore, TemplateTirPhase, TirFoldCache, TirView,
    classify_effective_tir_view_template, fold_tir_view, fold_tir_view_read_only,
    tir_view_is_read_only_fold_safe,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// Folds a compile-time template into a `StringSlice` expression.
///
/// WHAT: Checks if the template is foldable (RenderableString or WrapperTemplate),
/// folds it using `TemplateFoldContext`, and returns a `StringId`.
///
/// WHY: This pattern is repeated in both AST node and module constant
/// normalization. Consolidating it ensures consistent folding behavior.
///
/// Returns `None` if the template is not foldable (NonConst or SlotInsertHelper).
pub(super) fn try_fold_template_to_string(
    template: &Template,
    mut fold_inputs: TemplateFinalizationFoldInputs<'_, '_>,
) -> Result<TemplateFinalizationFoldResult, TemplateNormalizationError> {
    fold_registry_backed_template_to_string(template, &mut fold_inputs)
}

/// Folds through the authoritative registry-backed `TirView`.
///
/// WHAT: validates the module store identity carried by the template, classifies
///       the effective view and folds that exact root when it is const-renderable.
/// WHY: AST finalization always owns the module registry and store. Missing or
///      mismatched identity is an internal invariant failure, not permission to
///      rebuild template semantics from compatibility content.
fn fold_registry_backed_template_to_string(
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

    let registry = Rc::clone(&fold_inputs.template_ir_registry);

    let store_owner = fold_inputs.template_ir_store.borrow().owner();
    if !Arc::ptr_eq(&reference.store_owner, &store_owner) {
        return Err(CompilerError::compiler_error(format!(
            "AST finalization template root {} does not belong to the module TIR store.",
            reference.root
        ))
        .into());
    }

    {
        let store_borrow = fold_inputs.template_ir_store.borrow();
        if reference.root.store_id != store_borrow.store_id() {
            return Err(CompilerError::compiler_error(format!(
                "AST finalization template root {} does not match module TIR store {}.",
                reference.root,
                store_borrow.store_id()
            ))
            .into());
        }
    }

    increment_ast_counter(AstCounter::TirRegistryBackedFoldAttempts);

    let registry_borrow = registry.borrow();
    let view = TirView::with_minimum_phase(
        &registry_borrow,
        reference.root,
        reference.phase,
        TemplateTirPhase::Composed,
        reference.overlay_set_id,
    )?;

    // Classification and folding must observe the same effective overlays.
    // Non-const and helper-only shapes stop here without entering the fold
    // walker. Renderable slots with no contribution remain valid and fold to
    // empty output at their structural position.
    let template_const_kind = {
        let store = fold_inputs.template_ir_store.borrow();
        classify_effective_tir_view_template(&view, &store, fold_inputs.string_table)?
            .const_value_kind
    };

    match template_const_kind {
        TemplateConstValueKind::LoopControlSignal
        | TemplateConstValueKind::SlotInsertHelper
        | TemplateConstValueKind::NonConst => {
            increment_ast_counter(AstCounter::TirRegistryBackedFoldSuccesses);
            return Ok(TemplateFinalizationFoldResult {
                folded: None,
                const_value_kind: template_const_kind,
            });
        }

        TemplateConstValueKind::RenderableString | TemplateConstValueKind::WrapperTemplate => {}
    }

    // --- Read-only fold path (Phase 3A) ---
    //
    // Attempt to fold without cloning the store. The safety check verifies
    // the structural tree can be folded read-only: no conditional child
    // wrappers, no overlays, no reactive subscriptions, no runtime slot
    // plans. When the check passes, the fold walker only reads structural
    // nodes and never pushes synthetic wrapper nodes, so the live module
    // store can be borrowed directly instead of cloned.
    //
    // The fold context retains the registry so store-qualified child references
    // keep their exact overlay identity and can cross into registered stores.
    increment_ast_counter(AstCounter::TirReadOnlyFoldAttempts);
    let read_only_safe = {
        let store_borrow = fold_inputs.template_ir_store.borrow();
        tir_view_is_read_only_fold_safe(&view, &store_borrow)?
    };

    if read_only_safe {
        let store = fold_inputs.template_ir_store.borrow();

        let mut fold_context = make_fold_context(
            fold_inputs.source_file_scope,
            fold_inputs.path_format_config,
            fold_inputs.project_path_resolver,
            fold_inputs.string_table,
            fold_inputs.template_const_loop_iteration_limit,
            Some(Rc::clone(&registry)),
        );

        let result = fold_tir_view_read_only(&view, &store, &mut fold_context)?;
        let folded = template_emission_to_string_id(result, &mut fold_context)?;
        increment_ast_counter(AstCounter::TemplatesFoldedDuringFinalization);
        increment_ast_counter(AstCounter::TirReadOnlyFoldSuccesses);
        increment_ast_counter(AstCounter::TirRegistryBackedFoldSuccesses);
        return Ok(TemplateFinalizationFoldResult {
            folded: Some(folded),
            const_value_kind: template_const_kind,
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
        Some(Rc::clone(&registry)),
    );
    let result = fold_tir_view(&view, &store, &mut fold_context)?;
    let folded = template_emission_to_string_id(result, &mut fold_context)?;
    increment_ast_counter(AstCounter::TemplatesFoldedDuringFinalization);
    increment_ast_counter(AstCounter::TirRegistryBackedFoldSuccesses);
    Ok(TemplateFinalizationFoldResult {
        folded: Some(folded),
        const_value_kind: template_const_kind,
    })
}

/// Classification and optional folded output from one authoritative TIR view.
///
/// Returning both facts keeps module-constant finalization from reclassifying
/// the template through the compatibility-content materialization API after a
/// non-foldable result.
pub(super) struct TemplateFinalizationFoldResult {
    pub(super) folded: Option<StringId>,
    pub(super) const_value_kind: TemplateConstValueKind,
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
/// WHY: finalization folds need both store access and optional registry access.
/// Keeping these values together avoids long signatures as more TIR authority
/// is wired into the fold path.
pub(super) struct TemplateFinalizationFoldInputs<'a, 'strings> {
    pub(super) source_file_scope: &'a InternedPath,
    pub(super) path_format_config: &'a PathStringFormatConfig,
    pub(super) project_path_resolver: &'a ProjectPathResolver,
    pub(super) string_table: &'strings mut StringTable,
    pub(super) template_const_loop_iteration_limit: usize,
    pub(super) template_ir_store: &'a Rc<RefCell<TemplateIrStore>>,
    pub(super) template_ir_registry: Rc<RefCell<TemplateIrRegistry>>,
}

/// Creates a `TemplateFoldContext` from finalization parameters.
///
/// WHAT: bundles project-aware folding services and optional registry authority.
/// WHY: folding receives the exact store at each TIR entry point, so the context
///      no longer carries a duplicate snapshot or borrowed-store access model.
pub(super) fn make_fold_context<'a>(
    source_file_scope: &'a InternedPath,
    path_format_config: &'a PathStringFormatConfig,
    project_path_resolver: &'a ProjectPathResolver,
    string_table: &'a mut StringTable,
    template_const_loop_iteration_limit: usize,
    template_ir_registry: Option<Rc<RefCell<TemplateIrRegistry>>>,
) -> TemplateFoldContext<'a> {
    TemplateFoldContext {
        string_table,
        project_path_resolver,
        path_format_config,
        source_file_scope,
        template_const_loop_iteration_limit,
        template_ir_registry,
        bindings: Vec::new(),
        fold_cache: TirFoldCache::new(),
    }
}
