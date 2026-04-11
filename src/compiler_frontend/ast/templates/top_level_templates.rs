//! Start-template/fragment synthesis for the entry file.
//!
//! WHAT: converts entry-file top-level templates into ordered start fragments
//! (`ConstString` / generated runtime fragment functions) and extracts doc
//! fragments from comment templates.
//! WHY: builders consume canonical ordered fragments and doc metadata, while
//! runtime fragment setup/capture policy remains frontend-owned.

mod capture_analysis;
mod doc_fragments;
mod fragment_extraction;
mod fragment_synthesis;

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::headers::parse_file_headers::TopLevelTemplateItem;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::FxHashMap;

#[derive(Clone, Debug)]
pub enum AstStartTemplateItem {
    ConstString {
        value: StringId,
        #[allow(dead_code)] // Preserved for future source-mapping and error reporting.
        location: SourceLocation,
    },
    RuntimeStringFunction {
        function: InternedPath,
        location: SourceLocation,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AstDocFragmentKind {
    Doc,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AstDocFragment {
    pub kind: AstDocFragmentKind,
    pub value: StringId,
    pub location: SourceLocation,
}

#[derive(Clone)]
struct RuntimeTemplateCandidate {
    template_expression: Expression,
    location: SourceLocation,
    scope: InternedPath,
    source_index: usize,
    preceding_statements: Vec<AstNode>,
}

#[derive(Clone)]
struct RuntimeTemplateExtraction {
    runtime_candidates: Vec<RuntimeTemplateCandidate>,
    entry_scope: InternedPath,
    non_template_body: Vec<AstNode>,
}

pub(crate) fn synthesize_start_template_items(
    ast_nodes: &mut Vec<AstNode>,
    entry_dir: &InternedPath,
    top_level_template_items: &[TopLevelTemplateItem],
    const_templates_by_path: &FxHashMap<InternedPath, StringId>,
    project_path_resolver: &ProjectPathResolver,
    path_format_config: &PathStringFormatConfig,
    string_table: &mut StringTable,
) -> Result<Vec<AstStartTemplateItem>, CompilerError> {
    fragment_synthesis::synthesize_start_template_items(
        ast_nodes,
        entry_dir,
        top_level_template_items,
        const_templates_by_path,
        project_path_resolver,
        path_format_config,
        string_table,
    )
}

pub(crate) fn collect_and_strip_comment_templates(
    ast_nodes: &mut [AstNode],
    project_path_resolver: &ProjectPathResolver,
    path_format_config: &PathStringFormatConfig,
    string_table: &mut StringTable,
) -> Result<Vec<AstDocFragment>, CompilerError> {
    doc_fragments::collect_and_strip_comment_templates(
        ast_nodes,
        project_path_resolver,
        path_format_config,
        string_table,
    )
}

fn fold_template_with_context(
    template: &crate::compiler_frontend::ast::templates::template_types::Template,
    source_file_scope: &InternedPath,
    project_path_resolver: &ProjectPathResolver,
    path_format_config: &PathStringFormatConfig,
    string_table: &mut StringTable,
) -> Result<StringId, CompilerError> {
    let mut fold_context = TemplateFoldContext {
        string_table,
        project_path_resolver,
        path_format_config,
        source_file_scope,
    };
    template.fold_into_stringid(&mut fold_context)
}

#[cfg(test)]
#[path = "tests/template_tests.rs"]
mod template_tests;
