//! Function-call parsing and result-handling suffix integration.
//!
//! WHAT: parses raw call argument syntax, resolves user/host call signatures, and applies the
//! postfix `!` propagation and `catch` recovery forms that can follow a call expression.
//! WHY: call parsing sits at the boundary between general expression parsing and call-specific
//! validation, so keeping that flow together makes the refactor seams easier to follow.

use crate::ast_log;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallArgumentResolutionContext, CallDiagnosticContext, ExpectedParameterType,
    ParameterExpectation, expectations_from_host_function, expectations_from_user_parameters,
    resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression_with_trailing_newline_policy;
use crate::compiler_frontend::ast::expressions::parse_expression_input::{
    ExpressionParseInput, ExpressionParseResources,
};
use crate::compiler_frontend::ast::statements::fallible_handling::{
    FallibleCallSite, FallibleHostCallSite, HandledFallibleCall, HandledFallibleHostCall,
    parse_fallible_handling_suffix_for_call, parse_fallible_handling_suffix_for_host_call,
    token_stream_starts_fallible_handling_suffix,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::builtins::error_type::resolve_builtin_error_type_typed;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidBuiltinCallReason, InvalidCallShapeReason,
    InvalidGenericInstantiationReason, InvalidResultHandlingReason, InvalidResultOperandReason,
    TypeMismatchContext, UnsupportedOperatorCategory,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::external_packages::{
    ExternalFunctionDef, ExternalFunctionId, ExternalSignatureType,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::{
    CastTargetContext, ExpectedType, cast_target_context_for_type_id,
};
use crate::compiler_frontend::value_mode::ValueMode;
use rustc_hash::FxHashMap;

/// Input bundle for `parse_function_call` to avoid long argument lists.
pub struct FunctionCallParseInput<'a, 'b> {
    pub token_stream: &'a mut FileTokens,
    pub id: &'a InternedPath,
    pub context: &'a ScopeContext,
    pub signature: &'a FunctionSignature,
    pub value_required: bool,
    pub allow_boundary_catch: bool,
    pub warnings: Option<&'a mut Vec<CompilerDiagnostic>>,
    pub type_interner: &'a mut AstTypeInterner<'b>,
    pub string_table: &'a mut StringTable,
}

/// Input bundle for external function calls.
pub struct ExternalFunctionCallParseInput<'a, 'b> {
    pub token_stream: &'a mut FileTokens,
    pub external_function_id: ExternalFunctionId,
    pub external_function: &'a ExternalFunctionDef,
    pub context: &'a ScopeContext,
    pub value_required: bool,
    pub allow_boundary_catch: bool,
    pub warnings: Option<&'a mut Vec<CompilerDiagnostic>>,
    pub type_interner: &'a mut AstTypeInterner<'b>,
    pub string_table: &'a mut StringTable,
}

/// Thin wrapper around the typed implementation.
pub fn parse_function_call(
    input: FunctionCallParseInput<'_, '_>,
) -> Result<AstNode, ExpressionParseError> {
    parse_function_call_typed(input)
}

fn parse_function_call_typed(
    input: FunctionCallParseInput<'_, '_>,
) -> Result<AstNode, ExpressionParseError> {
    let FunctionCallParseInput {
        token_stream,
        id,
        context,
        signature,
        value_required,
        allow_boundary_catch,
        warnings,
        type_interner,
        string_table,
    } = input;

    // ------------------------
    //  Route to external call
    // ------------------------
    // External calls share the same argument parser, but they reject named targets until
    // external metadata carries stable public parameter names.
    if let Some((function_id, host_function)) = id
        .name()
        .and_then(|name| context.lookup_visible_external_function(name))
    {
        return parse_external_function_call_typed(ExternalFunctionCallParseInput {
            token_stream,
            external_function_id: function_id,
            external_function: host_function,
            context,
            value_required,
            allow_boundary_catch,
            warnings,
            type_interner,
            string_table,
        });
    }

    // ------------------------
    //  Parse and resolve arguments
    // ------------------------
    let parameter_expectations = expectations_from_user_parameters(&signature.parameters);
    let raw_args = parse_call_arguments_typed_with_expectations(
        token_stream,
        context,
        type_interner,
        string_table,
        &parameter_expectations,
        NamedArgumentSyntax::Supported {
            callee_name: id.name(),
        },
    )?;
    let args = resolve_user_function_call_arguments(
        id,
        &raw_args,
        &signature.parameters,
        token_stream.current_location(),
        string_table,
        type_interner,
        Some(context),
    )?;

    let call = HandledFallibleCall {
        name: id.to_owned(),
        result_type_ids: signature.success_return_type_ids(),
        args,
        call_location: token_stream.current_location(),
    };

    // ------------------------
    //  Apply fallible handling
    // ------------------------
    if let Some(error_return_type_id) = signature.error_return_type_id() {
        if token_stream_starts_fallible_handling_suffix(token_stream) {
            return parse_fallible_handling_suffix_for_call(
                token_stream,
                context,
                FallibleCallSite {
                    call,
                    error_return_type_id,
                    value_required,
                    allow_boundary_catch,
                },
                warnings,
                type_interner,
                string_table,
            );
        }

        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::UnhandledErrorReturn,
            token_stream.current_location(),
        )
        .into());
    } else if matches!(
        token_stream.current_token_kind(),
        TokenKind::Bang | TokenKind::Catch
    ) {
        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::NotResultExpression,
            token_stream.current_location(),
        )
        .into());
    }

    Ok(AstNode {
        kind: NodeKind::FunctionCall {
            name: call.name,
            args: call.args,
            result_type_ids: call.result_type_ids,
            location: call.call_location,
        },
        location: token_stream.current_location(),
        scope: context.scope.clone(),
    })
}

