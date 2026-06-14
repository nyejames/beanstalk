//! Reactive template metadata propagation inside the finalized AST.
//!
//! WHAT: computes and attaches value-level metadata for template-backed `String` values after
//! function bodies have been emitted but before templates are normalized for HIR.
//! WHY: top-level function signatures are resolved before bodies are parsed, so return metadata
//! must be derived from the completed AST rather than from declaration syntax. This pass keeps the
//! propagation direct: assignments and references carry expression metadata, functions substitute
//! direct call arguments, and ordinary string operations remain plain snapshots.

use super::finalizer::AstFinalizer;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, LoopBindings, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, FallibleHandling, ReactiveTemplateMetadata,
};
use crate::compiler_frontend::ast::expressions::expression_types::CastHandling;
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

#[derive(Clone, Debug)]
struct FunctionTemplateFlow {
    parameters: Vec<Declaration>,
    success_returns: Vec<Option<ReactiveTemplateMetadata>>,
}

#[derive(Clone, Debug, Default)]
struct ReactiveTemplateValueEnvironment {
    values: FxHashMap<InternedPath, Option<ReactiveTemplateMetadata>>,
}

impl ReactiveTemplateValueEnvironment {
    fn for_parameters(parameters: &[Declaration]) -> Self {
        let mut environment = Self::default();

        for parameter in parameters {
            environment.record_declaration(parameter);
        }

        environment
    }

    fn record_declaration(&mut self, declaration: &Declaration) {
        self.values.insert(
            declaration.id.clone(),
            declaration.value.reactive_template.clone(),
        );
    }

    fn record_assignment(&mut self, path: &InternedPath, value: &Expression) {
        self.values
            .insert(path.clone(), value.reactive_template.clone());
    }

    fn metadata_for_path(&self, path: &InternedPath) -> Option<ReactiveTemplateMetadata> {
        self.values.get(path).cloned().flatten()
    }
}

impl PartialEq for FunctionTemplateFlow {
    fn eq(&self, other: &Self) -> bool {
        self.success_returns == other.success_returns
    }
}

impl Eq for FunctionTemplateFlow {}

impl AstFinalizer<'_, '_> {
    /// Propagate direct reactive-template value metadata through the emitted AST.
    pub(super) fn propagate_reactive_template_metadata(&self, ast: &mut [AstNode]) {
        propagate_reactive_template_metadata_in_ast(ast);
    }
}

pub(crate) fn propagate_reactive_template_metadata_in_ast(ast: &mut [AstNode]) {
    let mut function_flows = initialize_function_template_flows(ast);

    // Function return metadata can depend on direct calls to other functions. A tiny fixed point
    // handles helper chains without introducing a general expression dependency graph.
    loop {
        let mut next_flows = function_flows.clone();
        refresh_function_template_flows(ast, &function_flows, &mut next_flows);

        if next_flows == function_flows {
            break;
        }

        function_flows = next_flows;
    }

    let mut value_environment = ReactiveTemplateValueEnvironment::default();
    annotate_nodes(ast, &function_flows, &mut value_environment);
}

fn initialize_function_template_flows(
    ast: &[AstNode],
) -> FxHashMap<InternedPath, FunctionTemplateFlow> {
    let mut flows = FxHashMap::default();
    collect_initial_function_flows_from_nodes(ast, &mut flows);
    flows
}

fn collect_initial_function_flows_from_nodes(
    nodes: &[AstNode],
    flows: &mut FxHashMap<InternedPath, FunctionTemplateFlow>,
) {
    for node in nodes {
        collect_initial_function_flows_from_node(node, flows);
    }
}

