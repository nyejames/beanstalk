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
    Expression, ExpressionKind, FallibleHandling, ReactiveTemplateMetadata,
};
use crate::compiler_frontend::ast::expressions::expression_rpn::{
    ExpressionRpnItem, PlaceExpression, PlaceExpressionKind,
};
use crate::compiler_frontend::ast::statements::functions::{FunctionSignature, ReturnChannel};
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::statements::value_production::types::ValueBlock;
use crate::compiler_frontend::ast::templates::reactive_template_metadata::{
    metadata_for_owned_runtime_slot_application_handoff,
    metadata_for_owned_runtime_template_handoff,
};
use crate::compiler_frontend::ast::templates::runtime_handoff;
use crate::compiler_frontend::ast::templates::runtime_handoff::{
    OwnedRuntimeSlotApplicationHandoff, OwnedRuntimeTemplateHandoff, OwnedRuntimeTemplateNode,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateControlFlow, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrNodeId, TemplateIrRegistry, TemplateIrStore, TemplateOverlaySet,
    TemplateOverlaySetId, TemplateTirBodyReference, TemplateTirPhase, TirExpressionOverlay,
    collect_tir_body_root_expression_overlay_payloads,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use rustc_hash::FxHashMap;

pub(super) fn annotate_nodes(
    nodes: &mut [AstNode],
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    for node in nodes {
        annotate_node(node, flows, value_environment, store, registry);
    }
}

/// Annotates expression payloads reachable from `node_id` and returns the
/// composed overlay set ID.
///
/// WHAT: collects every expression payload in the same-store TIR subtree
///       rooted at `node_id`, runs the reactive annotation pass on cloned
///       expressions, and composes the resulting overrides with the existing
///       overlay set at `current_overlay_set_id`.
/// WHY: centralizes the overlay-build logic shared by body-root annotation
///      (control-flow branches, fallback, loop bodies, aggregate wrappers)
///      and linear-template TIR-root annotation.
fn annotate_tir_root_expression_overlays(
    node_id: TemplateIrNodeId,
    current_overlay_set_id: TemplateOverlaySetId,
    phase: &mut TemplateTirPhase,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) -> Option<TemplateOverlaySetId> {
    let expression_payloads =
        match collect_tir_body_root_expression_overlay_payloads(store, node_id) {
            Ok(payloads) => payloads,
            Err(_) => return None,
        };

    if expression_payloads.is_empty() {
        if *phase < TemplateTirPhase::Composed {
            *phase = TemplateTirPhase::Composed;
        }
        return Some(current_overlay_set_id);
    }

    let mut annotated_overrides = Vec::with_capacity(expression_payloads.len());
    for (site_id, mut expression) in expression_payloads {
        annotate_expression(&mut expression, flows, value_environment, store, registry);
        annotated_overrides.push((site_id, Box::new(expression)));
    }

    let existing_overlay_set = registry
        .overlay_set(current_overlay_set_id)
        .cloned()
        .unwrap_or_default();
    let annotated_site_ids = annotated_overrides
        .iter()
        .map(|(site_id, _)| *site_id)
        .collect::<std::collections::HashSet<_>>();

    let mut overrides = if let Some(existing_overlay_id) = existing_overlay_set.expression_overrides
    {
        let existing_overlay = registry.expression_overlay(existing_overlay_id)?;
        existing_overlay
            .overrides
            .iter()
            .filter(|(site_id, _)| !annotated_site_ids.contains(site_id))
            .map(|(site_id, expression)| (*site_id, expression.clone()))
            .collect()
    } else {
        Vec::new()
    };
    overrides.extend(annotated_overrides);

    let expression_overlay_id =
        registry.allocate_expression_overlay(TirExpressionOverlay { overrides });
    let expression_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    let overlay_set_id =
        match registry.compose_overlay_sets(&[current_overlay_set_id, expression_overlay_set_id]) {
            Ok(id) => id,
            Err(_) => return None,
        };

    if *phase < TemplateTirPhase::Composed {
        *phase = TemplateTirPhase::Composed;
    }

    Some(overlay_set_id)
}

fn annotate_tir_body_reference(
    body_reference: Option<&mut TemplateTirBodyReference>,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) -> bool {
    let Some(body_reference) = body_reference else {
        return false;
    };

    let Some(root) = body_reference.same_store_root(store) else {
        return false;
    };

    let Some(new_overlay_set_id) = annotate_tir_root_expression_overlays(
        root,
        body_reference.overlay_set_id,
        &mut body_reference.phase,
        flows,
        value_environment,
        store,
        registry,
    ) else {
        return false;
    };

    body_reference.overlay_set_id = new_overlay_set_id;
    true
}

fn annotate_node(
    node: &mut AstNode,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    match &mut node.kind {
        NodeKind::Function(path, signature, body) => {
            let mut function_environment =
                ReactiveTemplateValueEnvironment::for_parameters(&signature.parameters);
            annotate_nodes(body, flows, &mut function_environment, store, registry);
            apply_flow_to_signature(path, signature, flows);
        }

        NodeKind::VariableDeclaration(declaration) => {
            annotate_declaration(declaration, flows, value_environment, store, registry);
        }

        NodeKind::Return(values) => {
            annotate_expressions(values, flows, value_environment, store, registry);
        }

        NodeKind::ReturnError(value)
        | NodeKind::PushStartRuntimeFragment(value)
        | NodeKind::ExpressionStatement(value) => {
            annotate_expression(value, flows, value_environment, store, registry);
        }

        NodeKind::ThenValue(produced_values) => {
            annotate_expressions(
                &mut produced_values.expressions,
                flows,
                value_environment,
                store,
                registry,
            );
        }

        NodeKind::If(condition, then_body, else_body) => {
            annotate_expression(condition, flows, value_environment, store, registry);
            let mut then_environment = value_environment.clone();
            annotate_nodes(then_body, flows, &mut then_environment, store, registry);
            if let Some(else_body) = else_body {
                let mut else_environment = value_environment.clone();
                annotate_nodes(else_body, flows, &mut else_environment, store, registry);
            }
        }

        NodeKind::Match {
            scrutinee,
            arms,
            default,
            ..
        } => {
            annotate_expression(scrutinee, flows, value_environment, store, registry);
            for arm in arms {
                let mut arm_environment = value_environment.clone();
                annotate_match_pattern(
                    &mut arm.pattern,
                    flows,
                    &mut arm_environment,
                    store,
                    registry,
                );
                if let Some(guard) = &mut arm.guard {
                    annotate_expression(guard, flows, &mut arm_environment, store, registry);
                }
                annotate_nodes(&mut arm.body, flows, &mut arm_environment, store, registry);
            }
            if let Some(default_body) = default {
                let mut default_environment = value_environment.clone();
                annotate_nodes(
                    default_body,
                    flows,
                    &mut default_environment,
                    store,
                    registry,
                );
            }
        }

        NodeKind::ScopedBlock { body } => {
            let mut body_environment = value_environment.clone();
            annotate_nodes(body, flows, &mut body_environment, store, registry);
        }

        NodeKind::RangeLoop {
            bindings,
            range,
            body,
        } => {
            let mut loop_environment = value_environment.clone();
            annotate_loop_bindings(bindings, flows, &mut loop_environment, store, registry);
            annotate_expression(
                &mut range.start,
                flows,
                &mut loop_environment,
                store,
                registry,
            );
            annotate_expression(
                &mut range.end,
                flows,
                &mut loop_environment,
                store,
                registry,
            );
            if let Some(step) = &mut range.step {
                annotate_expression(step, flows, &mut loop_environment, store, registry);
            }
            annotate_nodes(body, flows, &mut loop_environment, store, registry);
        }

        NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        } => {
            let mut loop_environment = value_environment.clone();
            annotate_loop_bindings(bindings, flows, &mut loop_environment, store, registry);
            annotate_expression(iterable, flows, &mut loop_environment, store, registry);
            annotate_nodes(body, flows, &mut loop_environment, store, registry);
        }

        NodeKind::WhileLoop(condition, body) => {
            annotate_expression(condition, flows, value_environment, store, registry);
            let mut body_environment = value_environment.clone();
            annotate_nodes(body, flows, &mut body_environment, store, registry);
        }

        NodeKind::Assert { condition, .. } => {
            annotate_expression(condition, flows, value_environment, store, registry);
        }

        NodeKind::StructDefinition(_, fields) => {
            for field in fields {
                annotate_declaration(field, flows, value_environment, store, registry);
            }
        }

        NodeKind::Assignment { target, value } => {
            annotate_expression(value, flows, value_environment, store, registry);
            if let Some(target_path) = reference_path_for_place_expression(target) {
                value_environment.record_assignment(target_path, value);
            }
        }

        NodeKind::MultiBind { value, .. } => {
            annotate_expression(value, flows, value_environment, store, registry);
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
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    if let ExpressionKind::Function(signature) = &mut declaration.value.kind {
        apply_flow_to_signature(&declaration.id, signature, flows);
        declaration.value.reactive_template =
            metadata_for_expression(&declaration.value, flows, value_environment, store);
        value_environment.record_declaration(declaration);
        return;
    }

    annotate_expression(
        &mut declaration.value,
        flows,
        value_environment,
        store,
        registry,
    );
    value_environment.record_declaration(declaration);
}

fn annotate_expressions(
    expressions: &mut [Expression],
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    for expression in expressions {
        annotate_expression(expression, flows, value_environment, store, registry);
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
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    match &mut expression.kind {
        ExpressionKind::Template(template) => {
            annotate_template(template, flows, value_environment, store, registry);
        }

        ExpressionKind::RuntimeTemplateHandoff(handoff) => {
            let handoff_metadata = annotate_owned_runtime_template_handoff(
                handoff,
                flows,
                value_environment,
                store,
                registry,
            );
            expression.reactive_template = Some(handoff_metadata);
        }

        ExpressionKind::RuntimeSlotApplicationHandoff(handoff) => {
            let handoff_metadata =
                annotate_runtime_slot_handoff(handoff, flows, value_environment, store, registry);
            expression.reactive_template = Some(handoff_metadata);
        }

        ExpressionKind::Function(_) => {}

        ExpressionKind::FunctionCall { args, .. }
        | ExpressionKind::HostFunctionCall { args, .. } => {
            annotate_call_arguments(args, flows, value_environment, store, registry);
        }

        ExpressionKind::FieldAccess { base, .. } => {
            annotate_expression(base, flows, value_environment, store, registry);
        }

        ExpressionKind::MethodCall { receiver, args, .. }
        | ExpressionKind::CollectionBuiltinCall { receiver, args, .. }
        | ExpressionKind::MapBuiltinCall { receiver, args, .. } => {
            annotate_expression(receiver, flows, value_environment, store, registry);
            annotate_call_arguments(args, flows, value_environment, store, registry);
        }

        ExpressionKind::HandledFallibleFunctionCall { args, .. }
        | ExpressionKind::HandledFallibleHostFunctionCall { args, .. } => {
            annotate_call_arguments(args, flows, value_environment, store, registry);
        }

        ExpressionKind::Copy(place) => {
            annotate_place_expression(place);
        }

        ExpressionKind::Runtime(rpn) => {
            for item in &mut rpn.items {
                match item {
                    ExpressionRpnItem::Operand(expression) => {
                        annotate_expression(expression, flows, value_environment, store, registry);
                    }
                    ExpressionRpnItem::Operator { .. } => {}
                }
            }
        }

        ExpressionKind::Collection(items) => {
            annotate_expressions(items, flows, value_environment, store, registry)
        }

        ExpressionKind::MapLiteral(entries) => {
            for entry in entries {
                annotate_expression(&mut entry.key, flows, value_environment, store, registry);
                annotate_expression(&mut entry.value, flows, value_environment, store, registry);
            }
        }

        ExpressionKind::StructInstance(fields)
        | ExpressionKind::StructDefinition(fields)
        | ExpressionKind::ChoiceConstruct { fields, .. } => {
            for field in fields {
                annotate_declaration(field, flows, value_environment, store, registry);
            }
        }

        ExpressionKind::Range(start, end) => {
            annotate_expression(start, flows, value_environment, store, registry);
            annotate_expression(end, flows, value_environment, store, registry);
        }

        #[cfg(test)]
        ExpressionKind::FallibleCarrierConstruct { value, .. } => {
            annotate_expression(value, flows, value_environment, store, registry);
        }

        ExpressionKind::OptionPropagation { value } | ExpressionKind::Coerced { value, .. } => {
            annotate_expression(value, flows, value_environment, store, registry);
        }

        ExpressionKind::HandledFallibleExpression { value, .. } => {
            annotate_expression(value, flows, value_environment, store, registry);
        }

        ExpressionKind::Cast(cast) => {
            annotate_expression(&mut cast.source, flows, value_environment, store, registry);
        }

        ExpressionKind::ValueBlock { block } => {
            annotate_value_block(block, flows, value_environment, store, registry)
        }

        ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Reference(_) => {}

        #[cfg(test)]
        ExpressionKind::Path(_) => {}
    }

    expression.reactive_template =
        metadata_for_expression(expression, flows, value_environment, store);
}

fn annotate_template(
    template: &mut Template,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    annotate_linear_template_tir_root(template, flows, value_environment, store, registry);
    annotate_control_flow(
        &mut template.control_flow,
        flows,
        value_environment,
        store,
        registry,
    );

    // `$children(..)` wrappers are exact registry-backed TIR references by this
    // stage. Their reactive payloads are annotated through effective TIR views
    // and overlays, so there is no recursive AST wrapper tree to walk here.
}

fn annotate_control_flow(
    control_flow: &mut Option<TemplateControlFlow>,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    let Some(control_flow) = control_flow else {
        return;
    };

    match control_flow {
        TemplateControlFlow::BranchChain(branch_chain) => {
            for branch in &mut branch_chain.branches {
                let mut branch_environment = value_environment.clone();
                annotate_branch_selector(
                    &mut branch.selector,
                    flows,
                    &mut branch_environment,
                    store,
                    registry,
                );
                annotate_tir_body_reference(
                    branch
                        .body_tir_reference
                        .as_mut()
                        .map(|reference| reference.body_reference_mut()),
                    flows,
                    &mut branch_environment,
                    store,
                    registry,
                );
            }

            if let Some(fallback) = &mut branch_chain.fallback {
                let mut fallback_environment = value_environment.clone();
                annotate_tir_body_reference(
                    fallback
                        .body_tir_reference
                        .as_mut()
                        .map(|reference| reference.body_reference_mut()),
                    flows,
                    &mut fallback_environment,
                    store,
                    registry,
                );
            }
        }

        TemplateControlFlow::Loop(template_loop) => {
            let mut loop_environment = value_environment.clone();
            annotate_loop_header(
                &mut template_loop.header,
                flows,
                &mut loop_environment,
                store,
                registry,
            );
            annotate_tir_body_reference(
                template_loop
                    .body_tir_reference
                    .as_mut()
                    .map(|reference| reference.body_reference_mut()),
                flows,
                &mut loop_environment,
                store,
                registry,
            );

            // Render-unit preparation caches the composed aggregate-wrapper
            // subtree on the AST loop. Annotate that authoritative TIR root;
            // normalization reports the broken invariant if it is absent.
            annotate_tir_body_reference(
                template_loop
                    .aggregate_wrapper_tir_reference
                    .as_mut()
                    .map(|reference| reference.body_reference_mut()),
                flows,
                &mut loop_environment,
                store,
                registry,
            );
        }
    }
}

fn annotate_branch_selector(
    selector: &mut TemplateBranchSelector,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    match selector {
        TemplateBranchSelector::Bool(condition) => {
            annotate_expression(condition, flows, value_environment, store, registry)
        }
        TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern } => {
            annotate_expression(scrutinee, flows, value_environment, store, registry);
            annotate_match_pattern(pattern, flows, value_environment, store, registry);
        }
    }
}

fn annotate_loop_header(
    header: &mut TemplateLoopHeader,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    match header {
        TemplateLoopHeader::Conditional { condition } => {
            annotate_expression(condition, flows, value_environment, store, registry)
        }
        TemplateLoopHeader::Range { bindings, range } => {
            annotate_loop_bindings(bindings, flows, value_environment, store, registry);
            annotate_expression(&mut range.start, flows, value_environment, store, registry);
            annotate_expression(&mut range.end, flows, value_environment, store, registry);
            if let Some(step) = &mut range.step {
                annotate_expression(step, flows, value_environment, store, registry);
            }
        }
        TemplateLoopHeader::Collection { bindings, iterable } => {
            annotate_loop_bindings(bindings, flows, value_environment, store, registry);
            annotate_expression(iterable, flows, value_environment, store, registry);
        }
    }
}

fn annotate_runtime_slot_handoff(
    handoff: &mut OwnedRuntimeSlotApplicationHandoff,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) -> ReactiveTemplateMetadata {
    let result = runtime_handoff::walk_owned_runtime_slot_application_handoff_mut(
        handoff,
        &mut |event| -> Result<(), std::convert::Infallible> {
            annotate_owned_runtime_template_handoff_event(
                event,
                flows,
                value_environment,
                store,
                registry,
            );
            Ok(())
        },
    );
    let Ok(()) = result;

    // After annotating nested expression payloads, compute the handoff's own
    // reactive template metadata from its structural shape so HIR can bind the
    // runtime slot application's reactive dependencies.
    metadata_for_owned_runtime_slot_application_handoff(handoff, &mut |expression| {
        expression.reactive_template.clone()
    })
}

fn annotate_owned_runtime_template_handoff(
    handoff: &mut OwnedRuntimeTemplateHandoff,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) -> ReactiveTemplateMetadata {
    let result =
        runtime_handoff::walk_owned_runtime_template_handoff_mut(handoff, &mut |event| -> Result<
            (),
            std::convert::Infallible,
        > {
            annotate_owned_runtime_template_handoff_event(
                event,
                flows,
                value_environment,
                store,
                registry,
            );
            Ok(())
        });
    let Ok(()) = result;

    // After annotating nested expression payloads, compute the handoff's own
    // reactive template metadata from its structural shape so HIR can bind the
    // runtime template's reactive dependencies.
    metadata_for_owned_runtime_template_handoff(handoff, &mut |expression| {
        expression.reactive_template.clone()
    })
}

fn annotate_owned_runtime_template_handoff_event(
    event: runtime_handoff::OwnedRuntimeTemplateWalkMutEvent<'_>,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    match event {
        runtime_handoff::OwnedRuntimeTemplateWalkMutEvent::Node(node) => {
            annotate_owned_runtime_template_node(node, flows, value_environment, store, registry);
        }

        runtime_handoff::OwnedRuntimeTemplateWalkMutEvent::HandoffAfterBody(_handoff) => {
            // `Style` no longer stores recursive wrapper templates; they are owned
            // by `Template.child_wrappers` and visited through normal template
            // recursion. There is nothing to annotate at the handoff boundary.
        }
    }
}

fn annotate_owned_runtime_template_node(
    node: &mut OwnedRuntimeTemplateNode,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    match node {
        OwnedRuntimeTemplateNode::DynamicExpression { expression, .. } => {
            annotate_expression(expression, flows, value_environment, store, registry);
        }

        OwnedRuntimeTemplateNode::BranchChain { branches, .. } => {
            for branch in branches {
                annotate_branch_selector(
                    &mut branch.selector,
                    flows,
                    value_environment,
                    store,
                    registry,
                );
            }
        }

        OwnedRuntimeTemplateNode::Loop { header, .. } => {
            annotate_loop_header(header, flows, value_environment, store, registry);
        }

        OwnedRuntimeTemplateNode::Sequence { .. }
        | OwnedRuntimeTemplateNode::ChildTemplate { .. }
        | OwnedRuntimeTemplateNode::ConditionalWrapper { .. }
        | OwnedRuntimeTemplateNode::Text { .. }
        | OwnedRuntimeTemplateNode::AggregateOutput { .. }
        | OwnedRuntimeTemplateNode::LoopControl { .. }
        | OwnedRuntimeTemplateNode::RuntimeSlotSite { .. }
        | OwnedRuntimeTemplateNode::Slot { .. } => {}
    }
}

fn annotate_loop_bindings(
    bindings: &mut LoopBindings,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    if let Some(item) = &mut bindings.item {
        annotate_declaration(item, flows, value_environment, store, registry);
    }
    if let Some(index) = &mut bindings.index {
        annotate_declaration(index, flows, value_environment, store, registry);
    }
}

fn annotate_call_arguments(
    arguments: &mut [CallArgument],
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    for argument in arguments {
        annotate_expression(
            &mut argument.value,
            flows,
            value_environment,
            store,
            registry,
        );
    }
}

fn annotate_fallible_handling(
    handling: &mut FallibleHandling,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    match handling {
        FallibleHandling::Propagate => {}
        FallibleHandling::Handler { body, .. } => {
            let mut handler_environment = value_environment.clone();
            annotate_nodes(body, flows, &mut handler_environment, store, registry);
        }
    }
}

fn annotate_match_pattern(
    pattern: &mut MatchPattern,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    match pattern {
        MatchPattern::Literal(value)
        | MatchPattern::OptionValue { value, .. }
        | MatchPattern::Relational { value, .. } => {
            annotate_expression(value, flows, value_environment, store, registry)
        }

        MatchPattern::ChoiceVariant { .. }
        | MatchPattern::OptionNone { .. }
        | MatchPattern::Capture { .. }
        | MatchPattern::OptionPresentCapture { .. } => {}

        #[cfg(test)]
        MatchPattern::Wildcard { .. } => {}
    }
}

fn annotate_value_block(
    block: &mut Box<ValueBlock>,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    match block.as_mut() {
        ValueBlock::If(value_if) => {
            annotate_expression(
                &mut value_if.condition,
                flows,
                value_environment,
                store,
                registry,
            );
            let mut then_environment = value_environment.clone();
            annotate_nodes(
                &mut value_if.then_body,
                flows,
                &mut then_environment,
                store,
                registry,
            );
            let mut else_environment = value_environment.clone();
            annotate_nodes(
                &mut value_if.else_body,
                flows,
                &mut else_environment,
                store,
                registry,
            );
        }
        ValueBlock::Match(value_match) => {
            annotate_expression(
                &mut value_match.scrutinee,
                flows,
                value_environment,
                store,
                registry,
            );
            for arm in &mut value_match.arms {
                let mut arm_environment = value_environment.clone();
                annotate_match_pattern(
                    &mut arm.pattern,
                    flows,
                    &mut arm_environment,
                    store,
                    registry,
                );
                if let Some(guard) = &mut arm.guard {
                    annotate_expression(guard, flows, &mut arm_environment, store, registry);
                }
                annotate_nodes(&mut arm.body, flows, &mut arm_environment, store, registry);
            }
            if let Some(default_body) = &mut value_match.default {
                let mut default_environment = value_environment.clone();
                annotate_nodes(
                    default_body,
                    flows,
                    &mut default_environment,
                    store,
                    registry,
                );
            }
        }
        ValueBlock::Catch(value_catch) => {
            annotate_expression(
                &mut value_catch.handled_value,
                flows,
                value_environment,
                store,
                registry,
            );
            annotate_fallible_handling(
                &mut value_catch.handler,
                flows,
                value_environment,
                store,
                registry,
            );
        }
    }
}

/// Annotates a linear template's same-store TIR root when it is authoritative.
///
/// WHAT: for linear templates with a same-store TIR root at `Composed` phase
///       or later, collects and annotates every expression payload in that TIR
///       subtree through expression overlays.
/// WHY: keeps the mutating annotation pass aligned with the TIR-authoritative
///      read-side metadata collection. Missing TIR authority leaves no semantic
///      payload to recover from the now-empty compatibility carrier.
fn annotate_linear_template_tir_root(
    template: &mut Template,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &mut ReactiveTemplateValueEnvironment,
    store: &mut TemplateIrStore,
    registry: &mut TemplateIrRegistry,
) {
    if template.control_flow.is_some() {
        return;
    }

    let Some(reference) = template.tir_reference.as_mut() else {
        return;
    };

    if !reference.phase.is_at_least(TemplateTirPhase::Composed) {
        return;
    }

    let store_owner = store.owner();
    if !std::sync::Arc::ptr_eq(&reference.store_owner, &store_owner) {
        return;
    }

    if reference.root.store_id != store.store_id() {
        return;
    }

    let root = match store.get_template(reference.root.template_id) {
        Some(tir_template) => tir_template.root,
        None => return,
    };

    let Some(new_overlay_set_id) = annotate_tir_root_expression_overlays(
        root,
        reference.overlay_set_id,
        &mut reference.phase,
        flows,
        value_environment,
        store,
        registry,
    ) else {
        return;
    };

    reference.overlay_set_id = new_overlay_set_id;
}
