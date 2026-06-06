//! Type-annotation parsing for declaration and signature syntax.
//!
//! WHAT: converts token streams into unresolved parsed type references with capacity
//!      expression tokens stored directly on collection variants.
//! WHY: parsing stays separate from semantic type resolution so header and AST
//!      callers can share syntax without rebuilding type-environment policy here.

use super::*;
use crate::compiler_frontend::datatypes::parsed::ParsedCollectionCapacity;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::Token;

// -------------------------
//  Type annotation parsing
// -------------------------

/// Parse a type annotation and return the parsed type reference.
///
/// WHAT: produces `ParsedTypeRef` — unresolved parsed syntax, not semantic identity.
/// WHY: resolution into `TypeId` or `DataType` happens later when the environment is available.
pub(crate) fn parse_type_annotation(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
    string_table: &StringTable,
) -> Result<ParsedTypeRef, CompilerDiagnostic> {
    // Regular declarations can be inferred datatypes, so they can break out early
    // if the next token indicates an assignment or boundary.
    if matches!(context, TypeAnnotationContext::DeclarationTarget)
        && matches!(
            token_stream.current_token_kind(),
            TokenKind::Assign | TokenKind::Newline | TokenKind::Comma
        )
    {
        return Ok(ParsedTypeRef::Inferred);
    }

    parse_required_type(token_stream, context, string_table)
}

fn parse_required_type(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
    string_table: &StringTable,
) -> Result<ParsedTypeRef, CompilerDiagnostic> {
    parse_required_type_with_generic_application(token_stream, context, string_table, true)
}

fn parse_required_type_with_generic_application(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
    string_table: &StringTable,
    allow_generic_application: bool,
) -> Result<ParsedTypeRef, CompilerDiagnostic> {
    let parsed_atom = parse_type_atom(token_stream, context, string_table)?;

    parse_type_postfixes(
        token_stream,
        parsed_atom,
        context,
        string_table,
        allow_generic_application,
    )
}

fn parse_type_atom(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
    string_table: &StringTable,
) -> Result<ParsedTypeRef, CompilerDiagnostic> {
    let location = token_stream.current_location();

    match token_stream.current_token_kind() {
        TokenKind::DatatypeInt => {
            token_stream.advance();
            Ok(ParsedTypeRef::BuiltinInt { location })
        }

        TokenKind::DatatypeFloat => {
            token_stream.advance();
            Ok(ParsedTypeRef::BuiltinFloat { location })
        }

        TokenKind::DatatypeBool => {
            token_stream.advance();
            Ok(ParsedTypeRef::BuiltinBool { location })
        }

        TokenKind::DatatypeString => {
            token_stream.advance();
            Ok(ParsedTypeRef::BuiltinString { location })
        }

        TokenKind::DatatypeChar => {
            token_stream.advance();
            Ok(ParsedTypeRef::BuiltinChar { location })
        }

        TokenKind::DatatypeNone => Err(CompilerDiagnostic::invalid_type_annotation(
            context,
            InvalidTypeAnnotationReason::NoneNotAllowed,
            token_stream.current_location(),
        )),

        TokenKind::Must | TokenKind::TraitThis => {
            if matches!(context, TypeAnnotationContext::TraitRequirement)
                && token_stream.current_token_kind() == &TokenKind::TraitThis
            {
                let location = token_stream.current_location();
                token_stream.advance();
                return Ok(ParsedTypeRef::This { location });
            }

            let _keyword = reserved_trait_keyword_or_dispatch_mismatch(
                token_stream.current_token_kind(),
                token_stream.current_location(),
                compilation_stage(context),
                "type annotation parsing",
            )
            .map_err(|error| compiler_error_to_diagnostic(&error))?;

            Err(CompilerDiagnostic::invalid_type_annotation(
                context,
                InvalidTypeAnnotationReason::ReservedTraitKeyword,
                token_stream.current_location(),
            ))
        }

        TokenKind::OpenCurly => parse_collection_type(token_stream, context, string_table),

        TokenKind::As => Err(CompilerDiagnostic::invalid_type_annotation(
            context,
            InvalidTypeAnnotationReason::AsNotValidHere,
            token_stream.current_location(),
        )),
        TokenKind::Type => Err(type_keyword_deferred_error(token_stream, context)),
        TokenKind::Of => Err(CompilerDiagnostic::unexpected_token(
            token_stream.current_token_kind().to_owned(),
            token_stream.current_location(),
        )),
        TokenKind::Symbol(type_name) => {
            let type_name = *type_name;
            token_stream.advance();

            // Check for namespace-qualified type syntax: `Namespace.Type`.
            if token_stream.current_token_kind() == &TokenKind::Dot
                && matches!(
                    token_stream.peek_next_token().cloned(),
                    Some(TokenKind::Symbol(_))
                )
            {
                let member_name = match token_stream.peek_next_token().cloned() {
                    Some(TokenKind::Symbol(name)) => name,
                    _ => unreachable!(),
                };
                token_stream.advance(); // consume '.'
                token_stream.advance();
                return Ok(ParsedTypeRef::Namespaced {
                    namespace: type_name,
                    name: member_name,
                    location: location.clone(),
                });
            }

            Ok(ParsedTypeRef::Named {
                name: type_name,
                location,
            })
        }
        TokenKind::Colon if matches!(context, TypeAnnotationContext::DeclarationTarget) => {
            Err(CompilerDiagnostic::invalid_type_annotation(
                context,
                InvalidTypeAnnotationReason::UnexpectedColon,
                token_stream.current_location(),
            ))
        }
        other
            if matches!(context, TypeAnnotationContext::DeclarationTarget)
                && matches!(
                    other,
                    TokenKind::Dot
                        | TokenKind::AddAssign
                        | TokenKind::SubtractAssign
                        | TokenKind::DivideAssign
                        | TokenKind::IntDivideAssign
                        | TokenKind::MultiplyAssign
                ) =>
        {
            Err(CompilerDiagnostic::invalid_type_annotation(
                context,
                InvalidTypeAnnotationReason::InvalidTokenAfterName {
                    token: other.to_owned(),
                },
                token_stream.current_location(),
            ))
        }
        _ => Err(CompilerDiagnostic::invalid_type_annotation(
            context,
            InvalidTypeAnnotationReason::ExpectedTypeAnnotation {
                found: token_stream.current_token_kind().to_owned(),
            },
            token_stream.current_location(),
        )),
    }
}