fn collect_initial_function_flows_from_node(
    node: &AstNode,
    flows: &mut FxHashMap<InternedPath, FunctionTemplateFlow>,
) {
    match &node.kind {
        NodeKind::Function(path, signature, body) => {
            flows.insert(path.clone(), empty_flow_for_signature(signature));
            collect_initial_function_flows_from_nodes(body, flows);
        }

        NodeKind::VariableDeclaration(declaration) => {
            collect_initial_function_flows_from_declaration(declaration, flows);
        }

        NodeKind::If(_, then_body, else_body) => {
            collect_initial_function_flows_from_nodes(then_body, flows);
            if let Some(else_body) = else_body {
                collect_initial_function_flows_from_nodes(else_body, flows);
            }
        }

        NodeKind::Match { arms, default, .. } => {
            for arm in arms {
                collect_initial_function_flows_from_nodes(&arm.body, flows);
            }
            if let Some(default_body) = default {
                collect_initial_function_flows_from_nodes(default_body, flows);
            }
        }

        NodeKind::ScopedBlock { body }
        | NodeKind::RangeLoop { body, .. }
        | NodeKind::CollectionLoop { body, .. }
        | NodeKind::WhileLoop(_, body) => {
            collect_initial_function_flows_from_nodes(body, flows);
        }

        NodeKind::ThenValue(_)
        | NodeKind::Return(_)
        | NodeKind::ReturnError(_)
        | NodeKind::Assert { .. }
        | NodeKind::PushStartRuntimeFragment(_)
        | NodeKind::FieldAccess { .. }
        | NodeKind::MethodCall { .. }
        | NodeKind::CollectionBuiltinCall { .. }
        | NodeKind::MapBuiltinCall { .. }
        | NodeKind::FunctionCall { .. }
        | NodeKind::HandledFallibleFunctionCall { .. }
        | NodeKind::HandledFallibleHostFunctionCall { .. }
        | NodeKind::HostFunctionCall { .. }
        | NodeKind::StructDefinition(_, _)
        | NodeKind::Assignment { .. }
        | NodeKind::MultiBind { .. }
        | NodeKind::Rvalue(_)
        | NodeKind::Break
        | NodeKind::Continue
        | NodeKind::Operator(_) => {}
    }
}

fn collect_initial_function_flows_from_declaration(
    declaration: &Declaration,
    flows: &mut FxHashMap<InternedPath, FunctionTemplateFlow>,
) {
    if let ExpressionKind::Function(signature, body) = &declaration.value.kind {
        flows.insert(declaration.id.clone(), empty_flow_for_signature(signature));
        collect_initial_function_flows_from_nodes(body, flows);
    }
}

fn empty_flow_for_signature(signature: &FunctionSignature) -> FunctionTemplateFlow {
    FunctionTemplateFlow {
        parameters: signature.parameters.clone(),
        success_returns: signature
            .returns
            .iter()
            .filter(|slot| slot.channel == ReturnChannel::Success)
            .map(|slot| slot.reactive_template.clone())
            .collect(),
    }
}

fn refresh_function_template_flows(
    ast: &[AstNode],
    current_flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    next_flows: &mut FxHashMap<InternedPath, FunctionTemplateFlow>,
) {
    for node in ast {
        refresh_function_template_flows_from_node(node, current_flows, next_flows);
    }
}

fn refresh_function_template_flows_from_node(
    node: &AstNode,
    current_flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    next_flows: &mut FxHashMap<InternedPath, FunctionTemplateFlow>,
) {
    match &node.kind {
        NodeKind::Function(path, signature, body) => {
            let returns = collect_return_metadata(body, signature, current_flows);
            if let Some(flow) = next_flows.get_mut(path) {
                flow.success_returns = returns;
            }
            refresh_function_template_flows(body, current_flows, next_flows);
        }

        NodeKind::VariableDeclaration(declaration) => {
            if let ExpressionKind::Function(signature, body) = &declaration.value.kind {
                let returns = collect_return_metadata(body, signature, current_flows);
                if let Some(flow) = next_flows.get_mut(&declaration.id) {
                    flow.success_returns = returns;
                }
                refresh_function_template_flows(body, current_flows, next_flows);
            }
        }

        NodeKind::If(_, then_body, else_body) => {
            refresh_function_template_flows(then_body, current_flows, next_flows);
            if let Some(else_body) = else_body {
                refresh_function_template_flows(else_body, current_flows, next_flows);
            }
        }

        NodeKind::Match { arms, default, .. } => {
            for arm in arms {
                refresh_function_template_flows(&arm.body, current_flows, next_flows);
            }
            if let Some(default_body) = default {
                refresh_function_template_flows(default_body, current_flows, next_flows);
            }
        }

        NodeKind::ScopedBlock { body }
        | NodeKind::RangeLoop { body, .. }
        | NodeKind::CollectionLoop { body, .. }
        | NodeKind::WhileLoop(_, body) => {
            refresh_function_template_flows(body, current_flows, next_flows);
        }

        _ => {}
    }
}

