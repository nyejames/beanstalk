//! Mutating annotation pass for reactive template metadata.
//!
//! WHAT: walks the finalized AST and attaches computed reactive template metadata
//! to expressions, declarations, assignments, and template structures.
//! WHY: separating the annotation traversal from flow analysis and metadata
//! collection keeps each phase focused on one responsibility.

use super::collector::metadata_for_expression;
use super::types::{
    FunctionTemplateFlow, ReactiveTemplateValueEnvironment, reference_path_for_place_expression,
};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, LoopBindings, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, FallibleHandling,
};
use crate::compiler_frontend::ast::expressions::expression_rpn::{
    ExpressionRpnItem, PlaceExpression, PlaceExpressionKind,
};
use crate::compiler_frontend::ast::statements::functions::{FunctionSignature, ReturnChannel};
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::statements::value_production::types::ValueBlock;
use crate::compiler_frontend::ast::templates::template::{TemplateAtom, TemplateContent};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateAggregatePiece, TemplateAggregateRenderPlan, TemplateBranchSelector,
    TemplateControlFlow, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::ast::templates::template_slots::{
    RuntimeSlotApplicationPlan, RuntimeSlotSitePiece,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use rustc_hash::FxHashMap;

pub(super) fn annotate_nodes(
    nodes: &mut [AstNode],
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    for node in nodes {
        annotate_node(node, flows, value_environment);
    }
}

fn annotate_node(
    node: &mut AstNode,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    match &mut node.kind {
        NodeKind::Function(path, signature, body) => {
            let mut function_environment =
                ReactiveTemplateValueEnvironment::for_parameters(&signature.parameters);
            annotate_nodes(body, flows, &mut function_environment);
            apply_flow_to_signature(path, signature, flows);
        }

        NodeKind::VariableDeclaration(declaration) => {
            annotate_declaration(declaration, flows, value_environment);
        }

        NodeKind::Return(values) => {
            annotate_expressions(values, flows, value_environment);
        }

        NodeKind::ReturnError(value)
        | NodeKind::PushStartRuntimeFragment(value)
        | NodeKind::ExpressionStatement(value) => {
            annotate_expression(value, flows, value_environment);
        }

        NodeKind::ThenValue(produced_values) => {
            annotate_expressions(&mut produced_values.expressions, flows, value_environment);
        }

        NodeKind::If(condition, then_body, else_body) => {
            annotate_expression(condition, flows, value_environment);
            let mut then_environment = value_environment.clone();
            annotate_nodes(then_body, flows, &mut then_environment);
            if let Some(else_body) = else_body {
                let mut else_environment = value_environment.clone();
                annotate_nodes(else_body, flows, &mut else_environment);
            }
        }

        NodeKind::Match {
            scrutinee,
            arms,
            default,
            ..
        } => {
            annotate_expression(scrutinee, flows, value_environment);
            for arm in arms {
                let mut arm_environment = value_environment.clone();
                annotate_match_pattern(&mut arm.pattern, flows, &mut arm_environment);
                if let Some(guard) = &mut arm.guard {
                    annotate_expression(guard, flows, &mut arm_environment);
                }
                annotate_nodes(&mut arm.body, flows, &mut arm_environment);
            }
            if let Some(default_body) = default {
                let mut default_environment = value_environment.clone();
                annotate_nodes(default_body, flows, &mut default_environment);
            }
        }

        NodeKind::ScopedBlock { body } => {
            let mut body_environment = value_environment.clone();
            annotate_nodes(body, flows, &mut body_environment);
        }

        NodeKind::RangeLoop {
            bindings,
            range,
            body,
        } => {
            let mut loop_environment = value_environment.clone();
            annotate_loop_bindings(bindings, flows, &mut loop_environment);
            annotate_expression(&mut range.start, flows, &mut loop_environment);
            annotate_expression(&mut range.end, flows, &mut loop_environment);
            if let Some(step) = &mut range.step {
                annotate_expression(step, flows, &mut loop_environment);
            }
            annotate_nodes(body, flows, &mut loop_environment);
        }

        NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        } => {
            let mut loop_environment = value_environment.clone();
            annotate_loop_bindings(bindings, flows, &mut loop_environment);
            annotate_expression(iterable, flows, &mut loop_environment);
            annotate_nodes(body, flows, &mut loop_environment);
        }

        NodeKind::WhileLoop(condition, body) => {
            annotate_expression(condition, flows, value_environment);
            let mut body_environment = value_environment.clone();
            annotate_nodes(body, flows, &mut body_environment);
        }

        NodeKind::Assert { condition, .. } => {
            annotate_expression(condition, flows, value_environment);
        }

        NodeKind::StructDefinition(_, fields) => {
            for field in fields {
                annotate_declaration(field, flows, value_environment);
            }
        }

        NodeKind::Assignment { target, value } => {
            annotate_expression(value, flows, value_environment);
            if let Some(target_path) = reference_path_for_place_expression(target) {
                value_environment.record_assignment(target_path, value);
            }
        }

        NodeKind::MultiBind { value, .. } => {
            annotate_expression(value, flows, value_environment);
        }

        NodeKind::Break | NodeKind::Continue => {}
    }
}

