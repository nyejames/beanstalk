//! Header declaration dispatch.
//!
//! WHAT: classifies one top-level declaration after its leading symbol has been seen and builds the
//! concrete `HeaderKind` payload.
//! WHY: declaration-kind parsing is separate from per-file token walking and from dependency sorting.

use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;

use crate::compiler_frontend::declaration_syntax::choice::parse_choice_shell as parse_choice_header_payload;
use crate::compiler_frontend::declaration_syntax::declaration_shell::{
    DeclarationSyntax, parse_declaration_syntax,
};
use crate::compiler_frontend::declaration_syntax::r#struct::parse_struct_shell;
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, for_each_named_type_in_data_type, parse_type_annotation,
};
use crate::compiler_frontend::headers::dependency_edges::{
    collect_constant_type_dependencies, collect_named_type_dependency_edge,
};
use crate::compiler_frontend::headers::types::{Header, HeaderBuildContext, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::reserved_trait_syntax::{
    ReservedTraitKeyword, reserved_trait_declaration_error, reserved_trait_keyword_error,
};
use crate::compiler_frontend::symbols::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use std::collections::HashSet;
use std::rc::Rc;

// WHAT: classifies one top-level declaration by its leading token and builds the concrete header
// payload (kind + body token slice + dependency set) that later AST passes consume.
//
// WHY: every declaration kind (function, struct, choice/union, constant) has a different leading
// token pattern. This function dispatches on that token and delegates to kind-specific helpers
// where they exist, or captures body tokens directly for simpler cases.
//
// Dispatch summary:
//   `|`  (TypeParameterBracket)  → function signature + body token capture
//   `=`  (Assign)                → struct `= |fields|` or exported constant `= <expr>`
//   `::`  (DoubleColon)          → choice/union variant list
//   type tokens / `~`            → exported constant with implicit `=` already consumed
//   `must` / `This`              → reserved trait syntax, error
//   anything else                → no header created (e.g. start-template body lines)
pub(super) fn create_header(
    full_name: InternedPath,
    exported: bool,
    token_stream: &mut FileTokens,
    name_location: SourceLocation,
    context: &mut HeaderBuildContext<'_>,
) -> Result<Header, CompilerError> {
    let Some(declaration_name) = full_name.name() else {
        return Err(CompilerError::compiler_error(
            "Header declaration path is missing its declaration name.",
        ));
    };
    let declaration_name_text = context.string_table.resolve(declaration_name);

    // Only imported symbols become inter-header dependency edges here.
    let mut dependencies: HashSet<InternedPath> = HashSet::new();
    let mut kind: HeaderKind = HeaderKind::StartFunction;
    let mut body = Vec::new();
    let current_token = token_stream.current_token_kind().to_owned();

    match current_token {
        // Function declaration: `name |params| -> return_type : body ;`
        TokenKind::TypeParameterBracket => {
            ensure_not_keyword_shadow_identifier(
                declaration_name_text,
                name_location.to_owned(),
                "Header Parsing",
            )?;
            emit_header_naming_warning(
                context.warnings,
                declaration_name_text,
                name_location.to_owned(),
                IdentifierNamingKind::ValueLike,
            );

            let signature_context = ScopeContext::new(
                ContextKind::ConstantHeader,
                full_name.to_owned(),
                Rc::clone(&context.visible_constant_placeholders),
                context.external_package_registry.to_owned(),
                vec![],
            )
            .with_project_path_resolver(context.project_path_resolver.clone())
            .with_source_file_scope(context.source_file.to_owned())
            .with_path_format_config(context.path_format_config.clone());

            let signature = FunctionSignature::new(
                token_stream,
                context.warnings,
                context.string_table,
                &full_name,
                &signature_context,
            )?;

            // Strict edges: parameter + return type references only.
            for param in &signature.parameters {
                for_each_named_type_in_data_type(&param.value.data_type, &mut |type_name| {
                    collect_named_type_dependency_edge(
                        type_name,
                        context.file_import_entries,
                        context.source_file,
                        context.external_package_registry,
                        context.string_table,
                        &mut dependencies,
                    );
                });
            }

            for ret in &signature.returns {
                for_each_named_type_in_data_type(ret.value.data_type(), &mut |type_name| {
                    collect_named_type_dependency_edge(
                        type_name,
                        context.file_import_entries,
                        context.source_file,
                        context.external_package_registry,
                        context.string_table,
                        &mut dependencies,
                    );
                });
            }

            capture_function_body_tokens(token_stream, &mut body)?;

            kind = HeaderKind::Function { signature };
        }

        // `must` keyword: reserved for future trait implementation syntax.
        TokenKind::Must => {
            return Err(reserved_trait_declaration_error(
                token_stream.current_location(),
            ));
        }

        // `This` keyword: reserved for future trait `This` self-type syntax.
        TokenKind::TraitThis => {
            return Err(reserved_trait_keyword_error(
                ReservedTraitKeyword::This,
                token_stream.current_location(),
                "Header Parsing",
                "Use a normal identifier or type name until traits are implemented",
            ));
        }

        // `=` (Assign): either `name = |fields|` (struct) or `#name = <expr>` (exported constant).
        // Peek ahead: if the next token is `|`, this is a struct definition; otherwise a constant.
        TokenKind::Assign => {
            if let Some(TokenKind::TypeParameterBracket) = token_stream.peek_next_token() {
                ensure_not_keyword_shadow_identifier(
                    declaration_name_text,
                    name_location.to_owned(),
                    "Header Parsing",
                )?;
                emit_header_naming_warning(
                    context.warnings,
                    declaration_name_text,
                    name_location.to_owned(),
                    IdentifierNamingKind::TypeLike,
                );

                token_stream.advance();

                // Parse field shell directly — avoids reparsing in the AST type-resolution pass.
                // WHY: the header stage owns top-level shell parsing; AST owns body/executable parsing.
                let struct_context = ScopeContext::new(
                    ContextKind::ConstantHeader,
                    full_name.to_owned(),
                    Rc::clone(&context.visible_constant_placeholders),
                    context.external_package_registry.to_owned(),
                    vec![],
                )
                .with_style_directives(context.style_directives)
                .with_project_path_resolver(context.project_path_resolver.clone())
                .with_source_file_scope(context.source_file.to_owned())
                .with_path_format_config(context.path_format_config.clone());

                // Field IDs are built as `token_stream.src_path.append(field_name)` inside
                // parse_signature_members. Set src_path to the struct's own path so field IDs
                // are `struct_path/field_name` — matching what resolve_struct_field_types expects.
                // WHY: token_stream.src_path is the file path at this point; fields need to be
                // children of the struct path, not siblings of the struct in the file namespace.
                let saved_src_path = token_stream.src_path.to_owned();
                token_stream.src_path = full_name.to_owned();
                let fields =
                    parse_struct_shell(token_stream, &struct_context, context.string_table);
                token_stream.src_path = saved_src_path;
                let fields = fields?;

                // Collect strict type edges from field types only (no default-expression edges).
                // WHY: struct field type refs are the only struct edges that constrain sort order.
                for field in &fields {
                    for_each_named_type_in_data_type(&field.value.data_type, &mut |type_name| {
                        collect_named_type_dependency_edge(
                            type_name,
                            context.file_import_entries,
                            context.source_file,
                            context.external_package_registry,
                            context.string_table,
                            &mut dependencies,
                        );
                    });
                }

                kind = HeaderKind::Struct { fields };
            } else if exported {
                ensure_not_keyword_shadow_identifier(
                    declaration_name_text,
                    name_location.to_owned(),
                    "Header Parsing",
                )?;
                emit_header_naming_warning(
                    context.warnings,
                    declaration_name_text,
                    name_location.to_owned(),
                    IdentifierNamingKind::TopLevelConstant,
                );

                let constant_header = create_constant_header_payload(
                    &full_name,
                    token_stream,
                    context,
                    &mut dependencies,
                )?;

                kind = HeaderKind::Constant {
                    declaration: constant_header,
                };
            }
        }

        // Type-starting tokens: `#name ~Type`, `#name Int`, `#name {collection}`, etc.
        // These only produce a header if the declaration is exported (`#`). Non-exported
        // declarations starting with a type are top-level template or body lines, not headers.
        TokenKind::Mutable
        | TokenKind::DatatypeInt
        | TokenKind::DatatypeFloat
        | TokenKind::DatatypeBool
        | TokenKind::DatatypeString
        | TokenKind::DatatypeChar
        | TokenKind::OpenCurly
        | TokenKind::Symbol(_)
            if exported =>
        {
            ensure_not_keyword_shadow_identifier(
                declaration_name_text,
                name_location.to_owned(),
                "Header Parsing",
            )?;
            emit_header_naming_warning(
                context.warnings,
                declaration_name_text,
                name_location.to_owned(),
                IdentifierNamingKind::TopLevelConstant,
            );

            let constant_header = create_constant_header_payload(
                &full_name,
                token_stream,
                context,
                &mut dependencies,
            )?;

            kind = HeaderKind::Constant {
                declaration: constant_header,
            };
        }

        // `::` (DoubleColon): choice/union declaration `name :: VariantA | VariantB | ...`
        TokenKind::DoubleColon => {
            ensure_not_keyword_shadow_identifier(
                declaration_name_text,
                name_location.to_owned(),
                "Header Parsing",
            )?;
            emit_header_naming_warning(
                context.warnings,
                declaration_name_text,
                name_location.to_owned(),
                IdentifierNamingKind::TypeLike,
            );

            let choice_context = ScopeContext::new(
                ContextKind::ConstantHeader,
                full_name.to_owned(),
                Rc::clone(&context.visible_constant_placeholders),
                context.external_package_registry.to_owned(),
                vec![],
            )
            .with_style_directives(context.style_directives)
            .with_project_path_resolver(context.project_path_resolver.clone())
            .with_source_file_scope(context.source_file.to_owned())
            .with_path_format_config(context.path_format_config.clone());

            let choice_header = parse_choice_header_payload(
                token_stream,
                &full_name,
                &choice_context,
                context.string_table,
                context.warnings,
            )?;

            // Collect strict type edges from payload field types.
            for variant in &choice_header {
                if let crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload::Record {
                    fields,
                } = &variant.payload
                {
                    for field in fields {
                        for_each_named_type_in_data_type(
                            &field.value.data_type,
                            &mut |type_name| {
                                collect_named_type_dependency_edge(
                                    type_name,
                                    context.file_import_entries,
                                    context.source_file,
                                    context.external_package_registry,
                                    context.string_table,
                                    &mut dependencies,
                                );
                            },
                        );
                    }
                }
            }

            kind = HeaderKind::Choice {
                variants: choice_header,
            };
        }

        // `as`: type alias declaration `Name as Type`
        TokenKind::As => {
            ensure_not_keyword_shadow_identifier(
                declaration_name_text,
                name_location.to_owned(),
                "Header Parsing",
            )?;
            emit_header_naming_warning(
                context.warnings,
                declaration_name_text,
                name_location.to_owned(),
                IdentifierNamingKind::TypeLike,
            );

            token_stream.advance();
            let target =
                parse_type_annotation(token_stream, TypeAnnotationContext::TypeAliasTarget)?;

            for_each_named_type_in_data_type(&target, &mut |type_name| {
                collect_named_type_dependency_edge(
                    type_name,
                    context.file_import_entries,
                    context.source_file,
                    context.external_package_registry,
                    context.string_table,
                    &mut dependencies,
                );
            });

            kind = HeaderKind::TypeAlias { target };
        }

        _ => {}
    }

    let mut header_tokens = FileTokens::new_with_file_id(full_name, token_stream.file_id, body);
    header_tokens.canonical_os_path = token_stream.canonical_os_path.clone();

    Ok(Header {
        kind,
        exported,
        dependencies,
        name_location,
        tokens: header_tokens,
        source_file: context.source_file.to_owned(),
        file_imports: context.file_import_entries.to_vec(),
    })
}

fn emit_header_naming_warning(
    warnings: &mut Vec<CompilerWarning>,
    identifier: &str,
    location: SourceLocation,
    naming_kind: IdentifierNamingKind,
) {
    if let Some(warning) = naming_warning_for_identifier(identifier, location, naming_kind) {
        warnings.push(warning);
    }
}

// WHAT: collects all tokens that make up a function body (`:` … `;`) into `body`,
// tracking scope depth to handle nested scopes (inner `if`/`loop`/etc.) correctly.
//
// WHY: extracted from `create_header` to reduce its length and make the scope-balancing
// contract explicit. The token stream must already be positioned on the first body token
// (i.e. `FunctionSignature::new` has already consumed the signature).
// Strict dependency edges are derived from the signature only; body tokens are captured but
// not scanned for imports — that is AST's responsibility at body-lowering time.
fn capture_function_body_tokens(
    token_stream: &mut FileTokens,
    body: &mut Vec<crate::compiler_frontend::tokenizer::tokens::Token>,
) -> Result<(), CompilerError> {
    let mut scopes_opened = 1;
    let mut scopes_closed = 0;

    // `FunctionSignature::new` stops on the first body token, so the first loop
    // iteration must inspect the current token before advancing.
    while scopes_opened > scopes_closed {
        match token_stream.current_token_kind() {
            TokenKind::End => {
                scopes_closed += 1;
                if scopes_opened > scopes_closed {
                    body.push(token_stream.current_token());
                }
            }

            // Colons used in templates parse into a different token (StartTemplateBody),
            // so there is no risk of templates creating a colon imbalance here.
            // All other language constructs follow the invariant: every `:` is closed by `;`.
            TokenKind::Colon => {
                scopes_opened += 1;
                body.push(token_stream.current_token());
            }

            // `::` is an expression/operator token (e.g. `Choice::Variant`) and must not
            // affect function-scope depth balancing.
            TokenKind::DoubleColon => {
                body.push(token_stream.current_token());
            }

            TokenKind::Eof => {
                crate::return_rule_error!(
                    "Unexpected end of file while parsing function body. Missing ';' to close this scope.",
                    token_stream.current_location(),
                    {
                        PrimarySuggestion => "Close the function body with ';'",
                        SuggestedInsertion => ";",
                    }
                )
            }

            _ => {
                body.push(token_stream.current_token());
            }
        }

        token_stream.advance();
    }

    Ok(())
}

fn create_constant_header_payload(
    full_name: &InternedPath,
    token_stream: &mut FileTokens,
    context: &mut HeaderBuildContext<'_>,
    dependencies: &mut HashSet<InternedPath>,
) -> Result<DeclarationSyntax, CompilerError> {
    let Some(declaration_name) = full_name.name() else {
        return Err(CompilerError::compiler_error(
            "Constant header path is missing its declaration name.",
        ));
    };
    let declaration_syntax =
        parse_declaration_syntax(token_stream, declaration_name, context.string_table)?;

    // Strict edges: declared type annotation only.
    // WHY: initializer-expression symbols are soft ordering hints, not strict structural deps.
    collect_constant_type_dependencies(&declaration_syntax, context, dependencies);

    *context.file_constant_order += 1;

    Ok(declaration_syntax)
}