fn collect_return_metadata(
    body: &[AstNode],
    signature: &FunctionSignature,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
) -> Vec<Option<ReactiveTemplateMetadata>> {
    let mut returns: Vec<Option<ReactiveTemplateMetadata>> = signature
        .returns
        .iter()
        .filter(|slot| slot.channel == ReturnChannel::Success)
        .map(|slot| slot.reactive_template.clone())
        .collect();

    let mut value_environment =
        ReactiveTemplateValueEnvironment::for_parameters(&signature.parameters);
    collect_return_metadata_from_nodes(body, flows, &mut returns, &mut value_environment);
    returns
}

fn collect_return_metadata_from_nodes(
    nodes: &[AstNode],
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    returns: &mut [Option<ReactiveTemplateMetadata>],
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    for node in nodes {
        collect_return_metadata_from_node(node, flows, returns, value_environment);
    }
}

fn collect_return_metadata_from_node(
    node: &AstNode,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    returns: &mut [Option<ReactiveTemplateMetadata>],
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    match &node.kind {
        NodeKind::Return(values) => {
            for (index, value) in values.iter().enumerate() {
                let Some(slot) = returns.get_mut(index) else {
                    continue;
                };
                merge_optional_metadata(
                    slot,
                    metadata_for_expression(value, flows, value_environment),
                );
            }
        }

        NodeKind::VariableDeclaration(declaration) => {
            let mut resolved_declaration = declaration.clone();
            resolved_declaration.value.reactive_template =
                metadata_for_expression(&declaration.value, flows, value_environment);
            value_environment.record_declaration(&resolved_declaration);
        }

        NodeKind::Assignment { target, value } => {
            if let Some(target_path) = reference_path_for_node(target) {
                let mut resolved_value = value.clone();
                resolved_value.reactive_template =
                    metadata_for_expression(value, flows, value_environment);
                value_environment.record_assignment(target_path, &resolved_value);
            }
        }

        NodeKind::If(_, then_body, else_body) => {
            let mut then_environment = value_environment.clone();
            collect_return_metadata_from_nodes(then_body, flows, returns, &mut then_environment);
            if let Some(else_body) = else_body {
                let mut else_environment = value_environment.clone();
                collect_return_metadata_from_nodes(
                    else_body,
                    flows,
                    returns,
                    &mut else_environment,
                );
            }
        }

        NodeKind::Match { arms, default, .. } => {
            for arm in arms {
                let mut arm_environment = value_environment.clone();
                collect_return_metadata_from_nodes(&arm.body, flows, returns, &mut arm_environment);
            }
            if let Some(default_body) = default {
                let mut default_environment = value_environment.clone();
                collect_return_metadata_from_nodes(
                    default_body,
                    flows,
                    returns,
                    &mut default_environment,
                );
            }
        }

        NodeKind::ScopedBlock { body }
        | NodeKind::RangeLoop { body, .. }
        | NodeKind::CollectionLoop { body, .. }
        | NodeKind::WhileLoop(_, body) => {
            let mut body_environment = value_environment.clone();
            collect_return_metadata_from_nodes(body, flows, returns, &mut body_environment);
        }

        _ => {}
    }
}

