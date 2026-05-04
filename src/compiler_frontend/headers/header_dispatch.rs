//! Header declaration dispatch.
//!
//! WHAT: classifies one top-level declaration after its leading symbol has been seen and builds the
//! concrete `HeaderKind` payload.
//! WHY: declaration-kind parsing is separate from per-file token walking and from dependency sorting.

use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::generics::GenericParameterList;

use crate::compiler_frontend::declaration_syntax::choice::parse_choice_shell as parse_choice_header_payload;
use crate::compiler_frontend::declaration_syntax::declaration_shell::{
    DeclarationSyntax, parse_declaration_syntax,
};
use crate::compiler_frontend::declaration_syntax::generic_parameters::parse_generic_parameter_list_after_type_keyword;
use crate::compiler_frontend::declaration_syntax::r#struct::parse_struct_shell;
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, for_each_named_type_in_data_type, parse_type_annotation,
};
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::external_packages::ExternalSymbolId;
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
use rustc_hash::FxHashSet;
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
    let declaration_name_text = context.string_table.resolve(declaration_name).to_owned();

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
        return Err(reserved_trait_declaration_error(
            token_stream.current_location(),
        ));
    }

    let current_token = token_stream.current_token_kind().to_owned();

    match current_token {
        // Function declaration: `name |params| -> return_type : body ;`
        TokenKind::TypeParameterBracket => {
            ensure_not_keyword_shadow_identifier(
                &declaration_name_text,
                name_location.to_owned(),
                "Header Parsing",
            )?;
            emit_header_naming_warning(
                context.warnings,
                &declaration_name_text,
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

            // Header-provided dependency edges: parameter + return type references only.
            for param in &signature.parameters {
                collect_type_dependency_edges(
                    &param.value.data_type,
                    &generic_parameters,
                    &full_name,
                    context,
                    &mut dependencies,
                );
            }

            for ret in &signature.returns {
                collect_type_dependency_edges(
                    ret.value.data_type(),
                    &generic_parameters,
                    &full_name,
                    context,
                    &mut dependencies,
                );
            }

            capture_function_body_tokens(token_stream, &mut body)?;

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
                "Header Parsing",
                "Use a normal identifier or type name until traits are implemented",
            ));
        }

        // `=` (Assign): either `name = |fields|` (struct) or `#name = <expr>` (exported constant).
        // Peek ahead: if the next token is `|`, this is a struct definition; otherwise a constant.
        TokenKind::Assign => {
            if let Some(TokenKind::TypeParameterBracket) = token_stream.peek_next_token() {
                ensure_not_keyword_shadow_identifier(
                    &declaration_name_text,
                    name_location.to_owned(),
                    "Header Parsing",
                )?;
                emit_header_naming_warning(
                    context.warnings,
                    &declaration_name_text,
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

                let fields = parse_struct_shell(
                    token_stream,
                    &struct_context,
                    context.string_table,
                    &full_name,
                )?;

                // Collect strict type edges from field types only (no default-expression edges).
                // WHY: struct field type refs are the only struct edges that constrain sort order.
                for field in &fields {
                    collect_type_dependency_edges(
                        &field.value.data_type,
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
            } else if exported {
                ensure_not_keyword_shadow_identifier(
                    &declaration_name_text,
                    name_location.to_owned(),
                    "Header Parsing",
                )?;
                emit_header_naming_warning(
                    context.warnings,
                    &declaration_name_text,
                    name_location.to_owned(),
                    IdentifierNamingKind::TopLevelConstant,
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
                &declaration_name_text,
                name_location.to_owned(),
                "Header Parsing",
            )?;
            emit_header_naming_warning(
                context.warnings,
                &declaration_name_text,
                name_location.to_owned(),
                IdentifierNamingKind::TopLevelConstant,
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
                &declaration_name_text,
                name_location.to_owned(),
                "Header Parsing",
            )?;
            emit_header_naming_warning(
                context.warnings,
                &declaration_name_text,
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
                        collect_type_dependency_edges(
                            &field.value.data_type,
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
                return Err(generic_type_alias_deferred_error(token_stream));
            }

            ensure_not_keyword_shadow_identifier(
                &declaration_name_text,
                name_location.to_owned(),
                "Header Parsing",
            )?;
            emit_header_naming_warning(
                context.warnings,
                &declaration_name_text,
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
        exported,
        dependencies,
        name_location,
        tokens: header_tokens,
        source_file: context.source_file.to_owned(),
        file_imports: context.file_import_entries.to_vec(),
        file_re_exports: context.file_re_export_entries.to_vec(),
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

fn parse_optional_generic_parameters(
    token_stream: &mut FileTokens,
    context: &mut HeaderBuildContext<'_>,
) -> Result<GenericParameterList, CompilerError> {
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

    for (prelude_name, symbol_id) in context.external_package_registry.prelude_symbols_by_name() {
        if matches!(symbol_id, ExternalSymbolId::Type(_)) {
            forbidden_names.insert(context.string_table.intern(prelude_name));
        }
    }

    forbidden_names
}

fn collect_type_dependency_edges(
    data_type: &crate::compiler_frontend::datatypes::DataType,
    generic_parameters: &GenericParameterList,
    current_header_path: &InternedPath,
    context: &mut HeaderBuildContext<'_>,
    dependencies: &mut HashSet<InternedPath>,
) {
    for_each_named_type_in_data_type(data_type, &mut |type_name| {
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

fn generic_type_alias_deferred_error(token_stream: &FileTokens) -> CompilerError {
    deferred_feature_rule_error(
        "Generic type aliases are not supported yet.",
        token_stream.current_location(),
        "Header Parsing",
        "Use an alias to a fully concrete generic type, such as `StringBox as Box of String`.",
    )
}

fn create_constant_header_payload(
    full_name: &InternedPath,
    token_stream: &mut FileTokens,
    context: &mut HeaderBuildContext<'_>,
    dependencies: &mut HashSet<InternedPath>,
) -> Result<(DeclarationSyntax, usize), CompilerError> {
    let Some(declaration_name) = full_name.name() else {
        return Err(CompilerError::compiler_error(
            "Constant header path is missing its declaration name.",
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
