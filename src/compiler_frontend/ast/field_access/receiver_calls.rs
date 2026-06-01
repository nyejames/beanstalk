//! User-defined receiver-method lookup and call parsing.
//!
//! WHAT: resolves declared receiver methods and validates call-site receiver semantics.
//! WHY: user receiver methods follow different rules than compiler-owned builtin members.

use super::receiver_access::{
    ReceiverAccessDiagnostic, ReceiverAccessRequirement, validate_receiver_access,
};
use super::{MemberStepContext, ReceiverAccessMode};
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallArgumentResolutionContext, CallDiagnosticContext, expectations_from_external_method,
    expectations_from_receiver_method_signature, resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::function_calls::parse_call_arguments_typed;
use crate::compiler_frontend::ast::generic_functions::{
    GenericCallExpectedContext, GenericFunctionInferenceInput, GenericFunctionInstantiationRequest,
    infer_generic_function_call, recursive_generic_function_instantiation,
};
use crate::compiler_frontend::ast::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::ast::receiver_methods::ReceiverMethodEntry;
use crate::compiler_frontend::ast::statements::fallible_handling::token_stream_starts_fallible_handling_suffix;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::builtins::error_type::resolve_builtin_error_type_typed;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidReceiverCallReason, InvalidResultHandlingReason,
};
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{GenericParameterId, TypeId};
use crate::compiler_frontend::datatypes::{DataType, diagnostic_type_spelling};
use crate::compiler_frontend::external_packages::ExternalAccessKind;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::traits::definitions::{
    ResolvedTraitDefinition, ResolvedTraitRequirement, TraitReceiverRequirement,
};
use crate::compiler_frontend::traits::evidence::TraitEvidenceDefinition;
use crate::compiler_frontend::traits::ids::TraitId;
use crate::compiler_frontend::value_mode::ValueMode;
use rustc_hash::FxHashSet;

fn lookup_receiver_method<'a>(
    context: &'a ScopeContext,
    receiver_type_id: TypeId,
    member_name: StringId,
    type_environment: &TypeEnvironment,
) -> Option<&'a ReceiverMethodEntry> {
    let receiver_key = type_environment.receiver_key_for_type_id(receiver_type_id)?;
    context.lookup_receiver_method(&receiver_key, member_name)
}

struct TraitSurfaceReceiverMethod {
    method_path: InternedPath,
    signature: FunctionSignature,
    receiver_mutable: bool,
}

struct DynamicTraitReceiverMethod<'a> {
    trait_definition: &'a ResolvedTraitDefinition,
    requirement: &'a ResolvedTraitRequirement,
    signature: FunctionSignature,
    receiver_mutable: bool,
}

struct GenericBoundRequirementCandidate<'a> {
    trait_definition: &'a ResolvedTraitDefinition,
    requirement: &'a ResolvedTraitRequirement,
}

enum SourceReceiverMethodTarget<'a> {
    Declared(&'a ReceiverMethodEntry),
    TraitSurface(TraitSurfaceReceiverMethod),
}

impl SourceReceiverMethodTarget<'_> {
    fn receiver_mutable(&self) -> bool {
        match self {
            SourceReceiverMethodTarget::Declared(entry) => entry.receiver_mutable,
            SourceReceiverMethodTarget::TraitSurface(method) => method.receiver_mutable,
        }
    }
}

fn fallible_receiver_result_type_ids(
    success_return_type_ids: Vec<TypeId>,
    error_return_type_id: TypeId,
    type_interner: &mut AstTypeInterner<'_>,
) -> Vec<TypeId> {
    let success_type_id = match success_return_type_ids.as_slice() {
        [] => type_interner.builtins().none,
        [single] => *single,
        multiple => type_interner
            .environment_mut_for_derived_types()
            .intern_tuple(multiple.to_vec()),
    };

    vec![type_interner.intern_fallible_carrier(success_type_id, error_return_type_id)]
}

fn generic_parameter_id_for_receiver_type(
    receiver_type_id: TypeId,
    type_environment: &TypeEnvironment,
) -> Option<GenericParameterId> {
    match type_environment.get(receiver_type_id) {
        Some(TypeDefinition::GenericParameter(parameter)) => Some(parameter.id),
        _ => None,
    }
}

