//! Function-signature parsing and function-call AST helpers.
//!
//! WHAT: parses function signatures, return lists, and host/user call metadata used by AST construction.
//! WHY: function syntax has enough dedicated parsing and type-shape rules to live outside the general statement parser.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ResultCallHandling,
};
use crate::compiler_frontend::ast::expressions::parse_expression::create_multiple_expressions;
use crate::compiler_frontend::ast::statements::result_handling::{
    ResultHandledCall, is_result_propagation_boundary, parse_named_result_handler_call,
    parse_result_fallback_values,
};
use crate::compiler_frontend::ast::statements::structs::{
    SignatureTypeContext, parse_explicit_signature_type, parse_parameters,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::host_functions::HostFunctionDef;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::{ast_log, return_rule_error, return_syntax_error, return_type_error};

// Arg names and types are required
// Can have default values
#[derive(Clone, Debug, PartialEq)]
pub enum FunctionReturn {
    Value(DataType),
    AliasCandidates {
        parameter_indices: Vec<usize>,
        data_type: DataType,
    },
}

impl FunctionReturn {
    pub fn data_type(&self) -> &DataType {
        match self {
            FunctionReturn::Value(data_type) => data_type,
            FunctionReturn::AliasCandidates { data_type, .. } => data_type,
        }
    }

    pub fn alias_candidates(&self) -> Option<&[usize]> {
        match self {
            FunctionReturn::Value(_) => None,
            FunctionReturn::AliasCandidates {
                parameter_indices, ..
            } => Some(parameter_indices.as_slice()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnChannel {
    Success,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReturnSlot {
    pub value: FunctionReturn,
    pub channel: ReturnChannel,
}

impl ReturnSlot {
    pub fn success(value: FunctionReturn) -> Self {
        Self {
            value,
            channel: ReturnChannel::Success,
        }
    }

    pub fn error(value: FunctionReturn) -> Self {
        Self {
            value,
            channel: ReturnChannel::Error,
        }
    }

    pub fn data_type(&self) -> &DataType {
        self.value.data_type()
    }
}

impl PartialEq<FunctionReturn> for ReturnSlot {
    fn eq(&self, other: &FunctionReturn) -> bool {
        self.channel == ReturnChannel::Success && &self.value == other
    }
}

impl PartialEq<ReturnSlot> for FunctionReturn {
    fn eq(&self, other: &ReturnSlot) -> bool {
        other == self
    }
}

#[derive(Clone, Debug, Default)]
pub struct FunctionSignature {
    pub parameters: Vec<Declaration>,
    pub returns: Vec<ReturnSlot>,
}

impl FunctionSignature {
    pub fn new(
        token_stream: &mut FileTokens,
        string_table: &mut StringTable,
        scope: &InternedPath,
        parent_context: &ScopeContext,
    ) -> Result<Self, CompilerError> {
        // Should start at the Colon
        // Need to skip it,
        token_stream.advance();

        let signature_context = ScopeContext::new_constant(scope.to_owned(), parent_context);
        let parameters = parse_parameters(
            token_stream,
            &mut true,
            string_table,
            false,
            &signature_context,
        )?;
        token_stream.advance();

        // parse_parameters leaves us on the closing `|`,
        // so we're now at the Arrow or Colon token
        match token_stream.current_token_kind() {
            TokenKind::Arrow => {}

            // Function does not return anything
            TokenKind::Colon => {
                token_stream.advance();
                return Ok(FunctionSignature {
                    parameters,
                    returns: Vec::new(),
                });
            }

            TokenKind::DatatypeInt
            | TokenKind::DatatypeFloat
            | TokenKind::DatatypeBool
            | TokenKind::DatatypeString
            | TokenKind::DatatypeNone
            | TokenKind::OpenCurly
            | TokenKind::Symbol(_) => {
                return_syntax_error!(
                    format!(
                        "Expected '->' or ':' after function parameters, but found what looks like a return declaration token '{:?}'.",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Signature Parsing",
                        PrimarySuggestion => "Add '->' before return declarations, for example '|args| -> Int:'",
                        AlternativeSuggestion => "If the function returns nothing, remove the return declaration and end the signature with ':'",
                        SuggestedInsertion => "->",
                        SuggestedLocation => "after the closing '|' of the parameter list",
                    }
                )
            }

            TokenKind::Newline | TokenKind::Eof | TokenKind::End => {
                return_syntax_error!(
                    "Function signature ended unexpectedly after parameters",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Signature Parsing",
                        PrimarySuggestion => "End the signature with ':' for no returns, or use '-> ReturnType:'",
                        SuggestedInsertion => ":",
                        SuggestedLocation => "after the closing '|' of the parameter list",
                    }
                )
            }

            _ => {
                return_syntax_error!(
                    format!(
                        "Expected an arrow operator or colon after function arguments. Found {:?} instead.",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Signature Parsing",
                        PrimarySuggestion => "Use '->' for functions with return values or ':' for functions without return values",
                    }
                )
            }
        }

        let returns = parse_return_list(token_stream, &parameters, string_table)?;

        Ok(FunctionSignature {
            parameters,
            returns,
        })
    }

    /// Success-channel return types only.
    pub fn return_data_types(&self) -> Vec<DataType> {
        self.success_returns()
            .iter()
            .map(|return_value| return_value.data_type().clone())
            .collect()
    }

    pub fn success_returns(&self) -> Vec<&FunctionReturn> {
        self.returns
            .iter()
            .filter(|slot| slot.channel == ReturnChannel::Success)
            .map(|slot| &slot.value)
            .collect()
    }

    pub fn error_return(&self) -> Option<&FunctionReturn> {
        self.returns
            .iter()
            .find(|slot| slot.channel == ReturnChannel::Error)
            .map(|slot| &slot.value)
    }

    pub fn error_return_index(&self) -> Option<usize> {
        self.returns
            .iter()
            .position(|slot| slot.channel == ReturnChannel::Error)
    }

    pub fn has_error_slot(&self) -> bool {
        self.error_return().is_some()
    }
}

fn parse_return_list(
    token_stream: &mut FileTokens,
    parameters: &[Declaration],
    string_table: &StringTable,
) -> Result<Vec<ReturnSlot>, CompilerError> {
    let mut returns = Vec::new();

    token_stream.advance();
    if token_stream.current_token_kind() == &TokenKind::Colon {
        return_syntax_error!(
            "Functions without return values must omit the return signature",
            token_stream.current_location(),
            {
                CompilationStage => "Function Signature Parsing",
                PrimarySuggestion => "Remove '->' and end the function signature with ':'",
            }
        );
    }

    loop {
        returns.push(parse_single_return_item(
            token_stream,
            parameters,
            string_table,
        )?);

        match token_stream.current_token_kind() {
            TokenKind::Comma => {
                token_stream.advance();
            }
            TokenKind::Colon => {
                token_stream.advance();
                validate_return_slots(&returns, token_stream, string_table)?;
                return Ok(returns);
            }
            TokenKind::Eof => {
                return_syntax_error!(
                    "Unexpected end of function signature while parsing return declarations",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Signature Parsing",
                        PrimarySuggestion => "Terminate the function signature with ':'",
                    }
                )
            }
            TokenKind::Newline | TokenKind::End => {
                return_syntax_error!(
                    "Function return declarations must end with ':'",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Signature Parsing",
                        PrimarySuggestion => "Add ':' after the final return declaration",
                        SuggestedInsertion => ":",
                    }
                )
            }
            TokenKind::Arrow => {
                return_syntax_error!(
                    "Unexpected '->' inside function return declarations",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Signature Parsing",
                        PrimarySuggestion => "Use '->' only once after parameters, then separate returns with ',' and end with ':'",
                    }
                )
            }
            other => {
                return_syntax_error!(
                    format!(
                        "Expected ',' or ':' after function return declaration, found '{other:?}'",
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Signature Parsing",
                        PrimarySuggestion => "Separate return declarations with ',' and end the signature with ':'",
                    }
                )
            }
        }
    }
}

fn parse_single_return_item(
    token_stream: &mut FileTokens,
    parameters: &[Declaration],
    string_table: &StringTable,
) -> Result<ReturnSlot, CompilerError> {
    let current_location = token_stream.current_location();
    if let Some(symbol) = parameter_alias_symbol(token_stream.current_token_kind()) {
        if parameters
            .iter()
            .any(|parameter| parameter.id.name() == Some(symbol))
        {
            return parse_alias_return_item(
                token_stream,
                parameters,
                string_table,
                current_location,
            );
        }

        let symbol_name = string_table.resolve(symbol);
        if symbol_name == "Void" {
            return_syntax_error!(
                "Void is not a valid function return declaration",
                current_location,
                {
                    CompilationStage => "Function Signature Parsing",
                    PrimarySuggestion => "Functions without return values should omit '->' entirely",
                }
            );
        }
    }

    parse_value_return_type(token_stream, string_table)
}

fn parse_value_return_type(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
) -> Result<ReturnSlot, CompilerError> {
    let data_type = parse_explicit_signature_type(
        token_stream,
        string_table,
        Ownership::ImmutableOwned,
        SignatureTypeContext::Return,
    )?;
    if token_stream.current_token_kind() == &TokenKind::Bang {
        token_stream.advance();
        return Ok(ReturnSlot::error(FunctionReturn::Value(data_type)));
    }

    Ok(ReturnSlot::success(FunctionReturn::Value(data_type)))
}

fn parse_alias_return_item(
    token_stream: &mut FileTokens,
    parameters: &[Declaration],
    string_table: &StringTable,
    current_location: SourceLocation,
) -> Result<ReturnSlot, CompilerError> {
    let mut parameter_indices = Vec::new();
    let mut alias_type: Option<DataType> = None;

    loop {
        let Some(current_symbol) = parameter_alias_symbol(token_stream.current_token_kind()) else {
            return_syntax_error!(
                "Expected a parameter name in an alias return declaration",
                token_stream.current_location(),
                {
                    CompilationStage => "Function Signature Parsing",
                    PrimarySuggestion => "Write alias returns like 'arg' or 'arg or other_arg'",
                }
            );
        };

        let Some((param_index, param)) = parameters
            .iter()
            .enumerate()
            .find(|(_, parameter)| parameter.id.name() == Some(current_symbol))
        else {
            return_syntax_error!(
                format!(
                    "Unknown return alias '{}'. Alias returns must name a function parameter.",
                    string_table.resolve(current_symbol)
                ),
                current_location,
                {
                    CompilationStage => "Function Signature Parsing",
                    PrimarySuggestion => "Use a parameter name in the return list or a concrete return type such as Int",
                }
            );
        };

        let param_type = param.value.data_type.clone();
        if let Some(existing_type) = &alias_type {
            if existing_type != &param_type {
                return_type_error!(
                    "All alias candidates in a single return slot must have the same type",
                    current_location,
                    {
                        CompilationStage => "Function Signature Parsing",
                        PrimarySuggestion => "Only combine parameters of the same type with 'or'",
                    }
                );
            }
        } else {
            alias_type = Some(param_type);
        }

        if parameter_indices.contains(&param_index) {
            return_syntax_error!(
                "Duplicate parameter used in the same alias return declaration",
                token_stream.current_location(),
                {
                    CompilationStage => "Function Signature Parsing",
                    PrimarySuggestion => "List each parameter at most once in an alias return declaration",
                }
            );
        }

        parameter_indices.push(param_index);
        token_stream.advance();

        match token_stream.current_token_kind() {
            TokenKind::Or => {
                token_stream.advance();
                if parameter_alias_symbol(token_stream.current_token_kind()).is_none() {
                    return_syntax_error!(
                        "Expected a parameter name after 'or' in an alias return declaration",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Function Signature Parsing",
                            PrimarySuggestion => "Write alias returns like 'arg or other_arg'",
                        }
                    );
                }
            }
            _ => break,
        }
    }

    let Some(data_type) = alias_type else {
        return_syntax_error!(
            "Alias return declarations must include at least one parameter name",
            current_location,
            {
                CompilationStage => "Function Signature Parsing",
                PrimarySuggestion => "Write alias returns like 'arg' or 'arg or other_arg'",
            }
        );
    };

    if token_stream.current_token_kind() == &TokenKind::Bang {
        return_syntax_error!(
            "Alias return declarations cannot be marked as an error slot in v1",
            token_stream.current_location(),
            {
                CompilationStage => "Function Signature Parsing",
                PrimarySuggestion => "Use a concrete type for the error slot (for example 'Error!')",
            }
        );
    }

    Ok(ReturnSlot::success(FunctionReturn::AliasCandidates {
        parameter_indices,
        data_type,
    }))
}

fn validate_return_slots(
    returns: &[ReturnSlot],
    token_stream: &FileTokens,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let error_slots: Vec<(usize, &ReturnSlot)> = returns
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.channel == ReturnChannel::Error)
        .collect();

    if error_slots.len() > 1 {
        return_syntax_error!(
            "Function signatures can only declare one distinguished error return slot",
            token_stream.current_location(),
            {
                CompilationStage => "Function Signature Parsing",
                PrimarySuggestion => "Keep a single 'Type!' error slot in the return signature",
            }
        );
    }

    if let Some((error_index, _)) = error_slots.first()
        && *error_index + 1 != returns.len()
    {
        return_syntax_error!(
            "The error return slot must be the final return slot in v1",
            token_stream.current_location(),
            {
                CompilationStage => "Function Signature Parsing",
                PrimarySuggestion => "Move the 'Type!' error slot to the end of the return signature",
            }
        );
    }

    for slot in returns {
        if let DataType::NamedType(type_name) = slot.data_type()
            && string_table.resolve(*type_name) == "Void"
        {
            return_syntax_error!(
                "Void is not a valid function return declaration",
                token_stream.current_location(),
                {
                    CompilationStage => "Function Signature Parsing",
                    PrimarySuggestion => "Functions without return values should omit '->' entirely",
                }
            );
        }
    }

    Ok(())
}

