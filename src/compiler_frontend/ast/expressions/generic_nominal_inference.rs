//! Shared inference for generic struct and choice constructors.
//!
//! WHAT: maps expected types plus constructor arguments onto generic declaration parameters.
//! WHY: structs and choices use the same nominal generic rules, and both must route named
//! arguments through the shared call-slot resolver before binding type parameters.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallDiagnosticContext, CallValidationError, expectations_from_constructor_fields,
    resolve_call_argument_slots_typed,
};
use crate::compiler_frontend::ast::expressions::constructor_views::ConstructorField;
use crate::compiler_frontend::ast::generic_bounds::{
    GenericBoundEvidenceContext, validate_nominal_generic_bound_evidence,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidGenericInstantiationReason,
};
use crate::compiler_frontend::datatypes::definitions::{
    ChoiceVariantDefinition, ChoiceVariantPayloadDefinition, FieldDefinition, TypeDefinition,
};
use crate::compiler_frontend::datatypes::environment::{
    GenericParameter as EnvironmentGenericParameter, TypeEnvironment,
};
use crate::compiler_frontend::datatypes::generic_bindings::GenericTypeBindings;
use crate::compiler_frontend::datatypes::generic_identity_bridge::{
    GenericInstantiationKey, TypeIdentityKey,
};
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::headers::module_symbols::GenericDeclarationMetadata;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

pub(crate) enum GenericNominalTemplate<'a> {
    StructFields(&'a [ConstructorField]),
    ChoiceVariants(&'a [ChoiceVariantDefinition]),
}

pub(crate) struct GenericNominalConstructorInput<'a> {
    pub nominal_path: &'a InternedPath,
    pub display_name: &'a str,
    pub metadata: &'a GenericDeclarationMetadata,
    pub template: GenericNominalTemplate<'a>,
    pub constructor_fields: Option<&'a [ConstructorField]>,
    pub raw_args: Option<&'a [CallArgument]>,
    pub diagnostics: CallDiagnosticContext<'a>,
    pub location: SourceLocation,
}

pub(crate) struct GenericNominalInference {
    /// The interned generic instance TypeId, if inference succeeded.
    pub instance_type_id: Option<TypeId>,
    /// HIR/diagnostic bridge key derived from the canonical inferred TypeId arguments.
    pub instance_key: Option<GenericInstantiationKey>,
}

pub(crate) fn infer_generic_nominal_constructor(
    input: GenericNominalConstructorInput<'_>,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<GenericNominalInference, CallValidationError> {
    let mut bindings = GenericTypeBindings::new();

    // ------------------------
    //  Collect type bindings
    // ------------------------
    // First from the expected result type (contextual type information),
    // then from the constructor arguments themselves.
    collect_expected_type_bindings(&input, context, type_interner.environment(), &mut bindings);
    collect_constructor_argument_bindings(
        &input,
        type_interner.environment(),
        &mut bindings,
        string_table,
    )?;

    // ------------------------
    //  Resolve parameters
    // ------------------------
    let canonical_parameters =
        canonical_parameters_for_nominal(input.nominal_path, type_interner.environment());

    let mut concrete_arguments = Vec::with_capacity(input.metadata.parameters.len());
    let mut missing_parameters = Vec::new();
    for (parameter_index, parameter) in input.metadata.parameters.parameters.iter().enumerate() {
        let Some(canonical_parameter) =
            canonical_parameters.and_then(|parameters| parameters.get(parameter_index))
        else {
            missing_parameters.push(parameter.name);
            continue;
        };

        if let Some(concrete) = bindings.get(canonical_parameter.id) {
            concrete_arguments.push(concrete);
        } else {
            missing_parameters.push(parameter.name);
        }
    }

    if !missing_parameters.is_empty() {
        return Err(CompilerDiagnostic::invalid_generic_instantiation(
            Some(string_table.intern(input.display_name)),
            InvalidGenericInstantiationReason::CannotInferArguments { missing_parameters },
            input.location,
        )
        .into());
    }

    // ------------------------
    //  Build instance key
    // ------------------------
    let (instance_type_id, instance_key) = {
        let nominal_id = type_interner
            .environment()
            .nominal_id_for_path(input.nominal_path);

        let argument_keys = concrete_arguments
            .iter()
            .map(|argument| {
                type_interner
                    .environment()
                    .type_id_to_type_identity_key(*argument)
            })
            .collect::<Option<Vec<TypeIdentityKey>>>();

        let instance_key = argument_keys.map(|arguments| GenericInstantiationKey {
            base_path: input.nominal_path.to_owned(),
            arguments,
        });

        let argument_ids = concrete_arguments.clone().into_boxed_slice();
        let instance_type_id = nominal_id
            .map(|nominal_id| type_interner.intern_generic_instance(nominal_id, argument_ids));

        (instance_type_id, instance_key)
    };

    if let Some(instance_type_id) = instance_type_id {
        let evidence_context = GenericBoundEvidenceContext {
            type_environment: type_interner.environment(),
            trait_environment: Some(context.trait_environment()),
            trait_evidence_environment: Some(context.trait_evidence_environment()),
            visible_trait_names: context
                .file_visibility
                .as_ref()
                .map(|visibility| &visibility.visible_trait_names),
            source_file_scope: context.source_file_scope.as_ref(),
        };
        validate_nominal_generic_bound_evidence(
            instance_type_id,
            input.location.clone(),
            &evidence_context,
        )?;
    }

    Ok(GenericNominalInference {
        instance_type_id,
        instance_key,
    })
}

