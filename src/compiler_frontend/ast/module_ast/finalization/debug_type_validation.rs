//! Debug-only validation for AST TypeId boundaries.
//!
//! WHAT: walks the finalized AST payloads and asserts that every carried `TypeId` exists in the
//! final module `TypeEnvironment`.
//! WHY: finalization is the last AST-owned boundary before HIR lowering. Keeping this check local
//! to finalization catches stale or orphaned semantic type IDs early during debug builds without
//! mixing recursive validation mechanics into the final assembly code.

use crate::compiler_frontend::ast::AstChoiceDefinition;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, LoopBindings, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, FallibleHandling,
};
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::statements::value_production::types::ValueBlock;
use crate::compiler_frontend::ast::templates::template::{TemplateAtom, TemplateContent};
use crate::compiler_frontend::datatypes::definitions::{
    ChoiceVariantPayloadDefinition, TypeDefinition,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;

/// Entry point for debug TypeId validation before HIR lowering.
///
/// WHAT: recursively walks every AST node, module constant, and choice definition to verify
/// that all referenced `TypeId`s resolve to real, non-generic definitions in the module
/// `TypeEnvironment`.
/// WHY: catches frontend bugs where unresolved or stale type identities leak across the AST→HIR
/// boundary, where they would cause opaque failures in later stages.
pub(super) fn debug_validate_type_ids_for_hir(
    nodes: &[AstNode],
    module_constants: &[Declaration],
    choice_definitions: &[AstChoiceDefinition],
    type_environment: &TypeEnvironment,
) {
    for node in nodes {
        debug_validate_node_type_ids(node, type_environment);
    }

    for module_constant in module_constants {
        debug_validate_declaration_type_id(module_constant, type_environment);
    }

    // Choice definitions are collected separately from the AST node tree.
    // Validate that their variant payload fields also carry resolved TypeIds.
    for choice_definition in choice_definitions {
        let Some(nominal_id) =
            type_environment.nominal_id_for_path(&choice_definition.nominal_path)
        else {
            debug_assert!(
                false,
                "AST choice definition path is missing from TypeEnvironment"
            );
            continue;
        };

        let Some(type_id) = type_environment.type_id_for_nominal_id(nominal_id) else {
            debug_assert!(
                false,
                "AST choice definition nominal id is missing a canonical TypeId"
            );
            continue;
        };

        let Some(variants) = type_environment.variants_for(type_id) else {
            debug_assert!(
                false,
                "AST choice definition TypeId is missing variant metadata"
            );
            continue;
        };

        for variant in variants {
            if let ChoiceVariantPayloadDefinition::Record { fields } = &variant.payload {
                for field in fields {
                    debug_validate_type_id(field.type_id, type_environment, "choice field");
                }
            }
        }
    }
}

fn debug_validate_node_type_ids(node: &AstNode, type_environment: &TypeEnvironment) {
    match &node.kind {
        NodeKind::Return(values) => {
            debug_validate_expressions_type_ids(values, type_environment);
        }

        NodeKind::ReturnError(value)
        | NodeKind::PushStartRuntimeFragment(value)
        | NodeKind::Rvalue(value) => {
            debug_validate_expression_type_id(value, type_environment);
        }

        NodeKind::If(condition, then_body, else_body) => {
            debug_validate_expression_type_id(condition, type_environment);
            debug_validate_nodes_type_ids(then_body, type_environment);
            if let Some(else_body) = else_body {
                debug_validate_nodes_type_ids(else_body, type_environment);
            }
        }

        NodeKind::Assert {
            condition,
            message: _,
        } => {
            debug_validate_expression_type_id(condition, type_environment);
        }

        NodeKind::Match {
            scrutinee,
            arms,
            default,
            ..
        } => {
            debug_validate_expression_type_id(scrutinee, type_environment);
            for arm in arms {
                debug_validate_match_pattern_type_ids(&arm.pattern, type_environment);
                if let Some(guard) = &arm.guard {
                    debug_validate_expression_type_id(guard, type_environment);
                }
                debug_validate_nodes_type_ids(&arm.body, type_environment);
            }
            if let Some(default) = default {
                debug_validate_nodes_type_ids(default, type_environment);
            }
        }

        NodeKind::ScopedBlock { body } => {
            debug_validate_nodes_type_ids(body, type_environment);
        }

        NodeKind::RangeLoop {
            bindings,
            range,
            body,
        } => {
            debug_validate_loop_bindings_type_ids(bindings, type_environment);
            debug_validate_expression_type_id(&range.start, type_environment);
            debug_validate_expression_type_id(&range.end, type_environment);
            if let Some(step) = &range.step {
                debug_validate_expression_type_id(step, type_environment);
            }
            debug_validate_nodes_type_ids(body, type_environment);
        }

        NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        } => {
            debug_validate_loop_bindings_type_ids(bindings, type_environment);
            debug_validate_expression_type_id(iterable, type_environment);
            debug_validate_nodes_type_ids(body, type_environment);
        }

        NodeKind::WhileLoop(condition, body) => {
            debug_validate_expression_type_id(condition, type_environment);
            debug_validate_nodes_type_ids(body, type_environment);
        }

        NodeKind::VariableDeclaration(declaration) => {
            debug_validate_declaration_type_id(declaration, type_environment);
        }

        NodeKind::FieldAccess { base, type_id, .. } => {
            debug_validate_node_type_ids(base, type_environment);
            debug_validate_type_id(*type_id, type_environment, "field access");
        }

        NodeKind::MethodCall {
            receiver,
            args,
            result_type_ids,
            ..
        }
        | NodeKind::DynamicTraitMethodCall {
            receiver,
            args,
            result_type_ids,
            ..
        }
        | NodeKind::CollectionBuiltinCall {
            receiver,
            args,
            result_type_ids,
            ..
        }
        | NodeKind::MapBuiltinCall {
            receiver,
            args,
            result_type_ids,
            ..
        } => {
            debug_validate_node_type_ids(receiver, type_environment);
            debug_validate_call_arguments_type_ids(args, type_environment);
            debug_validate_type_ids(result_type_ids, type_environment, "call result");
        }

        NodeKind::FunctionCall {
            args,
            result_type_ids,
            ..
        }
        | NodeKind::HostFunctionCall {
            args,
            result_type_ids,
            ..
        } => {
            debug_validate_call_arguments_type_ids(args, type_environment);
            debug_validate_type_ids(result_type_ids, type_environment, "call result");
        }

        NodeKind::HandledFallibleFunctionCall {
            args,
            result_type_ids,
            handling,
            ..
        } => {
            debug_validate_call_arguments_type_ids(args, type_environment);
            debug_validate_type_ids(
                result_type_ids,
                type_environment,
                "fallible-handled call result",
            );
            debug_validate_fallible_handling_type_ids(handling, type_environment);
        }

        NodeKind::HandledFallibleHostFunctionCall {
            args,
            result_type_ids,
            error_type_id,
            handling,
            ..
        } => {
            debug_validate_call_arguments_type_ids(args, type_environment);
            debug_validate_type_ids(
                result_type_ids,
                type_environment,
                "fallible-handled external call result",
            );
            debug_validate_type_ids(&[*error_type_id], type_environment, "external call error");
            debug_validate_fallible_handling_type_ids(handling, type_environment);
        }

        NodeKind::StructDefinition(_, fields) => {
            debug_validate_declarations_type_ids(fields, type_environment);
        }

        NodeKind::Function(_, signature, body) => {
            debug_validate_declarations_type_ids(&signature.parameters, type_environment);
            debug_validate_type_ids(
                &signature.success_return_type_ids(),
                type_environment,
                "function success return",
            );
            if let Some(error_return) = signature.error_return_type_id() {
                debug_validate_type_id(error_return, type_environment, "function error return");
            }
            debug_validate_nodes_type_ids(body, type_environment);
        }

        NodeKind::Assignment { target, value } => {
            debug_validate_node_type_ids(target, type_environment);
            debug_validate_expression_type_id(value, type_environment);
        }

        NodeKind::MultiBind { targets, value } => {
            for target in targets {
                debug_validate_type_id(target.type_id, type_environment, "multi-bind target");
            }
            debug_validate_expression_type_id(value, type_environment);
        }

        // Terminal nodes that carry no type identities.
        NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => {}

        NodeKind::ThenValue(produced_values) => {
            debug_validate_expressions_type_ids(&produced_values.expressions, type_environment);
        }
    }
}