fn metadata_for_expression(
    expression: &Expression,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
) -> Option<ReactiveTemplateMetadata> {
    match &expression.kind {
        ExpressionKind::Template(template) => {
            metadata_for_template(template, flows, value_environment)
        }

        ExpressionKind::FunctionCall { name, args, .. }
        | ExpressionKind::HandledFallibleFunctionCall { name, args, .. } => {
            metadata_for_function_call(name, args, flows, value_environment)
        }

        ExpressionKind::Coerced { value, .. } => {
            metadata_for_expression(value, flows, value_environment)
                .or_else(|| expression.reactive_template.clone())
        }

        ExpressionKind::Reference(path) => value_environment
            .metadata_for_path(path)
            .or_else(|| expression.reactive_template.clone()),

        _ => expression.reactive_template.clone(),
    }
}

fn metadata_for_function_call(
    name: &InternedPath,
    arguments: &[CallArgument],
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
) -> Option<ReactiveTemplateMetadata> {
    let flow = flows.get(name)?;
    let metadata = flow.success_returns.first()?.as_ref()?;
    let resolved_arguments = arguments
        .iter()
        .map(|argument| {
            let mut resolved_argument = argument.clone();
            resolved_argument.value.reactive_template =
                metadata_for_expression(&argument.value, flows, value_environment);
            resolved_argument
        })
        .collect::<Vec<_>>();

    metadata.instantiate_for_call(&flow.parameters, &resolved_arguments)
}

fn metadata_for_template(
    template: &Template,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
) -> Option<ReactiveTemplateMetadata> {
    let mut metadata = ReactiveTemplateMetadata::template_backed();

    merge_content_metadata(&template.content, flows, value_environment, &mut metadata);
    merge_control_flow_metadata(
        &template.control_flow,
        flows,
        value_environment,
        &mut metadata,
    );

    if let Some(plan) = &template.runtime_slot_application {
        merge_runtime_slot_application_metadata(plan, flows, value_environment, &mut metadata);
    }

    Some(metadata)
}

fn merge_content_metadata(
    content: &TemplateContent,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
    metadata: &mut ReactiveTemplateMetadata,
) {
    for atom in &content.atoms {
        match atom {
            TemplateAtom::Content(segment) => {
                if let Some(subscription) = &segment.reactive_subscription {
                    metadata.push_subscription(subscription.clone());
                }
                if let Some(expression_metadata) =
                    metadata_for_expression(&segment.expression, flows, value_environment)
                {
                    metadata.merge_from(&expression_metadata);
                }
            }

            TemplateAtom::Slot(_) => {}
        }
    }
}

fn merge_control_flow_metadata(
    control_flow: &Option<TemplateControlFlow>,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
    metadata: &mut ReactiveTemplateMetadata,
) {
    let Some(control_flow) = control_flow else {
        return;
    };

    match control_flow {
        TemplateControlFlow::BranchChain(branch_chain) => {
            for branch in &branch_chain.branches {
                merge_branch_selector_metadata(
                    &branch.selector,
                    flows,
                    value_environment,
                    metadata,
                );
                merge_content_metadata(&branch.content, flows, value_environment, metadata);
                if let Some(render_plan) = &branch.render_plan {
                    merge_render_plan_metadata(render_plan, flows, value_environment, metadata);
                }
            }

            if let Some(fallback) = &branch_chain.fallback {
                merge_content_metadata(&fallback.content, flows, value_environment, metadata);
                if let Some(render_plan) = &fallback.render_plan {
                    merge_render_plan_metadata(render_plan, flows, value_environment, metadata);
                }
            }
        }

        TemplateControlFlow::Loop(template_loop) => {
            merge_loop_header_metadata(&template_loop.header, flows, value_environment, metadata);
            merge_content_metadata(
                &template_loop.body_content,
                flows,
                value_environment,
                metadata,
            );
            if let Some(render_plan) = &template_loop.body_render_plan {
                merge_render_plan_metadata(render_plan, flows, value_environment, metadata);
            }
            if let Some(aggregate_plan) = &template_loop.aggregate_render_plan {
                merge_aggregate_render_plan_metadata(
                    aggregate_plan,
                    flows,
                    value_environment,
                    metadata,
                );
            }
        }

        TemplateControlFlow::LoopControl(_) => {}
    }
}