/// Parses the raw `(...)` argument list shared by all call-shaped syntax.
///
/// WHAT: test-only entry point for syntax-level argument parsing without
///      parameter expectations. Production callers use
///      `parse_call_arguments_typed_with_expectations` so `cast` receives a
///      concrete target at each argument boundary.
#[cfg(test)]
pub fn parse_call_arguments(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<Vec<CallArgument>, ExpressionParseError> {
    parse_call_arguments_inner(
        token_stream,
        context,
        type_interner,
        string_table,
        CallArgumentSyntaxContext::Ordinary,
        NamedArgumentSyntax::Supported { callee_name: None },
        None,
    )
}

/// Whether the call surface accepts named arguments.
///
/// WHAT: source calls and constructors support named arguments, while builtin
///      members and host calls are currently positional-only.
/// WHY: call-shape diagnostics should be produced before parsing a value whose
///      cast target depends on the named parameter.
#[derive(Clone, Copy)]
pub(crate) enum NamedArgumentSyntax {
    Supported { callee_name: Option<StringId> },
    UnsupportedCall { callee_name: Option<StringId> },
    UnsupportedBuiltinMember { member_name: Option<StringId> },
}

/// Parses a call argument list with explicit parameter expectations threaded into each argument.
///
/// WHAT: gives every argument expression a `CastTargetContext` derived from its corresponding
///      parameter type, so `cast` / `cast!` can resolve at concrete source/receiver/host
///      parameters and generic parameter slots can reject `cast` with `TargetIsGenericParameter`.
/// WHY: raw call parsing used to resolve arguments before validation; threading expectations keeps
///      the cast-target channel narrow and local to the argument parser.
pub(crate) fn parse_call_arguments_typed_with_expectations(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
    expectations: &[ParameterExpectation],
    named_arguments: NamedArgumentSyntax,
) -> Result<Vec<CallArgument>, ExpressionParseError> {
    parse_call_arguments_inner(
        token_stream,
        context,
        type_interner,
        string_table,
        CallArgumentSyntaxContext::Ordinary,
        named_arguments,
        Some(expectations),
    )
}

pub(crate) fn parse_generic_call_arguments_typed(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
    generic_function_name: Option<StringId>,
    expectations: &[ParameterExpectation],
) -> Result<Vec<CallArgument>, ExpressionParseError> {
    parse_call_arguments_inner(
        token_stream,
        context,
        type_interner,
        string_table,
        CallArgumentSyntaxContext::GenericFunction {
            function_name: generic_function_name,
        },
        NamedArgumentSyntax::Supported {
            callee_name: generic_function_name,
        },
        Some(expectations),
    )
}

#[derive(Clone, Copy)]
enum CallArgumentSyntaxContext {
    Ordinary,
    GenericFunction { function_name: Option<StringId> },
}

/// Builds a `CastTargetContext` from a single parameter expectation.
///
/// WHAT: converts the parameter's expected type into the same cast-target channel used by
///      declarations and assignments. `UnknownExternal` parameters are not builtin cast targets.
/// WHY: keeps call arguments consistent with other explicit typed boundaries without making
///      ordinary expression parsing globally type-directed.
fn cast_target_context_for_parameter_expectation(
    expectation: &ParameterExpectation,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> CastTargetContext {
    match expectation.expected_type {
        ExpectedParameterType::Known(type_id) => {
            cast_target_context_for_type_id(type_id, type_environment, string_table)
        }
        ExpectedParameterType::UnknownExternal => CastTargetContext::None,
    }
}

fn parse_call_arguments_inner(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
    syntax_context: CallArgumentSyntaxContext,
    named_arguments: NamedArgumentSyntax,
    expectations: Option<&[ParameterExpectation]>,
) -> Result<Vec<CallArgument>, ExpressionParseError> {
    ast_log!("Creating function call arguments");

    // ------------------------
    //  Consume opening paren
    // ------------------------
    if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
        return Err(CompilerDiagnostic::expected_token(
            TokenKind::OpenParenthesis,
            Some(token_stream.current_token_kind().to_owned()),
            token_stream.current_location(),
        )
        .into());
    }

    token_stream.advance();
    token_stream.skip_newlines();

    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        token_stream.advance();
        return Ok(Vec::new());
    }

    let mut args: Vec<CallArgument> = Vec::new();
    let mut positional_cursor = 0usize;
    let mut saw_named_argument = false;
    let mut occupied_parameter_slots =
        expectations.map(|expectations| vec![false; expectations.len()]);

    // Pre-index parameter names so named arguments can pick up their expectation
    // before the value expression is parsed.
    let parameter_name_to_slot: FxHashMap<StringId, usize> = expectations
        .map(|expectations| {
            expectations
                .iter()
                .enumerate()
                .filter_map(|(index, expectation)| expectation.name.map(|name| (name, index)))
                .collect()
        })
        .unwrap_or_default();

    // ------------------------
    //  Parse each argument
    // ------------------------
    loop {
        token_stream.skip_newlines();
        if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
            token_stream.advance();
            break;
        }

        let argument_location = token_stream.current_location();

        reject_simple_generic_argument_type_ascription(token_stream, syntax_context)?;

        // Detect named-target syntax (`name = expr`) or reject unsupported variants.
        let named_target = match token_stream.current_token_kind() {
            // `~name = expr` is not supported.
            TokenKind::Mutable
                if matches!(token_stream.peek_next_token(), Some(TokenKind::Symbol(_)))
                    && token_stream
                        .tokens
                        .get(token_stream.index + 2)
                        .map(|token| &token.kind)
                        == Some(&TokenKind::Assign) =>
            {
                return Err(CompilerDiagnostic::unexpected_token(
                    TokenKind::Mutable,
                    token_stream.current_location(),
                )
                .into());
            }

            // Standard named argument: `name = expr`.
            TokenKind::Symbol(name)
                if token_stream.peek_next_token() == Some(&TokenKind::Assign) =>
            {
                let target_location = token_stream.current_location();
                let target_name = *name;
                token_stream.advance();
                token_stream.advance();
                token_stream.skip_newlines();
                Some((target_name, target_location))
            }

            // Parenthesized names like `(name) = expr` are not supported.
            TokenKind::OpenParenthesis
                if matches!(token_stream.peek_next_token(), Some(TokenKind::Symbol(_)))
                    && token_stream
                        .tokens
                        .get(token_stream.index + 2)
                        .map(|token| &token.kind)
                        == Some(&TokenKind::CloseParenthesis)
                    && token_stream
                        .tokens
                        .get(token_stream.index + 3)
                        .map(|token| &token.kind)
                        == Some(&TokenKind::Assign) =>
            {
                return Err(CompilerDiagnostic::unexpected_token(
                    TokenKind::OpenParenthesis,
                    token_stream.current_location(),
                )
                .into());
            }

            _ => None,
        };

        let expectation_index = route_argument_slot_before_value_parse(ArgumentSlotRouteRequest {
            named_target: named_target.as_ref(),
            positional_cursor: &mut positional_cursor,
            saw_named_argument: &mut saw_named_argument,
            parameter_name_to_slot: &parameter_name_to_slot,
            occupied_parameter_slots: &mut occupied_parameter_slots,
            expectations,
            named_arguments,
            argument_location: argument_location.clone(),
        })?;

        let access_mode = if token_stream.current_token_kind() == &TokenKind::Mutable {
            token_stream.advance();
            CallAccessMode::Mutable
        } else {
            CallAccessMode::Shared
        };

        // A named target or access mode without a following value is an error.
        if token_stream.current_token_kind() == &TokenKind::Comma
            || token_stream.current_token_kind() == &TokenKind::CloseParenthesis
        {
            return Err(CompilerDiagnostic::unexpected_token(
                token_stream.current_token_kind().to_owned(),
                token_stream.current_location(),
            )
            .into());
        }

        let mut cast_target_context = expectation_index
            .and_then(|index| expectations.map(|expectations| expectations.get(index)))
            .flatten()
            .map(|expectation| {
                cast_target_context_for_parameter_expectation(
                    expectation,
                    type_interner.environment(),
                    string_table,
                )
            })
            .unwrap_or(CastTargetContext::None);

        let mut inferred = ExpectedType::Infer;
        let input = ExpressionParseInput::without_boundary_catch(
            ExpressionParseResources {
                token_stream,
                scope_context: context,
                type_interner,
                expected_type: &mut inferred,
                cast_target_context: &mut cast_target_context,
                value_mode: &ValueMode::ImmutableOwned,
                string_table,
            },
            false,
        );
        let value = create_expression_with_trailing_newline_policy(input)?;

        args.push(if let Some((name, target_location)) = named_target {
            CallArgument::named(value, name, access_mode, argument_location, target_location)
        } else {
            CallArgument::positional(value, access_mode, argument_location)
        });

        match token_stream.current_token_kind() {
            TokenKind::Comma => {
                token_stream.advance();
                token_stream.skip_newlines();
            }
            TokenKind::CloseParenthesis => {
                token_stream.advance();
                break;
            }
            _ => {
                return Err(CompilerDiagnostic::unexpected_token(
                    token_stream.current_token_kind().to_owned(),
                    token_stream.current_location(),
                )
                .into());
            }
        }
    }

    Ok(args)
}