fn parse_type_postfixes(
    token_stream: &mut FileTokens,
    parsed_type: ParsedTypeRef,
    context: TypeAnnotationContext,
    string_table: &StringTable,
    allow_generic_application: bool,
) -> Result<ParsedTypeRef, CompilerDiagnostic> {
    let with_generic_arguments = parse_generic_arguments(
        token_stream,
        parsed_type,
        context,
        string_table,
        allow_generic_application,
    )?;
    parse_optional_type_suffix(token_stream, with_generic_arguments, context)
}

/// Parse a type annotation enclosed in `{...}`.
///
/// WHAT: handles both growable and fixed collection syntax at the parser boundary.
///
/// Two main cases:
///  1. Growable: `{T}` — the entire inner content parses as a single type.
///  2. Fixed:    `{N T}` — tokens before the element type become the capacity expression.
///
/// Capacity-only shorthand (`{N}`) is only valid for declaration targets, where the
/// element type is inferred.
fn parse_collection_type(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
    string_table: &StringTable,
) -> Result<ParsedTypeRef, CompilerDiagnostic> {
    let location = token_stream.current_location();
    token_stream.advance(); // consume '{'

    let inner_tokens = collect_collection_inner_tokens(token_stream)?;
    token_stream.advance(); // consume the outer '}'

    if inner_tokens.is_empty() {
        return Ok(ParsedTypeRef::Collection {
            element: Box::new(ParsedTypeRef::Inferred),
            location,
            fixed_capacity: None,
        });
    }

    // Phase 1 parser boundary rule:
    //   - If the entire inner content parses as a valid element type, this is `{T}` (growable).
    //   - Otherwise, find the first valid element type suffix; tokens before it are capacity.
    //   - If no element type suffix is found, capacity-only shorthand is allowed only in
    //     declaration target context (with `Inferred` element type).

    // Try 1: if the contents start as an element type, that type must consume the whole
    // collection body. This keeps old post-element capacity syntax like `{Int 64}` from
    // silently becoming capacity-only shorthand.
    if collection_type_slice_can_start_type(&inner_tokens, context, string_table) {
        let parsed_slice = parse_type_slice(&inner_tokens, token_stream, context, string_table)?;
        if let Some(extra_token) = parsed_slice.next_token {
            return Err(CompilerDiagnostic::expected_token(
                TokenKind::CloseCurly,
                Some(extra_token.kind),
                extra_token.location,
            ));
        }

        let element = parsed_slice.parsed_type;
        reject_trait_this_composition(&element, context, location.clone())?;
        return Ok(ParsedTypeRef::Collection {
            element: Box::new(element),
            location,
            fixed_capacity: None,
        });
    }

    // Try 2: find the first valid element type suffix by scanning left-to-right.
    // Tokens before the suffix become the capacity expression.
    for split_idx in 1..inner_tokens.len() {
        let type_tokens = &inner_tokens[split_idx..];
        if !collection_type_slice_can_start_type(type_tokens, context, string_table) {
            continue;
        }

        if let Some(element) =
            parse_type_slice_exact(type_tokens, token_stream, context, string_table)
        {
            reject_trait_this_composition(&element, context, location.clone())?;
            return Ok(ParsedTypeRef::Collection {
                element: Box::new(element),
                location,
                fixed_capacity: parsed_capacity(&inner_tokens[..split_idx]),
            });
        }
    }

    // Try 3: capacity-only shorthand (declaration target only).
    if matches!(context, TypeAnnotationContext::DeclarationTarget) {
        return Ok(ParsedTypeRef::Collection {
            element: Box::new(ParsedTypeRef::Inferred),
            location,
            fixed_capacity: parsed_capacity(&inner_tokens),
        });
    }

    // No valid element type found in a non-declaration context.
    Err(CompilerDiagnostic::invalid_collection_type(
        InvalidCollectionTypeReason::ShorthandCapacityNotAllowed,
        location,
    ))
}