/// Collect generic parameter bindings from every expected type in the surrounding context.
///
/// WHAT: when the compiler already knows the nominal type being constructed (e.g. from a
/// variable declaration or function return type), the concrete generic arguments in that
/// expected type can be used to infer some or all of the constructor's generic parameters.
/// WHY: this is the primary inference source; constructor-argument inference only fills
/// gaps that the expected type leaves ambiguous.
fn collect_expected_type_bindings(
    input: &GenericNominalConstructorInput<'_>,
    context: &ScopeContext,
    type_environment: &TypeEnvironment,
    bindings: &mut GenericTypeBindings,
) {
    for &expected_type_id in &context.expected_result_type_ids {
        match type_environment.get(expected_type_id) {
            // A prior generic instance of the same nominal type gives us direct argument mappings.
            Some(TypeDefinition::GenericInstance(instance)) => {
                let Some(base_path) = type_environment.nominal_path_by_id(instance.base) else {
                    continue;
                };
                if base_path != input.nominal_path {
                    continue;
                }
                let Some(canonical_parameters) =
                    canonical_parameters_for_nominal(input.nominal_path, type_environment)
                else {
                    continue;
                };

                for (parameter, &argument) in
                    canonical_parameters.iter().zip(instance.arguments.iter())
                {
                    let parameter_id = parameter.id;
                    let Some(parameter_type_id) =
                        type_environment.type_id_for_generic_parameter(parameter_id)
                    else {
                        continue;
                    };
                    let _ = type_environment.collect_type_parameter_bindings_typeid(
                        parameter_type_id,
                        argument,
                        bindings,
                    );
                }
            }

            // A concrete struct definition of the same path lets us bind field types.
            Some(TypeDefinition::Struct(def)) if &def.path == input.nominal_path => {
                if let GenericNominalTemplate::StructFields(template_fields) = input.template {
                    let Some(expected_fields) = type_environment.fields_for(expected_type_id)
                    else {
                        continue;
                    };
                    collect_constructor_field_bindings_typeid(
                        template_fields,
                        expected_fields,
                        type_environment,
                        bindings,
                    );
                }
            }

            // A concrete choice definition of the same path lets us bind variant payload types.
            Some(TypeDefinition::Choice(def)) if &def.path == input.nominal_path => {
                if let GenericNominalTemplate::ChoiceVariants(template_variants) = input.template {
                    let Some(expected_variants) = type_environment.variants_for(expected_type_id)
                    else {
                        continue;
                    };
                    collect_choice_variant_bindings_typeid(
                        template_variants,
                        expected_variants,
                        type_environment,
                        bindings,
                    );
                }
            }

            _ => {}
        }
    }
}

/// Look up the canonical generic parameter list for a nominal type path.
fn canonical_parameters_for_nominal<'a>(
    nominal_path: &InternedPath,
    type_environment: &'a TypeEnvironment,
) -> Option<&'a [EnvironmentGenericParameter]> {
    let nominal_id = type_environment.nominal_id_for_path(nominal_path)?;
    let nominal_type_id = type_environment.type_id_for_nominal_id(nominal_id)?;
    let parameter_list_id = type_environment.generic_parameter_list_id_for_type(nominal_type_id)?;
    let parameter_list = type_environment.generic_parameters(parameter_list_id)?;

    Some(parameter_list.parameters.as_slice())
}

