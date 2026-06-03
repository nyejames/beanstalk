//! Trait definition resolution during AST environment construction.
//!
//! WHAT: converts header-stage trait shells into resolved trait metadata with stable IDs and
//! semantic requirement `TypeId`s.
//! WHY: AST owns type resolution, so trait requirement signatures are resolved here while the
//! trait subsystem owns the resulting compile-time metadata.

use super::builder::AstModuleEnvironmentBuilder;
use crate::compiler_frontend::ast::module_ast::scope_context::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionSignature, function_signature_from_syntax_with_unresolved_types,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::type_resolution::{
    GenericParameterScopeBuildInput, build_generic_parameter_scope, resolve_function_signature,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::datatypes::generic_parameters::{
    GenericParameter, GenericParameterList, GenericParameterScope, TypeParameterId,
};
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::datatypes::parsed::ParsedTypeRef;
use crate::compiler_frontend::declaration_syntax::signature_members::{
    FunctionReturnSyntax, FunctionSignatureSyntax, ReturnSlotSyntax, SignatureMemberSyntax,
};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::definitions::{
    BoundOnlyTraitReason, ResolvedTraitDefinition, ResolvedTraitRequirement, ResolvedTraitReturn,
    TraitDynamicSafety, TraitReceiverRequirement, TraitVisibility,
};
use crate::compiler_frontend::traits::environment::{
    TraitEnvironment, requirement_parameter_from_type, trait_this_name,
};
use crate::compiler_frontend::traits::ids::TraitRequirementId;
use crate::compiler_frontend::traits::syntax::{
    TraitDeclarationSyntax, TraitReferenceSyntax, TraitRequirementSyntax,
};
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

struct TraitRequirementResolutionInput<'a> {
    header: &'a Header,
    declaration: &'a TraitDeclarationSyntax,
    requirement: &'a TraitRequirementSyntax,
    this_name: StringId,
    this_type: TypeId,
    requirement_id: TraitRequirementId,
    generic_parameter_scope: Option<&'a GenericParameterScope>,
}

impl<'context, 'services> AstModuleEnvironmentBuilder<'context, 'services> {
    pub(in crate::compiler_frontend::ast) fn resolve_trait_definitions(
        &mut self,
        sorted_headers: &[Header],
        string_table: &mut StringTable,
    ) -> Result<TraitEnvironment, CompilerMessages> {
        let mut trait_environment = TraitEnvironment::new();
        trait_environment.register_core_displayable(&mut self.type_environment, string_table);

        for header in sorted_headers {
            let HeaderKind::Trait { declaration } = &header.kind else {
                continue;
            };

            let definition = self.resolve_trait_definition(
                header,
                declaration,
                &trait_environment,
                string_table,
            )?;

            if let Some(existing_id) = trait_environment.insert(definition) {
                let Some(existing_definition) = trait_environment.get(existing_id) else {
                    continue;
                };

                return Err(self.diagnostic_messages(
                    CompilerDiagnostic::duplicate_declaration(
                        declaration.name,
                        existing_definition.declaration_location.clone(),
                        declaration.name_location.clone(),
                    ),
                    string_table,
                ));
            }
        }

        self.validate_trait_conformance_references(
            sorted_headers,
            &trait_environment,
            string_table,
        )?;

        Ok(trait_environment)
    }