fn apply_flow_to_signature(
    path: &InternedPath,
    signature: &mut FunctionSignature,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
) {
    let Some(flow) = flows.get(path) else {
        return;
    };

    let mut success_index = 0;
    for slot in &mut signature.returns {
        if slot.channel != ReturnChannel::Success {
            continue;
        }

        slot.reactive_template = flow
            .success_returns
            .get(success_index)
            .cloned()
            .unwrap_or(None);
        success_index += 1;
    }
}

fn annotate_declaration(
    declaration: &mut Declaration,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    if let ExpressionKind::Function(signature) = &mut declaration.value.kind {
        apply_flow_to_signature(&declaration.id, signature, flows);
        declaration.value.reactive_template =
            metadata_for_expression(&declaration.value, flows, value_environment);
        value_environment.record_declaration(declaration);
        return;
    }

    annotate_expression(&mut declaration.value, flows, value_environment);
    value_environment.record_declaration(declaration);
}

fn annotate_expressions(
    expressions: &mut [Expression],
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    for expression in expressions {
        annotate_expression(expression, flows, value_environment);
    }
}

fn annotate_place_expression(place: &mut PlaceExpression) {
    match &mut place.kind {
        PlaceExpressionKind::Local(_) => {}
        PlaceExpressionKind::Field { base, .. } => annotate_place_expression(base),
    }
}

fn annotate_expression(
    expression: &mut Expression,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    match &mut expression.kind {
        ExpressionKind::Template(template) => {
            annotate_template(template, flows, value_environment);
        }

        ExpressionKind::Function(_) => {}

        ExpressionKind::FunctionCall { args, .. }
        | ExpressionKind::HostFunctionCall { args, .. } => {
            annotate_call_arguments(args, flows, value_environment);
        }

        ExpressionKind::FieldAccess { base, .. } => {
            annotate_expression(base, flows, value_environment);
        }

        ExpressionKind::MethodCall { receiver, args, .. }
        | ExpressionKind::CollectionBuiltinCall { receiver, args, .. }
        | ExpressionKind::MapBuiltinCall { receiver, args, .. } => {
            annotate_expression(receiver, flows, value_environment);
            annotate_call_arguments(args, flows, value_environment);
        }

        ExpressionKind::HandledFallibleFunctionCall { args, .. }
        | ExpressionKind::HandledFallibleHostFunctionCall { args, .. } => {
            annotate_call_arguments(args, flows, value_environment);
        }

        ExpressionKind::Copy(place) => {
            annotate_place_expression(place);
        }

        ExpressionKind::Runtime(rpn) => {
            for item in &mut rpn.items {
                match item {
                    ExpressionRpnItem::Operand(expression) => {
                        annotate_expression(expression, flows, value_environment);
                    }
                    ExpressionRpnItem::Operator { .. } => {}
                }
            }
        }

        ExpressionKind::Collection(items) => annotate_expressions(items, flows, value_environment),

        ExpressionKind::MapLiteral(entries) => {
            for entry in entries {
                annotate_expression(&mut entry.key, flows, value_environment);
                annotate_expression(&mut entry.value, flows, value_environment);
            }
        }

        ExpressionKind::StructInstance(fields)
        | ExpressionKind::StructDefinition(fields)
        | ExpressionKind::ChoiceConstruct { fields, .. } => {
            for field in fields {
                annotate_declaration(field, flows, value_environment);
            }
        }

        ExpressionKind::Range(start, end) => {
            annotate_expression(start, flows, value_environment);
            annotate_expression(end, flows, value_environment);
        }

        ExpressionKind::FallibleCarrierConstruct { value, .. }
        | ExpressionKind::OptionPropagation { value }
        | ExpressionKind::Coerced { value, .. } => {
            annotate_expression(value, flows, value_environment);
        }

        ExpressionKind::HandledFallibleExpression { value, .. } => {
            annotate_expression(value, flows, value_environment);
        }

        ExpressionKind::Cast(cast) => {
            annotate_expression(&mut cast.source, flows, value_environment);
        }

        ExpressionKind::ValueBlock { block } => {
            annotate_value_block(block, flows, value_environment)
        }

        ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Path(_)
        | ExpressionKind::Reference(_) => {}
    }

    expression.reactive_template = metadata_for_expression(expression, flows, value_environment);
}