struct ArgumentSlotRouteRequest<'a> {
    named_target: Option<&'a (StringId, SourceLocation)>,
    positional_cursor: &'a mut usize,
    saw_named_argument: &'a mut bool,
    parameter_name_to_slot: &'a FxHashMap<StringId, usize>,
    occupied_parameter_slots: &'a mut Option<Vec<bool>>,
    expectations: Option<&'a [ParameterExpectation]>,
    named_arguments: NamedArgumentSyntax,
    argument_location: SourceLocation,
}

fn route_argument_slot_before_value_parse(
    request: ArgumentSlotRouteRequest<'_>,
) -> Result<Option<usize>, ExpressionParseError> {
    // This mirrors call-validation routing only far enough to choose a cast target
    // before parsing the value. Full default filling, access checks, and type
    // validation stay in `call_validation`.
    let Some(expectations) = request.expectations else {
        if request.named_target.is_some() {
            *request.saw_named_argument = true;
        } else {
            *request.positional_cursor += 1;
        }

        return Ok(None);
    };

    if let Some((target_name, target_location)) = request.named_target {
        *request.saw_named_argument = true;

        match request.named_arguments {
            NamedArgumentSyntax::UnsupportedCall { callee_name } => {
                return Err(CompilerDiagnostic::invalid_call_shape(
                    InvalidCallShapeReason::NamedArgumentsNotSupported,
                    callee_name,
                    target_location.clone(),
                )
                .into());
            }

            NamedArgumentSyntax::UnsupportedBuiltinMember { member_name } => {
                return Err(CompilerDiagnostic::invalid_builtin_call(
                    InvalidBuiltinCallReason::NamedArgumentsNotSupported,
                    member_name,
                    target_location.clone(),
                )
                .into());
            }

            NamedArgumentSyntax::Supported { callee_name } => {
                let Some(slot) = request.parameter_name_to_slot.get(target_name).copied() else {
                    return Err(CompilerDiagnostic::invalid_call_shape(
                        InvalidCallShapeReason::NamedArgumentNotFound {
                            name: *target_name,
                            known_parameters: known_parameter_names(expectations),
                        },
                        callee_name,
                        target_location.clone(),
                    )
                    .into());
                };

                mark_argument_slot_available_before_value_parse(
                    slot,
                    expectations,
                    request.occupied_parameter_slots,
                    request.named_arguments,
                    target_location.clone(),
                )?;

                return Ok(Some(slot));
            }
        }
    }

    if *request.saw_named_argument {
        let callee_name = match request.named_arguments {
            NamedArgumentSyntax::Supported { callee_name }
            | NamedArgumentSyntax::UnsupportedCall { callee_name } => callee_name,
            NamedArgumentSyntax::UnsupportedBuiltinMember { .. } => None,
        };
        return Err(CompilerDiagnostic::invalid_call_shape(
            InvalidCallShapeReason::PositionalAfterNamed,
            callee_name,
            request.argument_location,
        )
        .into());
    }

    if let Some(occupied_slots) = request.occupied_parameter_slots {
        while *request.positional_cursor < occupied_slots.len()
            && occupied_slots[*request.positional_cursor]
        {
            *request.positional_cursor += 1;
        }

        if *request.positional_cursor >= occupied_slots.len() {
            return Err(CompilerDiagnostic::invalid_call_shape(
                InvalidCallShapeReason::ExtraPositionalArgument {
                    expected_count: expectations.len(),
                },
                None,
                request.argument_location,
            )
            .into());
        }

        let slot = *request.positional_cursor;
        occupied_slots[slot] = true;
        *request.positional_cursor += 1;
        return Ok(Some(slot));
    }

    let slot = *request.positional_cursor;
    *request.positional_cursor += 1;
    Ok(Some(slot))
}

