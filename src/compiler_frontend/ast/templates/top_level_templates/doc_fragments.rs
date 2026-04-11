//! Top-level doc-template collection and stripping.
//!
//! WHAT: extracts `$doc` comment template output into `AstDocFragment` metadata
//! and strips those declarations from executable function bodies.
//! WHY: documentation extraction is a separate concern from runtime fragment
//! synthesis and should remain independently auditable.

use super::{AstDocFragment, AstDocFragmentKind, fold_template_with_context};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::{CommentDirectiveKind, TemplateType};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::string_interning::StringTable;

use super::fragment_extraction::as_top_level_template_declaration;

pub(super) fn collect_and_strip_comment_templates(
    ast_nodes: &mut [AstNode],
    project_path_resolver: &ProjectPathResolver,
    path_format_config: &PathStringFormatConfig,
    string_table: &mut StringTable,
) -> Result<Vec<AstDocFragment>, CompilerError> {
    let mut fragments = Vec::new();

    for node in ast_nodes.iter_mut() {
        let NodeKind::Function(_, _, body) = &mut node.kind else {
            continue;
        };

        let mut retained = Vec::with_capacity(body.len());
        for statement in std::mem::take(body) {
            if let Some(comment_template) =
                as_top_level_template_comment_declaration(&statement, string_table)
            {
                collect_doc_fragments(
                    comment_template,
                    &mut fragments,
                    project_path_resolver,
                    path_format_config,
                    string_table,
                )?;
                continue;
            }

            retained.push(statement);
        }

        *body = retained;
    }

    fragments.sort_by_key(|fragment| {
        (
            fragment.location.scope.to_string(string_table),
            fragment.location.start_pos.line_number,
            fragment.location.start_pos.char_column,
        )
    });

    Ok(fragments)
}

fn as_top_level_template_comment_declaration<'a>(
    node: &'a AstNode,
    string_table: &StringTable,
) -> Option<&'a Template> {
    let declaration = as_top_level_template_declaration(node, string_table)?;
    let ExpressionKind::Template(template) = &declaration.value.kind else {
        return None;
    };

    matches!(template.kind, TemplateType::Comment(_)).then_some(template.as_ref())
}

fn collect_doc_fragments(
    template: &Template,
    fragments: &mut Vec<AstDocFragment>,
    project_path_resolver: &ProjectPathResolver,
    path_format_config: &PathStringFormatConfig,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    if matches!(
        template.kind,
        TemplateType::Comment(CommentDirectiveKind::Doc)
    ) {
        let rendered = fold_template_with_context(
            template,
            &template.location.scope,
            project_path_resolver,
            path_format_config,
            string_table,
        )?;
        fragments.push(AstDocFragment {
            kind: AstDocFragmentKind::Doc,
            value: rendered,
            location: template.location.to_owned(),
        });
    }

    for child in &template.doc_children {
        collect_doc_fragments(
            child,
            fragments,
            project_path_resolver,
            path_format_config,
            string_table,
        )?;
    }

    Ok(())
}
