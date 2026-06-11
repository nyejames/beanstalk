//! HIR expression, pattern, and place validation.
//!
//! WHAT: validates recursive value trees, pattern payloads, variant indexing, and place type
//! resolution after HIR lowering.
//! WHY: expression and place invariants are shared by borrow validation and every backend. Keeping
//! them together makes recursive validation flow explicit.

use super::HirValidator;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_side_table::HirLocation;
use crate::compiler_frontend::hir::ids::BlockId;
use crate::compiler_frontend::hir::patterns::{HirMatchArm, HirPattern};
use crate::compiler_frontend::hir::places::HirPlace;

impl<'a> HirValidator<'a> {
    // -------------------------
    //  Expression & Pattern Validation
    // -------------------------

    pub(super) fn validate_match_arm(
        &self,
        source_block_id: BlockId,
        arm: &HirMatchArm,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        self.validate_pattern(&arm.pattern, anchor)?;

        if let Some(guard) = &arm.guard {
            self.validate_expression(guard, anchor)?;
        }

        self.require_block_id(arm.body, anchor)?;
        self.require_same_function_cfg_owner(source_block_id, arm.body, anchor)?;
        Ok(())
    }

    pub(super) fn validate_pattern(
        &self,
        pattern: &HirPattern,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        match pattern {
            HirPattern::Literal(value) => {
                self.validate_literal_pattern_expression(value, anchor)?;
            }

            HirPattern::OptionNone => {}

            HirPattern::OptionValue { value } => {
                self.validate_literal_pattern_expression(value, anchor)?;
            }

            HirPattern::OptionRelational { value, .. } => {
                self.validate_literal_pattern_expression(value, anchor)?;
                self.validate_relational_pattern_expression(value, anchor)?;
            }

            HirPattern::Wildcard => {}

            HirPattern::Relational { value, .. } => {
                self.validate_literal_pattern_expression(value, anchor)?;
                self.validate_relational_pattern_expression(value, anchor)?;
            }

            HirPattern::ChoiceVariant { choice_id, .. } => {
                if self.module.choices.get(choice_id.0 as usize).is_none() {
                    return Err(self.error_with_hir(
                        format!("Invalid ChoiceId {choice_id:?} in pattern"),
                        anchor,
                    ));
                }
            }

            HirPattern::Capture => {
                // Capture patterns have no extra invariants beyond the local
                // registration performed during lowering.
            }

            HirPattern::OptionPresent => {
                // Present-capture patterns have no embedded expression to validate.
            }
        }

        Ok(())
    }

    pub(super) fn validate_relational_pattern_expression(
        &self,
        expression: &HirExpression,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if !matches!(
            expression.kind,
            HirExpressionKind::Int(_)
                | HirExpressionKind::Float(_)
                | HirExpressionKind::Char(_)
                | HirExpressionKind::StringLiteral(_)
        ) {
            return Err(self.error_with_hir(
                "Match relational pattern must be int/float/char/string",
                anchor,
            ));
        }

        Ok(())
    }

    pub(super) fn validate_literal_pattern_expression(
        &self,
        expression: &HirExpression,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        self.validate_expression(expression, anchor)?;

        if expression.value_kind != ValueKind::Const {
            return Err(
                self.error_with_hir("Match literal pattern must have ValueKind::Const", anchor)
            );
        }

        if !matches!(
            expression.kind,
            HirExpressionKind::Int(_)
                | HirExpressionKind::Float(_)
                | HirExpressionKind::Bool(_)
                | HirExpressionKind::Char(_)
                | HirExpressionKind::StringLiteral(_)
        ) {
            return Err(self.error_with_hir(
                "Match literal pattern must be int/float/bool/char/string",
                anchor,
            ));
        }

        Ok(())
    }