fn collect_collection_inner_tokens(
    token_stream: &mut FileTokens,
) -> Result<Vec<Token>, CompilerDiagnostic> {
    let mut inner_tokens = Vec::new();
    let mut nested_collection_depth = 0usize;

    loop {
        match token_stream.current_token_kind() {
            TokenKind::CloseCurly if nested_collection_depth == 0 => break,
            TokenKind::CloseCurly => {
                nested_collection_depth -= 1;
                inner_tokens.push(token_stream.current_token());
                token_stream.advance();
            }
            TokenKind::OpenCurly => {
                nested_collection_depth += 1;
                inner_tokens.push(token_stream.current_token());
                token_stream.advance();
            }
            TokenKind::Eof => {
                return Err(CompilerDiagnostic::expected_token(
                    TokenKind::CloseCurly,
                    Some(TokenKind::Eof),
                    token_stream.current_location(),
                ));
            }
            _ => {
                inner_tokens.push(token_stream.current_token());
                token_stream.advance();
            }
        }
    }

    Ok(inner_tokens)
}

fn parsed_capacity(tokens: &[Token]) -> Option<ParsedCollectionCapacity> {
    let first = tokens.first()?;

    Some(ParsedCollectionCapacity {
        tokens: tokens.to_vec(),
        location: first.location.clone(),
    })
}

struct ParsedTypeSlice {
    parsed_type: ParsedTypeRef,
    next_token: Option<Token>,
}

/// Parse a collected token slice as a type annotation.
///
/// WHAT: reuses the normal type parser on a temporary token stream instead of maintaining a
///       parallel type parser for collection capacity splitting.
/// WHY: collection syntax needs to detect the element-type suffix while keeping optional
///      suffixes, generic applications, namespaced types, and nested collections on the same
///      parser path as ordinary annotations.
fn parse_type_slice(
    tokens: &[Token],
    outer_stream: &FileTokens,
    context: TypeAnnotationContext,
    string_table: &StringTable,
) -> Result<ParsedTypeSlice, CompilerDiagnostic> {
    let mut slice_tokens = tokens.to_vec();
    let eof_location = tokens
        .last()
        .map(|token| token.location.clone())
        .unwrap_or_else(|| outer_stream.current_location());
    slice_tokens.push(Token::new(TokenKind::Eof, eof_location));

    let mut stream = FileTokens::new(outer_stream.src_path.clone(), slice_tokens);
    let parsed_type = parse_required_type(&mut stream, context, string_table)?;
    let next_token = if stream.current_token_kind() == &TokenKind::Eof {
        None
    } else {
        Some(stream.current_token())
    };

    Ok(ParsedTypeSlice {
        parsed_type,
        next_token,
    })
}

