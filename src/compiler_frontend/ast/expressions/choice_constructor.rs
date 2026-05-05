//! Choice constructor expression parsing.
//!
//! WHAT: parses `Choice::Variant` (unit) and `Choice::Variant(...)` (payload) expressions.
//! WHY: choice construction has distinct rules from function calls and struct constructors;
//!      unit variants use value syntax while payload variants use constructor-call syntax.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallDiagnosticContext, expectations_from_choice_payload_fields, resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::function_calls::parse_call_arguments;
use crate::compiler_frontend::ast::expressions::generic_nominal_inference::{
    GenericNominalConstructorInput, GenericNominalTemplate, infer_generic_nominal_constructor,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::generics::substitute_type_parameters;
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::numeric::coerce_expression_to_declared_type;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{return_compiler_error, return_rule_error};

/// Parse a `Choice::Variant` or `Choice::Variant(...)` construct expression.
///
/// WHAT: resolves the variant name and validates unit-vs-payload syntax rules.
/// - Unit variants: `Choice::Variant` (no parentheses).
/// - Payload variants: `Choice::Variant(...)` with positional/named arguments.
///
/// WHY: the caller has already verified the base symbol is a choice declaration
/// and that `::` follows it.
pub(crate) fn parse_choice_construct(
    token_stream: &mut FileTokens,
    choice_declaration: &Declaration,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let DataType::Choices {
        nominal_path,
        variants,
        ..
    } = &choice_declaration.value.data_type
    else {
        return_compiler_error!(
            "Choice construct parser was called with a non-choice declaration '{}'.",
            choice_declaration.id.to_portable_string(string_table)
        );
    };

    let choice_name = nominal_path
        .name_str(string_table)
        .unwrap_or("<choice>")
        .to_owned();

    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::DoubleColon {
        return_compiler_error!(
            "Choice construct parser expected '::' after choice name '{}'.",
            choice_name
        );
    }

    token_stream.advance();
    token_stream.skip_newlines();

    let variant_location = token_stream.current_location();
    let variant_name = match token_stream.current_token_kind() {
        TokenKind::Symbol(name) => *name,
        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = crate::compiler_frontend::reserved_trait_syntax::reserved_trait_keyword_or_dispatch_mismatch(
                token_stream.current_token_kind(),
                token_stream.current_location(),
                "Expression Parsing",
                "choice variant expression parsing",
            )?;

            return Err(
                crate::compiler_frontend::reserved_trait_syntax::reserved_trait_keyword_error(
                    keyword,
                    token_stream.current_location(),
                    "Expression Parsing",
                    "Use a normal choice variant name until traits are implemented",
                ),
            );
        }
        _ => {
            return_rule_error!(
                format!("Expected a variant name after '{}::'.", choice_name),
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use namespaced variant syntax like 'Choice::Variant'",
                }
            );
        }
    };

    let Some(variant_index) = variants
        .iter()
        .position(|variant| variant.id == variant_name)
    else {
        let available_variants = variants
            .iter()
            .map(|variant| string_table.resolve(variant.id).to_owned())
            .collect::<Vec<_>>()
            .join(", ");

        return_rule_error!(
            format!(
                "Unknown variant '{}::{}'. Available variants: [{}].",
                choice_name,
                string_table.resolve(variant_name),
                available_variants
            ),
            variant_location,
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Use one of the declared variants for this choice",
            }
        );
    };

    let variant = &variants[variant_index];
    let has_parens = token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis);
    let mut parsed_payload_args = None;
    let mut constructor_location = variant_location.clone();

    if matches!(variant.payload, ChoiceVariantPayload::Record { .. }) && has_parens {
        token_stream.advance(); // past variant name to '('
        constructor_location = token_stream.current_location();
        parsed_payload_args = Some(parse_call_arguments(token_stream, context, string_table)?);
    }

    let generic_metadata = context
        .generic_declarations_by_path
        .as_ref()
        .and_then(|declarations| declarations.get(nominal_path))
        .filter(|metadata| !metadata.parameters.is_empty());

    let (instantiated_variants, instantiated_data_type) = if let Some(metadata) = generic_metadata {
        let constructor_fields = match &variant.payload {
            ChoiceVariantPayload::Record { fields } => Some(fields.as_slice()),
            ChoiceVariantPayload::Unit => None,
        };
        let callee_name = format!("{}::{}", choice_name, string_table.resolve(variant_name));
        let inference = infer_generic_nominal_constructor(
            GenericNominalConstructorInput {
                nominal_path,
                display_name: &choice_name,
                metadata,
                template: GenericNominalTemplate::ChoiceVariants(variants),
                constructor_fields,
                raw_args: parsed_payload_args.as_deref(),
                diagnostics: CallDiagnosticContext::choice_constructor(&callee_name),
                location: constructor_location.clone(),
            },
            context,
            string_table,
        )?;

        let instantiated_variants: Vec<_> = variants
            .iter()
            .map(|variant| {
                let payload = match &variant.payload {
                    ChoiceVariantPayload::Unit => ChoiceVariantPayload::Unit,
                    ChoiceVariantPayload::Record { fields } => {
                        let substituted_fields = fields
                            .iter()
                            .map(|field| {
                                let mut instantiated_field = field.clone();
                                instantiated_field.value.data_type = substitute_type_parameters(
                                    &field.value.data_type,
                                    &inference.substitution,
                                );
                                instantiated_field
                            })
                            .collect();
                        ChoiceVariantPayload::Record {
                            fields: substituted_fields,
                        }
                    }
                };
                crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant {
                    id: variant.id,
                    payload,
                    location: variant.location.clone(),
                }
            })
            .collect();

        let data_type = DataType::Choices {
            nominal_path: nominal_path.to_owned(),
            variants: instantiated_variants.clone(),
            generic_instance_key: inference.instance_key,
        };

        (instantiated_variants, data_type)
    } else {
        (
            variants.to_owned(),
            choice_declaration.value.data_type.to_owned(),
        )
    };

    let selected_variant = &instantiated_variants[variant_index];

    match &selected_variant.payload {
        ChoiceVariantPayload::Unit => {
            token_stream.advance();

            if has_parens {
                token_stream.advance(); // past '('
                if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
                    return_rule_error!(
                        format!(
                            "Unit variant '{}::{}' cannot be called with empty parentheses.",
                            choice_name,
                            string_table.resolve(variant_name)
                        ),
                        token_stream.current_location(),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => format!("Use '{}::{}' without parentheses", choice_name, string_table.resolve(variant_name)),
                        }
                    );
                }
                // If there's content inside, we'll still report the unit-with-parens error,
                // but first advance to a reasonable sync point.
                return_rule_error!(
                    format!(
                        "Unit variant '{}::{}' cannot be called as a constructor.",
                        choice_name,
                        string_table.resolve(variant_name)
                    ),
                    variant_location,
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => format!("Use '{}::{}' without parentheses", choice_name, string_table.resolve(variant_name)),
                    }
                );
            }

            Ok(Expression::choice_construct(
                nominal_path.to_owned(),
                variant_name,
                variant_index,
                vec![],
                instantiated_data_type,
                variant_location,
                ValueMode::ImmutableOwned,
            ))
        }
        ChoiceVariantPayload::Record { fields } => {
            if !has_parens {
                token_stream.advance();
                return_rule_error!(
                    format!(
                        "Payload variant '{}::{}' requires constructor arguments.",
                        choice_name,
                        string_table.resolve(variant_name)
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => format!(
                            "Use '{}::{}(...)' with field values",
                            choice_name,
                            string_table.resolve(variant_name)
                        ),
                    }
                );
            }

            let Some(raw_args) = parsed_payload_args.as_ref() else {
                return_compiler_error!(
                    "Payload choice constructor reached validation without parsed arguments."
                );
            };

            let expectations = expectations_from_choice_payload_fields(fields);
            let resolved_args = resolve_call_arguments(
                CallDiagnosticContext::choice_constructor(&format!(
                    "{}::{}",
                    choice_name,
                    string_table.resolve(variant_name)
                )),
                raw_args,
                &expectations,
                constructor_location.clone(),
                string_table,
            )?;

            let enforce_const = context.kind.allows_const_record_coercion();
            let mut choice_fields = Vec::with_capacity(fields.len());

            for (field, arg) in fields.iter().zip(resolved_args.iter()) {
                let field_type = &field.value.data_type;
                let mut value = coerce_expression_to_declared_type(arg.value.clone(), field_type);

                if enforce_const {
                    let is_placeholder_reference =
                        if let ExpressionKind::Reference(path) = &value.kind {
                            path.name().is_some_and(|name| {
                                context
                                    .get_reference(&name)
                                    .is_some_and(Declaration::is_unresolved_constant_placeholder)
                            })
                        } else {
                            false
                        };

                    if !value.is_compile_time_constant() && !is_placeholder_reference {
                        let field_name = field.id.name_str(string_table).unwrap_or("<field>");
                        return_rule_error!(
                            format!(
                                "Const choice coercion requires compile-time field values. Field '{}' in '{}::{}' is not compile-time constant.",
                                field_name,
                                choice_name,
                                string_table.resolve(variant_name)
                            ),
                            value.location,
                            {
                                CompilationStage => "Expression Parsing",
                                PrimarySuggestion => "Use only compile-time values when constructing choices for top-level '#' constants",
                            }
                        );
                    }
                    value.value_mode = ValueMode::ImmutableOwned;
                }

                choice_fields.push(Declaration {
                    id: field.id.to_owned(),
                    value,
                });
            }

            let value_mode = if enforce_const {
                ValueMode::ImmutableOwned
            } else {
                ValueMode::MutableOwned
            };

            Ok(Expression::choice_construct(
                nominal_path.to_owned(),
                variant_name,
                variant_index,
                choice_fields,
                instantiated_data_type,
                variant_location,
                value_mode,
            ))
        }
    }
}
