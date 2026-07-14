//! Final AST assembly.
//!
//! WHAT: owns HIR-boundary cleanup and final [`Ast`] construction.
//! WHY: environment building and node emission should finish before template/module-constant
//! normalization mutates or packages the final AST output.

use super::super::build_context::AstPhaseContext;
use super::super::emission::AstEmission;
use super::super::environment::AstModuleEnvironment;
use super::const_fact_collection::ConstFactCollector;
use super::normalize_ast::TemplateNormalizationError;
use crate::compiler_frontend::ast::templates::top_level_templates::{
    collect_and_strip_comment_templates, collect_const_top_level_fragments,
};
use crate::compiler_frontend::ast::{Ast, AstChoiceDefinition};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::headers::parse_file_headers::TopLevelConstFragment;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use crate::timer_log;
use std::rc::Rc;
use std::time::Instant;

#[cfg(debug_assertions)]
use super::debug_type_validation::debug_validate_type_ids_for_hir;

/// Orchestrates the final AST assembly phase.
///
/// WHAT: consumes the resolved module environment and emitted AST nodes to
/// produce a fully normalized, validated [`Ast`] ready for HIR lowering.
///
/// WHY: separates finalization orchestration from environment building and
/// node emission so each phase has a single, clear responsibility.
pub(in crate::compiler_frontend::ast) struct AstFinalizer<'context, 'services> {
    pub(super) context: &'context AstPhaseContext<'services>,
    pub(super) environment: AstModuleEnvironment,
}

impl<'context, 'services> AstFinalizer<'context, 'services> {
    /// Creates a new finalizer with the given phase context and resolved environment.
    pub(in crate::compiler_frontend::ast) fn new(
        context: &'context AstPhaseContext<'services>,
        environment: AstModuleEnvironment,
    ) -> Self {
        Self {
            context,
            environment,
        }
    }

    /// Assembles the final [`Ast`] from emitted nodes and the resolved environment.
    ///
    /// WHAT: runs template normalization, module-constant normalization, type-boundary
    /// validation, const-fact collection, builtin merging, and choice-definition gathering
    /// in dependency order.
    ///
    /// WHY: each step mutates or consumes intermediate state that later steps depend on,
    /// so they must run sequentially in a single orchestration function.
    pub(in crate::compiler_frontend::ast) fn finalize(
        self,
        mut emitted: AstEmission,
        top_level_const_fragments: &[TopLevelConstFragment],
        string_table: &mut StringTable,
    ) -> Result<Ast, CompilerMessages> {
        // A project path resolver is required for all template folding and path
        // coercion operations that follow. Fail early if it is missing.
        let project_path_resolver = self.context.project_path_resolver.as_ref().ok_or_else(|| {
            let error = CompilerError::compiler_error(
                "AST construction requires a project path resolver for template folding and path coercion.",
            );
            self.error_messages(error, &emitted.warnings, string_table)
        })?;

        // ----------------------------
        //  Collect doc fragments
        // ----------------------------
        let doc_fragments_start = Instant::now();
        let doc_fragments = collect_and_strip_comment_templates(
            &mut emitted.ast,
            project_path_resolver,
            &self.context.path_format_config,
            string_table,
            self.context.template_const_loop_iteration_limit,
            Some(Rc::clone(
                self.context.registered_template_ir_store.registry(),
            )),
        )
        .map_err(TemplateNormalizationError::from)
        .map_err(|error| {
            self.template_normalization_error_messages(error, &emitted.warnings, string_table)
        })?;
        timer_log!(
            doc_fragments_start,
            "AST/finalize/doc fragments collected in: "
        );
        let _ = doc_fragments_start;

        // ----------------------------
        //  Collect const top-level fragments
        // ----------------------------
        let const_fragments_start = Instant::now();
        let const_top_level_fragments = collect_const_top_level_fragments(
            top_level_const_fragments,
            &emitted.const_templates_by_path,
        )
        .map_err(|error| self.error_messages(error, &emitted.warnings, string_table))?;
        timer_log!(
            const_fragments_start,
            "AST/finalize/const top-level fragments collected in: "
        );
        let _ = const_fragments_start;

        // ----------------------------
        //  Propagate reactive template metadata
        // ----------------------------
        let reactive_template_metadata_start = Instant::now();
        self.propagate_reactive_template_metadata(&mut emitted.ast);
        timer_log!(
            reactive_template_metadata_start,
            "AST/finalize/reactive template metadata propagated in: "
        );
        let _ = reactive_template_metadata_start;

        // ----------------------------
        //  Normalize AST templates for HIR
        // ----------------------------
        let ast_template_normalization_start = Instant::now();
        self.normalize_ast_templates_for_hir(&mut emitted.ast, project_path_resolver, string_table)
            .map_err(|error| {
                self.template_normalization_error_messages(error, &emitted.warnings, string_table)
            })?;
        timer_log!(
            ast_template_normalization_start,
            "AST/finalize/AST templates normalized in: "
        );
        let _ = ast_template_normalization_start;

        // ----------------------------
        //  Normalize module constants
        // ----------------------------
        let module_constant_normalization_start = Instant::now();
        let module_constants = self
            .normalize_module_constants_for_hir(project_path_resolver, string_table)
            .map_err(|error| {
                self.template_normalization_error_messages(error, &emitted.warnings, string_table)
            })?;
        timer_log!(
            module_constant_normalization_start,
            "AST/finalize/module constants normalized in: "
        );
        let _ = module_constant_normalization_start;

        // ----------------------------
        //  Validate type boundaries
        // ----------------------------
        let type_boundary_validation_start = Instant::now();
        self.validate_no_unresolved_executable_types(&emitted.ast, &module_constants, string_table)
            .map_err(|error| self.error_messages(error, &emitted.warnings, string_table))?;
        timer_log!(
            type_boundary_validation_start,
            "AST/finalize/type boundary validated in: "
        );
        let _ = type_boundary_validation_start;

        // ----------------------------
        //  Collect const facts
        // ----------------------------
        let const_fact_collection_start = Instant::now();
        let start_function_path = self
            .context
            .entry_dir
            .join_str(IMPLICIT_START_FUNC_NAME, string_table);
        // Const-fact collection reads template values through their exact
        // registry-qualified effective views, including foreign stores and
        // finalization overlays.
        let const_facts = ConstFactCollector::new(
            string_table,
            Rc::clone(self.context.registered_template_ir_store.registry()),
        )
        .collect(&module_constants, &emitted.ast, &start_function_path)
        .map_err(|error| {
            self.template_normalization_error_messages(error, &emitted.warnings, string_table)
        })?;
        timer_log!(
            const_fact_collection_start,
            "AST/finalize/const facts collected in: "
        );
        let _ = const_fact_collection_start;

        // ----------------------------
        //  Merge builtin AST nodes
        // ----------------------------
        let builtin_merge_start = Instant::now();
        if !self.environment.lookups.builtin_struct_ast_nodes.is_empty() {
            let mut ast_nodes = self.environment.lookups.builtin_struct_ast_nodes.clone();
            ast_nodes.extend(emitted.ast);
            emitted.ast = ast_nodes;
        }
        timer_log!(builtin_merge_start, "AST/finalize/builtin AST merge in: ");
        let _ = builtin_merge_start;

        let choice_definitions = self.collect_choice_definitions();

        let AstModuleEnvironment {
            lookups,
            type_environment,
        } = self.environment;

        #[cfg(debug_assertions)]
        {
            // Borrow the module TIR store for the debug TypeId walk. The borrow
            // guard must be dropped before the owned clone below.
            let template_ir_store = self.context.registered_template_ir_store.store().borrow();
            // Borrow the registry as well so debug validation can construct
            // finalized `TirView`s from template references.
            let template_ir_registry = self
                .context
                .registered_template_ir_store
                .registry()
                .borrow();
            debug_validate_type_ids_for_hir(
                &emitted.ast,
                &module_constants,
                &choice_definitions,
                &type_environment,
                &template_ir_store,
                &template_ir_registry,
            );
        }

        Ok(Ast {
            nodes: emitted.ast,
            module_constants,
            doc_fragments,
            entry_path: self.context.entry_dir.to_owned(),
            const_top_level_fragments,
            rendered_path_usages: std::mem::take(&mut *lookups.rendered_path_usages.borrow_mut()),
            warnings: emitted.warnings,
            choice_definitions,
            type_environment,
            const_facts,
        })
    }