fn parameter_alias_symbol(
    token_kind: &TokenKind,
) -> Option<crate::compiler_frontend::string_interning::StringId> {
    match token_kind {
        TokenKind::Symbol(symbol) => Some(*symbol),
        _ => None,
    }
}

/// Format a DataType for user-friendly error messages
/// Note: This function provides basic type names without resolving interned strings.
/// For full type information with resolved strings, use DataType::display_with_table().
fn format_type_for_error(data_type: &DataType) -> String {
    match data_type {
        DataType::StringSlice => "String".to_string(),
        DataType::Int => "Int".to_string(),
        DataType::Float => "Float".to_string(),
        DataType::Bool => "Bool".to_string(),
        DataType::Char => "Char".to_string(),
        DataType::BuiltinErrorKind => "ErrorKind".to_string(),
        DataType::Template | DataType::TemplateWrapper => "Template".to_string(),
        DataType::Function(..) => "Function".to_string(),
        DataType::Parameters(..) => "Args".to_string(),
        DataType::Returns(..) => "Returns".to_string(),
        DataType::Choices(types) => {
            let type_names: Vec<String> = types
                .iter()
                .map(|t| format_type_for_error(&t.value.data_type))
                .collect();
            format!("({})", type_names.join(" | "))
        }
        DataType::Inferred => "Inferred".to_string(),
        DataType::Range => "Range".to_string(),
        DataType::None => "None".to_string(),
        DataType::True => "True".to_string(),
        DataType::False => "False".to_string(),
        DataType::Result { ok, err } => {
            format!(
                "Result<{}, {}>",
                format_type_for_error(ok),
                format_type_for_error(err)
            )
        }
        DataType::Decimal => "Decimal".to_string(),
        DataType::Collection(inner, _) => format!("Collection<{}>", format_type_for_error(inner)),
        DataType::Struct { .. } => "Struct".to_string(),
        DataType::NamedType(_) => "NamedType".to_string(),
        DataType::Option(inner) => format!("Option<{}>", format_type_for_error(inner)),
        DataType::Reference(data_type) => {
            format!("{} Reference", format_type_for_error(data_type),)
        }
        DataType::Path(_) => "Path".to_string(),
    }
}

