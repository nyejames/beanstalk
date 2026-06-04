//! Declaration shell parsing for constants and variables.
//!
//! WHAT: parses the structural components of a declaration (mutability marker, type annotation,
//! initializer token slice, and initializer reference hints) into `DeclarationSyntax` and
//! `BindingTargetSyntax` shells.
//! WHY: header parsing stores these shells so that dependency sorting can see initializer
//! references, while AST resolves the full expression semantics later.
//! MUST NOT: perform type checking, constant folding, or semantic validation.

#![allow(clippy::result_large_err)]
use crate::compiler_frontend::compiler_messages::{CommonSyntaxMistakeReason, CompilerDiagnostic};
use crate::compiler_frontend::datatypes::parsed::ParsedTypeRef;
use crate::compiler_frontend::declaration_syntax::binding_mode::BindingMode;
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    CollectionCapacity, TypeAnnotationContext, parse_type_annotation_with_capacity,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap, StringTable};
use crate::compiler_frontend::token_scan::collect_declaration_initializer_tokens;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;

// All the component parts of a declaration before it is resolved / parsed.
// Header parsing stores the shell; AST resolves the shell into a fully typed declaration.
#[derive(Clone, Debug)]
pub struct DeclarationSyntax {
    pub binding_mode: BindingMode,
    pub type_annotation: ParsedTypeRef,
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
/// the expression. WHY: dependency sorting needs ordering hints, while expression parsing
/// remains the semantic authority for folding, calls, constructors, and diagnostics.
#[derive(Clone, Debug)]
pub struct InitializerReference {
    pub name: StringId,
    pub dot_member: Option<StringId>,
    pub location: SourceLocation,
    pub followed_by_call: bool,
    pub followed_by_choice_namespace: bool,
}

#[derive(Clone, Debug)]
pub struct BindingTargetSyntax {
    pub name: StringId,
    pub binding_mode: BindingMode,
    pub type_annotation: ParsedTypeRef,
    /// Collection capacity is parsed but not yet wired to codegen.
    #[allow(dead_code)]
    pub collection_capacity: Option<CollectionCapacity>,
    pub location: SourceLocation,
}

impl InitializerReference {
    /// Remap the reference name and source location into a merged string table.
    ///
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.name = remap.get(self.name);
        if let Some(dot_member) = &mut self.dot_member {
            *dot_member = remap.get(*dot_member);
        }
        self.location.remap_string_ids(remap);
    }
}

impl DeclarationSyntax {
    pub fn value_mode(&self) -> ValueMode {
        self.binding_mode.value_mode()
    }

    pub fn semantic_type(&self) -> ParsedTypeRef {
        self.type_annotation.clone()
    }

    /// Remap type annotation, collection capacity, initializer tokens, initializer references,
    /// and source location into a merged string table.
    ///
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.type_annotation.remap_string_ids(remap);
        if let Some(capacity) = &mut self.collection_capacity {
            capacity.remap_string_ids(remap);
        }
        for token in &mut self.initializer_tokens {
            token.remap_string_ids(remap);
        }
        for reference in &mut self.initializer_references {
            reference.remap_string_ids(remap);
        }
        self.location.remap_string_ids(remap);
    }
}

impl BindingTargetSyntax {
    /// Remap name, type annotation, optional collection capacity, and source location
    /// into a merged string table.
    ///
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.name = remap.get(self.name);
        self.type_annotation.remap_string_ids(remap);
        if let Some(capacity) = &mut self.collection_capacity {
            capacity.remap_string_ids(remap);
        }
        self.location.remap_string_ids(remap);
    }
}

