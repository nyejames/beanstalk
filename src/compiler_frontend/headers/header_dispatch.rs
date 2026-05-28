#![allow(clippy::result_large_err)]

//! Header declaration dispatch.
//!
//! WHAT: classifies one top-level declaration after its leading symbol has been seen and builds the
//! concrete `HeaderKind` payload.
//! WHY: declaration-kind parsing is separate from per-file token walking and from dependency sorting.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::datatypes::generic_parameters::GenericParameterList;

use crate::compiler_frontend::declaration_syntax::choice::parse_choice_shell as parse_choice_header_payload;
use crate::compiler_frontend::declaration_syntax::declaration_shell::{
    DeclarationSyntax, parse_declaration_syntax,
};
use crate::compiler_frontend::declaration_syntax::generic_parameters::parse_generic_parameter_list_after_type_keyword;
use crate::compiler_frontend::declaration_syntax::signature_members::parse_function_signature_syntax;
use crate::compiler_frontend::declaration_syntax::r#struct::parse_struct_shell;
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, for_each_named_type_in_parsed_ref, parse_type_annotation,
};

use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::dependency_edges::{
    collect_constant_type_dependencies, collect_named_type_dependency_edge,
};
use crate::compiler_frontend::headers::types::{Header, HeaderBuildContext, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::reserved_trait_syntax::{
    ReservedTraitKeyword, reserved_trait_declaration_diagnostic, reserved_trait_keyword_error,
};
use crate::compiler_frontend::symbols::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use rustc_hash::FxHashSet;
use std::collections::HashSet;

// WHAT: classifies one top-level declaration by its leading token and builds the concrete header
// payload (kind + body token slice + dependency set) that later AST passes consume.
//
// WHY: every declaration kind (function, struct, choice/union, constant) has a different leading
// token pattern. This function dispatches on that token and delegates to kind-specific helpers
// where they exist, or captures body tokens directly for simpler cases.
//
// Dispatch summary:
//   `|`  (TypeParameterBracket)  → function signature + body token capture
//   `=`  (Assign)                → struct `= |fields|`
//   `::`  (DoubleColon)          → choice/union variant list
//   `#`  (Hash)                  → compile-time constant binding `#=` / `#Type`
//   `must` / `This`              → reserved trait syntax, error
//   anything else                → no header created (e.g. start-template body lines)
pub(super) fn create_header(
    full_name: InternedPath,
    token_stream: &mut FileTokens,
    name_location: SourceLocation,
    context: &mut HeaderBuildContext<'_>,
) -> Result<Header, CompilerDiagnostic> {
    let Some(declaration_name) = full_name.name() else {
        return Err(internal_header_dispatch_error(
            "Header declaration path is missing its declaration name.",
            name_location,
        ));
    };
    let _declaration_name_text = context.string_table.resolve(declaration_name).to_owned();

    // Only imported symbols become inter-header dependency edges here.
    let mut dependencies: HashSet<InternedPath> = HashSet::new();
    let mut kind: HeaderKind = HeaderKind::StartFunction;
    let mut body = Vec::new();
    let generic_parameters = parse_optional_generic_parameters(token_stream, context)?;

    // Check for trait syntax after generic parameters and before token dispatch.
    // WHAT: detects `must` keyword after type name to recognize reserved trait syntax.
    // WHY: trait declarations must be validated and rejected before attempting other header forms.
    if token_stream.current_token_kind() == &TokenKind::Must {
        // Parse the reserved trait syntax to validate structure
        use crate::compiler_frontend::reserved_trait_syntax::parse_reserved_trait_syntax;
        parse_reserved_trait_syntax(token_stream, declaration_name, context.string_table)?;

        // Return the deferred trait diagnostic
        return Err(reserved_trait_declaration_diagnostic(
            token_stream.current_location(),
        ));
    }

    let current_token = token_stream.current_token_kind().to_owned();

    match current_token {
        // Function declaration: `name |params| -> return_type : body ;`
        TokenKind::TypeParameterBracket => {
            ensure_not_keyword_shadow_identifier(
                declaration_name,
                name_location.to_owned(),
                context.string_table,
            )?;
            emit_header_naming_warning(
                context.warnings,
                declaration_name,
                name_location.to_owned(),
                IdentifierNamingKind::ValueLike,
                context.string_table,
            );

            let signature = parse_function_signature_syntax(
                token_stream,
                context.warnings,
                context.string_table,
                &full_name,
            )?;

            // Header-provided dependency edges: parameter + return type references only.
            for param in &signature.parameters {
                collect_type_dependency_edges(
                    &param.type_annotation,
                    &generic_parameters,
                    &full_name,
                    context,
                    &mut dependencies,
                );
            }

            for ret in &signature.returns {
                if let crate::compiler_frontend::declaration_syntax::signature_members::FunctionReturnSyntax::Value {
                    type_annotation,
                    ..
                } = &ret.value
                {
                    collect_type_dependency_edges(
                        type_annotation,
                        &generic_parameters,
                        &full_name,
                        context,
                        &mut dependencies,
                    );
                }
            }

            capture_function_body_tokens(token_stream, &mut body, context.string_table)?;

            kind = HeaderKind::Function {
                generic_parameters,
                signature,
            };
        }

        // `This` keyword: reserved for future trait `This` self-type syntax.
        TokenKind::TraitThis => {
            return Err(reserved_trait_keyword_error(
                ReservedTraitKeyword::This,
                token_stream.current_location(),
            ));
        }

        // `=` only creates a declaration header for struct shells. Runtime top-level
        // `name = value` stays in the entry start body outside `#config.bst`.
        TokenKind::Assign => {
            if let Some(TokenKind::TypeParameterBracket) = token_stream.peek_next_token() {
                ensure_not_keyword_shadow_identifier(
                    declaration_name,
                    name_location.to_owned(),
                    context.string_table,
                )?;
                emit_header_naming_warning(
                    context.warnings,
                    declaration_name,
                    name_location.to_owned(),
                    IdentifierNamingKind::TypeLike,
                    context.string_table,
                );

                token_stream.advance();

                // Parse field shell directly — avoids reparsing in the AST type-resolution pass.
                // WHY: the header stage owns top-level shell parsing; AST owns body/executable parsing.
                let fields = parse_struct_shell(
                    token_stream,
                    context.string_table,
                    context.warnings,
                    &full_name,
                )?;

                // Collect strict type edges from field types only (no default-expression edges).
                // WHY: struct field type refs are the only struct edges that constrain sort order.
                for field in &fields {
                    collect_type_dependency_edges(
                        &field.type_annotation,
                        &generic_parameters,
                        &full_name,
                        context,
                        &mut dependencies,
                    );
                }

                kind = HeaderKind::Struct {
                    generic_parameters,
                    fields,
                };
            }
        }

        // `#` (Hash): compile-time constant declaration `name #= value` or `name #Type = value`.
        TokenKind::Hash => {
            ensure_not_keyword_shadow_identifier(
                declaration_name,
                name_location.to_owned(),
                context.string_table,
            )?;
            emit_header_naming_warning(
                context.warnings,
                declaration_name,
                name_location.to_owned(),
                IdentifierNamingKind::TopLevelConstant,
                context.string_table,
            );

            let (constant_header, source_order) = create_constant_header_payload(
                &full_name,
                token_stream,
                context,
                &mut dependencies,
            )?;

            kind = HeaderKind::Constant {
                declaration: constant_header,
                source_order,
            };
        }

        // `::` (DoubleColon): choice/union declaration `name :: VariantA | VariantB | ...`
        TokenKind::DoubleColon => {
            ensure_not_keyword_shadow_identifier(
                declaration_name,
                name_location.to_owned(),
                context.string_table,
            )?;
            emit_header_naming_warning(
                context.warnings,
                declaration_name,
                name_location.to_owned(),
                IdentifierNamingKind::TypeLike,
                context.string_table,
            );

            let choice_header = parse_choice_header_payload(
                token_stream,
                &full_name,
                context.string_table,
                context.warnings,
            )?;

            // Collect strict type edges from payload field types.
            for variant in &choice_header {
                if let crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayloadSyntax::Record {
                    fields,
                } = &variant.payload
                {
                    for field in fields {
                        collect_type_dependency_edges(
                            &field.type_annotation,
                            &generic_parameters,
                            &full_name,
                            context,
                            &mut dependencies,
                        );
                    }
                }
            }

            kind = HeaderKind::Choice {
                generic_parameters,
                variants: choice_header,
            };
        }

        // `as`: type alias declaration `Name as Type`
        TokenKind::As => {
            if !generic_parameters.is_empty() {
                return Err(generic_type_alias_deferred_error(
                    token_stream,
                    context.string_table,
                ));
            }

            ensure_not_keyword_shadow_identifier(
                declaration_name,
                name_location.to_owned(),
                context.string_table,
            )?;
            emit_header_naming_warning(
                context.warnings,
                declaration_name,
                name_location.to_owned(),
                IdentifierNamingKind::TypeLike,
                context.string_table,
            );

            token_stream.advance();
            let target =
                parse_type_annotation(token_stream, TypeAnnotationContext::TypeAliasTarget)?;

            for_each_named_type_in_parsed_ref(&target, &mut |type_name| {
                collect_named_type_dependency_edge(
                    type_name,
                    context.file_import_entries,
                    context.source_file,
                    context.external_package_registry,
                    context.string_table,
                    &mut dependencies,
                );
            });

            kind = HeaderKind::TypeAlias {
                generic_parameters: GenericParameterList::default(),
                target,
            };
        }

        _ => {}
    }

    let mut header_tokens = FileTokens::new_with_file_id(full_name, token_stream.file_id, body);
    header_tokens.canonical_os_path = token_stream.canonical_os_path.clone();

    Ok(Header {
        kind,
        file_role: context.file_role,
        dependencies,
        name_location,
        tokens: header_tokens,
        source_file: context.source_file.to_owned(),
        file_imports: context.file_import_entries.to_vec(),
    })
}

fn emit_header_naming_warning(
    warnings: &mut Vec<CompilerDiagnostic>,
    identifier: crate::compiler_frontend::symbols::string_interning::StringId,
    location: SourceLocation,
    naming_kind: IdentifierNamingKind,
    string_table: &crate::compiler_frontend::symbols::string_interning::StringTable,
) {
    if let Some(warning) =
        naming_warning_for_identifier(identifier, location, naming_kind, string_table)
    {
        warnings.push(warning);
    }
}

fn parse_optional_generic_parameters(
    token_stream: &mut FileTokens,
    context: &mut HeaderBuildContext<'_>,
) -> Result<GenericParameterList, CompilerDiagnostic> {
    if token_stream.current_token_kind() != &TokenKind::Type {
        return Ok(GenericParameterList::default());
    }

    let forbidden_names = generic_parameter_forbidden_names(context);
    parse_generic_parameter_list_after_type_keyword(
        token_stream,
        &forbidden_names,
        context.string_table,
    )
}

fn generic_parameter_forbidden_names(
    context: &mut HeaderBuildContext<'_>,
) -> FxHashSet<crate::compiler_frontend::symbols::string_interning::StringId> {
    let mut forbidden_names = FxHashSet::default();

    for import in context.file_import_entries {
        if let Some(local_name) = import.alias.or_else(|| import.header_path.name()) {
            forbidden_names.insert(local_name);
        }
    }

    // Mutation: prelude names are compiler-owned fixed symbols that must be interned
    // so they can be compared against parsed generic parameter names.
    for (prelude_name, symbol_id) in context.external_package_registry.prelude_symbols_by_name() {
        if matches!(symbol_id, ExternalSymbolId::Type(_)) {
            forbidden_names.insert(context.string_table.intern(prelude_name));
        }
    }

    forbidden_names
}

fn collect_type_dependency_edges(
    type_ref: &crate::compiler_frontend::datatypes::parsed::ParsedTypeRef,
    generic_parameters: &GenericParameterList,
    current_header_path: &InternedPath,
    context: &mut HeaderBuildContext<'_>,
    dependencies: &mut HashSet<InternedPath>,
) {
    for_each_named_type_in_parsed_ref(type_ref, &mut |type_name| {
        if generic_parameters.contains_name(type_name) {
            return;
        }

        if context.source_file.append(type_name) == *current_header_path {
            return;
        }

        collect_named_type_dependency_edge(
            type_name,
            context.file_import_entries,
            context.source_file,
            context.external_package_registry,
            context.string_table,
            dependencies,
        );
    });
}

// WHAT: collects all tokens that make up a function body (`:` … `;`) into `body`,
// tracking scope depth to handle nested scopes (inner `if`/`loop`/etc.) correctly.
//
// WHY: extracted from `create_header` to reduce its length and make the scope-balancing
// contract explicit. The token stream must already be positioned on the first body token
// (i.e. `FunctionSignature::new` has already consumed the signature).
// Header-provided dependency edges are derived from the signature only; body tokens are captured but
// not scanned for imports — that is AST's responsibility at body-lowering time.
fn capture_function_body_tokens(
    token_stream: &mut FileTokens,
    body: &mut Vec<crate::compiler_frontend::tokenizer::tokens::Token>,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
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
                // Diagnostic payloads carry the expected delimiter as a StringId so they can be
                // remapped and rendered through the active string table.
                return Err(CompilerDiagnostic::unexpected_end_of_file(
                    Some(string_table.intern(";")),
                    token_stream.current_location(),
                ));
            }

            _ => {
                body.push(token_stream.current_token());
            }
        }

        token_stream.advance();
    }

    Ok(())
}