/// Provide helpful hints for type conversion
fn get_type_conversion_hint(from_type: &DataType, to_type: &DataType) -> String {
    match (from_type, to_type) {
        (DataType::Int, DataType::StringSlice) => {
            "Try converting the integer to a string first".to_string()
        }
        (DataType::Float, DataType::StringSlice) => {
            "Try converting the float to a string first".to_string()
        }
        (DataType::Bool, DataType::StringSlice) => {
            "Try converting the boolean to a string first".to_string()
        }
        (DataType::StringSlice, DataType::Int) => {
            "Try parsing the string as an integer first".to_string()
        }
        _ => "Check the function documentation for the expected argument types".to_string(),
    }
}

/// Check if two types are compatible for function call arguments
fn types_compatible(arg_type: &DataType, param_type: &DataType) -> bool {
    if param_type.accepts_value_type(arg_type) {
        return true;
    }

    allows_mutable_collection_for_immutable_parameter(arg_type, param_type)
}

fn allows_mutable_collection_for_immutable_parameter(
    arg_type: &DataType,
    param_type: &DataType,
) -> bool {
    // WHAT: allow passing mutable collections to immutable collection parameters.
    // WHY: read-only call sites should accept strictly more-capable collection values without
    // forcing caller-side copies or broadening global type-compatibility rules.
    let (
        DataType::Collection(arg_element, arg_ownership),
        DataType::Collection(param_element, param_ownership),
    ) = (arg_type, param_type)
    else {
        return false;
    };

    arg_ownership.is_mutable()
        && !param_ownership.is_mutable()
        && param_element.accepts_value_type(arg_element.as_ref())
}