fn generic_parameter_ids_for_concrete_receiver(
    receiver_type_id: TypeId,
    scope_context: &ScopeContext,
) -> Vec<GenericParameterId> {
    let Some(active_context) = scope_context.active_generic_type_context() else {
        return Vec::new();
    };
    let Some(substitutions) = active_context.substitutions.as_ref() else {
        return Vec::new();
    };

    substitutions
        .iter()
        .filter_map(|(parameter_id, concrete_type_id)| {
            (*concrete_type_id == receiver_type_id).then_some(*parameter_id)
        })
        .collect()
}

fn generic_parameter_id_for_receiver_reference(
    receiver_node: &AstNode,
    scope_context: &ScopeContext,
) -> Option<GenericParameterId> {
    let active_context = scope_context.active_generic_type_context()?;

    let NodeKind::Rvalue(expression) = &receiver_node.kind else {
        return None;
    };

    let ExpressionKind::Reference(reference_path) = &expression.kind else {
        return None;
    };

    active_context
        .source_parameter_by_rebased_path
        .get(reference_path)
        .copied()
}

fn generic_bound_requirement_candidates<'a>(
    parameter_ids: &[GenericParameterId],
    member_name: StringId,
    scope_context: &'a ScopeContext,
    type_environment: &'a TypeEnvironment,
) -> Vec<GenericBoundRequirementCandidate<'a>> {
    let mut candidates = Vec::new();
    let mut seen_traits = FxHashSet::default();

    for parameter_id in parameter_ids {
        let Some(bounds) = type_environment.trait_bounds_for_generic_parameter(*parameter_id)
        else {
            continue;
        };

        for trait_id in bounds {
            if !seen_traits.insert(*trait_id) || !scope_context.trait_id_is_visible(*trait_id) {
                continue;
            }

            let Some(trait_definition) = scope_context.trait_environment().get(*trait_id) else {
                continue;
            };

            for requirement in &trait_definition.requirements {
                if requirement.name == member_name {
                    candidates.push(GenericBoundRequirementCandidate {
                        trait_definition,
                        requirement,
                    });
                }
            }
        }
    }

    candidates
}

fn evidence_for_bound_method<'a>(
    trait_id: TraitId,
    receiver_type_id: TypeId,
    member_name: StringId,
    member_location: &SourceLocation,
    scope_context: &'a ScopeContext,
) -> Result<Option<&'a TraitEvidenceDefinition>, ExpressionParseError> {
    let evidence_environment = scope_context.trait_evidence_environment();

    if let Some(evidence_id) = evidence_environment.builtin_for(receiver_type_id, trait_id) {
        return Ok(evidence_environment.get(evidence_id));
    }

    if let Some(evidence_id) = evidence_environment.canonical_for(receiver_type_id, trait_id) {
        return Ok(evidence_environment.get(evidence_id));
    }

    let source_file_scope =
        scope_context.required_source_file_scope("generic-bound receiver method dispatch")?;
    if evidence_environment
        .file_local_for(source_file_scope, receiver_type_id, trait_id)
        .is_some()
    {
        return Err(CompilerDiagnostic::invalid_receiver_call(
            InvalidReceiverCallReason::FileLocalGenericBoundEvidenceUnsupported,
            None,
            Some(member_name),
            member_location.clone(),
        )
        .into());
    }

    Ok(None)
}

fn method_path_from_evidence(
    evidence: &TraitEvidenceDefinition,
    requirement: &ResolvedTraitRequirement,
) -> Option<InternedPath> {
    evidence
        .requirements
        .iter()
        .find(|requirement_evidence| requirement_evidence.requirement_id == requirement.id)
        .map(|requirement_evidence| requirement_evidence.method_path.clone())
}

fn requirement_receiver_is_mutable(requirement: &ResolvedTraitRequirement) -> bool {
    matches!(
        requirement.receiver,
        TraitReceiverRequirement::Mutable { .. }
    )
}