fn parse_type_slice_exact(
    tokens: &[Token],
    outer_stream: &FileTokens,
    context: TypeAnnotationContext,
    string_table: &StringTable,
) -> Option<ParsedTypeRef> {
    let parsed_slice = parse_type_slice(tokens, outer_stream, context, string_table).ok()?;

    parsed_slice
        .next_token
        .is_none()
        .then_some(parsed_slice.parsed_type)
}

fn collection_type_slice_can_start_type(
    tokens: &[Token],
    context: TypeAnnotationContext,
    string_table: &StringTable,
) -> bool {
    let Some(first) = tokens.first() else {
        return false;
    };

    match &first.kind {
        TokenKind::DatatypeInt
        | TokenKind::DatatypeFloat
        | TokenKind::DatatypeBool
        | TokenKind::DatatypeString
        | TokenKind::DatatypeChar
        | TokenKind::OpenCurly => true,

        TokenKind::TraitThis => matches!(context, TypeAnnotationContext::TraitRequirement),

        TokenKind::Symbol(name) => {
            if tokens.get(1).map(|token| &token.kind) == Some(&TokenKind::Dot)
                && let Some(TokenKind::Symbol(member)) = tokens.get(2).map(|token| &token.kind)
            {
                return symbol_spelling_looks_type_name(*member, string_table);
            }

            symbol_spelling_looks_type_name(*name, string_table)
        }

        _ => false,
    }
}

fn symbol_spelling_looks_type_name(name: StringId, string_table: &StringTable) -> bool {
    string_table
        .resolve(name)
        .chars()
        .next()
        .is_some_and(|first| first.is_uppercase())
}

fn parse_generic_arguments(
    token_stream: &mut FileTokens,
    parsed_type: ParsedTypeRef,
    context: TypeAnnotationContext,
    string_table: &StringTable,
    allow_generic_application: bool,
) -> Result<ParsedTypeRef, CompilerDiagnostic> {
    let location = token_stream.current_location();
    if token_stream.current_token_kind() != &TokenKind::Of {
        return Ok(parsed_type);
    }

    if !allow_generic_application {
        return Err(nested_generic_application_error(
            token_stream.current_location(),
            context,
        ));
    }

    match parsed_type {
        ParsedTypeRef::This { .. } => {
            return Err(trait_this_composition_error(
                context,
                token_stream.current_location(),
            ));
        }
        ParsedTypeRef::Named { .. } => {}
        _ => {
            return Err(CompilerDiagnostic::invalid_generic_application(
                GenericApplicationErrorReason::OnNonNamedType,
                token_stream.current_location(),
            ));
        }
    };

    token_stream.advance();

    let mut arguments = Vec::new();
    loop {
        if generic_argument_list_is_finished(token_stream.current_token_kind()) {
            if arguments.is_empty() {
                return Err(CompilerDiagnostic::invalid_generic_application(
                    GenericApplicationErrorReason::EmptyArgumentList,
                    token_stream.current_location(),
                ));
            }
            break;
        }

        let argument = parse_generic_type_argument(token_stream, context, string_table)?;
        arguments.push(argument);

        match token_stream.current_token_kind() {
            TokenKind::Comma => {
                token_stream.advance();
                if generic_argument_list_is_finished(token_stream.current_token_kind()) {
                    return Err(CompilerDiagnostic::invalid_generic_application(
                        GenericApplicationErrorReason::MissingArgumentAfterComma,
                        token_stream.current_location(),
                    ));
                }
            }
            token if generic_argument_list_is_finished(token) => break,
            TokenKind::Of => {
                return Err(nested_generic_application_error(
                    token_stream.current_location(),
                    context,
                ));
            }
            other => {
                return Err(CompilerDiagnostic::unexpected_token(
                    other.to_owned(),
                    token_stream.current_location(),
                ));
            }
        }
    }

    Ok(ParsedTypeRef::Applied {
        base: Box::new(parsed_type),
        arguments,
        location,
    })
}

