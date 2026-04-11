//! Entry-start scanning and runtime-template candidate extraction.
//!
//! WHAT: finds top-level runtime template candidates in the entry start body and
//! rewrites the body after synthesis.
//! WHY: synthesis orchestration should not directly own AST body-shape details.

use super::{RuntimeTemplateCandidate, RuntimeTemplateExtraction};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::TOP_LEVEL_TEMPLATE_NAME;

pub(super) fn extract_runtime_template_candidates(
    ast_nodes: &mut [AstNode],
    entry_start_index: usize,
    string_table: &StringTable,
) -> Result<RuntimeTemplateExtraction, CompilerError> {
    let Some(entry_start_node) = ast_nodes.get_mut(entry_start_index) else {
        return Err(CompilerError::compiler_error(
            "Entry start function index is out of bounds while extracting runtime templates.",
        ));
    };

    let entry_scope = entry_start_node.scope.to_owned();
    let NodeKind::Function(_, _, body) = &mut entry_start_node.kind else {
        return Err(CompilerError::compiler_error(
            "Entry start function node is not a function while extracting runtime templates.",
        ));
    };

    // Work on a snapshot so extraction is independent of in-place mutation.
    let original_body = body.to_owned();
    let mut runtime_candidates = Vec::new();
    let mut non_template_body = Vec::with_capacity(original_body.len());

    for (index, node) in original_body.iter().enumerate() {
        if let Some(declaration) = as_top_level_template_declaration(node, string_table) {
            runtime_candidates.push(RuntimeTemplateCandidate {
                template_expression: declaration.value.to_owned(),
                location: node.location.to_owned(),
                scope: node.scope.to_owned(),
                source_index: index,
                preceding_statements: original_body[..index]
                    .iter()
                    .filter(|statement| {
                        as_top_level_template_declaration(statement, string_table).is_none()
                    })
                    .map(ToOwned::to_owned)
                    .collect(),
            });
            continue;
        }

        if let Some(template_expression) = as_top_level_template_return_expression(node) {
            runtime_candidates.push(RuntimeTemplateCandidate {
                template_expression: template_expression.to_owned(),
                location: node.location.to_owned(),
                scope: node.scope.to_owned(),
                source_index: index,
                preceding_statements: original_body[..index]
                    .iter()
                    .filter(|statement| {
                        as_top_level_template_declaration(statement, string_table).is_none()
                            && as_top_level_template_return_expression(statement).is_none()
                    })
                    .map(ToOwned::to_owned)
                    .collect(),
            });
            continue;
        }

        non_template_body.push(node.to_owned());
    }

    Ok(RuntimeTemplateExtraction {
        runtime_candidates,
        entry_scope,
        non_template_body,
    })
}

pub(super) fn replace_entry_start_body(
    ast_nodes: &mut [AstNode],
    entry_start_index: usize,
    body: Vec<AstNode>,
) -> Result<(), CompilerError> {
    let Some(entry_start_node) = ast_nodes.get_mut(entry_start_index) else {
        return Err(CompilerError::compiler_error(
            "Entry start function index is out of bounds while rewriting captured declarations.",
        ));
    };

    let NodeKind::Function(_, _, start_body) = &mut entry_start_node.kind else {
        return Err(CompilerError::compiler_error(
            "Entry start function node is not a function while rewriting captured declarations.",
        ));
    };

    *start_body = body;
    Ok(())
}

pub(super) fn as_top_level_template_declaration<'a>(
    node: &'a AstNode,
    string_table: &StringTable,
) -> Option<&'a Declaration> {
    let NodeKind::VariableDeclaration(declaration) = &node.kind else {
        return None;
    };

    let is_template_name = declaration
        .id
        .name_str(string_table)
        .is_some_and(|name| name == TOP_LEVEL_TEMPLATE_NAME);

    if !is_template_name {
        return None;
    }

    if !matches!(declaration.value.kind, ExpressionKind::Template(_)) {
        return None;
    }

    Some(declaration)
}

pub(super) fn as_top_level_template_return_expression(node: &AstNode) -> Option<&Expression> {
    let NodeKind::Return(values) = &node.kind else {
        return None;
    };

    (values.len() == 1).then_some(())?;
    let expression = &values[0];
    matches!(expression.kind, ExpressionKind::Template(_)).then_some(expression)
}
