//! Return lowering helpers for HIR statements.
//!
//! WHAT: lowers success and error returns into final HIR return terminators.
//! WHY: return coercion/alias handling is distinct from branch and loop CFG construction.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::expressions::{
    HirExpressionKind, HirVariantCarrier, HirVariantField, ValueKind,
};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::HirTypeKind;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

impl<'a> HirBuilder<'a> {
    pub(super) fn lower_return_statement(
        &mut self,
        values: &[Expression],
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let function_id = self.current_function_id_or_error(location)?;
        let return_aliases = self
            .function_by_id_or_error(function_id, location)?
            .return_aliases
            .clone();
        let mut lowered_values = Vec::with_capacity(values.len());

        for (return_index, value) in values.iter().enumerate() {
            let lowered = self.lower_expression(value)?;
            for prelude in lowered.prelude {
                self.emit_statement_to_current_block(prelude, location)?;
            }

            let should_alias = return_aliases
                .get(return_index)
                .and_then(|candidates| candidates.as_ref())
                .is_some();

            let lowered_value = if should_alias {
                match lowered.value.kind {
                    HirExpressionKind::Load(_) => lowered.value,
                    _ => {
                        return_hir_transformation_error!(
                            "Explicit alias returns must return a place expression",
                            self.hir_error_location(location)
                        )
                    }
                }
            } else {
                match lowered.value.kind {
                    HirExpressionKind::Load(place) => self.make_expression(
                        location,
                        HirExpressionKind::Copy(place),
                        lowered.value.ty,
                        ValueKind::RValue,
                        lowered.value.region,
                    ),
                    _ => lowered.value,
                }
            };

            lowered_values.push(lowered_value);
        }

        let return_value = self.expression_from_return_values(&lowered_values, location)?;
        let function_return_type = self
            .function_by_id_or_error(function_id, location)?
            .return_type;
        let return_value = match self.type_context.get(function_return_type).kind {
            HirTypeKind::Result { ok, .. } => {
                if return_value.ty != ok {
                    return_hir_transformation_error!(
                        "Lowered success return does not match function result ok type",
                        self.hir_error_location(location)
                    );
                }
                let return_region = return_value.region;

                let value_name = self.string_table.intern("value");
                self.make_expression(
                    location,
                    HirExpressionKind::VariantConstruct {
                        carrier: HirVariantCarrier::Result,
                        variant_index: 0,
                        fields: vec![HirVariantField {
                            name: Some(value_name),
                            value: return_value,
                        }],
                    },
                    function_return_type,
                    ValueKind::RValue,
                    return_region,
                )
            }
            _ => return_value,
        };
        let current_block = self.current_block_id_or_error(location)?;

        self.emit_terminator(current_block, HirTerminator::Return(return_value), location)
    }

    pub(super) fn lower_error_return_statement(
        &mut self,
        value: &Expression,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let function_id = self.current_function_id_or_error(location)?;
        let function_return_type = self
            .function_by_id_or_error(function_id, location)?
            .return_type;
        let HirTypeKind::Result { err, .. } = self.type_context.get(function_return_type).kind
        else {
            return_hir_transformation_error!(
                "return! reached HIR lowering in a function without a Result return type",
                self.hir_error_location(location)
            );
        };

        let lowered = self.lower_expression(value)?;
        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        let lowered_error = match lowered.value.kind {
            HirExpressionKind::Load(place) => self.make_expression(
                location,
                HirExpressionKind::Copy(place),
                lowered.value.ty,
                ValueKind::RValue,
                lowered.value.region,
            ),
            _ => lowered.value,
        };

        if lowered_error.ty != err {
            return_hir_transformation_error!(
                "Lowered error return does not match function result error type",
                self.hir_error_location(location)
            );
        }
        let error_region = lowered_error.region;

        let value_name = self.string_table.intern("value");
        let return_expression = self.make_expression(
            location,
            HirExpressionKind::VariantConstruct {
                carrier: HirVariantCarrier::Result,
                variant_index: 1,
                fields: vec![HirVariantField {
                    name: Some(value_name),
                    value: lowered_error,
                }],
            },
            function_return_type,
            ValueKind::RValue,
            error_region,
        );

        let current_block = self.current_block_id_or_error(location)?;
        self.emit_terminator(
            current_block,
            HirTerminator::Return(return_expression),
            location,
        )
    }
}