fn validate_user_function_argument_types(
    function_name: &InternedPath,
    args: &[Expression],
    parameters: &[Declaration],
    location: SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for (index, (expression, parameter)) in args.iter().zip(parameters.iter()).enumerate() {
        if !types_compatible(&expression.data_type, &parameter.value.data_type) {
            return_type_error!(
                format!(
                    "Argument {} to function '{}' has incorrect type. Expected {}, but got {}. {}",
                    index + 1,
                    function_name.name_str(string_table).unwrap_or("<unknown>"),
                    format_type_for_error(&parameter.value.data_type),
                    format_type_for_error(&expression.data_type),
                    get_type_conversion_hint(&expression.data_type, &parameter.value.data_type)
                ),
                location,
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Convert the argument to the expected type",
                }
            );
        }
    }

    Ok(())
}

// Built-in functions will do their own thing
pub fn parse_function_call(
    token_stream: &mut FileTokens,
    id: &InternedPath,
    context: &ScopeContext,
    signature: &FunctionSignature,
    value_required: bool,
    warnings: Option<&mut Vec<CompilerWarning>>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // Assumes we're starting at the first token after the name of the function call
    // Check if it's a host function first
    if let Some(host_func) = &context
        .host_registry
        .get_function(id.name_str(string_table).unwrap_or(""))
    {
        return parse_host_function_call(token_stream, host_func, context, string_table);
    }

    // Create expressions until hitting a closed parenthesis
    let args =
        create_function_call_arguments(token_stream, &signature.parameters, context, string_table)?;
    validate_user_function_argument_types(
        id,
        &args,
        &signature.parameters,
        token_stream.current_location(),
        string_table,
    )?;

    let call = ResultHandledCall {
        name: id.to_owned(),
        args,
        result_types: signature.return_data_types(),
        call_location: token_stream.current_location(),
    };

    if let Some(error_return) = signature.error_return() {
        if token_stream.current_token_kind() == &TokenKind::Bang {
            token_stream.advance();

            if is_result_propagation_boundary(token_stream.current_token_kind()) {
                let Some(expected_error_type) = context.expected_error_type.as_ref() else {
                    return_rule_error!(
                        "This call uses '!' propagation, but the surrounding function does not declare an error return slot",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Function Call Parsing",
                            PrimarySuggestion => "Declare a matching error slot in the surrounding function signature",
                        }
                    );
                };

                if expected_error_type != error_return.data_type() {
                    return_type_error!(
                        format!(
                            "Mismatched propagated error type. Called function returns '{}', but current function expects '{}'.",
                            error_return.data_type().display_with_table(string_table),
                            expected_error_type.display_with_table(string_table)
                        ),
                        token_stream.current_location(),
                        {
                            CompilationStage => "Function Call Parsing",
                            PrimarySuggestion => "Use a function with the same error type or change the surrounding function error slot type",
                        }
                    );
                }

                return Ok(AstNode {
                    kind: NodeKind::ResultHandledFunctionCall {
                        name: call.name,
                        args: call.args,
                        result_types: call.result_types,
                        handling: ResultCallHandling::Propagate,
                        location: call.call_location,
                    },
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }

            if call.result_types.is_empty() {
                return_rule_error!(
                    "This function has no success return values, so fallback values cannot be provided here",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Call Parsing",
                        PrimarySuggestion => "Use plain propagation syntax 'call(...)!' for error-only functions",
                    }
                );
            }

            let fallback_values = parse_result_fallback_values(
                token_stream,
                context,
                &call.result_types,
                "Fallback values",
                string_table,
            )?;

            return Ok(call.into_ast_node(
                ResultCallHandling::Fallback(fallback_values),
                token_stream.current_location(),
                &context.scope,
            ));
        }

        if matches!(token_stream.current_token_kind(), TokenKind::Symbol(_))
            && token_stream.peek_next_token() == Some(&TokenKind::Bang)
        {
            return parse_named_result_handler_call(
                token_stream,
                context,
                call,
                error_return.data_type(),
                value_required,
                warnings,
                string_table,
            );
        }

        return_rule_error!(
            "Calls to error-returning functions must be explicitly handled with '!' syntax",
            token_stream.current_location(),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Use 'call(...)!' to propagate or 'call(...) ! fallback' to provide fallback values",
            }
        );
    } else if token_stream.current_token_kind() == &TokenKind::Bang {
        return_rule_error!(
            "The '!' call-handling suffix is only valid for functions that declare an error return slot",
            token_stream.current_location(),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Remove '!' from this call or add an error slot to the called function",
            }
        );
    }

    Ok(AstNode {
        kind: NodeKind::FunctionCall {
            name: call.name,
            args: call.args,
            result_types: call.result_types,
            location: call.call_location,
        },
        location: token_stream.current_location(),
        scope: context.scope.clone(),
    })
}

