//! AST stage — Stage 4 of the frontend pipeline.
//!
//! WHAT: constructs a typed AST from pre-sorted, pre-shaped top-level headers produced by
//! Stages 2–3 (header parsing and dependency sorting).
//!
//! WHY: separating executable-body parsing from header discovery keeps the pipeline
//! deterministic and lets AST focus on semantic resolution, type checking, and body
//! lowering without rediscovering top-level symbols.
//!
//! ## Ownership contract
//!
//! **Header parsing + dependency sorting own:**
//! - top-level declaration discovery
//! - top-level declaration shell parsing
//! - strict top-level dependency ordering
//! - appending implicit `start` header last (outside dependency graph)
//!
//! **AST owns:**
//! - lowering sorted header payloads (no top-level shell reparsing)
//! - resolving and validating top-level types/symbols
//! - parsing function bodies and other executable/body-local declarations
//! - template composition, compile-time folding, and runtime render-plan preparation
//!
//! ## Pipeline
//!
//! 1. `build_ast_environment` — build imports, declarations, constants, signatures, and receiver metadata
//! 2. `emit_ast_nodes` — lower function/start/template bodies into typed AST nodes
//! 3. `finalize_ast` — normalize templates/constants and assemble [`Ast`] output
//!
//! Entry point: [`Ast::new`].

// Internal AST implementation modules.
//
// `module_ast` contains the environment, emission, finalization, and scope-context helpers that
// implement the AST pipeline. The rest of the AST surface is split by concern
// (expressions, statements, templates, field access, etc.).
pub(crate) mod ast_nodes;
mod import_bindings;
pub(crate) mod instrumentation;
mod module_ast;
mod receiver_methods;
mod type_resolution;

pub(crate) mod expressions {
    pub(crate) mod call_argument;
    pub(crate) mod call_validation;
    pub(crate) mod choice_constructor;
    pub(crate) mod eval_expression;
    pub(crate) mod expression;
    pub(crate) mod function_calls;
    pub(crate) mod generic_nominal_inference;
    pub(crate) mod mutation;
    pub(crate) mod parse_expression;
    pub(crate) mod parse_expression_dispatch;
    pub(crate) mod parse_expression_identifiers;
    pub(crate) mod parse_expression_literals;
    pub(crate) mod parse_expression_places;
    pub(crate) mod parse_expression_templates;
    pub(crate) mod struct_instance;
}

pub(crate) mod statements {
    pub(crate) mod body_dispatch;
    pub(crate) mod body_expr_stmt;
    pub(crate) mod body_return;
    pub(crate) mod body_symbol;
    pub(crate) mod branching;
    pub(crate) mod collections;
    pub(crate) mod condition_validation;
    pub(crate) mod declarations;
    pub(crate) mod functions;
    pub(crate) mod loops;
    pub(crate) mod match_patterns;
    pub(crate) mod multi_bind;
    pub(crate) mod result_handling;
    pub(crate) mod scoped_blocks;
}

mod field_access;
mod place_access;
pub(crate) mod templates;

// Public surface — minimal API consumed by later compiler stages.
//
// WHY: the AST module should expose one obvious entry surface. Internal helpers,
// pass implementations, and parser submodules stay private to `ast/`.
pub use module_ast::build_context::AstBuildContext;
pub(crate) use module_ast::environment::TopLevelDeclarationTable;
pub use module_ast::scope_context::{ContextKind, ScopeContext};
pub use templates::top_level_templates::AstDocFragment;
pub use templates::top_level_templates::AstDocFragmentKind;

// Imports for the AST entry point and body-parsing helper.
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::instrumentation::{log_ast_counters, reset_ast_counters};
use crate::compiler_frontend::ast::module_ast::build_context::AstPhaseContext;
use crate::compiler_frontend::ast::module_ast::emission::AstEmitter;
use crate::compiler_frontend::ast::module_ast::environment::AstModuleEnvironmentBuilder;
use crate::compiler_frontend::ast::module_ast::finalization::AstFinalizer;
use crate::compiler_frontend::ast::statements::body_dispatch::parse_function_body_statements;
use crate::compiler_frontend::ast::templates::top_level_templates::AstConstTopLevelFragment;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::headers::module_symbols::ModuleSymbols;
use crate::compiler_frontend::headers::parse_file_headers::{Header, TopLevelConstFragment};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::timer_log;
use std::time::Instant;