fn annotate_template(
    template: &mut Template,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    annotate_content(&mut template.content, flows, value_environment);
    annotate_content(&mut template.unformatted_content, flows, value_environment);
    annotate_control_flow(&mut template.control_flow, flows, value_environment);

    if let Some(render_plan) = &mut template.render_plan {
        annotate_render_plan(render_plan, flows, value_environment);
    }

    if let Some(plan) = &mut template.conditional_child_wrapper_plan {
        annotate_aggregate_render_plan(plan, flows, value_environment);
    }

    if let Some(plan) = &mut template.runtime_slot_application {
        annotate_runtime_slot_application(plan, flows, value_environment);
    }

    for child in &mut template.doc_children {
        annotate_template(child, flows, value_environment);
    }
    for child in &mut template.style.child_templates {
        annotate_template(child, flows, value_environment);
    }
    for child in &mut template.conditional_child_wrappers {
        annotate_template(child, flows, value_environment);
    }
}

fn annotate_content(
    content: &mut TemplateContent,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    for atom in &mut content.atoms {
        match atom {
            TemplateAtom::Content(segment) => {
                annotate_expression(&mut segment.expression, flows, value_environment)
            }
            TemplateAtom::Slot(slot) => {
                for wrapper in &mut slot.applied_child_wrappers {
                    annotate_template(wrapper, flows, value_environment);
                }
                for wrapper in &mut slot.child_wrappers {
                    annotate_template(wrapper, flows, value_environment);
                }
            }
        }
    }
}

fn annotate_control_flow(
    control_flow: &mut Option<TemplateControlFlow>,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    let Some(control_flow) = control_flow else {
        return;
    };

    match control_flow {
        TemplateControlFlow::BranchChain(branch_chain) => {
            for branch in &mut branch_chain.branches {
                let mut branch_environment = value_environment.clone();
                annotate_branch_selector(&mut branch.selector, flows, &mut branch_environment);
                annotate_content(&mut branch.content, flows, &mut branch_environment);
                if let Some(render_plan) = &mut branch.render_plan {
                    annotate_render_plan(render_plan, flows, &mut branch_environment);
                }
            }
            if let Some(fallback) = &mut branch_chain.fallback {
                let mut fallback_environment = value_environment.clone();
                annotate_content(&mut fallback.content, flows, &mut fallback_environment);
                if let Some(render_plan) = &mut fallback.render_plan {
                    annotate_render_plan(render_plan, flows, &mut fallback_environment);
                }
            }
        }

        TemplateControlFlow::Loop(template_loop) => {
            let mut loop_environment = value_environment.clone();
            annotate_loop_header(&mut template_loop.header, flows, &mut loop_environment);
            annotate_content(
                &mut template_loop.body_content,
                flows,
                &mut loop_environment,
            );
            if let Some(render_plan) = &mut template_loop.body_render_plan {
                annotate_render_plan(render_plan, flows, &mut loop_environment);
            }
            if let Some(aggregate_plan) = &mut template_loop.aggregate_render_plan {
                annotate_aggregate_render_plan(aggregate_plan, flows, &mut loop_environment);
            }
        }

        TemplateControlFlow::LoopControl(_) => {}
    }
}

fn annotate_branch_selector(
    selector: &mut TemplateBranchSelector,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    match selector {
        TemplateBranchSelector::Bool(condition) => {
            annotate_expression(condition, flows, value_environment)
        }
        TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern } => {
            annotate_expression(scrutinee, flows, value_environment);
            annotate_match_pattern(pattern, flows, value_environment);
        }
    }
}

fn annotate_loop_header(
    header: &mut TemplateLoopHeader,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    match header {
        TemplateLoopHeader::Conditional { condition } => {
            annotate_expression(condition, flows, value_environment)
        }
        TemplateLoopHeader::Range { bindings, range } => {
            annotate_loop_bindings(bindings, flows, value_environment);
            annotate_expression(&mut range.start, flows, value_environment);
            annotate_expression(&mut range.end, flows, value_environment);
            if let Some(step) = &mut range.step {
                annotate_expression(step, flows, value_environment);
            }
        }
        TemplateLoopHeader::Collection { bindings, iterable } => {
            annotate_loop_bindings(bindings, flows, value_environment);
            annotate_expression(iterable, flows, value_environment);
        }
    }
}

fn annotate_render_plan(
    plan: &mut TemplateRenderPlan,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    for piece in &mut plan.pieces {
        annotate_render_piece(piece, flows, value_environment);
    }
}

