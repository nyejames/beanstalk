use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, parse_type_annotation,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::token_scan::collect_declaration_initializer_tokens;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::{return_rule_error, return_syntax_error};

// All the component parts of a declaration before it is resolved / parsed.
// The compiler used to fully parse/resolve/type-check declarations immediately at AST time.
// Constants need this split representation because they are parsed in headers before full
// dependency/type resolution is available.
#[derive(Clone, Debug)]
pub struct DeclarationSyntax {
    pub mutable_marker: bool,
    pub type_annotation: DataType,
    pub initializer_tokens: Vec<Token>,
    pub location: SourceLocation,
}

#[derive(Clone, Debug)]
pub struct BindingTargetSyntax {
    pub name: StringId,
    pub mutable_marker: bool,
    pub type_annotation: DataType,
    pub location: SourceLocation,
}

impl DeclarationSyntax {
    pub fn to_data_type(&self, declaration_ownership: &Ownership) -> DataType {
        match &self.type_annotation {
            DataType::Collection(inner, _) => {
                if matches!(inner.as_ref(), DataType::Inferred) {
                    DataType::Collection(Box::new(*inner.clone()), Ownership::MutableOwned)
                } else {
                    DataType::Collection(Box::new(*inner.clone()), declaration_ownership.clone())
                }
            }
            other => other.clone(),
        }
    }
}

// Declaration Syntax for general variables / constants or parameters
pub fn parse_declaration_syntax(
    token_stream: &mut FileTokens,
    name: StringId,
    string_table: &mut StringTable,
) -> Result<DeclarationSyntax, CompilerError> {
    // This checks for mutability marker first (in the case of mutable methods)
    // Or whether the declaration has an explicit Type
    let target = parse_binding_target_syntax(name, token_stream)?;

    // Require assignment for declarations.
    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
        }
        TokenKind::Comma | TokenKind::Eof | TokenKind::Newline => {
            let var_name = string_table.resolve(name);
            return_rule_error!(
                format!("Variable '{}' must be initialized with a value", var_name),
                token_stream.current_location(), {
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
                token_stream.current_location(), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Add '=' after the declaration before the initializer",
                    SuggestedInsertion => "=",
                }
            )
        }
    }

    let initializer_tokens = collect_declaration_initializer_tokens(token_stream)?;
    if initializer_tokens.is_empty() {
        let var_name = string_table.resolve(name);
        return_rule_error!(
            format!("Variable '{}' must be initialized with a value", var_name),
            target.location.clone(), {
                CompilationStage => "Variable Declaration",
                PrimarySuggestion => "Add an initializer expression after '='",
            }
        )
    }

    Ok(DeclarationSyntax {
        mutable_marker: target.mutable_marker,
        type_annotation: target.type_annotation,
        initializer_tokens,
        location: target.location,
    })
}

pub fn parse_binding_target_syntax(
    name: StringId,
    token_stream: &mut FileTokens,
) -> Result<BindingTargetSyntax, CompilerError> {
    let target_location = token_stream.current_location();

    let mut mutable_marker = false;
    if token_stream.current_token_kind() == &TokenKind::Mutable {
        mutable_marker = true;
        token_stream.advance();
    }

    let type_annotation =
        parse_type_annotation(token_stream, TypeAnnotationContext::DeclarationTarget)?;

    Ok(BindingTargetSyntax {
        name,
        mutable_marker,
        type_annotation,
        location: target_location,
    })
}
