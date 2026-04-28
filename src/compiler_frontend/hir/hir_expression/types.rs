//! HIR type-lowering helpers.
//!
//! WHAT: maps frontend `DataType` values into canonical interned HIR types.
//! WHY: HIR and later analyses compare stable type IDs rather than re-traversing frontend types.

use crate::compiler_frontend::compiler_errors::CompilerError;
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

            DataType::Collection(inner) => HirTypeKind::Collection {
                element: self.lower_data_type(inner, location)?,
            },

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

            DataType::Choices { nominal_path, .. } => {
                let choice_id = self.resolve_choice_id(nominal_path, location)?;
                HirTypeKind::Choice { choice_id }
            }

            DataType::Parameters(fields) => {
                let struct_id = self.resolve_struct_id_from_nominal_fields(fields, location)?;
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
}