fn mark_argument_slot_available_before_value_parse(
    slot: usize,
    expectations: &[ParameterExpectation],
    occupied_parameter_slots: &mut Option<Vec<bool>>,
    named_arguments: NamedArgumentSyntax,
    location: SourceLocation,
) -> Result<(), ExpressionParseError> {
    let Some(occupied_slots) = occupied_parameter_slots else {
        return Ok(());
    };

    if occupied_slots[slot] {
        let callee_name = match named_arguments {
            NamedArgumentSyntax::Supported { callee_name }
            | NamedArgumentSyntax::UnsupportedCall { callee_name } => callee_name,
            NamedArgumentSyntax::UnsupportedBuiltinMember { .. } => None,
        };
        return Err(CompilerDiagnostic::invalid_call_shape(
            InvalidCallShapeReason::DuplicateArgument {
                parameter_name: expectations[slot].name,
                parameter_index: slot,
            },
            callee_name,
            location,
        )
        .into());
    }

    occupied_slots[slot] = true;
    Ok(())
}

fn known_parameter_names(expectations: &[ParameterExpectation]) -> Vec<StringId> {
    expectations
        .iter()
        .filter_map(|expectation| expectation.name)
        .collect()
}

fn reject_simple_generic_argument_type_ascription(
    token_stream: &FileTokens,
    syntax_context: CallArgumentSyntaxContext,
) -> Result<(), ExpressionParseError> {
    let CallArgumentSyntaxContext::GenericFunction { function_name } = syntax_context else {
        return Ok(());
    };

    if !starts_simple_value_with_attached_type(token_stream) {
        return Ok(());
    }

    let Some(type_token) = token_stream.tokens.get(token_stream.index + 1) else {
        return Ok(());
    };

    Err(CompilerDiagnostic::invalid_generic_instantiation(
        function_name,
        InvalidGenericInstantiationReason::ExplicitCallTypeArgumentsUnsupported,
        type_token.location.clone(),
    )
    .into())
}