// Declaration Syntax for general variables / constants or parameters
pub fn parse_declaration_syntax(
    token_stream: &mut FileTokens,
    name: StringId,
    string_table: &mut StringTable,
) -> Result<DeclarationSyntax, CompilerDiagnostic> {
    // This checks for mutability marker first (in the case of mutable methods)
    // Or whether the declaration has an explicit Type
    let target = parse_binding_target_syntax(name, token_stream)?;

    // Require assignment for declarations.
    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
        }
        TokenKind::Comma | TokenKind::Eof | TokenKind::Newline => {
            return Err(CompilerDiagnostic::uninitialized_variable(
                name,
                token_stream.current_location(),
            ));
        }
        _ => {
            return Err(CompilerDiagnostic::expected_token(
                TokenKind::Assign,
                Some(token_stream.current_token_kind().to_owned()),
                token_stream.current_location(),
            ));
        }
    }

    // Transitive mutation: the token scanner may intern EOF delimiters for diagnostics
    // when the initializer is unclosed at end-of-file.
    let initializer_tokens = collect_declaration_initializer_tokens(token_stream, string_table)?;
    if initializer_tokens.is_empty() {
        return Err(CompilerDiagnostic::uninitialized_variable(
            name,
            target.location.clone(),
        ));
    }

    Ok(DeclarationSyntax {
        binding_mode: target.binding_mode,
        type_annotation: target.type_annotation,
        collection_capacity: target.collection_capacity,
        initializer_references: collect_initializer_references(&initializer_tokens),
        initializer_tokens,
        location: target.location,
    })
}

pub(crate) fn collect_initializer_references(tokens: &[Token]) -> Vec<InitializerReference> {
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

        // Header dependency sorting only needs a shallow member hint. AST still owns the full
        // expression parse, but `namespace.member` constants need this member name so imports
        // like `intro.content` can create an ordering edge to the imported constant.
        let dot_member = if matches!(next, Some(TokenKind::Dot)) {
            tokens
                .get(index + 2)
                .and_then(|member_token| match &member_token.kind {
                    TokenKind::Symbol(member_name) => Some(*member_name),
                    _ => None,
                })
        } else {
            None
        };

        references.push(InitializerReference {
            name: *name,
            dot_member,
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
) -> Result<BindingTargetSyntax, CompilerDiagnostic> {
    let target_location = token_stream.current_location();

    let binding_mode = if token_stream.current_token_kind() == &TokenKind::Mutable {
        require_binding_marker_adjacent(token_stream, BindingMode::MutableRuntime)?;
        token_stream.advance();
        BindingMode::MutableRuntime
    } else if token_stream.current_token_kind() == &TokenKind::Hash {
        require_binding_marker_adjacent(token_stream, BindingMode::CompileTimeConstant)?;
        token_stream.advance();
        BindingMode::CompileTimeConstant
    } else {
        BindingMode::ImmutableRuntime
    };

    let parsed = parse_type_annotation_with_capacity(
        token_stream,
        TypeAnnotationContext::DeclarationTarget,
    )?;

    Ok(BindingTargetSyntax {
        name,
        binding_mode,
        type_annotation: parsed.parsed_type,
        collection_capacity: parsed.collection_capacity,
        location: target_location,
    })
}

// WHAT: checks that a binding-mode marker (`#` or `~`) is immediately adjacent to the token
// that follows it (`=` for inferred, or the first token of the explicit type annotation).
//
// WHY: the language requires `name #= value` and `name ~= value`, rejecting `name # = value`
// and `name ~ = value`. Tokens carry start/end positions, so adjacency is a precise structural
// check without guessing about whitespace.
//
// Returns an error when the marker is not adjacent to the next token, using the marker token's
// location as the diagnostic primary location.
fn require_binding_marker_adjacent(
    token_stream: &FileTokens,
    mode: BindingMode,
) -> Result<(), CompilerDiagnostic> {
    let Some(current_token) = token_stream.tokens.get(token_stream.index) else {
        return Ok(());
    };
    let Some(next_token) = token_stream.tokens.get(token_stream.index + 1) else {
        return Ok(());
    };

    let on_same_line =
        current_token.location.end_pos.line_number == next_token.location.start_pos.line_number;
    let adjacent = on_same_line
        && current_token.location.end_pos.char_column + 1
            == next_token.location.start_pos.char_column;

    if !adjacent {
        let reason = match mode {
            BindingMode::MutableRuntime => CommonSyntaxMistakeReason::InvalidMutableBindingSpacing,
            BindingMode::CompileTimeConstant => {
                CommonSyntaxMistakeReason::InvalidCompileTimeBindingSpacing
            }
            BindingMode::ImmutableRuntime => return Ok(()),
        };
        return Err(CompilerDiagnostic::common_syntax_mistake(
            reason,
            current_token.location.clone(),
        ));
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/shell_remap_tests.rs"]
mod shell_remap_tests;