fn replace_trait_this_type(
    type_id: TypeId,
    trait_this_type: TypeId,
    receiver_type_id: TypeId,
) -> TypeId {
    if type_id == trait_this_type {
        receiver_type_id
    } else {
        type_id
    }
}

fn declaration_for_trait_bound_parameter(
    id: InternedPath,
    type_id: TypeId,
    diagnostic_type: DataType,
    value_mode: ValueMode,
    location: SourceLocation,
) -> Declaration {
    Declaration {
        id,
        value: Expression::new(
            ExpressionKind::NoValue,
            location,
            type_id,
            diagnostic_type,
            value_mode,
        ),
    }
}

fn signature_from_trait_requirement(
    method_path: &InternedPath,
    trait_definition: &ResolvedTraitDefinition,
    requirement: &ResolvedTraitRequirement,
    receiver_type_id: TypeId,
    type_environment: &TypeEnvironment,
    string_table: &mut StringTable,
) -> FunctionSignature {
    let receiver_mutable = requirement_receiver_is_mutable(requirement);
    let receiver_mode = if receiver_mutable {
        ValueMode::MutableReference
    } else {
        ValueMode::ImmutableReference
    };
    let mut parameters = Vec::with_capacity(requirement.parameters.len() + 1);
    let receiver_name = method_path.join_str("__trait_bound_receiver", string_table);
    parameters.push(declaration_for_trait_bound_parameter(
        receiver_name,
        receiver_type_id,
        diagnostic_type_spelling(receiver_type_id, type_environment),
        receiver_mode,
        requirement.location.clone(),
    ));

    for parameter in &requirement.parameters {
        let type_id = replace_trait_this_type(
            parameter.type_id,
            trait_definition.this_type,
            receiver_type_id,
        );
        parameters.push(declaration_for_trait_bound_parameter(
            parameter.name.clone(),
            type_id,
            diagnostic_type_spelling(type_id, type_environment),
            parameter.value_mode.clone(),
            parameter.location.clone(),
        ));
    }

    let returns = requirement
        .returns
        .iter()
        .map(|return_slot| {
            let type_id = replace_trait_this_type(
                return_slot.type_id,
                trait_definition.this_type,
                receiver_type_id,
            );

            ReturnSlot {
                value: FunctionReturn::Value(diagnostic_type_spelling(type_id, type_environment)),
                type_id: Some(type_id),
                channel: return_slot.channel,
            }
        })
        .collect();

    FunctionSignature {
        parameters,
        returns,
    }
}

fn lookup_generic_bound_receiver_method(
    scope_context: &ScopeContext,
    receiver_node: &AstNode,
    receiver_type_id: TypeId,
    member_name: StringId,
    member_location: &SourceLocation,
    type_environment: &TypeEnvironment,
    string_table: &mut StringTable,
) -> Result<Option<TraitSurfaceReceiverMethod>, ExpressionParseError> {
    let mut parameter_ids = Vec::new();
    let receiver_is_unresolved_generic = if let Some(parameter_id) =
        generic_parameter_id_for_receiver_type(receiver_type_id, type_environment)
    {
        parameter_ids.push(parameter_id);
        true
    } else if let Some(parameter_id) =
        generic_parameter_id_for_receiver_reference(receiver_node, scope_context)
    {
        parameter_ids.push(parameter_id);
        false
    } else {
        parameter_ids.extend(generic_parameter_ids_for_concrete_receiver(
            receiver_type_id,
            scope_context,
        ));
        false
    };

    if parameter_ids.is_empty() {
        return Ok(None);
    }

    let candidates = generic_bound_requirement_candidates(
        &parameter_ids,
        member_name,
        scope_context,
        type_environment,
    );
    match candidates.as_slice() {
        [] => Ok(None),
        [_first, _second, ..] => Err(CompilerDiagnostic::invalid_receiver_call(
            InvalidReceiverCallReason::AmbiguousGenericBoundMethod,
            None,
            Some(member_name),
            member_location.clone(),
        )
        .into()),
        [candidate] => {
            let method_path = if receiver_is_unresolved_generic {
                // Validation-only generic bodies have no concrete evidence method yet. The
                // synthetic path is discarded with the validation AST nodes; concrete instances
                // reparse the same call and select the evidence method below.
                candidate
                    .trait_definition
                    .canonical_path
                    .append(candidate.requirement.name)
            } else {
                let Some(evidence) = evidence_for_bound_method(
                    candidate.trait_definition.id,
                    receiver_type_id,
                    member_name,
                    member_location,
                    scope_context,
                )?
                else {
                    return Ok(None);
                };

                let Some(method_path) = method_path_from_evidence(evidence, candidate.requirement)
                else {
                    return Ok(None);
                };
                method_path
            };

            let signature = signature_from_trait_requirement(
                &method_path,
                candidate.trait_definition,
                candidate.requirement,
                receiver_type_id,
                type_environment,
                string_table,
            );

            Ok(Some(TraitSurfaceReceiverMethod {
                method_path,
                signature,
                receiver_mutable: requirement_receiver_is_mutable(candidate.requirement),
            }))
        }
    }
}