/// Unified AST output for all source files in one compilation unit.
///
/// WHAT: the fully resolved, typed AST produced by Stage 4, ready for HIR lowering.
/// WHY: one container keeps the pipeline contract explicit — HIR receives exactly this
/// struct and nothing else from the AST stage.
/// Resolved choice definition carried from AST to HIR for pre-registration.
///
/// WHAT: pairs a choice's canonical path with its fully resolved variants.
/// WHY: HIR needs to register all choices before expression lowering so
///      `lower_data_type` can resolve `ChoiceId` via lookup instead of lazy creation.
#[derive(Clone, Debug)]
pub struct AstChoiceDefinition {
    pub nominal_path: InternedPath,
    pub variants: Vec<crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant>,
}

pub struct Ast {
    pub nodes: Vec<AstNode>,
    pub module_constants: Vec<crate::compiler_frontend::ast::ast_nodes::Declaration>,
    pub doc_fragments: Vec<AstDocFragment>,

    /// The path to the original entry point file.
    pub entry_path: InternedPath,

    /// Const top-level fragments with their runtime insertion indices.
    /// Builders merge these with the runtime fragment list returned by entry `start()`.
    pub const_top_level_fragments: Vec<AstConstTopLevelFragment>,
    pub rendered_path_usages: Vec<RenderedPathUsage>,
    pub warnings: Vec<CompilerWarning>,

    /// Resolved choice definitions for HIR pre-registration.
    ///
    /// WHAT: every choice declaration (local and imported) with fully resolved payload field types.
    /// WHY: HIR registers choices deterministically before function lowering.
    pub choice_definitions: Vec<AstChoiceDefinition>,
}

impl Ast {
    /// Constructs a complete typed AST from sorted headers and a pre-built symbol manifest.
    ///
    /// WHAT: Orchestrates all AST construction passes in sequence, consuming the manifest
    /// through finalization, then assembles the final [`Ast`] output.
    ///
    /// WHY: Centralizes the pass sequence so the full compilation pipeline is readable in
    /// one place without implementation details. Symbol discovery is owned by the header/
    /// dependency stages and passed in via `module_symbols`.
    pub fn new(
        sorted_headers: Vec<Header>,
        top_level_const_fragments: Vec<TopLevelConstFragment>,
        module_symbols: ModuleSymbols,
        context: AstBuildContext<'_>,
    ) -> Result<Ast, CompilerMessages> {
        reset_ast_counters();

        let header_count = sorted_headers.len();
        let (phase_context, string_table) = AstPhaseContext::from_build_context(context);

        let environment = AstModuleEnvironmentBuilder::new(&phase_context, module_symbols)
            .build(&sorted_headers, string_table)?;

        let node_emission_start = Instant::now();
        let emitted = AstEmitter::new(&phase_context, &environment, header_count)
            .emit(sorted_headers, string_table)?;
        timer_log!(node_emission_start, "AST/emit nodes completed in: ");
        let _ = node_emission_start;

        let finalization_start = Instant::now();
        let ast = AstFinalizer::new(&phase_context, &environment).finalize(
            emitted,
            &top_level_const_fragments,
            string_table,
        )?;
        timer_log!(finalization_start, "AST/finalize completed in: ");
        let _ = finalization_start;

        log_ast_counters();

        Ok(ast)
    }
}

// WHAT: public(crate) entrypoint for function/start-function body parsing.
// WHY: callers should import one obvious `ast`-root function while detailed statement parsing
// lives in focused helper modules.
pub(crate) fn function_body_to_ast(
    token_stream: &mut FileTokens,
    context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerError> {
    parse_function_body_statements(token_stream, context, warnings, string_table)
}

#[cfg(test)]
#[path = "tests/parser_error_recovery_tests.rs"]
mod parser_error_recovery_tests;
