//! Const evaluation helpers for template control flow.
//!
//! Validation uses this module to reject const-required control-flow shapes that
//! would otherwise leak runtime work into const template contexts. Template-head
//! parsing also uses it for the small amount of source-constant inlining needed
//! before validation can recognize dependency-sorted const declarations.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, LoopBindings, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::template::{TemplateAtom, TemplateContent};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::optimizers::constant_folding::{ConstantFoldResult, constant_fold};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

use super::types::{
    TemplateBranchChain, TemplateBranchSelector, TemplateControlFlow, TemplateLoopControlFlow,
    TemplateLoopHeader,
};

impl TemplateControlFlow {
    pub(crate) fn is_const_evaluable_value(&self) -> bool {
        control_flow_is_const_evaluable_with_bindings(self, &[])
    }
}

impl TemplateLoopControlFlow {
    pub(super) fn body_const_evaluation_bindings(
        &self,
        inherited_loop_binding_paths: &[InternedPath],
    ) -> Vec<InternedPath> {
        let mut loop_binding_paths = inherited_loop_binding_paths.to_vec();
        match &self.header {
            TemplateLoopHeader::Range { bindings, .. }
            | TemplateLoopHeader::Collection { bindings, .. } => {
                collect_loop_binding_paths(bindings, &mut loop_binding_paths);
            }

            TemplateLoopHeader::Conditional { .. } => {}
        }

        loop_binding_paths
    }
}

impl TemplateLoopHeader {
    pub(super) fn is_const_evaluable_value(&self) -> bool {
        match self {
            Self::Conditional { condition } => condition.is_compile_time_constant(),

            Self::Range { range, .. } => {
                range.start.is_compile_time_constant()
                    && range.end.is_compile_time_constant()
                    && range
                        .step
                        .as_ref()
                        .is_none_or(Expression::is_compile_time_constant)
            }

            Self::Collection { iterable, .. } => iterable.is_compile_time_constant(),
        }
    }
}

pub(crate) fn inline_source_consts_for_const_required_if_condition(
    condition: TemplateBranchSelector,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> TemplateBranchSelector {
    match condition {
        TemplateBranchSelector::Bool(condition) => TemplateBranchSelector::Bool(
            inline_source_consts_for_const_required_expression(condition, context, string_table),
        ),

        TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern } => {
            TemplateBranchSelector::OptionPresentCapture {
                scrutinee: inline_source_consts_for_const_required_expression(
                    scrutinee,
                    context,
                    string_table,
                ),
                pattern,
            }
        }
    }
}