    fn resolve_trait_definition(
        &mut self,
        header: &Header,
        declaration: &TraitDeclarationSyntax,
        trait_environment: &TraitEnvironment,
        string_table: &mut StringTable,
    ) -> Result<ResolvedTraitDefinition, CompilerMessages> {
        let visibility = self
            .import_environment
            .visibility_for(&header.source_file)
            .map_err(|error| self.error_messages(error, string_table))?
            .clone();

        let this_name = trait_this_name(string_table);
        let this_parameters =
            trait_this_parameter_list(this_name, declaration.name_location.clone());
        let registered_this = self
            .type_environment
            .register_generic_parameter_list(&this_parameters, &FxHashMap::default());
        let Some(this_canonical_id) = registered_this
            .canonical_by_local
            .get(&TypeParameterId(0))
            .copied()
        else {
            return Err(self.error_messages(
                CompilerError::compiler_error(
                    "Trait `This` synthetic type parameter was not registered.",
                ),
                string_table,
            ));
        };
        let this_type = self
            .type_environment
            .intern_generic_parameter(this_canonical_id, this_name);

        let generic_parameter_scope =
            build_generic_parameter_scope(GenericParameterScopeBuildInput {
                generic_parameters: &this_parameters,
                canonical_by_local: Some(&registered_this.canonical_by_local),
                visible_source_bindings: &visibility.visible_source_names,
                visible_type_aliases: &visibility.visible_type_alias_names,
                visible_external_symbols: &visibility.visible_external_symbols,
                declaration_table: self.declaration_table.as_ref(),
                generic_declarations_by_path: &self.module_symbols.generic_declarations_by_path,
                string_table,
            })
            .map_err(|diagnostic| self.diagnostic_messages(*diagnostic, string_table))?;

        let mut requirements = Vec::with_capacity(declaration.requirements.len());
        let mut requirement_locations_by_name = FxHashMap::default();
        let mut next_requirement_id = trait_environment.next_requirement_id();

        for requirement in &declaration.requirements {
            if let Some(first_location) = requirement_locations_by_name
                .insert(requirement.name, requirement.name_location.clone())
            {
                return Err(self.diagnostic_messages(
                    CompilerDiagnostic::duplicate_trait_requirement(
                        declaration.name,
                        requirement.name,
                        first_location,
                        requirement.name_location.clone(),
                    ),
                    string_table,
                ));
            }

            let resolved_requirement = self.resolve_trait_requirement(
                TraitRequirementResolutionInput {
                    header,
                    declaration,
                    requirement,
                    this_name,
                    this_type,
                    requirement_id: next_requirement_id,
                    generic_parameter_scope: generic_parameter_scope.as_ref(),
                },
                string_table,
            )?;
            requirements.push(resolved_requirement);
            next_requirement_id.0 += 1;
        }

        if header.export_mode.is_public() {
            self.validate_exported_trait_surface(
                declaration.name,
                &requirements,
                &header.source_file,
                trait_environment,
                string_table,
            )?;
        }

        let dynamic_safety = classify_trait_dynamic_safety(this_type, &requirements);

        Ok(ResolvedTraitDefinition {
            id: trait_environment.next_trait_id(),
            name: declaration.name,
            canonical_path: header.tokens.src_path.clone(),
            source_file: header.source_file.clone(),
            this_type,
            requirements,
            declaration_location: declaration.name_location.clone(),
            visibility: TraitVisibility::Source {
                exported: header.export_mode.is_public(),
            },
            dynamic_safety,
        })
    }

    fn resolve_trait_requirement(
        &mut self,
        input: TraitRequirementResolutionInput<'_>,
        string_table: &mut StringTable,
    ) -> Result<ResolvedTraitRequirement, CompilerMessages> {
        let TraitRequirementResolutionInput {
            header,
            declaration,
            requirement,
            this_name,
            this_type,
            requirement_id,
            generic_parameter_scope,
        } = input;

        let visibility = self
            .import_environment
            .visibility_for(&header.source_file)
            .map_err(|error| self.error_messages(error, string_table))?
            .clone();

        let signature_syntax =
            signature_with_trait_this_as_parameter(&requirement.signature, this_name);
        let unresolved_signature = self.unresolved_trait_requirement_signature(
            header,
            declaration,
            &signature_syntax,
            string_table,
        )?;

        let mut type_resolution_context =
            self.type_resolution_context_for(&visibility, generic_parameter_scope);
        let resolved_signature = resolve_function_signature(
            &header.tokens.src_path,
            &unresolved_signature,
            None,
            &mut type_resolution_context,
            string_table,
        )
        .map_err(|diagnostic| self.diagnostic_messages(*diagnostic, string_table))?;

        let Some(first_parameter) = resolved_signature.signature.parameters.first() else {
            return Err(self.diagnostic_messages(
                CompilerDiagnostic::unsupported_trait_feature(
                    declaration.name,
                    string_table.intern("missing This receiver"),
                    requirement.location.clone(),
                ),
                string_table,
            ));
        };

        let receiver = if first_parameter.value.value_mode.is_mutable() {
            TraitReceiverRequirement::Mutable { this_type }
        } else {
            TraitReceiverRequirement::Immutable { this_type }
        };

        let mut parameters = Vec::with_capacity(
            resolved_signature
                .signature
                .parameters
                .len()
                .saturating_sub(1),
        );
        for parameter in resolved_signature.signature.parameters.iter().skip(1) {
            parameters.push(requirement_parameter_from_type(
                parameter.id.clone(),
                parameter.value.value_mode.clone(),
                parameter.value.type_id,
                parameter.value.location.clone(),
            ));
        }

        let mut returns = Vec::with_capacity(resolved_signature.signature.returns.len());
        for return_slot in &resolved_signature.signature.returns {
            let Some(type_id) = return_slot.type_id else {
                return Err(self.diagnostic_messages(
                    CompilerDiagnostic::unsupported_trait_feature(
                        declaration.name,
                        string_table.intern("unresolved requirement return"),
                        requirement.location.clone(),
                    ),
                    string_table,
                ));
            };

            returns.push(ResolvedTraitReturn {
                type_id,
                channel: return_slot.channel,
                location: requirement.location.clone(),
            });
        }

        Ok(ResolvedTraitRequirement {
            id: requirement_id,
            name: requirement.name,
            name_location: requirement.name_location.clone(),
            receiver,
            parameters,
            returns,
            location: requirement.location.clone(),
        })
    }