    pub(super) fn validate_expression(
        &self,
        expression: &HirExpression,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        let value_location = HirLocation::Value(expression.id);
        if self
            .module
            .side_table
            .ast_source_id_for_hir(value_location)
            .is_none()
        {
            return Err(self.error_with_hir(
                format!(
                    "Value {} is missing AST->HIR side-table mapping",
                    expression.id
                ),
                anchor,
            ));
        }

        if self
            .module
            .side_table
            .hir_source_id_for_hir(value_location)
            .is_none()
        {
            return Err(self.error_with_hir(
                format!(
                    "Value {} is missing HIR source side-table mapping",
                    expression.id
                ),
                anchor,
            ));
        }

        self.require_type_id(expression.ty, anchor)?;
        self.require_region_id(expression.region, anchor)?;

        match &expression.kind {
            HirExpressionKind::Int(_)
            | HirExpressionKind::Float(_)
            | HirExpressionKind::Bool(_)
            | HirExpressionKind::Char(_)
            | HirExpressionKind::StringLiteral(_) => {}

            HirExpressionKind::VariantConstruct {
                carrier,
                variant_index,
                fields,
            } => {
                match carrier {
                    crate::compiler_frontend::hir::expressions::HirVariantCarrier::Choice {
                        choice_id,
                    } => {
                        let Some(choice) = self.module.choices.get(choice_id.0 as usize) else {
                            return Err(self.error_with_hir(
                                format!("Invalid ChoiceId {choice_id:?} in VariantConstruct"),
                                anchor,
                            ));
                        };
                        if *variant_index >= choice.variants.len() {
                            return Err(self.error_with_hir(
                                format!(
                                    "Variant index {variant_index} out of range for choice {choice_id:?} with {} variants",
                                    choice.variants.len()
                                ),
                                anchor,
                            ));
                        }
                        let variant = &choice.variants[*variant_index];
                        if fields.len() != variant.fields.len() {
                            return Err(self.error_with_hir(
                                format!(
                                    "VariantConstruct field count {} does not match choice variant field count {}",
                                    fields.len(),
                                    variant.fields.len()
                                ),
                                anchor,
                            ));
                        }
                        for (actual, expected) in fields.iter().zip(variant.fields.iter()) {
                            if actual.name != Some(expected.name) {
                                return Err(self.error_with_hir(
                                    format!(
                                        "VariantConstruct field name {:?} does not match declared name {:?}",
                                        actual.name, expected.name
                                    ),
                                    anchor,
                                ));
                            }
                            if actual.value.ty != expected.ty {
                                return Err(self.error_with_hir(
                                    format!(
                                        "VariantConstruct field type mismatch for field {:?}",
                                        expected.name
                                    ),
                                    anchor,
                                ));
                            }
                        }
                    }
                    crate::compiler_frontend::hir::expressions::HirVariantCarrier::Option => {
                        if *variant_index > 1 {
                            return Err(self.error_with_hir(
                                format!(
                                    "VariantConstruct(Option) variant index {variant_index} out of range (valid: 0..=1)"
                                ),
                                anchor,
                            ));
                        }
                        let expected = if *variant_index == 0 { 0 } else { 1 };
                        if fields.len() != expected {
                            return Err(self.error_with_hir(
                                format!(
                                    "VariantConstruct(Option) field count {} does not match expected {} for variant index {}",
                                    fields.len(), expected, variant_index
                                ),
                                anchor,
                            ));
                        }
                        if *variant_index == 1 {
                            let Some(inner_type) =
                                self.type_environment.option_inner_type(expression.ty)
                            else {
                                return Err(self.error_with_hir(
                                    "VariantConstruct(Option) expression type is not an option",
                                    anchor,
                                ));
                            };
                            if fields[0].value.ty != inner_type {
                                return Err(self.error_with_hir(
                                    "VariantConstruct(Option) some-value type does not match option inner type",
                                    anchor,
                                ));
                            }
                        }
                    }
                    crate::compiler_frontend::hir::expressions::HirVariantCarrier::Fallible => {
                        if *variant_index > 1 {
                            return Err(self.error_with_hir(
                                format!(
                                    "VariantConstruct(Result) variant index {variant_index} out of range (valid: 0..=1)"
                                ),
                                anchor,
                            ));
                        }
                        if fields.len() != 1 {
                            return Err(self.error_with_hir(
                                format!(
                                    "VariantConstruct(Result) field count {} does not match expected 1 for variant index {}",
                                    fields.len(), variant_index
                                ),
                                anchor,
                            ));
                        }
                    }
                }
                for field in fields {
                    self.validate_expression(&field.value, anchor)?;
                }
            }

            HirExpressionKind::Load(place) => {
                let _ = self.validate_place(place, anchor)?;
            }

            HirExpressionKind::Copy(place) => {
                let _ = self.validate_place(place, anchor)?;
            }

            HirExpressionKind::BinOp { left, right, .. } => {
                self.validate_expression(left, anchor)?;
                self.validate_expression(right, anchor)?;
            }

            HirExpressionKind::UnaryOp { operand, .. } => {
                self.validate_expression(operand, anchor)?;
            }

            HirExpressionKind::StructConstruct { struct_id, fields } => {
                self.require_struct_id(*struct_id, anchor)?;
                for (field_id, field_expression) in fields {
                    self.require_field_owned_by(*field_id, *struct_id, anchor)?;
                    self.validate_expression(field_expression, anchor)?;
                }
            }

            HirExpressionKind::Collection(elements) => {
                if self
                    .type_environment
                    .collection_shape(expression.ty)
                    .is_none()
                {
                    return Err(self.error_with_hir(
                        "HirExpressionKind::Collection expression type is not a collection type",
                        anchor,
                    ));
                }
                for element in elements {
                    self.validate_expression(element, anchor)?;
                }
            }

            HirExpressionKind::MapLiteral(entries) => {
                if self.type_environment.map_shape(expression.ty).is_none() {
                    return Err(self.error_with_hir(
                        "HirExpressionKind::MapLiteral expression type is not a map type",
                        anchor,
                    ));
                }
                for entry in entries {
                    self.validate_expression(&entry.key, anchor)?;
                    self.validate_expression(&entry.value, anchor)?;
                }
            }

            HirExpressionKind::TupleConstruct { elements } => {
                for element in elements {
                    self.validate_expression(element, anchor)?;
                }
            }

            HirExpressionKind::TupleGet { tuple, .. } => {
                self.validate_expression(tuple, anchor)?;
            }

            HirExpressionKind::Range { start, end } => {
                self.validate_expression(start, anchor)?;
                self.validate_expression(end, anchor)?;
            }

            HirExpressionKind::VariantPayloadGet {
                carrier,
                source,
                variant_index,
                field_index,
            } => {
                self.validate_expression(source, anchor)?;
                match carrier {
                    crate::compiler_frontend::hir::expressions::HirVariantCarrier::Choice {
                        choice_id,
                    } => {
                        let Some(choice) = self.module.choices.get(choice_id.0 as usize) else {
                            return Err(self.error_with_hir(
                                format!("Invalid ChoiceId {choice_id:?} in VariantPayloadGet"),
                                anchor,
                            ));
                        };
                        if *variant_index >= choice.variants.len() {
                            return Err(self.error_with_hir(
                                format!(
                                    "VariantPayloadGet variant index {variant_index} out of range for choice {choice_id:?} with {} variants",
                                    choice.variants.len()
                                ),
                                anchor,
                            ));
                        }
                        let variant = &choice.variants[*variant_index];
                        if *field_index >= variant.fields.len() {
                            return Err(self.error_with_hir(
                                format!(
                                    "VariantPayloadGet field index {field_index} out of range for variant with {} fields",
                                    variant.fields.len()
                                ),
                                anchor,
                            ));
                        }
                    }
                    crate::compiler_frontend::hir::expressions::HirVariantCarrier::Option => {
                        if *variant_index > 1 {
                            return Err(self.error_with_hir(
                                format!(
                                    "VariantPayloadGet variant index {variant_index} out of range for Option (valid: 0..=1)",
                                ),
                                anchor,
                            ));
                        }
                        let max_fields = if *variant_index == 0 { 0 } else { 1 };
                        if *field_index >= max_fields {
                            return Err(self.error_with_hir(
                                format!(
                                    "VariantPayloadGet field index {field_index} out of range for Option variant {variant_index} with {max_fields} fields",
                                ),
                                anchor,
                            ));
                        }
                    }
                    crate::compiler_frontend::hir::expressions::HirVariantCarrier::Fallible => {
                        if *variant_index > 1 {
                            return Err(self.error_with_hir(
                                format!(
                                    "VariantPayloadGet variant index {variant_index} out of range for Result (valid: 0..=1)",
                                ),
                                anchor,
                            ));
                        }
                        if *field_index >= 1 {
                            return Err(self.error_with_hir(
                                format!(
                                    "VariantPayloadGet field index {field_index} out of range for Result variant {variant_index} with 1 field",
                                ),
                                anchor,
                            ));
                        }
                    }
                }
            }

            HirExpressionKind::FallibleUnwrapSuccess { result }
            | HirExpressionKind::FallibleUnwrapError { result }
            | HirExpressionKind::BuiltinCast { value: result, .. } => {
                self.validate_expression(result, anchor)?;
            }
        }

        Ok(())
    }