pub(crate) fn inline_source_consts_for_const_required_expression(
    expression: Expression,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Expression {
    inline_source_consts_for_const_required_condition(expression, context, string_table)
}

pub(super) fn content_is_const_evaluable_with_bindings(
    content: &TemplateContent,
    loop_binding_paths: &[InternedPath],
) -> bool {
    content.atoms.iter().all(|atom| match atom {
        TemplateAtom::Slot(_) => true,
        TemplateAtom::Content(segment) => {
            expression_is_const_evaluable_with_bindings(&segment.expression, loop_binding_paths)
        }
    })
}

pub(super) fn expression_is_const_evaluable_with_bindings(
    expression: &Expression,
    loop_binding_paths: &[InternedPath],
) -> bool {
    match &expression.kind {
        ExpressionKind::Reference(path) => loop_binding_paths.iter().any(|known| known == path),

        ExpressionKind::Coerced { value, .. } => {
            expression_is_const_evaluable_with_bindings(value, loop_binding_paths)
        }

        ExpressionKind::Runtime(nodes) => nodes
            .iter()
            .all(|node| node_is_const_evaluable_with_bindings(node, loop_binding_paths)),

        ExpressionKind::Template(template) => {
            template_is_const_evaluable_with_bindings(template, loop_binding_paths)
        }

        _ => expression.is_compile_time_constant(),
    }
}

pub(super) fn option_capture_presence_is_const_decidable(
    scrutinee: &Expression,
    loop_binding_paths: &[InternedPath],
) -> bool {
    match &scrutinee.kind {
        ExpressionKind::OptionNone => true,

        // `T` in a `T?` context is represented as an explicit coercion. The
        // wrapped value is the present payload available to the then branch.
        ExpressionKind::Coerced { value, .. } => {
            expression_is_const_evaluable_with_bindings(value, loop_binding_paths)
        }

        // Const loop bindings are resolved per iteration during folding, so
        // validation can accept the reference when the loop source itself is const.
        ExpressionKind::Reference(path) => loop_binding_paths.iter().any(|known| known == path),

        _ => false,
    }
}

pub(super) fn collect_option_capture_binding_path(
    pattern: &MatchPattern,
    output: &mut Vec<InternedPath>,
) {
    if let MatchPattern::OptionPresentCapture { binding_path, .. } = pattern {
        output.push(binding_path.clone());
    }
}

fn template_is_const_evaluable_with_bindings(
    template: &Template,
    loop_binding_paths: &[InternedPath],
) -> bool {
    content_is_const_evaluable_with_bindings(&template.content, loop_binding_paths)
        && template.control_flow.as_ref().is_none_or(|control_flow| {
            control_flow_is_const_evaluable_with_bindings(control_flow, loop_binding_paths)
        })
}

fn control_flow_is_const_evaluable_with_bindings(
    control_flow: &TemplateControlFlow,
    loop_binding_paths: &[InternedPath],
) -> bool {
    match control_flow {
        TemplateControlFlow::BranchChain(branch_chain) => {
            branch_chain_is_const_evaluable_with_bindings(branch_chain, loop_binding_paths)
        }

        TemplateControlFlow::Loop(template_loop) => {
            let loop_body_binding_paths =
                template_loop.body_const_evaluation_bindings(loop_binding_paths);

            template_loop.header.is_const_evaluable_value()
                && content_is_const_evaluable_with_bindings(
                    &template_loop.body_content,
                    &loop_body_binding_paths,
                )
        }

        TemplateControlFlow::LoopControl(_) => true,
    }
}

fn branch_chain_is_const_evaluable_with_bindings(
    branch_chain: &TemplateBranchChain,
    loop_binding_paths: &[InternedPath],
) -> bool {
    branch_chain.branches.iter().all(|branch| {
        branch_selector_is_const_evaluable_with_bindings(&branch.selector, loop_binding_paths)
            && content_is_const_evaluable_with_bindings(
                &branch.content,
                &branch_const_binding_paths(&branch.selector, loop_binding_paths),
            )
    }) && branch_chain.fallback.as_ref().is_none_or(|fallback| {
        content_is_const_evaluable_with_bindings(&fallback.content, loop_binding_paths)
    })
}

fn branch_selector_is_const_evaluable_with_bindings(
    selector: &TemplateBranchSelector,
    loop_binding_paths: &[InternedPath],
) -> bool {
    match selector {
        TemplateBranchSelector::Bool(condition) => {
            expression_is_const_evaluable_with_bindings(condition, loop_binding_paths)
        }

        TemplateBranchSelector::OptionPresentCapture { scrutinee, .. } => {
            option_capture_presence_is_const_decidable(scrutinee, loop_binding_paths)
        }
    }
}

fn branch_const_binding_paths(
    selector: &TemplateBranchSelector,
    inherited_binding_paths: &[InternedPath],
) -> Vec<InternedPath> {
    let mut binding_paths = inherited_binding_paths.to_vec();

    match selector {
        TemplateBranchSelector::OptionPresentCapture { pattern, .. } => {
            collect_option_capture_binding_path(pattern, &mut binding_paths);
        }

        TemplateBranchSelector::Bool(_) => {}
    }

    binding_paths
}

fn collect_loop_binding_paths(bindings: &LoopBindings, output: &mut Vec<InternedPath>) {
    if let Some(item) = &bindings.item {
        output.push(item.id.clone());
    }

    if let Some(index) = &bindings.index {
        output.push(index.id.clone());
    }
}

fn inline_source_consts_for_const_required_condition(
    expression: Expression,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Expression {
    // This is intentionally shape-preserving instead of using the broader
    // ConstValueResolver: option-present template `if` needs the outer `T?`
    // coercion to survive so validation and folding still see an option scrutinee.
    let substituted = substitute_source_consts_in_expression(expression, context);

    if let ExpressionKind::Runtime(nodes) = &substituted.kind {
        return fold_substituted_runtime_condition(&substituted, nodes, string_table);
    }

    substituted
}

fn substitute_source_consts_in_expression(
    expression: Expression,
    context: &ScopeContext,
) -> Expression {
    match &expression.kind {
        ExpressionKind::Reference(path) => source_const_value_for_path(path, context)
            .cloned()
            .unwrap_or(expression),

        ExpressionKind::Coerced { value, to_type } => {
            let resolved = substitute_source_consts_in_expression((**value).clone(), context);

            if matches!(resolved.kind, ExpressionKind::Reference(_)) {
                return expression;
            }

            Expression {
                kind: ExpressionKind::Coerced {
                    value: Box::new(resolved),
                    to_type: *to_type,
                },
                ..expression
            }
        }

        ExpressionKind::Runtime(nodes) => {
            let substituted_nodes = nodes
                .iter()
                .map(|node| substitute_source_consts_in_node(node, context))
                .collect();

            Expression {
                kind: ExpressionKind::Runtime(substituted_nodes),
                ..expression
            }
        }

        _ => expression,
    }
}

fn substitute_source_consts_in_node(node: &AstNode, context: &ScopeContext) -> AstNode {
    let NodeKind::Rvalue(expression) = &node.kind else {
        return node.clone();
    };

    AstNode {
        kind: NodeKind::Rvalue(substitute_source_consts_in_expression(
            expression.clone(),
            context,
        )),
        location: node.location.clone(),
        scope: node.scope.clone(),
    }
}

fn fold_substituted_runtime_condition(
    expression: &Expression,
    nodes: &[AstNode],
    string_table: &mut StringTable,
) -> Expression {
    match constant_fold(nodes, string_table) {
        Ok(ConstantFoldResult::Folded(stack)) => {
            if stack.len() == 1
                && let NodeKind::Rvalue(folded) = &stack[0].kind
            {
                return folded.clone();
            }

            expression.clone()
        }

        Ok(ConstantFoldResult::Unchanged) | Err(_) => expression.clone(),
    }
}

fn source_const_value_for_path<'a>(
    path: &InternedPath,
    context: &'a ScopeContext,
) -> Option<&'a Expression> {
    let declaration = context
        .top_level_declarations
        .get_visible_resolved_by_path(path, context.visible_declaration_ids.as_ref())?;

    if source_const_value_is_condition_decidable(&declaration.value) {
        Some(&declaration.value)
    } else {
        None
    }
}

fn source_const_value_is_condition_decidable(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::OptionNone => true,
        ExpressionKind::Coerced { value, .. } => value.is_compile_time_constant(),
        _ => expression.is_compile_time_constant(),
    }
}

fn node_is_const_evaluable_with_bindings(
    node: &AstNode,
    loop_binding_paths: &[InternedPath],
) -> bool {
    match &node.kind {
        NodeKind::Rvalue(expression) => {
            expression_is_const_evaluable_with_bindings(expression, loop_binding_paths)
        }
        NodeKind::Operator(_) => true,
        _ => false,
    }
}
