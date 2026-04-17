//! Top-level const fragment collection and doc fragment extraction.
//!
//! WHAT: collects folded top-level const string fragments and extracts doc fragments from
//! comment templates.
//! WHY: builders consume ordered const fragments (with runtime insertion indices) and doc
//! metadata; all runtime template handling moves into the entry start() function body via
//! PushStartRuntimeFragment nodes.

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::templates::doc_fragments;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::headers::parse_file_headers::TopLevelConstFragment;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::FxHashMap;

/// A top-level const template that has been folded to a string at compile time.
///
/// WHAT: carries the folded string value and its insertion index relative to runtime fragments.
/// WHY: builders merge const fragments with the runtime fragment list using the insertion index
/// to reconstruct source-order interleaving.
#[derive(Clone, Debug)]
pub struct AstConstTopLevelFragment {
    /// Number of runtime fragments preceding this const fragment in source order.
    pub runtime_insertion_index: usize,
    pub value: StringId,
    pub location: SourceLocation,
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

/// Collects const top-level fragments from folded template values.
///
/// WHAT: maps each header-parsed const fragment to its folded string value using the
/// const template path map produced during AST emission (pass_emit_nodes).
/// WHY: const fragments are folded during emit; this function gathers the results into
/// the ordered `AstConstTopLevelFragment` list consumed by HIR/builders.
pub(crate) fn collect_const_top_level_fragments(
    top_level_const_fragments: &[TopLevelConstFragment],
    const_templates_by_path: &FxHashMap<InternedPath, StringId>,
) -> Result<Vec<AstConstTopLevelFragment>, CompilerError> {
    let mut result = Vec::with_capacity(top_level_const_fragments.len());
    for fragment in top_level_const_fragments {
        let value = const_templates_by_path
            .get(&fragment.header_path)
            .copied()
            .ok_or_else(|| {
                CompilerError::compiler_error(
                    "Top-level const fragment has no corresponding folded template value. This is a compiler bug.",
                )
            })?;
        result.push(AstConstTopLevelFragment {
            runtime_insertion_index: fragment.runtime_insertion_index,
            value,
            location: fragment.location.clone(),
        });
    }
    Ok(result)
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

#[cfg(test)]
#[path = "tests/template_tests.rs"]
mod template_tests;