struct ConcreteTraitEvidenceRequirementCandidate<'a> {
    trait_definition: &'a ResolvedTraitDefinition,
    requirement: &'a ResolvedTraitRequirement,
    evidence: &'a TraitEvidenceDefinition,
}

struct SourceReceiverMethodCallInput<'a, 'interner> {
    token_stream: &'a mut FileTokens,
    receiver_node: &'a AstNode,
    member_name: StringId,
    member_location: SourceLocation,
    receiver_access_mode: ReceiverAccessMode,
    scope_context: &'a ScopeContext,
    source_method: SourceReceiverMethodTarget<'a>,
    type_interner: &'a mut AstTypeInterner<'interner>,
    string_table: &'a mut StringTable,
}

struct DynamicTraitMethodCallInput<'a, 'interner> {
    token_stream: &'a mut FileTokens,
    receiver_node: &'a AstNode,
    member_name: StringId,
    member_location: SourceLocation,
    receiver_access_mode: ReceiverAccessMode,
    scope_context: &'a ScopeContext,
    method: DynamicTraitReceiverMethod<'a>,
    type_interner: &'a mut AstTypeInterner<'interner>,
    string_table: &'a mut StringTable,
}

fn concrete_trait_evidence_requirement_candidates<'a>(
    receiver_type_id: TypeId,
    member_name: StringId,
    scope_context: &'a ScopeContext,
) -> Result<Vec<ConcreteTraitEvidenceRequirementCandidate<'a>>, ExpressionParseError> {
    let source_file_scope =
        scope_context.required_source_file_scope("concrete trait receiver fallback")?;
    let evidence_environment = scope_context.trait_evidence_environment();
    let mut candidates = Vec::new();

    for evidence_id in
        evidence_environment.receiver_fallback_candidates(receiver_type_id, source_file_scope)
    {
        let Some(evidence) = evidence_environment.get(evidence_id) else {
            continue;
        };

        if !scope_context.trait_id_is_visible(evidence.trait_id) {
            continue;
        }

        let Some(trait_definition) = scope_context.trait_environment().get(evidence.trait_id)
        else {
            continue;
        };

        for requirement in &trait_definition.requirements {
            if requirement.name == member_name {
                candidates.push(ConcreteTraitEvidenceRequirementCandidate {
                    trait_definition,
                    requirement,
                    evidence,
                });
            }
        }
    }

    Ok(candidates)
}

fn lookup_concrete_trait_evidence_receiver_method(
    scope_context: &ScopeContext,
    receiver_type_id: TypeId,
    member_name: StringId,
    member_location: &SourceLocation,
    type_environment: &TypeEnvironment,
    string_table: &mut StringTable,
) -> Result<Option<TraitSurfaceReceiverMethod>, ExpressionParseError> {
    if generic_parameter_id_for_receiver_type(receiver_type_id, type_environment).is_some() {
        return Ok(None);
    }

    let candidates = concrete_trait_evidence_requirement_candidates(
        receiver_type_id,
        member_name,
        scope_context,
    )?;
    match candidates.as_slice() {
        [] => Ok(None),
        [_first, _second, ..] => Err(CompilerDiagnostic::invalid_receiver_call(
            InvalidReceiverCallReason::AmbiguousTraitEvidenceMethod,
            None,
            Some(member_name),
            member_location.clone(),
        )
        .into()),
        [candidate] => {
            let Some(method_path) =
                method_path_from_evidence(candidate.evidence, candidate.requirement)
            else {
                return Ok(None);
            };
            let signature = signature_from_trait_requirement(
                &method_path,
                candidate.trait_definition,
                candidate.requirement,
                receiver_type_id,
                type_environment,
                string_table,
            );

            Ok(Some(TraitSurfaceReceiverMethod {
                method_path,
                signature,
                receiver_mutable: requirement_receiver_is_mutable(candidate.requirement),
            }))
        }
    }
}

