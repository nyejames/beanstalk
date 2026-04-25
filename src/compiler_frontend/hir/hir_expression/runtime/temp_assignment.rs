use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::ids::LocalId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

impl<'a> HirBuilder<'a> {
    pub(super) fn materialize_short_circuit_jump_argument_local(
        &mut self,
        value: HirExpression,
        location: &SourceLocation,
    ) -> Result<LocalId, CompilerError> {
        let value = self.materialize_short_circuit_assignment_value(value, location);
        let local = self.allocate_temp_local(value.ty, Some(location.to_owned()))?;
        self.emit_assign_local_statement(local, value, location)?;
        Ok(local)
    }

    fn materialize_short_circuit_assignment_value(
        &mut self,
        value: HirExpression,
        location: &SourceLocation,
    ) -> HirExpression {
        // Assigning a place expression directly into the branch-merge temp can preserve aliasing
        // edges to user locals. Materialize as a copied value so branch-local temps stay detached.
        if let HirExpressionKind::Load(place) = value.kind {
            return self.make_expression(
                location,
                HirExpressionKind::Copy(place),
                value.ty,
                ValueKind::RValue,
                value.region,
            );
        }

        value
    }
}