fn merge_branch_selector_metadata(
    selector: &TemplateBranchSelector,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
    metadata: &mut ReactiveTemplateMetadata,
) {
    match selector {
        TemplateBranchSelector::Bool(condition) => {
            if let Some(condition_metadata) =
                metadata_for_expression(condition, flows, value_environment)
            {
                metadata.merge_from(&condition_metadata);
            }
        }

        TemplateBranchSelector::OptionPresentCapture { scrutinee, .. } => {
            if let Some(scrutinee_metadata) =
                metadata_for_expression(scrutinee, flows, value_environment)
            {
                metadata.merge_from(&scrutinee_metadata);
            }
        }
    }
}

fn merge_loop_header_metadata(
    header: &TemplateLoopHeader,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
    metadata: &mut ReactiveTemplateMetadata,
) {
    match header {
        TemplateLoopHeader::Conditional { condition } => {
            if let Some(condition_metadata) =
                metadata_for_expression(condition, flows, value_environment)
            {
                metadata.merge_from(&condition_metadata);
            }
        }

        TemplateLoopHeader::Range { range, .. } => {
            if let Some(start_metadata) =
                metadata_for_expression(&range.start, flows, value_environment)
            {
                metadata.merge_from(&start_metadata);
            }
            if let Some(end_metadata) =
                metadata_for_expression(&range.end, flows, value_environment)
            {
                metadata.merge_from(&end_metadata);
            }
            if let Some(step) = &range.step
                && let Some(step_metadata) = metadata_for_expression(step, flows, value_environment)
            {
                metadata.merge_from(&step_metadata);
            }
        }

        TemplateLoopHeader::Collection { iterable, .. } => {
            if let Some(iterable_metadata) =
                metadata_for_expression(iterable, flows, value_environment)
            {
                metadata.merge_from(&iterable_metadata);
            }
        }
    }
}

fn merge_runtime_slot_application_metadata(
    plan: &RuntimeSlotApplicationPlan,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
    metadata: &mut ReactiveTemplateMetadata,
) {
    merge_render_plan_metadata(&plan.wrapper_plan, flows, value_environment, metadata);

    for source in &plan.contribution_sources {
        merge_render_plan_metadata(&source.render_plan, flows, value_environment, metadata);
    }

    for site in &plan.slot_sites {
        for piece in &site.render_plan.pieces {
            match piece {
                RuntimeSlotSitePiece::Render(render_piece) => {
                    merge_render_piece_metadata(render_piece, flows, value_environment, metadata);
                }

                RuntimeSlotSitePiece::ContributionSource(_) => {}
            }
        }
    }
}

fn merge_aggregate_render_plan_metadata(
    plan: &TemplateAggregateRenderPlan,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
    metadata: &mut ReactiveTemplateMetadata,
) {
    for piece in &plan.pieces {
        match piece {
            TemplateAggregatePiece::Render(render_piece) => {
                merge_render_piece_metadata(render_piece, flows, value_environment, metadata);
            }

            TemplateAggregatePiece::Aggregate => {}
        }
    }
}

fn merge_render_plan_metadata(
    plan: &TemplateRenderPlan,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
    metadata: &mut ReactiveTemplateMetadata,
) {
    for piece in &plan.pieces {
        merge_render_piece_metadata(piece, flows, value_environment, metadata);
    }
}

fn merge_render_piece_metadata(
    piece: &RenderPiece,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
    metadata: &mut ReactiveTemplateMetadata,
) {
    match piece {
        RenderPiece::DynamicExpression(dynamic) => {
            if let Some(subscription) = &dynamic.reactive_subscription {
                metadata.push_subscription(subscription.clone());
            }
            if let Some(expression_metadata) =
                metadata_for_expression(&dynamic.expression, flows, value_environment)
            {
                metadata.merge_from(&expression_metadata);
            }
        }

        RenderPiece::ChildTemplate(child) => {
            if let Some(expression_metadata) =
                metadata_for_expression(&child.expression, flows, value_environment)
            {
                metadata.merge_from(&expression_metadata);
            }
        }

        RenderPiece::Text(_)
        | RenderPiece::HeadContent(_)
        | RenderPiece::LoopControl(_)
        | RenderPiece::Slot(_)
        | RenderPiece::RuntimeSlotSite(_) => {}
    }
}