    // -------------------------
    //  Place Validation
    // -------------------------

    pub(super) fn validate_place(
        &self,
        place: &HirPlace,
        anchor: Option<HirLocation>,
    ) -> Result<TypeId, CompilerError> {
        match place {
            HirPlace::Local(local_id) => self.local_types.get(local_id).copied().ok_or_else(|| {
                self.error_with_hir(format!("Unknown local id {local_id:?}"), anchor)
            }),

            HirPlace::Field { base, field } => {
                let base_type = self.validate_place(base, anchor)?;
                self.require_type_id(base_type, anchor)?;

                if !self.is_struct_type(base_type) {
                    return Err(self.error_with_hir(
                        "Field place base does not resolve to struct type",
                        anchor,
                    ));
                }

                let base_struct_id = self.field_owner.get(field).copied().ok_or_else(|| {
                    self.error_with_hir(format!("Unknown field id {field:?}"), anchor)
                })?;

                self.require_field_owned_by(*field, base_struct_id, anchor)?;
                self.field_types.get(field).copied().ok_or_else(|| {
                    self.error_with_hir(format!("Unknown field id {field:?}"), anchor)
                })
            }

            HirPlace::Index { base, index } => {
                self.validate_expression(index, anchor)?;
                let base_type = self.validate_place(base, anchor)?;
                self.require_type_id(base_type, anchor)?;

                match self.type_environment.collection_element_type(base_type) {
                    Some(element) => Ok(element),
                    None => Err(self.error_with_hir(
                        "Index place base does not resolve to collection type",
                        anchor,
                    )),
                }
            }
        }
    }

    pub(super) fn is_struct_type(&self, type_id: TypeId) -> bool {
        match self.type_environment.get(type_id) {
            Some(TypeDefinition::Struct(..)) => true,
            Some(TypeDefinition::GenericInstance(instance)) => self
                .type_environment
                .struct_definition(instance.base)
                .is_some(),
            _ => false,
        }
    }
}
