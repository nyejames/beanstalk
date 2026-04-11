use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{
    HirExpression, HirExpressionKind, LocalId, ValueKind,
};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

impl<'a> HirBuilder<'a> {
    pub(super) fn emit_short_circuit_merge_assignment(
        &mut self,
        local: LocalId,
        value: HirExpression,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let value = self.materialize_short_circuit_assignment_value(value, location);
        self.emit_assign_local_statement(local, value, location)
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
