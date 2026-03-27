//! Literal lowering helpers for HIR expression construction.
//!
//! WHAT: centralizes the common lowering path for compile-time scalar literals.
//! WHY: these cases differ only by the final HIR expression kind, so one helper keeps the main
//! dispatcher smaller and removes repeated region/type boilerplate.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{HirExpressionKind, ValueKind};
use crate::compiler_frontend::tokenizer::tokens::TextLocation;

use super::LoweredExpression;

impl<'a> HirBuilder<'a> {
    pub(super) fn lower_literal_expression(
        &mut self,
        location: &TextLocation,
        data_type: &DataType,
        kind: HirExpressionKind,
    ) -> Result<LoweredExpression, CompilerError> {
        let region = self.current_region_or_error(location)?;
        let ty = self.lower_data_type(data_type, location)?;

        Ok(LoweredExpression {
            prelude: vec![],
            value: self.make_expression(location, kind, ty, ValueKind::Const, region),
        })
    }
}