    fn unresolved_trait_requirement_signature(
        &mut self,
        header: &Header,
        declaration: &TraitDeclarationSyntax,
        signature_syntax: &FunctionSignatureSyntax,
        string_table: &mut StringTable,
    ) -> Result<FunctionSignature, CompilerMessages> {
        let source_file_scope = header.canonical_source_file(string_table);
        let signature_context = ScopeContext::new(
            ContextKind::ConstantHeader,
            header.tokens.src_path.to_owned(),
            Rc::clone(&self.declaration_table),
            self.context.external_package_registry.clone(),
            vec![],
        )
        .with_style_directives(self.context.style_directives)
        .with_build_profile(self.context.build_profile)
        .with_project_path_resolver(self.context.project_path_resolver.clone())
        .with_path_format_config(self.context.path_format_config.clone())
        .with_template_const_loop_iteration_limit(self.context.template_const_loop_iteration_limit)
        .with_rendered_path_usage_sink(Rc::clone(&self.rendered_path_usages))
        .with_resolved_type_aliases(Rc::new(self.resolved_type_aliases_by_path.clone()))
        .with_generic_declarations(Rc::new(
            self.module_symbols.generic_declarations_by_path.clone(),
        ))
        .with_resolved_struct_fields_by_path(Rc::new(self.resolved_struct_fields_by_path.clone()))
        .with_nominal_type_ids_by_path(Rc::new(self.nominal_type_ids_by_path.clone()))
        .with_source_file_scope(source_file_scope);

        let mut compatibility_cache = TypeCompatibilityCache::new();
        let mut type_interner =
            AstTypeInterner::new(&mut self.type_environment, &mut compatibility_cache);
        let signature = function_signature_from_syntax_with_unresolved_types(
            signature_syntax,
            &signature_context,
            &mut type_interner,
            string_table,
        )
        .map_err(|diagnostic| self.diagnostic_messages(diagnostic, string_table))?;
        self.warnings
            .extend(signature_context.take_emitted_warnings());

        if signature.parameters.is_empty() {
            return Err(self.diagnostic_messages(
                CompilerDiagnostic::unsupported_trait_feature(
                    declaration.name,
                    string_table.intern("marker requirement"),
                    declaration.name_location.clone(),
                ),
                string_table,
            ));
        }

        Ok(signature)
    }

    fn validate_trait_conformance_references(
        &self,
        sorted_headers: &[Header],
        trait_environment: &TraitEnvironment,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        for header in sorted_headers {
            let HeaderKind::TraitConformance { conformance } = &header.kind else {
                continue;
            };

            let visibility = self
                .import_environment
                .visibility_for(&header.source_file)
                .map_err(|error| self.error_messages(error, string_table))?;

            for trait_ref in &conformance.traits {
                self.resolve_visible_trait_reference(
                    trait_ref,
                    visibility,
                    trait_environment,
                    string_table,
                )?;
            }
        }

        Ok(())
    }

    pub(in crate::compiler_frontend::ast) fn resolve_visible_trait_reference(
        &self,
        trait_ref: &TraitReferenceSyntax,
        visibility: &crate::compiler_frontend::headers::import_environment::FileVisibility,
        trait_environment: &TraitEnvironment,
        string_table: &mut StringTable,
    ) -> Result<crate::compiler_frontend::traits::ids::TraitId, CompilerMessages> {
        if let Some(path) = visibility.visible_trait_names.get(&trait_ref.name)
            && let Some(id) = trait_environment.id_for_path(path)
        {
            return Ok(id);
        }

        if let Some(id) =
            trait_environment.displayable_trait_id_for_name(trait_ref.name, string_table)
        {
            return Ok(id);
        }

        Err(self.diagnostic_messages(
            CompilerDiagnostic::unknown_trait_name(trait_ref.name, trait_ref.location.clone()),
            string_table,
        ))
    }

    fn validate_exported_trait_surface(
        &mut self,
        trait_name: StringId,
        requirements: &[ResolvedTraitRequirement],
        public_facade_file: &crate::compiler_frontend::interned_path::InternedPath,
        trait_environment: &TraitEnvironment,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        for requirement in requirements {
            for parameter in &requirement.parameters {
                self.validate_public_trait_type(
                    trait_name,
                    parameter.type_id,
                    public_facade_file,
                    parameter.location.clone(),
                    trait_environment,
                    string_table,
                )?;
            }

            for return_slot in &requirement.returns {
                self.validate_public_trait_type(
                    trait_name,
                    return_slot.type_id,
                    public_facade_file,
                    return_slot.location.clone(),
                    trait_environment,
                    string_table,
                )?;
            }
        }

        Ok(())
    }

