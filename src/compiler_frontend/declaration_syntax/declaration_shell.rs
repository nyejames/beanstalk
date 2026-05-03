use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    CollectionCapacity, TypeAnnotationContext, parse_type_annotation_with_capacity,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::token_scan::collect_declaration_initializer_tokens;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{return_rule_error, return_syntax_error};

// All the component parts of a declaration before it is resolved / parsed.
// The compiler used to fully parse/resolve/type-check declarations immediately at AST time.
// Constants need this split representation because they are parsed in headers before full
// dependency/type resolution is available.
#[derive(Clone, Debug)]
pub struct DeclarationSyntax {
    pub mutable_marker: bool,
    pub type_annotation: DataType,
    /// Collection capacity is parsed but not yet wired to codegen.
    #[allow(dead_code)]
    pub collection_capacity: Option<CollectionCapacity>,
    pub initializer_tokens: Vec<Token>,
    pub initializer_references: Vec<InitializerReference>,
    pub location: SourceLocation,
}

/// A lightweight value-reference hint extracted from declaration initializer tokens.
///
/// WHAT: records symbol-shaped references in a constant initializer without resolving or parsing
/// the expression. WHY: the AST constant graph needs ordering hints, while expression parsing
/// remains the semantic authority for folding, calls, constructors, and diagnostics.
#[derive(Clone, Debug)]
pub struct InitializerReference {
    pub name: StringId,
    pub location: SourceLocation,
    pub followed_by_call: bool,
    pub followed_by_choice_namespace: bool,
}

#[derive(Clone, Debug)]
pub struct BindingTargetSyntax {
    pub name: StringId,
    pub mutable_marker: bool,
    pub type_annotation: DataType,
    /// Collection capacity is parsed but not yet wired to codegen.
    #[allow(dead_code)]
    pub collection_capacity: Option<CollectionCapacity>,
    pub location: SourceLocation,
}

impl DeclarationSyntax {
    pub fn value_mode(&self) -> ValueMode {
        if self.mutable_marker {
            ValueMode::MutableOwned
        } else {
            ValueMode::ImmutableOwned
        }
    }

    pub fn semantic_type(&self) -> DataType {
        self.type_annotation.clone()
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
        collection_capacity: target.collection_capacity,
        initializer_references: collect_initializer_references(&initializer_tokens),
        initializer_tokens,
        location: target.location,
    })
}

fn collect_initializer_references(tokens: &[Token]) -> Vec<InitializerReference> {
    let mut references = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        let TokenKind::Symbol(name) = &token.kind else {
            continue;
        };

        let previous = index
            .checked_sub(1)
            .and_then(|previous_index| tokens.get(previous_index))
            .map(|previous_token| &previous_token.kind);
        if matches!(previous, Some(TokenKind::Dot | TokenKind::DoubleColon)) {
            continue;
        }

        let next = tokens.get(index + 1).map(|next_token| &next_token.kind);
        if matches!(next, Some(TokenKind::Assign)) {
            continue;
        }

        references.push(InitializerReference {
            name: *name,
            location: token.location.clone(),
            followed_by_call: matches!(next, Some(TokenKind::OpenParenthesis)),
            followed_by_choice_namespace: matches!(next, Some(TokenKind::DoubleColon)),
        });
    }

    references
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

    let parsed = parse_type_annotation_with_capacity(
        token_stream,
        TypeAnnotationContext::DeclarationTarget,
    )?;

    Ok(BindingTargetSyntax {
        name,
        mutable_marker,
        type_annotation: parsed.data_type,
        collection_capacity: parsed.collection_capacity,
        location: target_location,
    })
}
