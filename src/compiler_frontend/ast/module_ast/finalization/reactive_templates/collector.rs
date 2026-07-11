//! Flow-aware reactive template metadata collection.
//!
//! WHAT: computes reactive template metadata for expressions and function calls
//! using the current function flow map and value environment. Template structure
//! is traversed by the template-owned helper in `templates::reactive_template_metadata`;
//! this module supplies a flow-aware expression resolver.
//! WHY: this lookup is read-only with respect to the AST; the separate
//! `annotation` phase applies the collected metadata back to the tree.

use super::types::{FunctionTemplateFlow, ReactiveTemplateValueEnvironment};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ReactiveTemplateMetadata,
};
use crate::compiler_frontend::ast::templates::reactive_template_metadata;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::TemplateIrStore;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use rustc_hash::FxHashMap;

pub(super) fn metadata_for_expression(
    expression: &Expression,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
    store: &TemplateIrStore,
) -> Option<ReactiveTemplateMetadata> {
    match &expression.kind {
        ExpressionKind::Template(template) => {
            metadata_for_template(template, flows, value_environment, store)
        }

        ExpressionKind::FunctionCall { name, args, .. }
        | ExpressionKind::HandledFallibleFunctionCall { name, args, .. } => {
            metadata_for_function_call(name, args, flows, value_environment, store)
        }

        ExpressionKind::Coerced { value, .. } => {
            metadata_for_expression(value, flows, value_environment, store)
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
    store: &TemplateIrStore,
) -> Option<ReactiveTemplateMetadata> {
    let flow = flows.get(name)?;
    let metadata = flow.success_returns.first()?.as_ref()?;
    let resolved_arguments = arguments
        .iter()
        .map(|argument| {
            let mut resolved_argument = argument.clone();
            resolved_argument.value.reactive_template =
                metadata_for_expression(&argument.value, flows, value_environment, store);
            resolved_argument
        })
        .collect::<Vec<_>>();

    metadata.instantiate_for_call(&flow.parameters, &resolved_arguments)
}

fn metadata_for_template(
    template: &Template,
    flows: &FxHashMap<InternedPath, FunctionTemplateFlow>,
    value_environment: &ReactiveTemplateValueEnvironment,
    store: &TemplateIrStore,
) -> Option<ReactiveTemplateMetadata> {
    let mut metadata = ReactiveTemplateMetadata::template_backed();

    // Use the store-aware traversal so control-flow bodies are read from
    // finalized same-store TIR body roots.
    reactive_template_metadata::merge_reactive_template_metadata_with_store_and_resolver(
        template,
        store,
        &mut metadata,
        &mut |expression| metadata_for_expression(expression, flows, value_environment, store),
    );

    Some(metadata)
}
