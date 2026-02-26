use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TextLocation, Token, TokenKind};
use crate::{return_rule_error, return_syntax_error};

#[derive(Clone, Debug)]
pub struct DeclarationSyntax {
    pub name: StringId,
    pub mutable_marker: bool,
    // Concrete parsed type syntax (including collection syntax), if provided.
    pub explicit_type: DataType,
    // Named type annotations are resolved later after symbol tables are available.
    pub explicit_named_type: Option<StringId>,
    pub initializer_tokens: Vec<Token>,
    pub location: TextLocation,
}

impl DeclarationSyntax {
    pub fn to_tokens(&self) -> Vec<Token> {
        let mut tokens = Vec::with_capacity(4 + self.initializer_tokens.len());
        tokens.push(Token::new(
            TokenKind::Symbol(self.name),
            self.location.clone(),
        ));

        if self.mutable_marker {
            tokens.push(Token::new(TokenKind::Mutable, self.location.clone()));
        }

        if let Some(type_name) = self.explicit_named_type {
            tokens.push(Token::new(
                TokenKind::Symbol(type_name),
                self.location.clone(),
            ));
        } else {
            append_explicit_type_tokens(&mut tokens, &self.explicit_type, &self.location);
        }

        tokens.push(Token::new(TokenKind::Assign, self.location.clone()));
        tokens.extend(self.initializer_tokens.clone());
        tokens
    }

    pub fn to_data_type(&self, declaration_ownership: &Ownership) -> DataType {
        if self.explicit_named_type.is_some() {
            return DataType::Inferred;
        }

        match &self.explicit_type {
            DataType::Collection(inner, _) => {
                if matches!(inner.as_ref(), DataType::Inferred) {
                    DataType::Collection(Box::new((**inner).clone()), Ownership::MutableOwned)
                } else {
                    DataType::Collection(Box::new((**inner).clone()), declaration_ownership.clone())
                }
            }
            _ => self.explicit_type.clone(),
        }
    }
}

pub fn parse_declaration_syntax(
    token_stream: &mut FileTokens,
    name: StringId,
    string_table: &mut StringTable,
) -> Result<DeclarationSyntax, crate::compiler_frontend::compiler_errors::CompilerError> {
    let declaration_location = token_stream.current_location();

    let mut mutable_marker = false;
    if token_stream.current_token_kind() == &TokenKind::Mutable {
        mutable_marker = true;
        token_stream.advance();
    }

    let (explicit_type, explicit_named_type) =
        parse_explicit_type_annotation(token_stream, string_table)?;

    // Require assignment for declarations.
    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
        }
        TokenKind::Comma | TokenKind::Eof | TokenKind::Newline => {
            let var_name = string_table.resolve(name);
            return_rule_error!(
                format!("Variable '{}' must be initialized with a value", var_name),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Add '= value' after the variable declaration",
                }
            )
        }
        _ => {
            return_syntax_error!(
                format!(
                    "Unexpected token '{:?}' in declaration. Expected '=' after declaration type.",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Add '=' after the declaration before the initializer",
                    SuggestedInsertion => "=",
                }
            )
        }
    }

    let initializer_tokens = collect_initializer_tokens(token_stream);
    if initializer_tokens.is_empty() {
        let var_name = string_table.resolve(name);
        return_rule_error!(
            format!("Variable '{}' must be initialized with a value", var_name),
            declaration_location.to_error_location(string_table), {
                CompilationStage => "Variable Declaration",
                PrimarySuggestion => "Add an initializer expression after '='",
            }
        )
    }

    Ok(DeclarationSyntax {
        name,
        mutable_marker,
        explicit_type,
        explicit_named_type,
        initializer_tokens,
        location: declaration_location,
    })
}