fn debug_validate_expression_type_id(expression: &Expression, type_environment: &TypeEnvironment) {
    debug_validate_type_id(expression.type_id, type_environment, "expression");

    match &expression.kind {
        ExpressionKind::Runtime(nodes) => {
            debug_validate_nodes_type_ids(nodes, type_environment);
        }

        ExpressionKind::Copy(place) => {
            debug_validate_node_type_ids(place, type_environment);
        }

        ExpressionKind::Function(signature, body) => {
            debug_validate_declarations_type_ids(&signature.parameters, type_environment);
            debug_validate_type_ids(
                &signature.success_return_type_ids(),
                type_environment,
                "function expression success return",
            );
            if let Some(error_return) = signature.error_return_type_id() {
                debug_validate_type_id(
                    error_return,
                    type_environment,
                    "function expression error return",
                );
            }
            debug_validate_nodes_type_ids(body, type_environment);
        }

        ExpressionKind::FunctionCall {
            args,
            result_type_ids,
            ..
        }
        | ExpressionKind::HostFunctionCall {
            args,
            result_type_ids,
            ..
        } => {
            debug_validate_call_arguments_type_ids(args, type_environment);
            debug_validate_type_ids(result_type_ids, type_environment, "expression call result");
        }

        ExpressionKind::HandledFallibleFunctionCall {
            args,
            result_type_ids,
            handling,
            ..
        } => {
            debug_validate_call_arguments_type_ids(args, type_environment);
            debug_validate_type_ids(
                result_type_ids,
                type_environment,
                "fallible-handled expression call result",
            );
            debug_validate_fallible_handling_type_ids(handling, type_environment);
        }

        ExpressionKind::HandledFallibleHostFunctionCall {
            args,
            result_type_ids,
            error_type_id,
            handling,
            ..
        } => {
            debug_validate_call_arguments_type_ids(args, type_environment);
            debug_validate_type_ids(
                result_type_ids,
                type_environment,
                "fallible-handled external expression call result",
            );
            debug_validate_type_ids(&[*error_type_id], type_environment, "external call error");
            debug_validate_fallible_handling_type_ids(handling, type_environment);
        }

        ExpressionKind::BuiltinCast { value, .. }
        | ExpressionKind::FallibleCarrierConstruct { value, .. }
        | ExpressionKind::OptionPropagation { value } => {
            debug_validate_expression_type_id(value, type_environment);
        }

        ExpressionKind::HandledFallibleExpression { value, handling } => {
            debug_validate_expression_type_id(value, type_environment);
            debug_validate_fallible_handling_type_ids(handling, type_environment);
        }

        ExpressionKind::Template(template) => {
            debug_validate_template_content_type_ids(&template.content, type_environment);
            debug_validate_template_content_type_ids(
                &template.unformatted_content,
                type_environment,
            );
            if let Some(render_plan) = &template.render_plan {
                for expression in render_plan.flatten_expressions() {
                    debug_validate_expression_type_id(&expression, type_environment);
                }
            }
            for child in &template.doc_children {
                debug_validate_template_content_type_ids(&child.content, type_environment);
            }
        }

        ExpressionKind::Collection(items) => {
            debug_validate_expressions_type_ids(items, type_environment);
        }

        ExpressionKind::MapLiteral(entries) => {
            for entry in entries {
                debug_validate_expression_type_id(&entry.key, type_environment);
                debug_validate_expression_type_id(&entry.value, type_environment);
            }
        }

        ExpressionKind::StructDefinition(fields) | ExpressionKind::StructInstance(fields) => {
            debug_validate_declarations_type_ids(fields, type_environment);
        }

        ExpressionKind::Range(start, end) => {
            debug_validate_expression_type_id(start, type_environment);
            debug_validate_expression_type_id(end, type_environment);
        }

        ExpressionKind::Coerced { value, to_type } => {
            debug_validate_expression_type_id(value, type_environment);
            debug_validate_type_id(*to_type, type_environment, "coercion target");
        }

        ExpressionKind::ChoiceConstruct { fields, .. } => {
            debug_validate_declarations_type_ids(fields, type_environment);
        }

        ExpressionKind::ValueBlock { block } => match block.as_ref() {
            ValueBlock::If(value_if) => {
                debug_validate_expression_type_id(&value_if.condition, type_environment);
                debug_validate_nodes_type_ids(&value_if.then_body, type_environment);
                debug_validate_nodes_type_ids(&value_if.else_body, type_environment);
            }
            ValueBlock::Match(value_match) => {
                debug_validate_expression_type_id(&value_match.scrutinee, type_environment);
                for arm in &value_match.arms {
                    if let Some(guard) = &arm.guard {
                        debug_validate_expression_type_id(guard, type_environment);
                    }
                    debug_validate_nodes_type_ids(&arm.body, type_environment);
                }
                if let Some(default_body) = &value_match.default {
                    debug_validate_nodes_type_ids(default_body, type_environment);
                }
            }
            ValueBlock::Catch(value_catch) => {
                debug_validate_expression_type_id(&value_catch.handled_value, type_environment);
            }
        },

        ExpressionKind::ConstructDynamicTraitValue { value, coercion } => {
            debug_validate_expression_type_id(value, type_environment);
            debug_validate_type_id(
                coercion.source_concrete_type_id,
                type_environment,
                "dynamic trait coercion source",
            );
            debug_validate_type_id(
                coercion.target_dynamic_trait_type_id,
                type_environment,
                "dynamic trait coercion target",
            );
        }

        // Leaf expressions that carry no nested type identities.
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
}

fn debug_validate_match_pattern_type_ids(
    pattern: &MatchPattern,
    type_environment: &TypeEnvironment,
) {
    match pattern {
        MatchPattern::Literal(expression) => {
            debug_validate_expression_type_id(expression, type_environment);
        }

        MatchPattern::OptionValue { value, .. } => {
            debug_validate_expression_type_id(value, type_environment);
        }

        MatchPattern::Relational { value, .. } => {
            debug_validate_expression_type_id(value, type_environment);
        }

        MatchPattern::ChoiceVariant { captures, .. } => {
            for capture in captures {
                debug_validate_type_id(capture.type_id, type_environment, "choice capture");
            }
        }

        // Patterns that carry no nested type identities.
        MatchPattern::OptionNone { .. }
        | MatchPattern::Wildcard { .. }
        | MatchPattern::Capture { .. }
        | MatchPattern::OptionPresentCapture { .. } => {}
    }
}

fn debug_validate_fallible_handling_type_ids(
    handling: &FallibleHandling,
    type_environment: &TypeEnvironment,
) {
    match handling {
        FallibleHandling::Handler { body, .. } => {
            debug_validate_nodes_type_ids(body, type_environment);
        }

        FallibleHandling::Propagate => {}
    }
}

fn debug_validate_template_content_type_ids(
    content: &TemplateContent,
    type_environment: &TypeEnvironment,
) {
    for atom in &content.atoms {
        if let TemplateAtom::Content(segment) = atom {
            debug_validate_expression_type_id(&segment.expression, type_environment);
        }
    }
}

fn debug_validate_loop_bindings_type_ids(
    bindings: &LoopBindings,
    type_environment: &TypeEnvironment,
) {
    if let Some(item) = &bindings.item {
        debug_validate_declaration_type_id(item, type_environment);
    }
    if let Some(index) = &bindings.index {
        debug_validate_declaration_type_id(index, type_environment);
    }
}

fn debug_validate_nodes_type_ids(nodes: &[AstNode], type_environment: &TypeEnvironment) {
    for node in nodes {
        debug_validate_node_type_ids(node, type_environment);
    }
}

fn debug_validate_expressions_type_ids(
    expressions: &[Expression],
    type_environment: &TypeEnvironment,
) {
    for expression in expressions {
        debug_validate_expression_type_id(expression, type_environment);
    }
}

fn debug_validate_declarations_type_ids(
    declarations: &[Declaration],
    type_environment: &TypeEnvironment,
) {
    for declaration in declarations {
        debug_validate_declaration_type_id(declaration, type_environment);
    }
}

fn debug_validate_declaration_type_id(
    declaration: &Declaration,
    type_environment: &TypeEnvironment,
) {
    debug_validate_expression_type_id(&declaration.value, type_environment);
}

fn debug_validate_call_arguments_type_ids(
    args: &[CallArgument],
    type_environment: &TypeEnvironment,
) {
    for argument in args {
        debug_validate_expression_type_id(&argument.value, type_environment);
    }
}

fn debug_validate_type_ids(type_ids: &[TypeId], type_environment: &TypeEnvironment, owner: &str) {
    for type_id in type_ids {
        debug_validate_type_id(*type_id, type_environment, owner);
    }
}

fn debug_validate_type_id(type_id: TypeId, type_environment: &TypeEnvironment, owner: &str) {
    let definition = type_environment.get(type_id);

    debug_assert!(
        definition.is_some(),
        "AST {owner} carried orphan TypeId({}) not registered in the final TypeEnvironment",
        type_id.0,
    );

    // Generic parameters must be fully resolved to concrete types before HIR.
    // Carrying an unresolved generic parameter across the boundary indicates a
    // frontend type-resolution bug.
    debug_assert!(
        !matches!(definition, Some(TypeDefinition::GenericParameter(..))),
        "AST {owner} carried unresolved generic parameter TypeId({}) to the HIR boundary",
        type_id.0,
    );
}