/// Recognize the narrow `identity(42 Int)`-style foreign syntax before the
/// expression parser tries to parse the type keyword as another expression.
///
/// This deliberately stays small: broader type-looking symbol recovery would be
/// speculative in the shared call parser and could change ordinary call errors.
fn starts_simple_value_with_attached_type(token_stream: &FileTokens) -> bool {
    let Some(value_token) = token_stream.tokens.get(token_stream.index) else {
        return false;
    };
    let Some(type_token) = token_stream.tokens.get(token_stream.index + 1) else {
        return false;
    };
    let Some(boundary_token) = token_stream.tokens.get(token_stream.index + 2) else {
        return false;
    };

    matches!(
        value_token.kind,
        TokenKind::IntLiteral(_)
            | TokenKind::FloatLiteral(_)
            | TokenKind::StringSliceLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::CharLiteral(_)
            | TokenKind::NoneLiteral
    ) && matches!(
        type_token.kind,
        TokenKind::DatatypeInt
            | TokenKind::DatatypeFloat
            | TokenKind::DatatypeBool
            | TokenKind::DatatypeString
            | TokenKind::DatatypeChar
            | TokenKind::DatatypeNone
    ) && matches!(
        boundary_token.kind,
        TokenKind::Comma | TokenKind::CloseParenthesis | TokenKind::Newline
    )
}

