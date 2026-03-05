use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_multiple_expressions;
use crate::compiler_frontend::ast::statements::structs::{
    SignatureTypeContext, parse_explicit_signature_type, parse_parameters,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::host_functions::HostFunctionDef;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TextLocation, TokenKind};
use crate::{ast_log, return_syntax_error, return_type_error};

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

    #[allow(dead_code)]
    pub fn is_alias(&self) -> bool {
        matches!(self, FunctionReturn::AliasCandidates { .. })
    }
}

#[derive(Clone, Debug, Default)]
pub struct FunctionSignature {
    pub parameters: Vec<Declaration>,
    pub returns: Vec<FunctionReturn>,
}

impl FunctionSignature {
    pub fn new(
        token_stream: &mut FileTokens,
        string_table: &mut StringTable,
        scope: &InternedPath,
    ) -> Result<Self, CompilerError> {
        // Should start at the Colon
        // Need to skip it,
        token_stream.advance();

        let parameters = parse_parameters(
            token_stream,
            &mut true,
            string_table,
            false,
            Some(&ScopeContext::new_constant(scope.to_owned())),
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

            _ => {
                return_syntax_error!(
                    format!(
                        "Expected an arrow operator or colon after function arguments. Found {:?} instead.",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location().to_error_location(string_table),
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

    pub fn return_data_types(&self) -> Vec<DataType> {
        self.returns
            .iter()
            .map(|return_value| return_value.data_type().clone())
            .collect()
    }
}

fn parse_return_list(
    token_stream: &mut FileTokens,
    parameters: &[Declaration],
    string_table: &StringTable,
) -> Result<Vec<FunctionReturn>, CompilerError> {
    let mut returns = Vec::new();

    token_stream.advance();
    if token_stream.current_token_kind() == &TokenKind::Colon {
        return_syntax_error!(
            "Functions without return values must omit the return signature",
            token_stream.current_location().to_error_location(string_table),
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
                return Ok(returns);
            }
            TokenKind::Eof => {
                return_syntax_error!(
                    "Unexpected end of function signature while parsing return declarations",
                    token_stream.current_location().to_error_location(string_table),
                    {
                        CompilationStage => "Function Signature Parsing",
                        PrimarySuggestion => "Terminate the function signature with ':'",
                    }
                )
            }
            other => {
                return_syntax_error!(
                    format!(
                        "Expected ',' or ':' after function return declaration, found '{other:?}'",
                    ),
                    token_stream.current_location().to_error_location(string_table),
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
) -> Result<FunctionReturn, CompilerError> {
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
                current_location.to_error_location(string_table),
                {
                    CompilationStage => "Function Signature Parsing",
                    PrimarySuggestion => "Functions without return values should omit '->' entirely",
                }
            );
        }

        return_syntax_error!(
            format!(
                "Unknown return declaration '{}'. Function returns must use a concrete supported type or a parameter alias.",
                symbol_name
            ),
            current_location.to_error_location(string_table),
            {
                CompilationStage => "Function Signature Parsing",
                PrimarySuggestion => "Use a supported return type such as Int or a parameter alias such as 'arg' or 'arg or other_arg'",
            }
        );
    }

    parse_value_return_type(token_stream, string_table)
}

fn parse_value_return_type(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
) -> Result<FunctionReturn, CompilerError> {
    let data_type = parse_explicit_signature_type(
        token_stream,
        string_table,
        Ownership::ImmutableOwned,
        SignatureTypeContext::Return,
    )?;

    Ok(FunctionReturn::Value(data_type))
}

fn parse_alias_return_item(
    token_stream: &mut FileTokens,
    parameters: &[Declaration],
    string_table: &StringTable,
    current_location: TextLocation,
) -> Result<FunctionReturn, CompilerError> {
    let mut parameter_indices = Vec::new();
    let mut alias_type: Option<DataType> = None;

    loop {
        let Some(current_symbol) = parameter_alias_symbol(token_stream.current_token_kind()) else {
            return_syntax_error!(
                "Expected a parameter name in an alias return declaration",
                token_stream.current_location().to_error_location(string_table),
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
                current_location.to_error_location(string_table),
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
                    current_location.to_error_location(string_table),
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
                token_stream.current_location().to_error_location(string_table),
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
                        token_stream.current_location().to_error_location(string_table),
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
            current_location.to_error_location(string_table),
            {
                CompilationStage => "Function Signature Parsing",
                PrimarySuggestion => "Write alias returns like 'arg' or 'arg or other_arg'",
            }
        );
    };

    Ok(FunctionReturn::AliasCandidates {
        parameter_indices,
        data_type,
    })
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
        DataType::CoerceToString => "String".to_string(),
        DataType::Decimal => "Decimal".to_string(),
        DataType::Collection(inner, _) => format!("Collection<{}>", format_type_for_error(inner)),
        DataType::Struct(..) => "Struct".to_string(),
        DataType::Option(inner) => format!("Option<{}>", format_type_for_error(inner)),
        DataType::Reference(data_type) => {
            format!("{} Reference", format_type_for_error(data_type),)
        }
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
    // Basic type compatibility check
    // This is a simplified version - in a full implementation, this would handle
    // more complex type relationships, ownership, mutability, etc.
    match (arg_type, param_type) {
        // CoerceToString accepts any type - this is the key for host_io_functions() function
        (_, DataType::CoerceToString) => true,

        // Exact type matches
        (DataType::StringSlice, DataType::StringSlice) => true,
        (DataType::Int, DataType::Int) => true,
        (DataType::Float, DataType::Float) => true,
        (DataType::Bool, DataType::Bool) => true,
        (DataType::Template, DataType::Template) => true,

        // Handle inferred types - they should be compatible with their target
        (DataType::Inferred, _target) | (_target, DataType::Inferred) => {
            // For now, assume inferred types are compatible
            // In a full implementation; this would check the inferred type
            true
        }

        // Numeric type promotions (if we want to allow them)
        // (DataType::Int, DataType::Float) => true,  // Int can be promoted to Float

        // All other combinations are incompatible
        _ => false,
    }
}

// Built-in functions will do their own thing
pub fn parse_function_call(
    token_stream: &mut FileTokens,
    id: &InternedPath,
    context: &ScopeContext,
    signature: &FunctionSignature,
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

    // TODO
    // Makes sure the call value is correct for the function call
    // If so, the function call args are sorted into their correct order (if some are named or optional)
    // Once this is always working then default args can be removed from the JS output
    // let args = create_func_call_args(&expressions, argument_refs, &x.current_position())?;

    // look for which arguments are being accessed from the function call
    // let return_type = create_args_from_types(returns);
    // let accessed_args = get_accessed_args(x, name, &DataType::Object(return_type), &mut Vec::new(), captured_declarations)?;

    // Inline this function call if it's pure and the function call is pure
    // if is_pure && call_value.is_pure() {
    //     let original_function = variable_declarations
    //         .iter()
    //         .find(|a| a.name == *name)
    //         .unwrap();
    //     return inline_function_call(&args, &accessed_args, &original_function.value);
    // }

    Ok(AstNode {
        kind: NodeKind::FunctionCall {
            name: id.to_owned(),
            args,
            result_types: signature.return_data_types(),
            location: token_stream.current_location(),
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
            token_stream.current_location().to_error_location(string_table),
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
            .filter(|argument| matches!(argument.value.kind, ExpressionKind::None))
            .count();

        if missing_required > 0 {
            return_syntax_error!(
                format!(
                    "This function requires {missing_required} argument(s) without defaults, but none were provided.",
                ),
                token_stream.current_location().to_error_location(string_table),
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
                token_stream.current_location().to_error_location(string_table),
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
            .map(|var| var.value.data_type.to_owned())
            .collect();

        let call_context = context.new_child_expression(required_argument_types.to_owned());

        create_multiple_expressions(token_stream, &call_context, true, string_table)
    }
}

/// Coerce an expression to a string at compile time if possible
/// This handles compile-time constant folding for CoerceToString parameters
#[allow(dead_code)]
fn coerce_to_string_at_compile_time(
    expr: &Expression,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    match &expr.kind {
        // String literals pass through unchanged (optimization: no conversion needed)
        ExpressionKind::StringSlice(_) => Ok(expr.clone()),

        // Integer literals: fold to string at compile time (42 → "42")
        ExpressionKind::Int(value) => {
            let string_value = value.to_string();
            let interned = string_table.get_or_intern(string_value);
            Ok(Expression::string_slice(
                interned,
                expr.location.clone(),
                Ownership::ImmutableOwned,
            ))
        }

        // Float literals: fold to string at compile time (3.14 → "3.14")
        ExpressionKind::Float(value) => {
            let string_value = value.to_string();
            let interned = string_table.get_or_intern(string_value);
            Ok(Expression::string_slice(
                interned,
                expr.location.clone(),
                Ownership::ImmutableOwned,
            ))
        }

        // Boolean literals: fold to string at compile time (true → "true", false → "false")
        ExpressionKind::Bool(value) => {
            let string_value = value.to_string();
            let interned = string_table.get_or_intern(string_value);
            Ok(Expression::string_slice(
                interned,
                expr.location.clone(),
                Ownership::ImmutableOwned,
            ))
        }

        // Template literals: evaluate to string at compile time if possible
        ExpressionKind::Template(template) => {
            // Try to fold the template to a string
            if let Ok(folded_string) = template.fold_into_stringid(&None, string_table) {
                Ok(Expression::string_slice(
                    folded_string,
                    expr.location.clone(),
                    Ownership::ImmutableOwned,
                ))
            } else {
                // Template contains runtime expressions, can't fold at compile time
                // Return as-is for runtime conversion
                Ok(expr.clone())
            }
        }

        // Runtime expressions, variable references, function calls, etc.
        // These cannot be folded at compile time and will need runtime conversion
        _ => Ok(expr.clone()),
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
    location: TextLocation,
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
                location.to_error_location(string_table),
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
                location.to_error_location(string_table),
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
                location.to_error_location(string_table),
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

    // Check argument types
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
                location.to_error_location(string_table),
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Convert the argument to the expected type",
                }
            );
        }
    }

    Ok(())
}