fn lookup_dynamic_trait_receiver_method<'a>(
    scope_context: &'a ScopeContext,
    receiver_type_id: TypeId,
    member_name: StringId,
    type_environment: &TypeEnvironment,
    string_table: &mut StringTable,
) -> Option<DynamicTraitReceiverMethod<'a>> {
    let Some(TypeDefinition::DynamicTrait(dynamic_definition)) =
        type_environment.get(receiver_type_id)
    else {
        return None;
    };

    let trait_definition = scope_context
        .trait_environment()
        .get(dynamic_definition.trait_id)?;

    let requirement = trait_definition
        .requirements
        .iter()
        .find(|requirement| requirement.name == member_name)?;

    // Dynamic dispatch is requirement-based. The synthetic path is used only to build the
    // already-shared call signature shape; HIR carries the trait/requirement IDs instead.
    let synthetic_method_path = trait_definition.canonical_path.append(requirement.name);
    let receiver_mutable = requirement_receiver_is_mutable(requirement);
    let signature = signature_from_trait_requirement(
        &synthetic_method_path,
        trait_definition,
        requirement,
        receiver_type_id,
        type_environment,
        string_table,
    );

    Some(DynamicTraitReceiverMethod {
        trait_definition,
        requirement,
        signature,
        receiver_mutable,
    })
}

fn parse_dynamic_trait_method_call_typed(
    input: DynamicTraitMethodCallInput<'_, '_>,
) -> Result<AstNode, ExpressionParseError> {
    let DynamicTraitMethodCallInput {
        token_stream,
        receiver_node,
        member_name,
        member_location,
        receiver_access_mode,
        scope_context,
        method,
        type_interner,
        string_table,
    } = input;

    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return Err(CompilerDiagnostic::invalid_receiver_call(
            InvalidReceiverCallReason::MustUseParentheses,
            None,
            Some(member_name),
            member_location,
        )
        .into());
    }

    token_stream.advance();

    validate_receiver_access(
        receiver_node,
        receiver_access_mode,
        &member_location,
        ReceiverAccessRequirement {
            requires_mutable: method.receiver_mutable,
            diagnostic: ReceiverAccessDiagnostic::ReceiverMethod {
                method_name: member_name,
            },
        },
    )?;

    let raw_args =
        parse_call_arguments_typed(token_stream, scope_context, type_interner, string_table)?;
    let method_name = string_table.resolve(member_name).to_owned();
    let expectations =
        expectations_from_receiver_method_signature(&method.signature.parameters[1..]);
    let type_check_context = type_interner.type_check_context();
    let args = resolve_call_arguments(
        CallDiagnosticContext::receiver_method(&method_name),
        &raw_args,
        &expectations,
        member_location.clone(),
        CallArgumentResolutionContext {
            string_table,
            type_environment: type_check_context.type_environment,
            compatibility_cache: type_check_context.compatibility_cache,
            scope_context: Some(scope_context),
        },
    )?;

    let result_type_ids =
        if let Some(error_return_type_id) = method.signature.error_return_type_id() {
            if !token_stream_starts_fallible_handling_suffix(token_stream) {
                return Err(CompilerDiagnostic::invalid_result_handling(
                    InvalidResultHandlingReason::UnhandledErrorReturn,
                    token_stream.current_location(),
                )
                .into());
            }

            fallible_receiver_result_type_ids(
                method.signature.success_return_type_ids(),
                error_return_type_id,
                type_interner,
            )
        } else {
            if matches!(
                token_stream.current_token_kind(),
                TokenKind::Bang | TokenKind::Catch
            ) {
                return Err(CompilerDiagnostic::invalid_result_handling(
                    InvalidResultHandlingReason::NotResultExpression,
                    token_stream.current_location(),
                )
                .into());
            }

            method.signature.success_return_type_ids()
        };

    Ok(AstNode {
        kind: NodeKind::DynamicTraitMethodCall {
            receiver: Box::new(receiver_node.to_owned()),
            trait_id: method.trait_definition.id,
            requirement_id: method.requirement.id,
            method: member_name,
            receiver_requires_mutable: method.receiver_mutable,
            args,
            result_type_ids,
            location: member_location.clone(),
        },
        scope: scope_context.scope.to_owned(),
        location: member_location,
    })
}

