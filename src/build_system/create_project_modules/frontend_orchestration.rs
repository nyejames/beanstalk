//! Per-module frontend compilation pipeline for Beanstalk projects.
//!
//! Drives a single discovered module through the full frontend pipeline:
//! tokenization → header parsing → dependency sort → AST → HIR → borrow checking.

use crate::build_system::build::{InputFile, Module, ResolvedConstFragment};
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::ast::ast::Ast;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::headers::parse_file_headers::Headers;
use crate::compiler_frontend::hir::hir_nodes::HirModule;
use crate::compiler_frontend::module_dependencies::SortedHeaders;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::identity::SourceFileTable;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenizeMode};
use crate::compiler_frontend::{CompilerFrontend, FrontendBuildProfile};
use crate::projects::settings::Config;
use crate::{borrow_log, timer_log};
use std::path::Path;
use std::time::Instant;

/// Lifetime-bound context for compiling one module through the full frontend pipeline.
///
/// WHAT: bundles the long-lived inputs shared across tokenization, headers, AST, HIR, and borrow
/// checking for a single module.
/// WHY: bundling these together keeps call sites in the coordinator short and makes the
/// `StringTable` handoff between orchestration and `CompilerFrontend` explicit in one place.
pub(super) struct FrontendModuleBuildContext<'a> {
    pub(super) config: &'a Config,
    pub(super) build_profile: FrontendBuildProfile,
    pub(super) project_path_resolver: Option<ProjectPathResolver>,
    pub(super) style_directives: &'a StyleDirectiveRegistry,
    pub(super) string_table: &'a mut StringTable,
}