fn annotate_nodes(
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
        | NodeKind::Rvalue(value) => {
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

        NodeKind::FieldAccess { base, .. } => annotate_node(base, flows, value_environment),

        NodeKind::MethodCall { receiver, args, .. }
        | NodeKind::CollectionBuiltinCall { receiver, args, .. }
        | NodeKind::MapBuiltinCall { receiver, args, .. } => {
            annotate_node(receiver, flows, value_environment);
            annotate_call_arguments(args, flows, value_environment);
        }

        NodeKind::FunctionCall { args, .. } | NodeKind::HostFunctionCall { args, .. } => {
            annotate_call_arguments(args, flows, value_environment);
        }

        NodeKind::HandledFallibleFunctionCall { args, handling, .. }
        | NodeKind::HandledFallibleHostFunctionCall { args, handling, .. } => {
            annotate_call_arguments(args, flows, value_environment);
            annotate_fallible_handling(handling, flows, value_environment);
        }

        NodeKind::StructDefinition(_, fields) => {
            for field in fields {
                annotate_declaration(field, flows, value_environment);
            }
        }

        NodeKind::Assignment { target, value } => {
            annotate_node(target, flows, value_environment);
            annotate_expression(value, flows, value_environment);
            if let Some(target_path) = reference_path_for_node(target) {
                value_environment.record_assignment(target_path, value);
            }
        }

        NodeKind::MultiBind { value, .. } => {
            annotate_expression(value, flows, value_environment);
        }

        NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => {}
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
    if let ExpressionKind::Function(signature, body) = &mut declaration.value.kind {
        let mut function_environment =
            ReactiveTemplateValueEnvironment::for_parameters(&signature.parameters);
        annotate_nodes(body, flows, &mut function_environment);
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

fn annotate_expression(
    expression: &mut Expression,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
) {
    match &mut expression.kind {
        ExpressionKind::Template(template) => {
            annotate_template(template, flows, value_environment);
        }

        ExpressionKind::Function(signature, body) => {
            let mut function_environment =
                ReactiveTemplateValueEnvironment::for_parameters(&signature.parameters);
            annotate_nodes(body, flows, &mut function_environment);
        }

        ExpressionKind::FunctionCall { args, .. }
        | ExpressionKind::HostFunctionCall { args, .. } => {
            annotate_call_arguments(args, flows, value_environment);
        }

        ExpressionKind::HandledFallibleFunctionCall { args, handling, .. }
        | ExpressionKind::HandledFallibleHostFunctionCall { args, handling, .. } => {
            annotate_call_arguments(args, flows, value_environment);
            annotate_fallible_handling(handling, flows, value_environment);
        }

        ExpressionKind::Copy(place) => annotate_node(place, flows, value_environment),

        ExpressionKind::Runtime(nodes) => annotate_nodes(nodes, flows, value_environment),

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

        ExpressionKind::HandledFallibleExpression { value, handling } => {
            annotate_expression(value, flows, value_environment);
            annotate_fallible_handling(handling, flows, value_environment);
        }

        ExpressionKind::Cast(cast) => {
            annotate_expression(&mut cast.source, flows, value_environment);
            if let CastHandling::Recover(handling) = &mut cast.handling {
                annotate_fallible_handling(handling, flows, value_environment);
            }
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
        }
    }
}

fn merge_optional_metadata(
    target: &mut Option<ReactiveTemplateMetadata>,
    source: Option<ReactiveTemplateMetadata>,
) {
    let Some(source) = source else {
        return;
    };

    match target {
        Some(existing) => existing.merge_from(&source),
        None => *target = Some(source),
    }
}

fn reference_path_for_node(node: &AstNode) -> Option<&InternedPath> {
    let NodeKind::Rvalue(expression) = &node.kind else {
        return None;
    };

    let ExpressionKind::Reference(path) = &expression.kind else {
        return None;
    };

    Some(path)
}

#[cfg(test)]
#[path = "tests/reactive_templates_tests.rs"]
mod reactive_templates_tests;