    fn validate_public_trait_type(
        &self,
        trait_name: StringId,
        type_id: crate::compiler_frontend::datatypes::ids::TypeId,
        public_facade_file: &crate::compiler_frontend::interned_path::InternedPath,
        location: SourceLocation,
        trait_environment: &TraitEnvironment,
        string_table: &StringTable,
    ) -> Result<(), CompilerMessages> {
        let mut visited_types = FxHashSet::default();
        if self.public_type_id_is_nameable(
            type_id,
            public_facade_file,
            trait_environment,
            &mut visited_types,
        ) {
            return Ok(());
        }

        Err(self.diagnostic_messages(
            CompilerDiagnostic::trait_private_surface_leak(trait_name, type_id, location),
            string_table,
        ))
    }
}

fn trait_this_parameter_list(
    this_name: StringId,
    location: SourceLocation,
) -> GenericParameterList {
    GenericParameterList {
        parameters: vec![GenericParameter {
            id: TypeParameterId(0),
            name: this_name,
            location,
            trait_bounds: Vec::new(),
        }],
    }
}

fn classify_trait_dynamic_safety(
    this_type: TypeId,
    requirements: &[ResolvedTraitRequirement],
) -> TraitDynamicSafety {
    for requirement in requirements {
        for parameter in &requirement.parameters {
            if parameter.type_id == this_type {
                return TraitDynamicSafety::BoundOnly {
                    reason: BoundOnlyTraitReason::ThisParameter,
                    offending_requirement: requirement.id,
                };
            }
        }

        for return_slot in &requirement.returns {
            if return_slot.type_id == this_type {
                return TraitDynamicSafety::BoundOnly {
                    reason: BoundOnlyTraitReason::ThisReturn,
                    offending_requirement: requirement.id,
                };
            }
        }
    }

    TraitDynamicSafety::DynamicSafe
}

fn signature_with_trait_this_as_parameter(
    signature: &FunctionSignatureSyntax,
    this_name: StringId,
) -> FunctionSignatureSyntax {
    FunctionSignatureSyntax {
        parameters: signature
            .parameters
            .iter()
            .map(|parameter| signature_member_with_trait_this(parameter, this_name))
            .collect(),
        returns: signature
            .returns
            .iter()
            .map(|return_slot| return_slot_with_trait_this(return_slot, this_name))
            .collect(),
    }
}

fn signature_member_with_trait_this(
    member: &SignatureMemberSyntax,
    this_name: StringId,
) -> SignatureMemberSyntax {
    let mut member = member.clone();
    member.type_annotation = parsed_type_with_trait_this(&member.type_annotation, this_name);
    member
}

fn return_slot_with_trait_this(
    return_slot: &ReturnSlotSyntax,
    this_name: StringId,
) -> ReturnSlotSyntax {
    ReturnSlotSyntax {
        value: match &return_slot.value {
            FunctionReturnSyntax::Value {
                type_annotation,
                location,
            } => FunctionReturnSyntax::Value {
                type_annotation: parsed_type_with_trait_this(type_annotation, this_name),
                location: location.clone(),
            },
            FunctionReturnSyntax::AliasCandidates {
                parameter_indices,
                location,
            } => FunctionReturnSyntax::AliasCandidates {
                parameter_indices: parameter_indices.clone(),
                location: location.clone(),
            },
        },
        channel: return_slot.channel,
        location: return_slot.location.clone(),
    }
}

fn parsed_type_with_trait_this(parsed_type: &ParsedTypeRef, this_name: StringId) -> ParsedTypeRef {
    match parsed_type {
        ParsedTypeRef::This { location } => ParsedTypeRef::Named {
            name: this_name,
            location: location.clone(),
        },

        ParsedTypeRef::Applied {
            base,
            arguments,
            location,
        } => ParsedTypeRef::Applied {
            base: Box::new(parsed_type_with_trait_this(base, this_name)),
            arguments: arguments
                .iter()
                .map(|argument| parsed_type_with_trait_this(argument, this_name))
                .collect(),
            location: location.clone(),
        },

        ParsedTypeRef::Collection { element, location } => ParsedTypeRef::Collection {
            element: Box::new(parsed_type_with_trait_this(element, this_name)),
            location: location.clone(),
        },

        ParsedTypeRef::Optional { inner, location } => ParsedTypeRef::Optional {
            inner: Box::new(parsed_type_with_trait_this(inner, this_name)),
            location: location.clone(),
        },

        ParsedTypeRef::Result { ok, err, location } => ParsedTypeRef::Result {
            ok: Box::new(parsed_type_with_trait_this(ok, this_name)),
            err: Box::new(parsed_type_with_trait_this(err, this_name)),
            location: location.clone(),
        },

        _ => parsed_type.clone(),
    }
}