fn parse_explicit_type_annotation(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
) -> Result<(DataType, Option<StringId>), crate::compiler_frontend::compiler_errors::CompilerError>
{
    match token_stream.current_token_kind() {
        TokenKind::Assign | TokenKind::Newline => Ok((DataType::Inferred, None)),
        TokenKind::DatatypeInt => {
            token_stream.advance();
            Ok((DataType::Int, None))
        }
        TokenKind::DatatypeFloat => {
            token_stream.advance();
            Ok((DataType::Float, None))
        }
        TokenKind::DatatypeBool => {
            token_stream.advance();
            Ok((DataType::Bool, None))
        }
        TokenKind::DatatypeString => {
            token_stream.advance();
            Ok((DataType::StringSlice, None))
        }
        TokenKind::OpenCurly => {
            token_stream.advance();

            let inner = token_stream.current_token_kind().to_datatype();
            if inner.is_some() {
                token_stream.advance();
            }

            if token_stream.current_token_kind() != &TokenKind::CloseCurly {
                return_syntax_error!(
                    "Missing closing curly brace for collection type declaration",
                    token_stream.current_location().to_error_location(string_table), {
                        CompilationStage => "Variable Declaration",
                        PrimarySuggestion => "Add '}' to close the collection type declaration",
                        SuggestedInsertion => "}",
                    }
                )
            }
            token_stream.advance();

            Ok((
                DataType::Collection(
                    Box::new(inner.unwrap_or(DataType::Inferred)),
                    Ownership::ImmutableOwned,
                ),
                None,
            ))
        }
        TokenKind::Symbol(name) => {
            let type_name = *name;
            token_stream.advance();
            Ok((DataType::Inferred, Some(type_name)))
        }
        TokenKind::Colon => {
            todo!("Labeled scope")
        }
        TokenKind::Dot
        | TokenKind::AddAssign
        | TokenKind::SubtractAssign
        | TokenKind::DivideAssign
        | TokenKind::MultiplyAssign => {
            return_syntax_error!(
                format!(
                    "Invalid token '{:?}' after declaration name. Expected a type or assignment operator.",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Use a type declaration (Int, String, etc.) or assignment operator '='",
                }
            )
        }
        _ => {
            return_syntax_error!(
                format!(
                    "Invalid token '{:?}' after declaration name. Expected a type or assignment operator.",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Use a type declaration (Int, String, etc.) or assignment operator '='",
                }
            )
        }
    }
}

fn append_explicit_type_tokens(
    tokens: &mut Vec<Token>,
    explicit_type: &DataType,
    location: &TextLocation,
) {
    match explicit_type {
        DataType::Inferred => {}
        DataType::Int => tokens.push(Token::new(TokenKind::DatatypeInt, location.clone())),
        DataType::Float => tokens.push(Token::new(TokenKind::DatatypeFloat, location.clone())),
        DataType::Bool => tokens.push(Token::new(TokenKind::DatatypeBool, location.clone())),
        DataType::StringSlice => {
            tokens.push(Token::new(TokenKind::DatatypeString, location.clone()))
        }
        DataType::Collection(inner, _) => {
            tokens.push(Token::new(TokenKind::OpenCurly, location.clone()));
            append_explicit_type_tokens(tokens, inner.as_ref(), location);
            tokens.push(Token::new(TokenKind::CloseCurly, location.clone()));
        }
        _ => {}
    }
}

fn collect_initializer_tokens(token_stream: &mut FileTokens) -> Vec<Token> {
    let mut collected = Vec::new();
    let mut paren_depth = 0usize;
    let mut curly_depth = 0usize;
    let mut template_depth = 0usize;

    while token_stream.index < token_stream.length {
        let token_kind = token_stream.current_token_kind().clone();

        let at_top_level = paren_depth == 0 && curly_depth == 0 && template_depth == 0;
        let continues_multiline_expression = if matches!(token_kind, TokenKind::Newline) {
            let prev_continues = collected
                .last()
                .is_some_and(|token: &Token| token.kind.continues_expression());
            let next_continues = token_stream
                .peek_next_token()
                .is_some_and(|next| next.continues_expression());
            prev_continues || next_continues
        } else {
            false
        };

        if at_top_level
            && matches!(
                token_kind,
                TokenKind::Comma | TokenKind::End | TokenKind::Eof
            )
        {
            break;
        }

        if at_top_level
            && matches!(token_kind, TokenKind::Newline)
            && !continues_multiline_expression
        {
            break;
        }

        match token_kind {
            TokenKind::OpenParenthesis => paren_depth += 1,
            TokenKind::CloseParenthesis => paren_depth = paren_depth.saturating_sub(1),
            TokenKind::OpenCurly => curly_depth += 1,
            TokenKind::CloseCurly => curly_depth = curly_depth.saturating_sub(1),
            TokenKind::TemplateHead => template_depth += 1,
            TokenKind::TemplateClose => template_depth = template_depth.saturating_sub(1),
            _ => {}
        }

        collected.push(token_stream.current_token());
        token_stream.advance();
    }

    collected
}
