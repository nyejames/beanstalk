//! Type-annotation parsing for declaration and signature syntax.
//!
//! WHAT: converts token streams into unresolved parsed type references plus
//! collection-capacity metadata.
//! WHY: parsing stays separate from semantic type resolution so header and AST
//! callers can share syntax without rebuilding type-environment policy here.

use super::*;
use crate::compiler_frontend::symbols::string_interning::StringIdRemap;

// -------------------------
//  Type annotation parsing
// -------------------------

/// Collection capacity parsed from a collection type annotation such as `{Int 64}`.
///
/// WHAT: capacity is allocation metadata, not part of type identity.
/// WHY: keeps capacity separate from the generic type model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CollectionCapacity {
    pub value: i64,
    pub location: SourceLocation,
}

/// Result of parsing a type annotation, including optional collection capacity.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedTypeAnnotation {
    pub parsed_type: ParsedTypeRef,
    pub collection_capacity: Option<CollectionCapacity>,
}

impl CollectionCapacity {
    /// Remap the source location into a merged string table.
    ///
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.location.remap_string_ids(remap);
    }
}

impl ParsedTypeAnnotation {
    pub fn new(parsed_type: ParsedTypeRef) -> Self {
        Self {
            parsed_type,
            collection_capacity: None,
        }
    }

    /// Remap the parsed type and optional collection capacity into a merged string table.
    ///
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.parsed_type.remap_string_ids(remap);
        if let Some(capacity) = &mut self.collection_capacity {
            capacity.remap_string_ids(remap);
        }
    }
}