/// Collect generic parameter bindings from the constructor's actual arguments.
///
/// WHAT: after resolving named/positional call arguments against the constructor fields,
/// compare each argument's resolved type with the corresponding field's declared type
/// to discover additional generic parameter constraints.
/// WHY: this catches parameters that contextual type information alone could not infer
/// (e.g. a parameter that only appears in a field whose type is not fixed by the context).
fn collect_constructor_argument_bindings(
    input: &GenericNominalConstructorInput<'_>,
    type_environment: &TypeEnvironment,
    bindings: &mut GenericTypeBindings,
    string_table: &mut StringTable,
) -> Result<(), CallValidationError> {
    let (Some(fields), Some(raw_args)) = (input.constructor_fields, input.raw_args) else {
        return Ok(());
    };

    let expectations = expectations_from_constructor_fields(fields);
    let resolved_slots = resolve_call_argument_slots_typed(
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

        // Skip if either side lacks a resolved type_id (e.g. unresolved constant).
        if type_environment.get(field.type_id).is_none()
            || type_environment.get(argument.value.type_id).is_none()
        {
            continue;
        }

        let _ = type_environment.collect_type_parameter_bindings_typeid(
            field.type_id,
            argument.value.type_id,
            bindings,
        );
    }

    Ok(())
}

/// Pairwise generic binding collection between template fields and expected struct fields.
///
/// WHAT: for each field in the struct declaration template, match it with the corresponding
/// expected field and collect type-parameter bindings from their respective type_ids.
fn collect_constructor_field_bindings_typeid(
    template_fields: &[ConstructorField],
    expected_fields: &[FieldDefinition],
    type_environment: &TypeEnvironment,
    bindings: &mut GenericTypeBindings,
) {
    if template_fields.len() != expected_fields.len() {
        return;
    }

    for (template_field, expected_field) in template_fields.iter().zip(expected_fields.iter()) {
        let _ = type_environment.collect_type_parameter_bindings_typeid(
            template_field.type_id,
            expected_field.type_id,
            bindings,
        );
    }
}

/// Pairwise generic binding collection between template payload fields and expected payload fields.
///
/// WHAT: helper for choice variant record payloads; mirrors
/// `collect_constructor_field_bindings_typeid` but operates on `FieldDefinition` slices
/// instead of `ConstructorField` slices.
fn collect_choice_field_bindings_typeid(
    template_fields: &[FieldDefinition],
    expected_fields: &[FieldDefinition],
    type_environment: &TypeEnvironment,
    bindings: &mut GenericTypeBindings,
) {
    if template_fields.len() != expected_fields.len() {
        return;
    }

    for (template_field, expected_field) in template_fields.iter().zip(expected_fields.iter()) {
        let _ = type_environment.collect_type_parameter_bindings_typeid(
            template_field.type_id,
            expected_field.type_id,
            bindings,
        );
    }
}

/// Pairwise generic binding collection between template choice variants and expected choice variants.
///
/// WHAT: for each variant in the choice declaration template, match it with the corresponding
/// expected variant and, if both are record payloads, delegate to `collect_choice_field_bindings_typeid`.
fn collect_choice_variant_bindings_typeid(
    template_variants: &[ChoiceVariantDefinition],
    expected_variants: &[ChoiceVariantDefinition],
    type_environment: &TypeEnvironment,
    bindings: &mut GenericTypeBindings,
) {
    if template_variants.len() != expected_variants.len() {
        return;
    }

    for (template_variant, expected_variant) in template_variants.iter().zip(expected_variants) {
        let (
            ChoiceVariantPayloadDefinition::Record {
                fields: template_fields,
            },
            ChoiceVariantPayloadDefinition::Record {
                fields: expected_fields,
            },
        ) = (&template_variant.payload, &expected_variant.payload)
        else {
            continue;
        };

        collect_choice_field_bindings_typeid(
            template_fields,
            expected_fields,
            type_environment,
            bindings,
        );
    }
}
