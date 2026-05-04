//! Final AST assembly.
//!
//! WHAT: owns HIR-boundary cleanup and final [`Ast`] construction.
//! WHY: environment building and node emission should finish before template/module-constant
//! normalization mutates or packages the final AST output.

use super::super::build_context::AstPhaseContext;
use super::super::emission::AstEmission;
use super::super::environment::AstModuleEnvironment;
use crate::compiler_frontend::ast::templates::top_level_templates::{
    collect_and_strip_comment_templates, collect_const_top_level_fragments,
};
use crate::compiler_frontend::ast::{Ast, AstChoiceDefinition};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::headers::parse_file_headers::TopLevelConstFragment;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::timer_log;
use std::time::Instant;

pub(in crate::compiler_frontend::ast) struct AstFinalizer<'context, 'services, 'environment> {
    pub(super) context: &'context AstPhaseContext<'services>,
    pub(super) environment: &'environment AstModuleEnvironment,
}

impl<'context, 'services, 'environment> AstFinalizer<'context, 'services, 'environment> {
    pub(in crate::compiler_frontend::ast) fn new(
        context: &'context AstPhaseContext<'services>,
        environment: &'environment AstModuleEnvironment,
    ) -> Self {
        Self {
            context,
            environment,
        }
    }

    /// Assembles the final [`Ast`] from emitted nodes and the resolved environment.
    pub(in crate::compiler_frontend::ast) fn finalize(
        &self,
        mut emitted: AstEmission,
        top_level_const_fragments: &[TopLevelConstFragment],
        string_table: &mut StringTable,
    ) -> Result<Ast, CompilerMessages> {
        let project_path_resolver = self.context.project_path_resolver.as_ref().ok_or_else(|| {
            self.error_messages(
                CompilerError::compiler_error(
                    "AST construction requires a project path resolver for template folding and path coercion.",
                ),
                &emitted.warnings,
                string_table,
            )
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
        )
        .map_err(|error| self.error_messages(error, &emitted.warnings, string_table))?;
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
        //  Normalize AST templates for HIR
        // ----------------------------
        let ast_template_normalization_start = Instant::now();
        self.normalize_ast_templates_for_hir(&mut emitted.ast, project_path_resolver, string_table)
            .map_err(|error| self.error_messages(error, &emitted.warnings, string_table))?;
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
            .map_err(|error| self.error_messages(error, &emitted.warnings, string_table))?;
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
        //  Merge builtin AST nodes
        // ----------------------------
        let builtin_merge_start = Instant::now();
        if !self.environment.builtin_struct_ast_nodes.is_empty() {
            let mut ast_nodes = self.environment.builtin_struct_ast_nodes.clone();
            ast_nodes.extend(emitted.ast);
            emitted.ast = ast_nodes;
        }
        timer_log!(builtin_merge_start, "AST/finalize/builtin AST merge in: ");
        let _ = builtin_merge_start;

        let choice_definitions = self.collect_choice_definitions();

        Ok(Ast {
            nodes: emitted.ast,
            module_constants,
            doc_fragments,
            entry_path: self.context.entry_dir.to_owned(),
            const_top_level_fragments,
            rendered_path_usages: std::mem::take(
                &mut *self.environment.rendered_path_usages.borrow_mut(),
            ),
            warnings: emitted.warnings,
            choice_definitions,
        })
    }

    pub(in crate::compiler_frontend::ast) fn error_messages(
        &self,
        error: CompilerError,
        warnings: &[crate::compiler_frontend::compiler_warnings::CompilerWarning],
        string_table: &StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_error_with_warnings(error, warnings.to_owned(), string_table)
    }

    fn collect_choice_definitions(&self) -> Vec<AstChoiceDefinition> {
        let mut choice_definitions = vec![];
        for declaration in self.environment.declaration_table.iter() {
            if let DataType::Choices {
                nominal_path,
                variants,
                ..
            } = &declaration.value.data_type
            {
                if self
                    .environment
                    .module_symbols
                    .generic_declarations_by_path
                    .contains_key(nominal_path)
                {
                    continue;
                }

                choice_definitions.push(AstChoiceDefinition {
                    nominal_path: nominal_path.to_owned(),
                    variants: variants.to_owned(),
                });
            }
        }

        choice_definitions
    }
}
