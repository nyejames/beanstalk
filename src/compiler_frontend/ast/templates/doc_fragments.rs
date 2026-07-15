//! Top-level doc-template collection and stripping.
//!
//! WHAT: extracts `$doc` comment template output from authoritative TIR into
//! `AstDocFragment` metadata and strips those declarations from executable
//! function bodies.
//! WHY: documentation extraction is a separate concern from runtime fragment
//! synthesis and should remain independently auditable.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::Template;
use crate::compiler_frontend::ast::templates::template::{CommentDirectiveKind, TemplateType};
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::ast::templates::tir::{TemplateIrRegistry, TirFoldCache};
use crate::compiler_frontend::ast::templates::top_level_templates::{
    AstDocFragment, AstDocFragmentKind,
};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::cell::RefCell;
use std::rc::Rc;

// -------------------------
//  Fragment Extraction
// -------------------------

pub(in crate::compiler_frontend::ast::templates) fn collect_and_strip_comment_templates(
    ast_nodes: &mut [AstNode],
    project_path_resolver: &ProjectPathResolver,
    path_format_config: &PathStringFormatConfig,
    string_table: &mut StringTable,
    template_const_loop_iteration_limit: usize,
    template_ir_registry: Option<Rc<RefCell<TemplateIrRegistry>>>,
) -> Result<Vec<AstDocFragment>, TemplateError> {
    let mut fragments = Vec::new();
    let mut context = DocFragmentCollectionContext {
        project_path_resolver,
        path_format_config,
        string_table,
        template_const_loop_iteration_limit,
        template_ir_registry,
    };

    for node in ast_nodes.iter_mut() {
        let NodeKind::Function(_, _, body) = &mut node.kind else {
            continue;
        };

        let mut retained = Vec::with_capacity(body.len());

        for statement in std::mem::take(body) {
            if let Some(comment_template) = as_top_level_template_comment_declaration(
                &statement,
                context.template_ir_registry.as_ref(),
            ) {
                collect_doc_fragments(comment_template, &mut fragments, &mut context)?;
                continue;
            }

            retained.push(statement);
        }

        *body = retained;
    }

    // Sort fragments deterministically by source location.
    fragments.sort_by_key(|fragment| {
        (
            fragment.location.scope.to_string(context.string_table),
            fragment.location.start_pos.line_number,
            fragment.location.start_pos.char_column,
        )
    });

    Ok(fragments)
}

/// Shared state for doc-template extraction.
///
/// WHAT: carries fold services through top-level comment extraction.
/// WHY: every fold should see the same module registry authority without
/// growing helper signatures.
struct DocFragmentCollectionContext<'a, 'strings> {
    project_path_resolver: &'a ProjectPathResolver,
    path_format_config: &'a PathStringFormatConfig,
    string_table: &'strings mut StringTable,
    template_const_loop_iteration_limit: usize,
    template_ir_registry: Option<Rc<RefCell<TemplateIrRegistry>>>,
}

// -------------------------
//  Internal Helpers
// -------------------------

/// Matches a top-level `PushStartRuntimeFragment` node containing a comment
/// template.
///
/// WHAT: reads the authoritative TIR kind when the registry can resolve the
///       template, then falls back to the durable boundary cache.
/// WHY: top-level templates may cross from a foreign TIR store while older
///      extraction callers do not provide a registry.
fn as_top_level_template_comment_declaration<'a>(
    node: &'a AstNode,
    registry: Option<&Rc<RefCell<TemplateIrRegistry>>>,
) -> Option<&'a Template> {
    let NodeKind::PushStartRuntimeFragment(expression) = &node.kind else {
        return None;
    };

    let ExpressionKind::Template(template) = &expression.kind else {
        return None;
    };

    let template_kind = template_kind_at_doc_fragment_boundary(template, registry);

    matches!(template_kind, TemplateType::Comment(_)).then_some(template.as_ref())
}

/// Extracts one top-level `$doc` fragment.
fn collect_doc_fragments(
    template: &Template,
    fragments: &mut Vec<AstDocFragment>,
    context: &mut DocFragmentCollectionContext<'_, '_>,
) -> Result<(), TemplateError> {
    let template_kind =
        template_kind_at_doc_fragment_boundary(template, context.template_ir_registry.as_ref());

    if matches!(
        template_kind,
        TemplateType::Comment(CommentDirectiveKind::Doc)
    ) {
        let mut fold_context = TemplateFoldContext {
            string_table: context.string_table,
            project_path_resolver: context.project_path_resolver,
            path_format_config: context.path_format_config,
            source_file_scope: &template.location.scope,
            template_const_loop_iteration_limit: context.template_const_loop_iteration_limit,
            template_ir_registry: context.template_ir_registry.as_ref().map(Rc::clone),
            bindings: Vec::new(),
            fold_cache: TirFoldCache::new(),
        };
        let rendered = template.fold_into_stringid(&mut fold_context)?;

        fragments.push(AstDocFragment {
            kind: AstDocFragmentKind::Doc,
            value: rendered,
            location: template.location.to_owned(),
        });
    }

    Ok(())
}

/// Reads a doc-fragment template's kind from TIR when that authority is
/// reachable, otherwise uses the durable cross-store cache.
fn template_kind_at_doc_fragment_boundary(
    template: &Template,
    registry: Option<&Rc<RefCell<TemplateIrRegistry>>>,
) -> TemplateType {
    registry
        .and_then(|registry| template.tir_kind_via_registry(&registry.borrow()))
        .unwrap_or_else(|| template.kind.clone())
}