pub fn create_function_call_arguments(
    token_stream: &mut FileTokens,
    required_arguments: &[Declaration],
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Vec<Expression>, CompilerError> {
    // Starts at the first token after the function name
    ast_log!("Creating function call arguments");

    // make sure there is an open parenthesis
    if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
        return_syntax_error!(
            format!(
                "Expected a parenthesis after function call. Found '{:?}' instead.",
                token_stream.current_token_kind()
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Add '(' after the function name",
                SuggestedInsertion => "(",
            }
        )
    }

    token_stream.advance();

    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        let missing_required = required_arguments
            .iter()
            .filter(|argument| matches!(argument.value.kind, ExpressionKind::NoValue))
            .count();

        if missing_required > 0 {
            return_syntax_error!(
                format!(
                    "This function requires {missing_required} argument(s) without defaults, but none were provided.",
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Function Call Parsing",
                    PrimarySuggestion => "Provide the required arguments or add defaults in the declaration",
                }
            )
        }

        token_stream.advance();
        return Ok(Vec::new());
    }

    if required_arguments.is_empty() {
        // Make sure there is a closing parenthesis
        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return_syntax_error!(
                format!(
                    "This function does not accept any arguments, found '{:?}' instead",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Function Call Parsing",
                    PrimarySuggestion => "Remove the arguments or check the function signature",
                }
            )
        }

        // Advance past the closing parenthesis
        token_stream.advance();

        Ok(Vec::new())
    } else {
        let required_argument_types: Vec<DataType> = required_arguments
            .iter()
            .map(|argument| match &argument.value.data_type {
                // WHAT: keep immutable-collection argument parsing permissive.
                // WHY: call compatibility allows mutable collections for immutable parameters.
                // Parsing should defer this ownership-specific check to function call validation.
                DataType::Collection(_, ownership) if !ownership.is_mutable() => DataType::Inferred,
                _ => argument.value.data_type.to_owned(),
            })
            .collect();

        let call_context = context.new_child_expression(required_argument_types.to_owned());

        create_multiple_expressions(token_stream, &call_context, true, string_table)
    }
}