impl FrontendModuleBuildContext<'_> {
    /// Compile one discovered module through the full frontend pipeline.
    pub(super) fn compile_module(
        self,
        module: &[InputFile],
        entry_file_path: &Path,
    ) -> Result<Module, CompilerMessages> {
        let mut compiler = CompilerFrontend::new(
            self.config,
            std::mem::take(self.string_table),
            self.style_directives.to_owned(),
            self.project_path_resolver.clone(),
            NewlineMode::NormalizeToLf,
        );

        // Always move the frontend's active string table back to the caller, even on errors.
        // Frontend diagnostics carry interned source paths from later stages, so dropping the
        // evolved table here would leave those path IDs unresolvable at the CLI boundary.
        let compile_result = (|| {
            let mut warnings = Vec::new();
            Self::attach_source_files(&mut compiler, module, entry_file_path)?;

            let project_tokens = timed_frontend_stage("Tokenized in: ", || {
                Self::tokenize_module(&mut compiler, module)
            })?;
            let module_headers = timed_frontend_stage("Headers Parsed in: ", || {
                Self::parse_headers(
                    &mut compiler,
                    project_tokens,
                    &mut warnings,
                    entry_file_path,
                )
            })?;
            let sorted = timed_frontend_stage("Dependency graph created in: ", || {
                Self::sort_headers(&mut compiler, module_headers, &warnings)
            })?;
            let entry_runtime_fragment_count = sorted.entry_runtime_fragment_count;
            let module_ast = timed_frontend_stage("AST created in: ", || {
                self.build_ast(&mut compiler, sorted, entry_file_path, &mut warnings)
            })?;

            // Resolve const fragment StringIds to strings before AST is consumed by HIR.
            let const_top_level_fragments = module_ast
                .const_top_level_fragments
                .iter()
                .map(|fragment| ResolvedConstFragment {
                    runtime_insertion_index: fragment.runtime_insertion_index,
                    html: compiler.string_table.resolve(fragment.value).to_owned(),
                })
                .collect::<Vec<_>>();

            let hir_module = timed_frontend_stage("HIR generated in: ", || {
                Self::lower_hir(&mut compiler, module_ast, &mut warnings)
            })?;
            let borrow_analysis = timed_frontend_stage("Borrow checking completed in: ", || {
                Self::check_borrows(&compiler, &hir_module, &mut warnings)
            })?;

            borrow_log!("=== BORROW CHECKER OUTPUT ===");
            borrow_log!(format!(
                "Borrow checking completed successfully (states={} functions={} blocks={} conflicts_checked={} stmt_facts={} term_facts={} value_facts={})",
                borrow_analysis.analysis.total_state_snapshots(),
                borrow_analysis.stats.functions_analyzed,
                borrow_analysis.stats.blocks_analyzed,
                borrow_analysis.stats.conflicts_checked,
                borrow_analysis.analysis.statement_facts.len(),
                borrow_analysis.analysis.terminator_facts.len(),
                borrow_analysis.analysis.value_facts.len()
            ));
            borrow_log!("=== END BORROW CHECKER OUTPUT ===");

            Ok(Module {
                entry_point: entry_file_path.to_path_buf(),
                hir: hir_module,
                borrow_analysis,
                warnings,
                const_top_level_fragments,
                entry_runtime_fragment_count,
            })
        })();
        *self.string_table = compiler.string_table;
        compile_result
    }

    fn attach_source_files(
        compiler: &mut CompilerFrontend,
        module: &[InputFile],
        entry_file_path: &Path,
    ) -> Result<(), CompilerMessages> {
        let canonical_files = module
            .iter()
            .map(|input_file| input_file.source_path.clone())
            .collect::<Vec<_>>();
        let source_files = SourceFileTable::build(
            &canonical_files,
            entry_file_path,
            compiler.project_path_resolver.as_ref(),
            &mut compiler.string_table,
        )
        .map_err(|error| CompilerMessages::from_error_ref(error, &compiler.string_table))?;
        compiler.set_source_files(source_files);
        Ok(())
    }

    fn tokenize_module(
        compiler: &mut CompilerFrontend,
        module: &[InputFile],
    ) -> Result<Vec<FileTokens>, CompilerMessages> {
        let tokenizer_result = module
            .iter()
            .map(|module| {
                compiler.source_to_tokens(
                    &module.source_code,
                    &module.source_path,
                    TokenizeMode::Normal,
                )
            })
            .collect::<Vec<_>>();

        let mut project_tokens = Vec::with_capacity(tokenizer_result.len());
        let mut errors = Vec::new();
        for file in tokenizer_result {
            match file {
                Ok(tokens) => project_tokens.push(tokens),
                Err(error) => errors.push(error),
            }
        }

        if errors.is_empty() {
            Ok(project_tokens)
        } else {
            Err(CompilerMessages::from_errors_with_warnings(
                errors,
                Vec::new(),
                &compiler.string_table,
            ))
        }
    }

    fn parse_headers(
        compiler: &mut CompilerFrontend,
        project_tokens: Vec<FileTokens>,
        warnings: &mut Vec<CompilerWarning>,
        entry_file_path: &Path,
    ) -> Result<Headers, CompilerMessages> {
        compiler
            .tokens_to_headers(project_tokens, warnings, entry_file_path)
            .map_err(|errors| {
                CompilerMessages::from_errors_with_warnings(
                    errors,
                    warnings.clone(),
                    &compiler.string_table,
                )
            })
    }

    fn sort_headers(
        compiler: &mut CompilerFrontend,
        module_headers: Headers,
        warnings: &[CompilerWarning],
    ) -> Result<SortedHeaders, CompilerMessages> {
        compiler.sort_headers(module_headers).map_err(|errors| {
            CompilerMessages::from_errors_with_warnings(
                errors,
                warnings.to_vec(),
                &compiler.string_table,
            )
        })
    }

    fn build_ast(
        &self,
        compiler: &mut CompilerFrontend,
        sorted: SortedHeaders,
        entry_file_path: &Path,
        warnings: &mut Vec<CompilerWarning>,
    ) -> Result<Ast, CompilerMessages> {
        match compiler.headers_to_ast(sorted, entry_file_path, self.build_profile) {
            Ok(ast) => {
                warnings.extend(ast.warnings.clone());
                Ok(ast)
            }
            Err(messages) => Err(merge_stage_messages(
                messages,
                warnings,
                &compiler.string_table,
            )),
        }
    }

    fn lower_hir(
        compiler: &mut CompilerFrontend,
        module_ast: Ast,
        warnings: &mut Vec<CompilerWarning>,
    ) -> Result<HirModule, CompilerMessages> {
        compiler
            .generate_hir(module_ast)
            .map_err(|messages| merge_stage_messages(messages, warnings, &compiler.string_table))
    }

    fn check_borrows(
        compiler: &CompilerFrontend,
        hir_module: &HirModule,
        warnings: &mut Vec<CompilerWarning>,
    ) -> Result<BorrowCheckReport, CompilerMessages> {
        compiler
            .check_borrows(hir_module)
            .map_err(|messages| merge_stage_messages(messages, warnings, &compiler.string_table))
    }
}

fn merge_stage_messages(
    messages: CompilerMessages,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &StringTable,
) -> CompilerMessages {
    warnings.extend(messages.warnings);
    CompilerMessages::from_errors_with_warnings(messages.errors, warnings.clone(), string_table)
}

fn timed_frontend_stage<T>(
    label: &str,
    stage: impl FnOnce() -> Result<T, CompilerMessages>,
) -> Result<T, CompilerMessages> {
    let start = Instant::now();
    let result = stage();
    timer_log!(start, label);
    let _ = (&start, label);
    result
}
