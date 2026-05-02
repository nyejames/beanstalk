//! Shared inference for generic struct and choice constructors.
//!
//! WHAT: maps expected types plus constructor arguments onto generic declaration parameters.
//! WHY: structs and choices use the same nominal generic rules, and both must route named
//! arguments through the shared call-slot resolver before binding type parameters.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallDiagnosticContext, expectations_from_struct_fields, resolve_call_argument_slots,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::generics::{
    GenericBaseType, GenericInstantiationKey, TypeIdentityKey, TypeParameterId, TypeSubstitution,
    collect_type_parameter_bindings, data_type_to_type_identity_key,
};
use crate::compiler_frontend::declaration_syntax::choice::{ChoiceVariant, ChoiceVariantPayload};
use crate::compiler_frontend::headers::module_symbols::GenericDeclarationMetadata;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::FxHashMap;

pub(crate) enum GenericNominalTemplate<'a> {
    StructFields(&'a [Declaration]),
    ChoiceVariants(&'a [ChoiceVariant]),
}

pub(crate) struct GenericNominalConstructorInput<'a> {
    pub nominal_path: &'a InternedPath,
    pub display_name: &'a str,
    pub metadata: &'a GenericDeclarationMetadata,
    pub template: GenericNominalTemplate<'a>,
    pub constructor_fields: Option<&'a [Declaration]>,
    pub raw_args: Option<&'a [CallArgument]>,
    pub diagnostics: CallDiagnosticContext<'a>,
    pub location: SourceLocation,
}

pub(crate) struct GenericNominalInference {
    pub substitution: TypeSubstitution,
    pub instance_key: Option<GenericInstantiationKey>,
}

pub(crate) fn infer_generic_nominal_constructor(
    input: GenericNominalConstructorInput<'_>,
    context: &ScopeContext,
    string_table: &StringTable,
) -> Result<GenericNominalInference, CompilerError> {
    let mut bindings: FxHashMap<TypeParameterId, DataType> = FxHashMap::default();

    collect_expected_type_bindings(&input, context, &mut bindings);
    collect_constructor_argument_bindings(&input, &mut bindings, string_table)?;

    let mut concrete_arguments = Vec::with_capacity(input.metadata.parameters.len());
    let mut missing_parameters = Vec::new();
    for parameter in &input.metadata.parameters.parameters {
        if let Some(concrete) = bindings.get(&parameter.id).cloned() {
            concrete_arguments.push(concrete);
        } else {
            missing_parameters.push(string_table.resolve(parameter.name).to_owned());
        }
    }

    if !missing_parameters.is_empty() {
        return Err(CompilerError::new_rule_error(
            format!(
                "Cannot infer type argument(s) for generic type '{}': {}. Provide an explicit type annotation or constructor arguments with concrete types.",
                input.display_name,
                missing_parameters.join(", ")
            ),
            input.location,
        ));
    }

    let mut substitution = TypeSubstitution::empty();
    for (parameter, argument) in input
        .metadata
        .parameters
        .parameters
        .iter()
        .zip(concrete_arguments.iter())
    {
        substitution.insert(parameter.id, argument.to_owned());
    }

    let argument_keys = concrete_arguments
        .iter()
        .map(data_type_to_type_identity_key)
        .collect::<Option<Vec<TypeIdentityKey>>>();
    let instance_key = argument_keys.map(|arguments| GenericInstantiationKey {
        base_path: input.nominal_path.to_owned(),
        arguments,
    });

    Ok(GenericNominalInference {
        substitution,
        instance_key,
    })
}

fn collect_expected_type_bindings(
    input: &GenericNominalConstructorInput<'_>,
    context: &ScopeContext,
    bindings: &mut FxHashMap<TypeParameterId, DataType>,
) {
    for expected in &context.expected_result_types {
        match expected {
            DataType::GenericInstance {
                base: GenericBaseType::ResolvedNominal(path),
                arguments,
            } if path == input.nominal_path => {
                for (parameter, argument) in input
                    .metadata
                    .parameters
                    .parameters
                    .iter()
                    .zip(arguments.iter())
                {
                    let parameter_type = DataType::TypeParameter {
                        id: parameter.id,
                        name: parameter.name,
                    };
                    let _ = collect_type_parameter_bindings(&parameter_type, argument, bindings);
                }
            }
            DataType::Struct {
                nominal_path,
                fields: expected_fields,
                ..
            } if nominal_path == input.nominal_path => {
                if let GenericNominalTemplate::StructFields(template_fields) = input.template {
                    collect_field_bindings(template_fields, expected_fields, bindings);
                }
            }
            DataType::Choices {
                nominal_path,
                variants: expected_variants,
                ..
            } if nominal_path == input.nominal_path => {
                if let GenericNominalTemplate::ChoiceVariants(template_variants) = input.template {
                    collect_choice_variant_bindings(template_variants, expected_variants, bindings);
                }
            }
            _ => {}
        }
    }
}

fn collect_constructor_argument_bindings(
    input: &GenericNominalConstructorInput<'_>,
    bindings: &mut FxHashMap<TypeParameterId, DataType>,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let (Some(fields), Some(raw_args)) = (input.constructor_fields, input.raw_args) else {
        return Ok(());
    };

    let expectations = expectations_from_struct_fields(fields);
    let resolved_slots = resolve_call_argument_slots(
        input.diagnostics,
        raw_args,
        &expectations,
        input.location.clone(),
        string_table,
    )?;

    for (field, slot) in fields.iter().zip(resolved_slots.iter()) {
        let Some(argument) = slot else {
            continue;
        };
        let _ = collect_type_parameter_bindings(
            &field.value.data_type,
            &argument.value.data_type,
            bindings,
        );
    }

    Ok(())
}

fn collect_field_bindings(
    template_fields: &[Declaration],
    expected_fields: &[Declaration],
    bindings: &mut FxHashMap<TypeParameterId, DataType>,
) {
    if template_fields.len() != expected_fields.len() {
        return;
    }

    for (template_field, expected_field) in template_fields.iter().zip(expected_fields.iter()) {
        let _ = collect_type_parameter_bindings(
            &template_field.value.data_type,
            &expected_field.value.data_type,
            bindings,
        );
    }
}

fn collect_choice_variant_bindings(
    template_variants: &[ChoiceVariant],
    expected_variants: &[ChoiceVariant],
    bindings: &mut FxHashMap<TypeParameterId, DataType>,
) {
    if template_variants.len() != expected_variants.len() {
        return;
    }

    for (template_variant, expected_variant) in template_variants.iter().zip(expected_variants) {
        let (
            ChoiceVariantPayload::Record {
                fields: template_fields,
            },
            ChoiceVariantPayload::Record {
                fields: expected_fields,
            },
        ) = (&template_variant.payload, &expected_variant.payload)
        else {
            continue;
        };

        collect_field_bindings(template_fields, expected_fields, bindings);
    }
}