fn parse_source_receiver_method_target_call_typed(
    input: SourceReceiverMethodCallInput<'_, '_>,
) -> Result<AstNode, ExpressionParseError> {
    let SourceReceiverMethodCallInput {
        token_stream,
        receiver_node,
        member_name,
        member_location,
        receiver_access_mode,
        scope_context,
        source_method,
        type_interner,
        string_table,
    } = input;

    if receiver_node.expression_is_const_record_value()? {
        return Err(CompilerDiagnostic::invalid_receiver_call(
            InvalidReceiverCallReason::ConstStructNoRuntimeCalls,
            None,
            Some(member_name),
            member_location,
        )
        .into());
    }

    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return Err(CompilerDiagnostic::invalid_receiver_call(
            InvalidReceiverCallReason::MustUseParentheses,
            None,
            Some(member_name),
            member_location,
        )
        .into());
    }

    token_stream.advance();

    let method_name = string_table.resolve(member_name).to_owned();
    validate_receiver_access(
        receiver_node,
        receiver_access_mode,
        &member_location,
        ReceiverAccessRequirement {
            requires_mutable: source_method.receiver_mutable(),
            diagnostic: ReceiverAccessDiagnostic::ReceiverMethod {
                method_name: member_name,
            },
        },
    )?;

    let raw_args =
        parse_call_arguments_typed(token_stream, scope_context, type_interner, string_table)?;

    let (method_path, call_signature, generic_request) = match &source_method {
        SourceReceiverMethodTarget::Declared(method_entry) => {
            if let Some(template) =
                scope_context.lookup_generic_function_template(&method_entry.function_path)
            {
                let receiver_expr = receiver_node.get_expr()?.to_owned();
                let receiver_access = if method_entry.receiver_mutable {
                    CallAccessMode::Mutable
                } else {
                    CallAccessMode::Shared
                };
                let receiver_arg = CallArgument::positional(
                    receiver_expr,
                    receiver_access,
                    member_location.clone(),
                );

                let mut inference_args = Vec::with_capacity(raw_args.len() + 1);
                inference_args.push(receiver_arg);
                inference_args.extend(raw_args.iter().cloned());

                let inference = infer_generic_function_call(GenericFunctionInferenceInput {
                    template,
                    raw_arguments: &inference_args,
                    expected_context: GenericCallExpectedContext::None,
                    call_location: member_location.clone(),
                    type_environment: type_interner.environment_mut_for_derived_types(),
                    string_table,
                })?;

                if scope_context.is_generic_function_instantiation_active(&inference.key) {
                    return Err(recursive_generic_function_instantiation(
                        template.function_path.name(),
                        member_location.clone(),
                    )
                    .into());
                }

                let request = GenericFunctionInstantiationRequest {
                    key: inference.key,
                    instance_path: inference.instance_path.clone(),
                    call_location: member_location.clone(),
                };

                (inference.instance_path, inference.signature, Some(request))
            } else {
                (
                    method_entry.function_path.to_owned(),
                    method_entry.signature.to_owned(),
                    None,
                )
            }
        }

        SourceReceiverMethodTarget::TraitSurface(method) => {
            (method.method_path.clone(), method.signature.clone(), None)
        }
    };

    let expectations = expectations_from_receiver_method_signature(&call_signature.parameters[1..]);
    let type_check_context = type_interner.type_check_context();
    let args = resolve_call_arguments(
        CallDiagnosticContext::receiver_method(&method_name),
        &raw_args,
        &expectations,
        member_location.clone(),
        CallArgumentResolutionContext {
            string_table,
            type_environment: type_check_context.type_environment,
            compatibility_cache: type_check_context.compatibility_cache,
            scope_context: Some(scope_context),
        },
    )?;
    let result_type_ids = if let Some(error_return_type_id) = call_signature.error_return_type_id()
    {
        if !token_stream_starts_fallible_handling_suffix(token_stream) {
            return Err(CompilerDiagnostic::invalid_result_handling(
                InvalidResultHandlingReason::UnhandledErrorReturn,
                token_stream.current_location(),
            )
            .into());
        }

        fallible_receiver_result_type_ids(
            call_signature.success_return_type_ids(),
            error_return_type_id,
            type_interner,
        )
    } else {
        if matches!(
            token_stream.current_token_kind(),
            TokenKind::Bang | TokenKind::Catch
        ) {
            return Err(CompilerDiagnostic::invalid_result_handling(
                InvalidResultHandlingReason::NotResultExpression,
                token_stream.current_location(),
            )
            .into());
        }

        call_signature.success_return_type_ids()
    };

    if let Some(request) = generic_request {
        scope_context.record_generic_function_instantiation_request(request);
    }

    increment_ast_counter(AstCounter::PostfixReceiverNodesCopied);

    Ok(AstNode {
        kind: NodeKind::MethodCall {
            receiver: Box::new(receiver_node.to_owned()),
            method_path,
            method: member_name,
            args,
            result_type_ids,
            location: member_location.clone(),
        },
        scope: scope_context.scope.to_owned(),
        location: member_location,
    })
}