fn generic_type_alias_deferred_error(
    token_stream: &FileTokens,
    string_table: &mut StringTable,
) -> CompilerDiagnostic {
    // Mutation: deferred-feature diagnostics intern a descriptive feature name that does not
    // appear in the source text, so the payload can be remapped and rendered later.
    CompilerDiagnostic::deferred_feature(
        string_table.intern("generic type aliases"),
        token_stream.current_location(),
    )
}

fn create_constant_header_payload(
    full_name: &InternedPath,
    token_stream: &mut FileTokens,
    context: &mut HeaderBuildContext<'_>,
    dependencies: &mut HashSet<InternedPath>,
) -> Result<(DeclarationSyntax, usize), CompilerDiagnostic> {
    let Some(declaration_name) = full_name.name() else {
        return Err(internal_header_dispatch_error(
            "Constant header path is missing its declaration name.",
            token_stream.current_location(),
        ));
    };
    let declaration_syntax =
        parse_declaration_syntax(token_stream, declaration_name, context.string_table)?;

    // Header-provided dependency edges: declared type annotation only.
    // WHY: constant initializer references are now first-class dependency edges generated by
    // headers/constant_dependencies.rs; this function only collects type-surface edges.
    collect_constant_type_dependencies(&declaration_syntax, context, dependencies);

    let source_order = *context.file_constant_order;
    *context.file_constant_order += 1;

    Ok((declaration_syntax, source_order))
}

fn internal_header_dispatch_error(
    message: &'static str,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerError::new(message, location, ErrorType::Compiler).into()
}
