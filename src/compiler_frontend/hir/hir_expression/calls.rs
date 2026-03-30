//! Call-expression lowering helpers.
//!
//! WHAT: lowers resolved user and host calls into explicit HIR call statements and values.
//! WHY: call lowering is reused across AST expression forms and needs one place to manage
//! prelude sequencing, tuple return shaping, and temporary bindings.

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::HirTypeKind;
use crate::compiler_frontend::hir::hir_nodes::{
    HirExpressionKind, HirPlace, HirStatement, HirStatementKind, ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

use super::LoweredExpression;

impl<'a> HirBuilder<'a> {
    pub(crate) fn lower_receiver_method_call_expression(
        &mut self,
        method_path: &InternedPath,
        receiver: &AstNode,
        args: &[Expression],
        result_types: &[DataType],
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let function_id = self.resolve_function_id_or_error(method_path, location)?;
        let mut full_args = Vec::with_capacity(args.len() + 1);
        full_args.push(receiver.get_expr()?);
        full_args.extend(args.iter().cloned());

        self.lower_call_expression(
            CallTarget::UserFunction(function_id),
            &full_args,
            result_types,
            location,
        )
    }

    // WHAT: lowers a resolved call target plus arguments into HIR call statements and values.
    // WHY: calls may emit preludes, temporary bindings, and tuple shaping, so the lowering needs
    //      one dedicated path instead of being duplicated across expression forms.
    pub(crate) fn lower_call_expression(
        &mut self,
        target: CallTarget,
        args: &[Expression],
        result_types: &[DataType],
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let mut prelude = Vec::new();
        let mut lowered_args = Vec::with_capacity(args.len());

        for arg in args {
            let lowered = self.lower_expression(arg)?;
            prelude.extend(lowered.prelude);
            lowered_args.push(lowered.value);
        }

        let no_return = result_types.is_empty();
        let statement_id = self.allocate_node_id();
        let region = self.current_region_or_error(location)?;

        if no_return {
            let statement = HirStatement {
                id: statement_id,
                kind: HirStatementKind::Call {
                    target,
                    args: lowered_args,
                    result: None,
                },
                location: location.to_owned(),
            };

            self.side_table.map_statement(location, &statement);
            prelude.push(statement);

            let value = self.unit_expression(location, region);
            self.log_call_result_binding(location, None, &value);
            return Ok(LoweredExpression { prelude, value });
        }

        let call_result_type = if result_types.len() == 1 {
            self.lower_data_type(&result_types[0], location)?
        } else {
            let field_types = result_types
                .iter()
                .map(|ret| self.lower_data_type(ret, location))
                .collect::<Result<Vec<_>, _>>()?;
            self.intern_type_kind(HirTypeKind::Tuple {
                fields: field_types,
            })
        };

        let temp_local = self.allocate_temp_local(call_result_type, Some(location.to_owned()))?;

        let statement = HirStatement {
            id: statement_id,
            kind: HirStatementKind::Call {
                target,
                args: lowered_args,
                result: Some(temp_local),
            },
            location: location.to_owned(),
        };

        self.side_table.map_statement(location, &statement);
        prelude.push(statement);

        let value = self.make_expression(
            location,
            HirExpressionKind::Load(HirPlace::Local(temp_local)),
            call_result_type,
            ValueKind::RValue,
            region,
        );

        self.log_call_result_binding(location, Some(temp_local), &value);

        Ok(LoweredExpression { prelude, value })
    }
}