pub(super) fn parse_receiver_method_call_typed(
    token_stream: &mut FileTokens,
    member_step_context: MemberStepContext<'_>,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<Option<AstNode>, ExpressionParseError> {
    let MemberStepContext {
        receiver_node,
        receiver_type_id,
        member_name,
        member_location,
        receiver_access_mode,
        scope_context,
    } = member_step_context;

    if let Some(dynamic_method) = lookup_dynamic_trait_receiver_method(
        scope_context,
        receiver_type_id,
        member_name,
        type_interner.environment(),
        string_table,
    ) {
        let node = parse_dynamic_trait_method_call_typed(DynamicTraitMethodCallInput {
            token_stream,
            receiver_node,
            member_name,
            member_location,
            receiver_access_mode,
            scope_context,
            method: dynamic_method,
            type_interner,
            string_table,
        })?;
        return Ok(Some(node));
    }

    // ----------------------------
    //  Try source receiver method surfaces
    // ----------------------------
    let source_method = if let Some(method_entry) = lookup_receiver_method(
        scope_context,
        receiver_type_id,
        member_name,
        type_interner.environment(),
    ) {
        Some(SourceReceiverMethodTarget::Declared(method_entry))
    } else {
        lookup_generic_bound_receiver_method(
            scope_context,
            receiver_node,
            receiver_type_id,
            member_name,
            &member_location,
            type_interner.environment(),
            string_table,
        )?
        .map(SourceReceiverMethodTarget::TraitSurface)
    };

    if let Some(source_method) = source_method {
        let node = parse_source_receiver_method_target_call_typed(SourceReceiverMethodCallInput {
            token_stream,
            receiver_node,
            member_name,
            member_location,
            receiver_access_mode,
            scope_context,
            source_method,
            type_interner,
            string_table,
        })?;
        return Ok(Some(node));
    }

    // ----------------------------
    //  Try external platform-package receiver method
    // ----------------------------
    let method_name_str = string_table.resolve(member_name).to_owned();
    if let Some((external_id, external_def)) = scope_context.lookup_visible_external_method(
        receiver_type_id,
        member_name,
        type_interner.environment(),
    ) {
        if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
            return Err(CompilerDiagnostic::invalid_receiver_call(
                InvalidReceiverCallReason::MustUseParentheses,
                None,
                Some(member_name),
                member_location,
            )
            .into());
        }

        token_stream.advance();

        let requires_mutable = external_def.receiver_access == ExternalAccessKind::Mutable;
        validate_receiver_access(
            receiver_node,
            receiver_access_mode,
            &member_location,
            ReceiverAccessRequirement {
                requires_mutable,
                diagnostic: ReceiverAccessDiagnostic::ReceiverMethod {
                    method_name: member_name,
                },
            },
        )?;

        let raw_args =
            parse_call_arguments_typed(token_stream, scope_context, type_interner, string_table)?;
        let expectations = {
            let env = type_interner.environment_mut_for_derived_types();
            expectations_from_external_method(external_def, env)
        };
        let type_check_context = type_interner.type_check_context();
        let mut args = resolve_call_arguments(
            CallDiagnosticContext::receiver_method(&method_name_str),
            &raw_args,
            &expectations,
            member_location.clone(),
            CallArgumentResolutionContext {
                string_table,
                type_environment: type_check_context.type_environment,
                compatibility_cache: type_check_context.compatibility_cache,
                scope_context: Some(scope_context),
            },
        )?;

        // Prepend the receiver as the first argument (mirrors user-method lowering).
        increment_ast_counter(AstCounter::PostfixReceiverNodesCopied);
        let receiver_expr = receiver_node.get_expr()?.to_owned();
        let receiver_access = if requires_mutable {
            CallAccessMode::Mutable
        } else {
            CallAccessMode::Shared
        };
        let receiver_arg =
            CallArgument::positional(receiver_expr, receiver_access, member_location.clone());
        args.insert(0, receiver_arg);

        let builtin_error_type =
            resolve_builtin_error_type_typed(scope_context, &member_location, string_table)?;
        let success_return_type_ids = external_def.success_return_type_ids(
            type_interner.environment_mut_for_derived_types(),
            builtin_error_type.type_id,
        );
        let error_return_type_id = external_def.error_return_type_id(
            type_interner.environment_mut_for_derived_types(),
            builtin_error_type.type_id,
        );

        let result_type_ids = if external_def.is_fallible() {
            let Some(error_return_type_id) = error_return_type_id else {
                return Err(CompilerError::compiler_error(format!(
                    "Fallible external receiver method '{}' has no frontend-visible concrete error slot.",
                    external_def.name
                ))
                .into());
            };

            if !token_stream_starts_fallible_handling_suffix(token_stream) {
                return Err(CompilerDiagnostic::invalid_result_handling(
                    InvalidResultHandlingReason::UnhandledErrorReturn,
                    token_stream.current_location(),
                )
                .into());
            }

            fallible_receiver_result_type_ids(
                success_return_type_ids,
                error_return_type_id,
                type_interner,
            )
        } else {
            if matches!(
                token_stream.current_token_kind(),
                TokenKind::Bang | TokenKind::Catch
            ) {
                return Err(CompilerDiagnostic::invalid_result_handling(
                    InvalidResultHandlingReason::NotResultExpression,
                    token_stream.current_location(),
                )
                .into());
            }

            success_return_type_ids
        };

        return Ok(Some(AstNode {
            kind: NodeKind::HostFunctionCall {
                name: external_id,
                args,
                result_type_ids,
                location: member_location.clone(),
            },
            scope: scope_context.scope.to_owned(),
            location: member_location,
        }));
    }

    // ----------------------------
    //  Try visible trait evidence for concrete receiver fallback
    // ----------------------------
    if let Some(evidence_method) = lookup_concrete_trait_evidence_receiver_method(
        scope_context,
        receiver_type_id,
        member_name,
        &member_location,
        type_interner.environment(),
        string_table,
    )? {
        let node = parse_source_receiver_method_target_call_typed(SourceReceiverMethodCallInput {
            token_stream,
            receiver_node,
            member_name,
            member_location,
            receiver_access_mode,
            scope_context,
            source_method: SourceReceiverMethodTarget::TraitSurface(evidence_method),
            type_interner,
            string_table,
        })?;
        return Ok(Some(node));
    }

    Ok(None)
}