fn resolve_user_function_call_arguments(
    function_name: &InternedPath,
    raw_args: &[CallArgument],
    parameters: &[Declaration],
    location: SourceLocation,
    string_table: &mut StringTable,
    type_interner: &mut AstTypeInterner<'_>,
    _scope_context: Option<&ScopeContext>,
) -> Result<Vec<CallArgument>, ExpressionParseError> {
    let callee_name = function_name
        .name_str(string_table)
        .map(|name| name.to_owned())
        .unwrap_or_else(|| String::from("<unknown>"));
    let expectations = expectations_from_user_parameters(parameters);
    let type_check_context = type_interner.type_check_context();

    resolve_call_arguments(
        CallDiagnosticContext::function(&callee_name),
        raw_args,
        &expectations,
        location,
        CallArgumentResolutionContext {
            string_table,
            type_environment: type_check_context.type_environment,
            compatibility_cache: type_check_context.compatibility_cache,
        },
    )
    .map_err(ExpressionParseError::from)
}

/// Parses an external-function call using the shared argument resolver plus external-only validation.
pub fn parse_external_function_call(
    input: ExternalFunctionCallParseInput<'_, '_>,
) -> Result<AstNode, ExpressionParseError> {
    parse_external_function_call_typed(input)
}

fn parse_external_function_call_typed(
    input: ExternalFunctionCallParseInput<'_, '_>,
) -> Result<AstNode, ExpressionParseError> {
    let ExternalFunctionCallParseInput {
        token_stream,
        external_function_id,
        external_function,
        context,
        value_required,
        allow_boundary_catch,
        warnings,
        type_interner,
        string_table,
    } = input;
    let location = token_stream.current_location();

    // ------------------------
    //  Parse raw arguments
    // ------------------------
    // External metadata does not expose public parameter names yet, so named arguments remain
    // intentionally unsupported.
    let expectations = {
        let type_environment = type_interner.environment_mut_for_derived_types();
        expectations_from_host_function(external_function, type_environment)
    };
    let callee_name = string_table.intern(&external_function.name);
    let raw_args = parse_call_arguments_typed_with_expectations(
        token_stream,
        context,
        type_interner,
        string_table,
        &expectations,
        NamedArgumentSyntax::UnsupportedCall {
            callee_name: Some(callee_name),
        },
    )?;

    // ------------------------
    //  Resolve and validate arguments
    // ------------------------
    let type_check_context = type_interner.type_check_context();
    let args = resolve_call_arguments(
        CallDiagnosticContext::host_function(&external_function.name),
        &raw_args,
        &expectations,
        location.clone(),
        CallArgumentResolutionContext {
            string_table,
            type_environment: type_check_context.type_environment,
            compatibility_cache: type_check_context.compatibility_cache,
        },
    )
    .map_err(ExpressionParseError::from)?;
    validate_host_specific_call_rules(
        external_function_id,
        external_function,
        &args,
        location.clone(),
        string_table,
        type_interner.environment(),
    )?;

    // ------------------------
    //  Validate signature and returns
    // ------------------------
    let builtin_error_type = resolve_builtin_error_type_typed(context, &location, string_table)?;
    validate_external_signature_types_are_registered(external_function, context, location.clone())?;
    let diagnostic_result_types = external_function.success_return_data_types();
    let result_type_ids = external_function.success_return_type_ids(
        type_interner.environment_mut_for_derived_types(),
        builtin_error_type.type_id,
    );
    validate_external_return_slots_are_visible(
        external_function,
        &diagnostic_result_types,
        &result_type_ids,
        location.clone(),
    )?;

    // ------------------------
    //  Apply fallible handling
    // ------------------------
    let error_return_type_id = external_function.error_return_type_id(
        type_interner.environment_mut_for_derived_types(),
        builtin_error_type.type_id,
    );

    if external_function.is_fallible() {
        let Some(error_return_type_id) = error_return_type_id else {
            return Err(CompilerError::compiler_error(format!(
                "Fallible external function '{}' has no frontend-visible concrete error slot.",
                external_function.name
            ))
            .into());
        };

        let call = HandledFallibleHostCall {
            name: external_function_id,
            args,
            result_type_ids,
            error_type_id: error_return_type_id,
            call_location: location.clone(),
        };

        if token_stream_starts_fallible_handling_suffix(token_stream) {
            return parse_fallible_handling_suffix_for_host_call(
                token_stream,
                context,
                FallibleHostCallSite {
                    call,
                    value_required,
                    allow_boundary_catch,
                },
                warnings,
                type_interner,
                string_table,
            );
        }

        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::UnhandledErrorReturn,
            token_stream.current_location(),
        )
        .into());
    } else if matches!(
        token_stream.current_token_kind(),
        TokenKind::Bang | TokenKind::Catch
    ) {
        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::NotResultExpression,
            token_stream.current_location(),
        )
        .into());
    }

    Ok(AstNode {
        kind: NodeKind::HostFunctionCall {
            name: external_function_id,
            args,
            result_type_ids,
            location: location.clone(),
        },
        location,
        scope: context.scope.clone(),
    })
}

