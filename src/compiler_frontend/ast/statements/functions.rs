//! Function-signature parsing and function-call AST helpers.
//!
//! WHAT: parses function signatures, return lists, and host/user call metadata used by AST construction.
//! WHY: function syntax has enough dedicated parsing and type-shape rules to live outside the general statement parser.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::signature_members::{
    SignatureMemberContext, parse_signature_members,
};
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, parse_type_annotation,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::{return_syntax_error, return_type_error};

/// One function return slot, either a concrete value type or a parameter-alias set.
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
        warnings: &mut Vec<CompilerWarning>,
        string_table: &mut StringTable,
        scope: &InternedPath,
        parent_context: &ScopeContext,
    ) -> Result<Self, CompilerError> {
        token_stream.advance();

        let signature_context = ScopeContext::new_constant(scope.to_owned(), parent_context);
        let parameters = parse_signature_members(
            token_stream,
            string_table,
            &signature_context,
            SignatureMemberContext::FunctionParameter,
            scope,
        )?;
        warnings.extend(signature_context.take_emitted_warnings());
        token_stream.advance();

        // The shared `| ... |` parser stops on the closing `|`,
        // so the next token decides whether this signature has returns.
        match token_stream.current_token_kind() {
            TokenKind::Arrow => {}

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
            | TokenKind::DatatypeChar
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
    string_table: &mut StringTable,
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
    string_table: &mut StringTable,
) -> Result<ReturnSlot, CompilerError> {
    let current_location = token_stream.current_location();
    if let Some(symbol) = parameter_alias_symbol(token_stream.current_token_kind(), string_table) {
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
    _string_table: &StringTable,
) -> Result<ReturnSlot, CompilerError> {
    let data_type = parse_type_annotation(token_stream, TypeAnnotationContext::SignatureReturn)?;

    if token_stream.current_token_kind() == &TokenKind::Bang {
        token_stream.advance();
        return Ok(ReturnSlot::error(FunctionReturn::Value(data_type)));
    }

    Ok(ReturnSlot::success(FunctionReturn::Value(data_type)))
}

fn parse_alias_return_item(
    token_stream: &mut FileTokens,
    parameters: &[Declaration],
    string_table: &mut StringTable,
    current_location: SourceLocation,
) -> Result<ReturnSlot, CompilerError> {
    let mut parameter_indices = Vec::new();
    let mut alias_type: Option<DataType> = None;

    loop {
        let Some(current_symbol) =
            parameter_alias_symbol(token_stream.current_token_kind(), string_table)
        else {
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
                if parameter_alias_symbol(token_stream.current_token_kind(), string_table).is_none()
                {
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
    string_table: &mut StringTable,
) -> Option<crate::compiler_frontend::symbols::string_interning::StringId> {
    match token_kind {
        TokenKind::Symbol(symbol) => Some(*symbol),
        TokenKind::This => Some(string_table.intern("this")),
        _ => None,
    }
}

#[cfg(test)]
#[path = "tests/function_parsing_tests.rs"]
mod function_parsing_tests;