/// Parse a type annotation and return the parsed type reference.
///
/// WHAT: produces `ParsedTypeRef` — unresolved parsed syntax, not semantic identity.
/// WHY: resolution into `TypeId` or `DataType` happens later when the environment is available.
pub(crate) fn parse_type_annotation(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeRef, CompilerDiagnostic> {
    parse_type_annotation_with_capacity(token_stream, context).map(|parsed| parsed.parsed_type)
}

pub(crate) fn parse_type_annotation_with_capacity(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeAnnotation, CompilerDiagnostic> {
    // Regular declarations can be inferred datatypes, so they can break out early
    // if the next token indicates an assignment or boundary.
    if matches!(context, TypeAnnotationContext::DeclarationTarget)
        && matches!(
            token_stream.current_token_kind(),
            TokenKind::Assign | TokenKind::Newline | TokenKind::Comma
        )
    {
        return Ok(ParsedTypeAnnotation::new(ParsedTypeRef::Inferred));
    }

    parse_required_type(token_stream, context)
}

fn parse_required_type(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeAnnotation, CompilerDiagnostic> {
    parse_required_type_with_generic_application(token_stream, context, true)
}

fn parse_required_type_with_generic_application(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
    allow_generic_application: bool,
) -> Result<ParsedTypeAnnotation, CompilerDiagnostic> {
    let parsed_atom = parse_type_atom(token_stream, context)?;

    parse_type_postfixes(
        token_stream,
        parsed_atom,
        context,
        allow_generic_application,
    )
}

fn parse_type_atom(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeAnnotation, CompilerDiagnostic> {
    let location = token_stream.current_location();

    match token_stream.current_token_kind() {
        TokenKind::DatatypeInt => {
            token_stream.advance();
            Ok(ParsedTypeAnnotation::new(ParsedTypeRef::BuiltinInt {
                location,
            }))
        }

        TokenKind::DatatypeFloat => {
            token_stream.advance();
            Ok(ParsedTypeAnnotation::new(ParsedTypeRef::BuiltinFloat {
                location,
            }))
        }

        TokenKind::DatatypeBool => {
            token_stream.advance();
            Ok(ParsedTypeAnnotation::new(ParsedTypeRef::BuiltinBool {
                location,
            }))
        }

        TokenKind::DatatypeString => {
            token_stream.advance();
            Ok(ParsedTypeAnnotation::new(ParsedTypeRef::BuiltinString {
                location,
            }))
        }

        TokenKind::DatatypeChar => {
            token_stream.advance();
            Ok(ParsedTypeAnnotation::new(ParsedTypeRef::BuiltinChar {
                location,
            }))
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
                return Ok(ParsedTypeAnnotation::new(ParsedTypeRef::This { location }));
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

        TokenKind::OpenCurly => parse_collection_type(token_stream, context),

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
                && let Some(TokenKind::Symbol(member_name)) =
                    token_stream.peek_next_token().cloned()
            {
                token_stream.advance(); // consume '.'
                token_stream.advance();
                return Ok(ParsedTypeAnnotation::new(ParsedTypeRef::Namespaced {
                    namespace: type_name,
                    name: member_name,
                    location: location.clone(),
                }));
            }

            Ok(ParsedTypeAnnotation::new(ParsedTypeRef::Named {
                name: type_name,
                location,
            }))
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
    parsed_type: ParsedTypeAnnotation,
    context: TypeAnnotationContext,
    allow_generic_application: bool,
) -> Result<ParsedTypeAnnotation, CompilerDiagnostic> {
    let with_generic_arguments = parse_generic_arguments(
        token_stream,
        parsed_type,
        context,
        allow_generic_application,
    )?;
    parse_optional_type_suffix(token_stream, with_generic_arguments, context)
}

fn parse_collection_type(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeAnnotation, CompilerDiagnostic> {
    let location = token_stream.current_location();
    token_stream.advance();

    let inner = if token_stream.current_token_kind() == &TokenKind::CloseCurly {
        ParsedTypeAnnotation::new(ParsedTypeRef::Inferred)
    } else {
        parse_required_type_with_generic_application(token_stream, context, true)?
    };
    reject_trait_this_composition(&inner.parsed_type, context, location.clone())?;

    // Check for optional capacity after the element type.
    let capacity = if let TokenKind::IntLiteral(value) = token_stream.current_token_kind() {
        let capacity_location = token_stream.current_location();
        if *value < 0 {
            return Err(CompilerDiagnostic::invalid_collection_type(
                InvalidCollectionTypeReason::NegativeCapacity,
                capacity_location,
            ));
        }
        let cap = CollectionCapacity {
            value: *value,
            location: capacity_location,
        };
        token_stream.advance();
        Some(cap)
    } else {
        None
    };

    if token_stream.current_token_kind() != &TokenKind::CloseCurly {
        return Err(CompilerDiagnostic::expected_token(
            TokenKind::CloseCurly,
            Some(token_stream.current_token_kind().clone()),
            token_stream.current_location(),
        ));
    }

    token_stream.advance();

    Ok(ParsedTypeAnnotation {
        parsed_type: ParsedTypeRef::Collection {
            element: Box::new(inner.parsed_type),
            location,
        },
        collection_capacity: capacity.or(inner.collection_capacity),
    })
}

fn parse_generic_arguments(
    token_stream: &mut FileTokens,
    parsed_type: ParsedTypeAnnotation,
    context: TypeAnnotationContext,
    allow_generic_application: bool,
) -> Result<ParsedTypeAnnotation, CompilerDiagnostic> {
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

    match parsed_type.parsed_type {
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

        arguments.push(parse_generic_type_argument(token_stream, context)?);

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

    Ok(ParsedTypeAnnotation {
        parsed_type: ParsedTypeRef::Applied {
            base: Box::new(parsed_type.parsed_type),
            arguments,
            location,
        },
        collection_capacity: parsed_type.collection_capacity,
    })
}

fn parse_generic_type_argument(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeRef, CompilerDiagnostic> {
    let argument_location = token_stream.current_location();
    let parsed_argument = parse_type_atom(token_stream, context)?;

    reject_trait_this_composition(&parsed_argument.parsed_type, context, argument_location)?;

    if token_stream.current_token_kind() == &TokenKind::Of {
        return Err(nested_generic_application_error(
            token_stream.current_location(),
            context,
        ));
    }

    Ok(parsed_argument.parsed_type)
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
    parsed_type: ParsedTypeAnnotation,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeAnnotation, CompilerDiagnostic> {
    let location = token_stream.current_location();
    if token_stream.current_token_kind() != &TokenKind::QuestionMark {
        return Ok(parsed_type);
    }

    reject_trait_this_composition(&parsed_type.parsed_type, context, location.clone())?;

    if matches!(parsed_type.parsed_type, ParsedTypeRef::Optional { .. }) {
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

    Ok(ParsedTypeAnnotation {
        parsed_type: ParsedTypeRef::Optional {
            inner: Box::new(parsed_type.parsed_type),
            location,
        },
        collection_capacity: parsed_type.collection_capacity,
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