/// Verifies that every declared return slot has a corresponding frontend-visible type.
fn validate_external_return_slots_are_visible(
    external_function: &ExternalFunctionDef,
    diagnostic_result_types: &[DataType],
    result_type_ids: &[TypeId],
    location: SourceLocation,
) -> Result<(), ExpressionParseError> {
    if external_function.returns.len() != diagnostic_result_types.len()
        || external_function.returns.len() != result_type_ids.len()
    {
        return Err(CompilerError::compiler_error(format!(
            "External function '{}' declares a return slot that is not frontend-visible at {:?}.",
            external_function.name, location
        ))
        .into());
    }

    Ok(())
}

/// Ensures every type referenced in an external function signature is registered
/// in the external package registry.
fn validate_external_signature_types_are_registered(
    external_function: &ExternalFunctionDef,
    context: &ScopeContext,
    location: SourceLocation,
) -> Result<(), ExpressionParseError> {
    for parameter in &external_function.parameters {
        validate_external_signature_type_is_registered(
            external_function,
            &parameter.language_type,
            context,
            location.clone(),
        )?;
    }

    for slot in &external_function.returns {
        validate_external_signature_type_is_registered(
            external_function,
            &slot.value_type,
            context,
            location.clone(),
        )?;
    }

    if let Some(error_type) = &external_function.error_return_type {
        validate_external_signature_type_is_registered(
            external_function,
            error_type,
            context,
            location,
        )?;
    }

    Ok(())
}

