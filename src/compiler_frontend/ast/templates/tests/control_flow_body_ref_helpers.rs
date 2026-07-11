//! Helpers for tests that hand-build template control flow.
//!
//! WHAT: installs same-store refs for nested template content in fixtures that
//! bypass the parser/render-unit preparation pipeline, and exposes a small
//! body-content materializer for tests that hand-build control-flow bodies.
//! WHY: production control flow now treats body TIR roots as authoritative.
//! Manual fixtures must provide those roots explicitly instead of relying on
//! deleted content mirrors inside branch, fallback, or loop control-flow state.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    TemplateAtom, TemplateContent, TemplateSegmentOrigin,
};
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateControlFlowTirReference;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrNode, TemplateIrNodeKind, TemplateIrStore, finalized_template_tir_id,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

pub(crate) fn install_same_store_control_flow_body_refs(
    template: &mut Template,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
) -> Result<(), TemplateError> {
    prepare_content_nested_templates(&mut template.content, store, string_table)?;

    Ok(())
}

/// Builds the authoritative TIR root for a text wrapper around loop output.
pub(crate) fn materialize_text_aggregate_wrapper_ref(
    prefix: Option<StringId>,
    suffix: Option<StringId>,
    location: SourceLocation,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
) -> TemplateControlFlowTirReference {
    let mut children = Vec::with_capacity(3);

    if let Some(text) = prefix {
        children.push(store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Text {
                text,
                byte_len: string_table.resolve(text).len() as u32,
                origin: TemplateSegmentOrigin::Body,
            },
            location.clone(),
        )));
    }

    children.push(store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::AggregateOutput,
        location.clone(),
    )));

    if let Some(text) = suffix {
        children.push(store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Text {
                text,
                byte_len: string_table.resolve(text).len() as u32,
                origin: TemplateSegmentOrigin::Body,
            },
            location.clone(),
        )));
    }

    let root = if children.len() == 1 {
        children[0]
    } else {
        store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence { children },
            location,
        ))
    };

    TemplateControlFlowTirReference::new(store, root)
}

pub(crate) fn materialize_body_content_ref(
    content: &TemplateContent,
    location: SourceLocation,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
) -> Result<TemplateControlFlowTirReference, TemplateError> {
    let mut body_template = Template::empty();
    body_template.content = content.clone();
    body_template.location = location;

    let body_template_id = finalized_template_tir_id(&body_template, store, string_table)?;
    let body_root = store
        .get_template(body_template_id)
        .expect("body template was just materialized into the same store")
        .root;

    Ok(TemplateControlFlowTirReference::new(store, body_root))
}

fn prepare_content_nested_templates(
    content: &mut TemplateContent,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
) -> Result<(), TemplateError> {
    for atom in &mut content.atoms {
        match atom {
            TemplateAtom::Content(segment) => {
                prepare_expression_nested_templates(&mut segment.expression, store, string_table)?;
            }
        }
    }

    Ok(())
}

fn prepare_expression_nested_templates(
    expression: &mut Expression,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
) -> Result<(), TemplateError> {
    if let ExpressionKind::Template(template) = &mut expression.kind {
        install_same_store_control_flow_body_refs(template, store, string_table)?;
    }

    Ok(())
}
