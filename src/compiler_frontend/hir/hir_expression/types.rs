//! HIR type-lowering helpers.
//!
//! WHAT: maps frontend `DataType` values into canonical interned HIR types.
//! WHY: HIR and later analyses compare stable type IDs rather than re-traversing frontend types.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::generics::{BuiltinGenericType, GenericBaseType};
use crate::compiler_frontend::datatypes::{BuiltinScalarReceiver, DataType, ReceiverKey};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeId};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

impl<'a> HirBuilder<'a> {
    // WHAT: maps frontend `DataType` values into interned HIR types.
    // WHY: HIR stores canonical type IDs so downstream analyses can compare types cheaply and
    //      deterministically.
    pub(crate) fn lower_data_type(
        &mut self,
        data_type: &DataType,
        location: &SourceLocation,
    ) -> Result<TypeId, CompilerError> {
        let kind = match data_type {
            DataType::Inferred => {
                return_hir_transformation_error!(
                    "DataType::Inferred reached HIR lowering",
                    self.hir_error_location(location)
                )
            }
            DataType::NamedType(_) => {
                return_hir_transformation_error!(
                    "Unresolved DataType::NamedType reached HIR lowering",
                    self.hir_error_location(location)
                )
            }
            DataType::TypeParameter { .. } => {
                return_hir_transformation_error!(
                    "Unresolved DataType::TypeParameter reached HIR lowering",
                    self.hir_error_location(location)
                )
            }
            DataType::Reference(inner) => return self.lower_data_type(inner, location),

            DataType::Bool | DataType::True | DataType::False => HirTypeKind::Bool,
            DataType::Int => HirTypeKind::Int,
            DataType::Float => HirTypeKind::Float,
            DataType::Decimal => HirTypeKind::Decimal,
            DataType::Char => HirTypeKind::Char,
            DataType::BuiltinErrorKind => HirTypeKind::String,
            DataType::StringSlice
            | DataType::Template
            | DataType::TemplateWrapper
            | DataType::Path(_) => HirTypeKind::String,
            DataType::Range => HirTypeKind::Range,
            DataType::None => HirTypeKind::Unit,

            DataType::GenericInstance { base, arguments } => {
                if matches!(
                    base,
                    GenericBaseType::Builtin(BuiltinGenericType::Collection)
                ) && let [single_argument] = arguments.as_slice()
                {
                    HirTypeKind::Collection {
                        element: self.lower_data_type(single_argument, location)?,
                    }
                } else {
                    return_hir_transformation_error!(
                        "Unresolved generic instance reached HIR lowering",
                        self.hir_error_location(location)
                    )
                }
            }

            DataType::Returns(values) => {
                if values.is_empty() {
                    HirTypeKind::Unit
                } else if values.len() == 1 {
                    return self.lower_data_type(&values[0], location);
                } else {
                    let fields = values
                        .iter()
                        .map(|ty| self.lower_data_type(ty, location))
                        .collect::<Result<Vec<_>, _>>()?;
                    HirTypeKind::Tuple { fields }
                }
            }

            DataType::Function(receiver, signature) => {
                let receiver = receiver
                    .as_ref()
                    .as_ref()
                    .map(|receiver| match receiver {
                        ReceiverKey::Struct(path) => {
                            let Some(struct_id) = self.structs_by_name.get(path).copied() else {
                                return_hir_transformation_error!(
                                    format!(
                                        "Unresolved receiver struct '{}' during HIR type lowering",
                                        self.symbol_name_for_diagnostics(path)
                                    ),
                                    self.hir_error_location(location)
                                );
                            };

                            Ok(self.intern_type_kind(HirTypeKind::Struct { struct_id }))
                        }
                        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Int) => {
                            Ok(self.intern_type_kind(HirTypeKind::Int))
                        }
                        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Float) => {
                            Ok(self.intern_type_kind(HirTypeKind::Float))
                        }
                        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Bool) => {
                            Ok(self.intern_type_kind(HirTypeKind::Bool))
                        }
                        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::String) => {
                            Ok(self.intern_type_kind(HirTypeKind::String))
                        }
                        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Char) => {
                            Ok(self.intern_type_kind(HirTypeKind::Char))
                        }
                    })
                    .transpose()?;

                let params = signature
                    .parameters
                    .iter()
                    .map(|param| self.lower_data_type(&param.value.data_type, location))
                    .collect::<Result<Vec<_>, _>>()?;

                let returns = signature
                    .returns
                    .iter()
                    .map(|ret| self.lower_data_type(ret.data_type(), location))
                    .collect::<Result<Vec<_>, _>>()?;

                HirTypeKind::Function {
                    receiver,
                    params,
                    returns,
                }
            }

            DataType::Option(inner) => HirTypeKind::Option {
                inner: self.lower_data_type(inner, location)?,
            },

            DataType::Result { ok, err } => {
                let ok = self.lower_data_type(ok, location)?;
                let err = self.lower_data_type(err, location)?;
                HirTypeKind::Result { ok, err }
            }

            DataType::Choices {
                nominal_path,
                variants,
                generic_instance_key: Some(key),
            } => {
                let choice_id =
                    self.resolve_or_register_generic_choice(key, variants, nominal_path, location)?;
                HirTypeKind::Choice { choice_id }
            }

            DataType::Choices { nominal_path, .. } => {
                let choice_id = self.resolve_choice_id(nominal_path, location)?;
                HirTypeKind::Choice { choice_id }
            }

            DataType::Parameters(fields) => {
                let struct_id = self.resolve_struct_id_from_nominal_fields(fields, location)?;
                HirTypeKind::Struct { struct_id }
            }

            DataType::Struct {
                nominal_path,
                fields,
                generic_instance_key: Some(key),
                ..
            } => {
                let struct_id =
                    self.resolve_or_register_generic_struct(key, fields, nominal_path, location)?;
                HirTypeKind::Struct { struct_id }
            }

            DataType::Struct { nominal_path, .. } => HirTypeKind::Struct {
                struct_id: self.resolve_struct_id_from_nominal_path(nominal_path, location)?,
            },

            DataType::External { type_id } => HirTypeKind::External { type_id: *type_id },
        };

        Ok(self.intern_type_kind(kind))
    }

    // WHAT: interns a HIR type kind and returns its canonical ID.
    // WHY: type interning keeps repeated type shapes stable across the module and avoids arena
    //      duplication during lowering.
    pub(crate) fn intern_type_kind(&mut self, kind: HirTypeKind) -> TypeId {
        if let Some(existing) = self.type_interner.get(&kind) {
            return *existing;
        }

        let id = self.type_context.insert(HirType { kind: kind.clone() });
        self.type_interner.insert(kind, id);
        id
    }

    pub(crate) fn resolve_or_register_generic_struct(
        &mut self,
        key: &crate::compiler_frontend::datatypes::generics::GenericInstantiationKey,
        fields: &[crate::compiler_frontend::ast::ast_nodes::Declaration],
        nominal_path: &crate::compiler_frontend::interned_path::InternedPath,
        location: &SourceLocation,
    ) -> Result<crate::compiler_frontend::hir::ids::StructId, CompilerError> {
        use crate::compiler_frontend::hir::hir_side_table::HirLocation;
        use crate::compiler_frontend::hir::structs::{HirField, HirStruct};

        if let Some(&struct_id) = self.generic_structs_by_key.get(key) {
            return Ok(struct_id);
        }

        let struct_id = self.allocate_struct_id();
        let mut hir_fields = Vec::with_capacity(fields.len());

        for field in fields {
            let field_type = self.lower_data_type(&field.value.data_type, location)?;
            let field_id = self.allocate_field_id();

            self.fields_by_struct_and_name
                .insert((struct_id, field.id.to_owned()), field_id);
            self.side_table
                .bind_field_name(field_id, field.id.to_owned());
            self.side_table
                .map_ast_to_hir(&field.value.location, HirLocation::Field(field_id));
            self.side_table
                .map_hir_source_location(HirLocation::Field(field_id), &field.value.location);

            hir_fields.push(HirField {
                id: field_id,
                ty: field_type,
            });
        }

        let hir_struct = HirStruct {
            id: struct_id,
            fields: hir_fields,
        };

        self.generic_structs_by_key
            .insert(key.to_owned(), struct_id);
        self.side_table
            .bind_struct_name(struct_id, nominal_path.to_owned());
        self.push_struct(hir_struct);

        Ok(struct_id)
    }

    pub(crate) fn resolve_or_register_generic_choice(
        &mut self,
        key: &crate::compiler_frontend::datatypes::generics::GenericInstantiationKey,
        variants: &[crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant],
        nominal_path: &crate::compiler_frontend::interned_path::InternedPath,
        location: &SourceLocation,
    ) -> Result<crate::compiler_frontend::hir::ids::ChoiceId, CompilerError> {
        use crate::compiler_frontend::hir::module::HirChoice;

        if let Some(&choice_id) = self.generic_choices_by_key.get(key) {
            return Ok(choice_id);
        }

        let choice_id = self.allocate_choice_id();
        self.generic_choices_by_key
            .insert(key.to_owned(), choice_id);
        self.side_table
            .bind_choice_name(choice_id, nominal_path.to_owned());
        let index = choice_id.0 as usize;
        debug_assert!(index == self.module.choices.len());
        self.module.choices.push(HirChoice {
            id: choice_id,
            variants: vec![],
        });

        let hir_variants = self.lower_choice_variants(variants, location)?;
        self.module.choices[index].variants = hir_variants;

        Ok(choice_id)
    }
}
