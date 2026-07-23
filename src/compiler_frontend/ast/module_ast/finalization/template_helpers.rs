//! Shared template folding helpers for AST finalization.
//!
//! WHAT: Provides common template folding utilities used by both AST node
//! normalization and module constant normalization.
//!
//! WHY: Consolidates duplicated template folding logic to ensure consistent
//! behavior across all normalization contexts.

use crate::compiler_frontend::ast::module_ast::finalization::normalize_ast::TemplateNormalizationError;
use crate::compiler_frontend::ast::templates::template::Template;
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext,
};
use crate::compiler_frontend::ast::templates::tir::{
    PreparedRuntime, PreparedTemplate, TemplateHelperKind, TemplateIrStore,
    TemplatePreparationMode, TemplateTirPhase, TirFoldCache, TirView, fold_prepared_template,
    prepare_tir_view,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::synthetic_interface_provenance::SyntheticInterfaceProvenance;
use std::cell::RefCell;
use std::rc::Rc;

/// Exclusive finalization result for one prepared template value.
///
/// WHAT: pairs exactly one semantic outcome with the data needed by its owner.
/// WHY: a folded value, runtime proof and helper artifact must never be represented
///      as independent optional/disposition fields that can contradict each other.
pub(super) enum FinalizedTemplateValue {
    Folded(StringId, SyntheticInterfaceProvenance),
    Runtime(PreparedRuntime),
    Helper(TemplateHelperKind),
}

/// Prepares and finalizes one exact template value for its owning boundary.
pub(super) fn finalize_template_value(
    template: &Template,
    fold_inputs: TemplateValueFinalizationInputs<'_, '_>,
    preparation_mode: TemplatePreparationMode,
) -> Result<FinalizedTemplateValue, TemplateNormalizationError> {
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

    let store = store_handle.borrow();
    let view = TirView::with_minimum_phase(
        &store,
        reference.root,
        reference.phase,
        TemplateTirPhase::Composed,
        reference.context,
    )?;

    // Preparation validates and classifies the exact view before cache lookup
    // or folding. Its compact result is the sole final-value decision source.
    let preparation = prepare_tir_view(&view, preparation_mode)?;
    let fold_preparation = match preparation {
        PreparedTemplate::Helper(kind) => {
            increment_ast_counter(AstCounter::TirFinalizationFoldSuccesses);
            return Ok(FinalizedTemplateValue::Helper(kind));
        }
        PreparedTemplate::Runtime(prepared) => {
            return Ok(FinalizedTemplateValue::Runtime(prepared));
        }
        PreparedTemplate::Foldable(prepared) => prepared,
    };

    let mut fold_context = make_fold_context(
        fold_inputs.source_file_scope,
        fold_inputs.path_format_config,
        fold_inputs.project_path_resolver,
        fold_inputs.string_table,
        fold_inputs.template_const_loop_iteration_limit,
    );
    let result = fold_prepared_template(&fold_preparation, view, &mut fold_context)?;
    let provenance = result.provenance;
    let folded = template_emission_to_string_id(result.emission, &mut fold_context)?;
    increment_ast_counter(AstCounter::TemplatesFoldedDuringFinalization);
    increment_ast_counter(AstCounter::TirFinalizationFoldSuccesses);
    Ok(FinalizedTemplateValue::Folded(folded, provenance))
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
/// WHAT: bundles the stable services and TIR ownership handle needed to build
/// the exact finalization view and run one fold operation.
/// WHY: finalization owns the module-store handle while the active `TirView`
/// carries structural authority through preparation and folding.
pub(super) struct TemplateValueFinalizationInputs<'a, 'strings> {
    pub(super) source_file_scope: &'a InternedPath,
    pub(super) path_format_config: &'a PathStringFormatConfig,
    pub(super) project_path_resolver: &'a ProjectPathResolver,
    pub(super) string_table: &'strings mut StringTable,
    pub(super) template_const_loop_iteration_limit: usize,
    pub(super) template_ir_store: &'a Rc<RefCell<TemplateIrStore>>,
}

/// Creates a `TemplateFoldContext` from finalization parameters.
///
/// WHAT: bundles project-aware folding services for one fold operation.
/// WHY: TIR structural authority comes from the exact view at each fold entry
///      point rather than from a duplicate context field.
pub(super) fn make_fold_context<'a>(
    source_file_scope: &'a InternedPath,
    path_format_config: &'a PathStringFormatConfig,
    project_path_resolver: &'a ProjectPathResolver,
    string_table: &'a mut StringTable,
    template_const_loop_iteration_limit: usize,
) -> TemplateFoldContext<'a> {
    TemplateFoldContext {
        string_table,
        project_path_resolver,
        path_format_config,
        source_file_scope,
        template_const_loop_iteration_limit,
        bindings: Vec::new(),
        fold_cache: TirFoldCache::new(),
    }
}
