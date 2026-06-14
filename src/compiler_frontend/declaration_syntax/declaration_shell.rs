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
    TypeAnnotationContext, parse_type_annotation,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::utilities::token_scan::{
    collect_declaration_initializer_tokens, collect_symbol_references,
};
use crate::compiler_frontend::value_mode::ValueMode;

pub use crate::compiler_frontend::utilities::token_scan::InitializerReference;

// All the component parts of a declaration before it is resolved / parsed.
// Header parsing stores the shell; AST resolves the shell into a fully typed declaration.
#[derive(Clone, Debug)]
pub struct DeclarationSyntax {
    pub binding_mode: BindingMode,
    pub type_annotation: ParsedTypeRef,
    pub initializer_tokens: Vec<Token>,
    pub initializer_references: Vec<InitializerReference>,
    pub location: SourceLocation,
}

#[derive(Clone, Debug)]
pub struct BindingTargetSyntax {
    pub name: StringId,
    pub binding_mode: BindingMode,
    pub type_annotation: ParsedTypeRef,
    pub location: SourceLocation,
}

impl DeclarationSyntax {
    pub fn value_mode(&self) -> ValueMode {
        self.binding_mode.value_mode()
    }

    pub fn semantic_type(&self) -> ParsedTypeRef {
        self.type_annotation.clone()
    }

    /// Remap type annotation, initializer tokens, initializer references,
    /// and source location into a merged string table.
    ///
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.type_annotation.remap_string_ids(remap);
        for token in &mut self.initializer_tokens {
            token.remap_string_ids(remap);
        }
        for reference in &mut self.initializer_references {
            reference.remap_string_ids(remap);
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
    let target = parse_binding_target_syntax(name, token_stream, string_table)?;

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
        initializer_references: collect_symbol_references(&initializer_tokens),
        initializer_tokens,
        location: target.location,
    })
}

pub fn parse_binding_target_syntax(
    name: StringId,
    token_stream: &mut FileTokens,
    string_table: &StringTable,
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
    } else if token_stream.current_token_kind() == &TokenKind::Reactive {
        require_binding_marker_adjacent(token_stream, BindingMode::ReactiveRuntime)?;
        token_stream.advance();
        BindingMode::ReactiveRuntime
    } else {
        BindingMode::ImmutableRuntime
    };

    let type_annotation = parse_type_annotation(
        token_stream,
        TypeAnnotationContext::DeclarationTarget,
        string_table,
    )?;

    Ok(BindingTargetSyntax {
        name,
        binding_mode,
        type_annotation,
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
pub(crate) fn require_binding_marker_adjacent(
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
            BindingMode::ReactiveRuntime => {
                CommonSyntaxMistakeReason::InvalidReactiveBindingSpacing
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
