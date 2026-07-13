//! Helpers for tests that hand-build template control flow.
//!
//! WHAT: installs same-store refs for nested template content in fixtures that
//! bypass the parser/render-unit preparation pipeline.
//! WHY: production control flow treats body TIR roots as authoritative, so
//! manual fixtures must prepare nested template references in the same store.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{TemplateAtom, TemplateContent};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::TemplateIrStore;
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(crate) fn install_same_store_control_flow_body_refs(
    template: &mut Template,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
) -> Result<(), TemplateError> {
    prepare_content_nested_templates(&mut template.content, store, string_table)?;

    Ok(())
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
