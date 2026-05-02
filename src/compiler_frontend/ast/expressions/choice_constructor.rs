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
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::generics::{
    GenericInstantiationKey, TypeParameterId, TypeSubstitution, collect_type_parameter_bindings,
    data_type_to_type_identity_key, substitute_type_parameters,
};
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::numeric::coerce_expression_to_declared_type;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{return_compiler_error, return_rule_error};
use rustc_hash::FxHashMap;

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

    // --- Generic choice constructor inference ---
    // If the choice is a generic declaration, infer type arguments from the expected type
    // context and (for payload variants) constructor arguments, then substitute before validating.
    let is_generic = context
        .generic_declarations_by_path
        .as_ref()
        .is_some_and(|decls| {
            decls
                .get(nominal_path)
                .is_some_and(|m| !m.parameters.is_empty())
        });

    let (
        instantiated_variants,
        _generic_instance_key,
        instantiated_data_type,
        raw_args_for_inference,
    ) = if is_generic {
        let metadata = context
            .generic_declarations_by_path
            .as_ref()
            .unwrap()
            .get(nominal_path)
            .unwrap();
        let mut bindings: FxHashMap<TypeParameterId, DataType> = FxHashMap::default();

        // 1. Collect bindings from expected type(s).
        for expected in &context.expected_result_types {
            match expected {
                DataType::Choices {
                    nominal_path: expected_path,
                    variants: expected_variants,
                    ..
                } if expected_path == nominal_path && expected_variants.len() == variants.len() => {
                    for (template_variant, expected_variant) in
                        variants.iter().zip(expected_variants.iter())
                    {
                        match (&template_variant.payload, &expected_variant.payload) {
                            (
                                ChoiceVariantPayload::Record { fields: tf },
                                ChoiceVariantPayload::Record { fields: ef },
                            ) if tf.len() == ef.len() => {
                                for (t_field, e_field) in tf.iter().zip(ef.iter()) {
                                    let _ = collect_type_parameter_bindings(
                                        &t_field.value.data_type,
                                        &e_field.value.data_type,
                                        &mut bindings,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
                DataType::GenericInstance {
                    base:
                        crate::compiler_frontend::datatypes::generics::GenericBaseType::ResolvedNominal(
                            path,
                        ),
                    arguments,
                } if path == nominal_path => {
                    for (param, arg) in metadata.parameters.parameters.iter().zip(arguments.iter())
                    {
                        let _ = collect_type_parameter_bindings(
                            &DataType::TypeParameter {
                                id: param.id,
                                name: param.name,
                            },
                            arg,
                            &mut bindings,
                        );
                    }
                }
                _ => {}
            }
        }

        // 2. For payload variants, collect bindings from constructor arguments.
        let payload_fields = if let ChoiceVariantPayload::Record { fields } = &variant.payload {
            Some(fields)
        } else {
            None
        };

        let raw_args_for_inference = if payload_fields.is_some() && has_parens {
            token_stream.advance(); // past variant name to '('
            let raw_args = parse_call_arguments(token_stream, context, string_table)?;
            Some(raw_args)
        } else {
            None
        };

        if let Some(fields) = payload_fields
            && let Some(ref raw_args) = raw_args_for_inference
        {
            for (template_field, arg) in fields.iter().zip(raw_args.iter()) {
                let _ = collect_type_parameter_bindings(
                    &template_field.value.data_type,
                    &arg.value.data_type,
                    &mut bindings,
                );
            }
        }

        // 3. Build concrete arguments in parameter order.
        let mut concrete_args = Vec::with_capacity(metadata.parameters.len());
        let mut missing_params = Vec::new();
        for param in &metadata.parameters.parameters {
            if let Some(concrete) = bindings.get(&param.id).cloned() {
                concrete_args.push(concrete);
            } else {
                missing_params.push(string_table.resolve(param.name).to_owned());
            }
        }

        if !missing_params.is_empty() {
            return_rule_error!(
                format!(
                    "Cannot infer type argument(s) for generic choice '{}': {}. Provide an explicit type annotation or constructor arguments with concrete types.",
                    choice_name,
                    missing_params.join(", ")
                ),
                variant_location.clone(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Add an explicit type annotation (e.g. 'ResultShape of String, Error = ResultShape::Ok(...)' ) or use arguments with unambiguous types",
                }
            );
        }

        // 4. Substitute into all variants.
        let mut substitution = TypeSubstitution::empty();
        for (param, arg) in metadata
            .parameters
            .parameters
            .iter()
            .zip(concrete_args.iter())
        {
            substitution.insert(param.id, arg.clone());
        }
        let instantiated_variants: Vec<_> = variants
            .iter()
            .map(|v| {
                let payload = match &v.payload {
                    ChoiceVariantPayload::Unit => ChoiceVariantPayload::Unit,
                    ChoiceVariantPayload::Record { fields } => {
                        let substituted_fields = fields
                            .iter()
                            .map(|field| {
                                let mut resolved = field.clone();
                                resolved.value.data_type = substitute_type_parameters(
                                    &field.value.data_type,
                                    &substitution,
                                );
                                resolved
                            })
                            .collect();
                        ChoiceVariantPayload::Record {
                            fields: substituted_fields,
                        }
                    }
                };
                crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant {
                    id: v.id,
                    payload,
                    location: v.location.clone(),
                }
            })
            .collect();

        // 5. Build generic instance key.
        let arg_keys: Vec<_> = concrete_args
            .iter()
            .filter_map(data_type_to_type_identity_key)
            .collect();
        let key = if arg_keys.len() == concrete_args.len() {
            Some(GenericInstantiationKey {
                base_path: nominal_path.to_owned(),
                arguments: arg_keys,
            })
        } else {
            None
        };

        let data_type = DataType::Choices {
            nominal_path: nominal_path.to_owned(),
            variants: instantiated_variants.clone(),
            generic_instance_key: key.clone(),
        };

        (
            instantiated_variants,
            key,
            data_type,
            raw_args_for_inference,
        )
    } else {
        (
            variants.to_owned(),
            None,
            choice_declaration.value.data_type.to_owned(),
            None,
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

            // Parse constructor arguments using shared call-argument machinery.
            // For generic choices, args were already parsed during inference; reuse them.
            let (raw_args, constructor_location) = if is_generic {
                // Args were parsed earlier during inference. We need to reconstruct the location.
                let loc = token_stream.current_location();
                (raw_args_for_inference.unwrap_or_default(), loc)
            } else {
                token_stream.advance(); // past variant name to '('
                let loc = token_stream.current_location();
                let args = parse_call_arguments(token_stream, context, string_table)?;
                (args, loc)
            };

            let expectations = expectations_from_choice_payload_fields(fields);
            let resolved_args = resolve_call_arguments(
                CallDiagnosticContext::choice_constructor(&format!(
                    "{}::{}",
                    choice_name,
                    string_table.resolve(variant_name)
                )),
                &raw_args,
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
                    let is_deferred_reference = if let ExpressionKind::Reference(path) = &value.kind
                        && path.name().is_some_and(|name| {
                            context
                                .get_reference(&name)
                                .is_some_and(Declaration::is_unresolved_constant_placeholder)
                        }) {
                        true
                    } else {
                        false
                    };

                    if !value.is_compile_time_constant() && !is_deferred_reference {
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