fn parse_generic_type_argument(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
    string_table: &StringTable,
) -> Result<ParsedTypeRef, CompilerDiagnostic> {
    let argument_location = token_stream.current_location();
    let parsed_argument = parse_type_atom(token_stream, context, string_table)?;

    reject_trait_this_composition(&parsed_argument, context, argument_location)?;

    if token_stream.current_token_kind() == &TokenKind::Of {
        return Err(nested_generic_application_error(
            token_stream.current_location(),
            context,
        ));
    }

    Ok(parsed_argument)
}

fn generic_argument_list_is_finished(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::Assign
            | TokenKind::Newline
            | TokenKind::Colon
            | TokenKind::TypeParameterBracket
            | TokenKind::CloseCurly
            | TokenKind::Bang
            | TokenKind::QuestionMark
            | TokenKind::Eof
            | TokenKind::End
            | TokenKind::IntLiteral(_)
    )
}

fn nested_generic_application_error(
    location: SourceLocation,
    _context: TypeAnnotationContext,
) -> CompilerDiagnostic {
    CompilerDiagnostic::invalid_generic_application(
        GenericApplicationErrorReason::NestedApplication,
        location,
    )
}

fn parse_optional_type_suffix(
    token_stream: &mut FileTokens,
    parsed_type: ParsedTypeRef,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeRef, CompilerDiagnostic> {
    let location = token_stream.current_location();
    if token_stream.current_token_kind() != &TokenKind::QuestionMark {
        return Ok(parsed_type);
    }

    reject_trait_this_composition(&parsed_type, context, location.clone())?;

    if matches!(parsed_type, ParsedTypeRef::Optional { .. }) {
        return Err(CompilerDiagnostic::invalid_type_annotation(
            context,
            InvalidTypeAnnotationReason::DuplicateOptional,
            token_stream.current_location(),
        ));
    }

    token_stream.advance();
    if token_stream.current_token_kind() == &TokenKind::QuestionMark {
        return Err(CompilerDiagnostic::invalid_type_annotation(
            context,
            InvalidTypeAnnotationReason::DuplicateOptional,
            token_stream.current_location(),
        ));
    }

    Ok(ParsedTypeRef::Optional {
        inner: Box::new(parsed_type),
        location,
    })
}

fn parsed_type_contains_trait_this(parsed_type: &ParsedTypeRef) -> bool {
    match parsed_type {
        ParsedTypeRef::This { .. } => true,

        ParsedTypeRef::Applied {
            base, arguments, ..
        } => {
            parsed_type_contains_trait_this(base)
                || arguments.iter().any(parsed_type_contains_trait_this)
        }

        ParsedTypeRef::Collection { element, .. }
        | ParsedTypeRef::Optional { inner: element, .. } => {
            parsed_type_contains_trait_this(element)
        }

        ParsedTypeRef::Result { ok, err, .. } => {
            parsed_type_contains_trait_this(ok) || parsed_type_contains_trait_this(err)
        }

        _ => false,
    }
}

fn reject_trait_this_composition(
    parsed_type: &ParsedTypeRef,
    context: TypeAnnotationContext,
    location: SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    if parsed_type_contains_trait_this(parsed_type) {
        return Err(trait_this_composition_error(context, location));
    }
    Ok(())
}

fn trait_this_composition_error(
    context: TypeAnnotationContext,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::invalid_type_annotation(
        context,
        InvalidTypeAnnotationReason::TraitThisMustBeDirect,
        location,
    )
}

fn type_keyword_deferred_error(
    token_stream: &FileTokens,
    context: TypeAnnotationContext,
) -> CompilerDiagnostic {
    CompilerDiagnostic::invalid_type_annotation(
        context,
        InvalidTypeAnnotationReason::ExpectedTypeAnnotation {
            found: TokenKind::Type,
        },
        token_stream.current_location(),
    )
}

fn compilation_stage(context: TypeAnnotationContext) -> &'static str {
    match context {
        TypeAnnotationContext::DeclarationTarget => "Variable Declaration",
        TypeAnnotationContext::SignatureParameter => "Parameter Type Parsing",
        TypeAnnotationContext::SignatureReturn => "Function Signature Parsing",
        TypeAnnotationContext::TypeAliasTarget => "Type Alias Parsing",
        TypeAnnotationContext::TraitRequirement => "Trait Requirement Parsing",
    }
}