fn annotate_render_piece(
    piece: &mut RenderPiece,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    match piece {
        RenderPiece::DynamicExpression(dynamic) => {
            annotate_expression(&mut dynamic.expression, flows, value_environment)
        }
        RenderPiece::ChildTemplate(child) => {
            annotate_expression(&mut child.expression, flows, value_environment)
        }
        RenderPiece::Slot(slot) => {
            for wrapper in &mut slot.applied_child_wrappers {
                annotate_template(wrapper, flows, value_environment);
            }
            for wrapper in &mut slot.child_wrappers {
                annotate_template(wrapper, flows, value_environment);
            }
        }
        RenderPiece::Text(_)
        | RenderPiece::HeadContent(_)
        | RenderPiece::LoopControl(_)
        | RenderPiece::RuntimeSlotSite(_) => {}
    }
}

fn annotate_aggregate_render_plan(
    plan: &mut TemplateAggregateRenderPlan,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    for piece in &mut plan.pieces {
        match piece {
            TemplateAggregatePiece::Render(render_piece) => {
                annotate_render_piece(render_piece, flows, value_environment);
            }
            TemplateAggregatePiece::Aggregate => {}
        }
    }
}

fn annotate_runtime_slot_application(
    plan: &mut RuntimeSlotApplicationPlan,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    annotate_render_plan(&mut plan.wrapper_plan, flows, value_environment);

    for source in &mut plan.contribution_sources {
        annotate_render_plan(&mut source.render_plan, flows, value_environment);
    }

    for site in &mut plan.slot_sites {
        for piece in &mut site.render_plan.pieces {
            match piece {
                RuntimeSlotSitePiece::Render(render_piece) => {
                    annotate_render_piece(render_piece, flows, value_environment);
                }
                RuntimeSlotSitePiece::ContributionSource(_) => {}
            }
        }
    }
}

fn annotate_loop_bindings(
    bindings: &mut LoopBindings,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    if let Some(item) = &mut bindings.item {
        annotate_declaration(item, flows, value_environment);
    }
    if let Some(index) = &mut bindings.index {
        annotate_declaration(index, flows, value_environment);
    }
}

fn annotate_call_arguments(
    arguments: &mut [CallArgument],
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    for argument in arguments {
        annotate_expression(&mut argument.value, flows, value_environment);
    }
}

fn annotate_fallible_handling(
    handling: &mut FallibleHandling,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    match handling {
        FallibleHandling::Propagate => {}
        FallibleHandling::Handler { body, .. } => {
            let mut handler_environment = value_environment.clone();
            annotate_nodes(body, flows, &mut handler_environment);
        }
    }
}

fn annotate_match_pattern(
    pattern: &mut MatchPattern,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    match pattern {
        MatchPattern::Literal(value)
        | MatchPattern::OptionValue { value, .. }
        | MatchPattern::Relational { value, .. } => {
            annotate_expression(value, flows, value_environment)
        }

        MatchPattern::ChoiceVariant { .. }
        | MatchPattern::OptionNone { .. }
        | MatchPattern::Wildcard { .. }
        | MatchPattern::Capture { .. }
        | MatchPattern::OptionPresentCapture { .. } => {}
    }
}

fn annotate_value_block(
    block: &mut Box<ValueBlock>,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    match block.as_mut() {
        ValueBlock::If(value_if) => {
            annotate_expression(&mut value_if.condition, flows, value_environment);
            let mut then_environment = value_environment.clone();
            annotate_nodes(&mut value_if.then_body, flows, &mut then_environment);
            let mut else_environment = value_environment.clone();
            annotate_nodes(&mut value_if.else_body, flows, &mut else_environment);
        }
        ValueBlock::Match(value_match) => {
            annotate_expression(&mut value_match.scrutinee, flows, value_environment);
            for arm in &mut value_match.arms {
                let mut arm_environment = value_environment.clone();
                annotate_match_pattern(&mut arm.pattern, flows, &mut arm_environment);
                if let Some(guard) = &mut arm.guard {
                    annotate_expression(guard, flows, &mut arm_environment);
                }
                annotate_nodes(&mut arm.body, flows, &mut arm_environment);
            }
            if let Some(default_body) = &mut value_match.default {
                let mut default_environment = value_environment.clone();
                annotate_nodes(default_body, flows, &mut default_environment);
            }
        }
        ValueBlock::Catch(value_catch) => {
            annotate_expression(&mut value_catch.handled_value, flows, value_environment);
            annotate_fallible_handling(&mut value_catch.handler, flows, value_environment);
        }
    }
}