/// Checks that a single external signature type is known to the frontend.
fn validate_external_signature_type_is_registered(
    external_function: &ExternalFunctionDef,
    signature_type: &ExternalSignatureType,
    context: &ScopeContext,
    location: SourceLocation,
) -> Result<(), ExpressionParseError> {
    let ExternalSignatureType::External(type_id) = signature_type else {
        return Ok(());
    };

    if context
        .external_package_registry
        .get_type_by_id(*type_id)
        .is_some()
    {
        return Ok(());
    }

    Err(CompilerError::compiler_error(format!(
        "External function '{}' references unknown external type {:?} at {:?}.",
        external_function.name, type_id, location
    ))
    .into())
}

/// Validates external-specific semantic rules that sit on top of shared call validation.
fn validate_host_specific_call_rules(
    function_id: ExternalFunctionId,
    _function: &ExternalFunctionDef,
    args: &[CallArgument],
    location: SourceLocation,
    _string_table: &StringTable,
    type_environment: &TypeEnvironment,
) -> Result<(), ExpressionParseError> {
    validate_host_specific_call_rules_typed(
        function_id,
        _function,
        args,
        location,
        _string_table,
        type_environment,
    )
}

fn validate_host_specific_call_rules_typed(
    function_id: ExternalFunctionId,
    _function: &ExternalFunctionDef,
    args: &[CallArgument],
    location: SourceLocation,
    _string_table: &StringTable,
    type_environment: &TypeEnvironment,
) -> Result<(), ExpressionParseError> {
    if function_id == ExternalFunctionId::Io {
        for argument in args.iter() {
            let arg_type_id = argument.value.type_id;

            if type_environment
                .fallible_carrier_slots(arg_type_id)
                .is_some()
            {
                return Err(CompilerDiagnostic::invalid_result_operand(
                    InvalidResultOperandReason::ResultNotUnwrapped,
                    UnsupportedOperatorCategory::Other,
                    arg_type_id,
                    location.clone(),
                )
                .into());
            }

            let builtins = type_environment.builtins();
            let is_renderable = arg_type_id == builtins.string
                || arg_type_id == builtins.int
                || arg_type_id == builtins.float
                || arg_type_id == builtins.bool
                || arg_type_id == builtins.char;

            if !is_renderable {
                return Err(CompilerDiagnostic::type_mismatch(
                    type_environment.builtins().string,
                    arg_type_id,
                    TypeMismatchContext::FunctionArgument,
                    location.clone(),
                )
                .into());
            }
        }

        return Ok(());
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/cast_boundary_tests.rs"]
mod cast_boundary_tests;

#[cfg(test)]
#[path = "tests/function_call_tests.rs"]
mod function_call_tests;