    /// Wraps a [`CompilerError`] into [`CompilerMessages`] with the current environment's
    /// type information attached for diagnostic rendering.
    pub(in crate::compiler_frontend::ast) fn error_messages(
        &self,
        error: CompilerError,
        warnings: &[CompilerDiagnostic],
        string_table: &StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_error_with_warnings(error, warnings.to_owned(), string_table)
            .with_type_context_for_all_diagnostics(self.environment.type_environment.clone())
    }

    /// Converts a [`TemplateNormalizationError`] into [`CompilerMessages`], routing
    /// diagnostic and infrastructure errors through their respective constructors.
    fn template_normalization_error_messages(
        &self,
        error: TemplateNormalizationError,
        warnings: &[CompilerDiagnostic],
        string_table: &StringTable,
    ) -> CompilerMessages {
        match error {
            TemplateNormalizationError::Diagnostic(diagnostic) => {
                CompilerMessages::from_diagnostic_with_warnings(
                    *diagnostic,
                    warnings.to_owned(),
                    string_table,
                )
                .with_type_context_for_all_diagnostics(self.environment.type_environment.clone())
            }
            TemplateNormalizationError::Infrastructure(error) => {
                self.error_messages(*error, warnings, string_table)
            }
        }
    }

    /// Collects all non-generic choice definitions from the resolved environment.
    ///
    /// WHAT: iterates the declaration table and extracts choice definitions that
    /// have no generic parameters, so HIR can emit them as concrete nominal types.
    ///
    /// WHY: generic choices are templates, not concrete types, and must not be
    /// emitted as standalone definitions.
    fn collect_choice_definitions(&self) -> Vec<AstChoiceDefinition> {
        let mut choice_definitions = vec![];
        for entry in self.environment.lookups.declaration_table.iter() {
            let type_id = entry.value.type_id;
            let Some(choice_def) = self
                .environment
                .type_environment
                .choice_definition_for(type_id)
            else {
                continue;
            };

            // Skip generic choice declarations (they have type parameters).
            if choice_def.generic_parameters.is_some() {
                continue;
            }

            choice_definitions.push(AstChoiceDefinition {
                nominal_path: choice_def.path.to_owned(),
            });
        }

        choice_definitions
    }
}