/// Parse a host function call
pub fn parse_host_function_call(
    token_stream: &mut FileTokens,
    host_func: &HostFunctionDef,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let location = token_stream.current_location();

    let params_as_args = host_func.params_to_signature(string_table);

    // Parse arguments using the same logic as regular function calls
    let args = create_function_call_arguments(
        token_stream,
        &params_as_args.parameters,
        context,
        string_table,
    )?;

    // Validate the host function call
    validate_host_function_call(host_func, &args, location.clone(), string_table)?;

    // Create an interned path name from the name
    let name = InternedPath::from_single_str(host_func.name, string_table);

    Ok(AstNode {
        kind: NodeKind::HostFunctionCall {
            name,
            args,
            result_types: params_as_args.return_data_types(),
            location: location.clone(),
        },
        location,
        scope: context.scope.clone(),
    })
}

/// Validate a host function call against its signature
pub fn validate_host_function_call(
    function: &HostFunctionDef,
    args: &[Expression],
    location: SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    // Check argument count
    if args.len() != function.parameters.len() {
        let expected = function.parameters.len();
        let got = args.len();

        if expected == 0 {
            return_type_error!(
                format!(
                    "Function '{}' doesn't take any arguments, but {} {} provided. Did you mean to call it without parentheses?",
                    function.name,
                    got,
                    if got == 1 { "was" } else { "were" }
                ),
                location,
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Remove the parentheses and arguments",
                }
            );
        } else if got == 0 {
            return_type_error!(
                format!(
                    "Function '{}' expects {} argument{}, but none were provided",
                    function.name,
                    expected,
                    if expected == 1 { "" } else { "s" }
                ),
                location,
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Add the required arguments to the function call",
                }
            );
        } else {
            return_type_error!(
                format!(
                    "Function '{}' expects {} argument{}, got {}. {}",
                    function.name,
                    expected,
                    if expected == 1 { "" } else { "s" },
                    got,
                    if got > expected {
                        "Too many arguments provided"
                    } else {
                        "Not enough arguments provided"
                    }
                ),
                location,
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => if got > expected {
                        "Remove extra arguments"
                    } else {
                        "Add missing arguments"
                    },
                }
            );
        }
    }

    if function.name == crate::compiler_frontend::host_functions::IO_FUNC_NAME {
        for (i, expression) in args.iter().enumerate() {
            if expression.data_type.is_result() {
                return_type_error!(
                    format!(
                        "Argument {} to function '{}' has incorrect type. Expected a renderable value, but got {}. Result values must be handled before reaching io(...).",
                        i + 1,
                        function.name,
                        format_type_for_error(&expression.data_type)
                    ),
                    location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Handle the Result with '!' syntax before passing it to io(...)",
                    }
                );
            }

            if !matches!(
                expression.data_type,
                DataType::StringSlice
                    | DataType::Template
                    | DataType::TemplateWrapper
                    | DataType::Int
                    | DataType::Float
                    | DataType::Bool
                    | DataType::Char
                    | DataType::Path(_)
            ) {
                return_type_error!(
                    format!(
                        "Argument {} to function '{}' has incorrect type. Expected a final scalar or textual value, but got {}.",
                        i + 1,
                        function.name,
                        expression.data_type.display_with_table(string_table)
                    ),
                    location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Render collections/structs/templates earlier or pass a scalar/textual value to io(...)",
                    }
                );
            }
        }

        return Ok(());
    }

    for (i, (expression, param)) in args.iter().zip(&function.parameters).enumerate() {
        if !types_compatible(&expression.data_type, &param.language_type) {
            return_type_error!(
                format!(
                    "Argument {} to function '{}' has incorrect type. Expected {}, but got {}. {}",
                    i + 1,
                    function.name,
                    format_type_for_error(&param.language_type),
                    format_type_for_error(&expression.data_type),
                    get_type_conversion_hint(&expression.data_type, &param.language_type)
                ),
                location,
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Convert the argument to the expected type",
                }
            );
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/function_parsing_tests.rs"]
mod function_parsing_tests;
