use crate::backends::function_registry::CallTarget;
use crate::backends::js::JsEmitter;
use crate::backends::js::js_host_functions::resolve_host_function_path;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirFunction, HirMatchArm, HirPattern, HirStatement, HirStatementKind, HirTerminator,
};

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_block_statements(
        &mut self,
        block: &crate::compiler_frontend::hir::hir_nodes::HirBlock,
    ) -> Result<(), CompilerError> {
        for statement in &block.statements {
            self.emit_statement(statement)?;
        }

        Ok(())
    }

    pub(crate) fn emit_statement(&mut self, statement: &HirStatement) -> Result<(), CompilerError> {
        self.emit_location_comment(&statement.location);

        match &statement.kind {
            HirStatementKind::Assign { target, value } => {
                let target = self.lower_place(target)?;
                let value = self.lower_expr(value)?;
                self.emit_line(&format!("{} = {};", target, value));
            }

            HirStatementKind::Call {
                target,
                args,
                result,
            } => {
                let target = self.lower_call_target(target)?;
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(arg))
                    .collect::<Result<Vec<_>, _>>()?;

                let call = format!("{}({})", target, args.join(", "));

                if let Some(result_local) = result {
                    let result_name = self.local_name(*result_local)?;
                    self.emit_line(&format!("{} = {};", result_name, call));
                } else {
                    self.emit_line(&format!("{};", call));
                }
            }

            HirStatementKind::Expr(expression) => {
                let expression = self.lower_expr(expression)?;
                self.emit_line(&format!("{};", expression));
            }

            HirStatementKind::Drop(_) => {
                // No-op for GC backend.
            }
        }

        Ok(())
    }

    fn lower_call_target(&self, target: &CallTarget) -> Result<String, CompilerError> {
        match target {
            CallTarget::UserFunction(path) => Ok(self.user_call_name(path)?.to_owned()),
            CallTarget::HostFunction(path) => {
                let Some(host_target) = resolve_host_function_path(path, self.string_table) else {
                    return Err(CompilerError::compiler_error(format!(
                        "JavaScript backend: unknown host function '{}'",
                        path.to_string(self.string_table)
                    )));
                };

                Ok(host_target.to_owned())
            }
        }
    }

    pub(crate) fn emit_return_terminator(
        &mut self,
        expression: &crate::compiler_frontend::hir::hir_nodes::HirExpression,
    ) -> Result<(), CompilerError> {
        if self.is_unit_expression(expression) {
            self.emit_line("return;");
            return Ok(());
        }

        let value = self.lower_expr(expression)?;
        self.emit_line(&format!("return {};", value));
        Ok(())
    }

    pub(crate) fn emit_panic_terminator(
        &mut self,
        message: &Option<crate::compiler_frontend::hir::hir_nodes::HirExpression>,
    ) -> Result<(), CompilerError> {
        if let Some(message) = message {
            let message = self.lower_expr(message)?;
            self.emit_line(&format!("throw new Error({});", message));
        } else {
            self.emit_line("throw new Error(\"panic\");");
        }

        Ok(())
    }

    pub(crate) fn emit_dispatcher_for_function(
        &mut self,
        function: &HirFunction,
        reachable_blocks: &[BlockId],
    ) -> Result<(), CompilerError> {
        let state_identifier = self.next_temp_identifier("__bb");

        self.emit_line(&format!("let {} = {};", state_identifier, function.entry.0));
        self.emit_line("while (true) {");
        self.indent += 1;
        self.emit_line(&format!("switch ({}) {{", state_identifier));
        self.indent += 1;

        for block_id in reachable_blocks {
            let block = match self.block_by_id(*block_id) {
                Ok(block) => block.clone(),
                Err(error) => {
                    self.indent -= 2;
                    return Err(error);
                }
            };

            self.emit_line(&format!("case {}: {{", block.id.0));
            self.indent += 1;

            if let Err(error) = self.emit_block_statements(&block) {
                self.indent -= 3;
                return Err(error);
            }

            if let Err(error) =
                self.emit_dispatcher_terminator(&state_identifier, &block.terminator)
            {
                self.indent -= 3;
                return Err(error);
            }

            self.indent -= 1;
            self.emit_line("}");
        }

        self.emit_line("default: {");
        self.with_indent(|emitter| {
            emitter.emit_line(&format!(
                "throw new Error(\"Invalid control-flow block: \" + {});",
                state_identifier
            ));
        });
        self.emit_line("}");

        self.indent -= 1;
        self.emit_line("}");
        self.indent -= 1;
        self.emit_line("}");

        Ok(())
    }

    fn emit_dispatcher_terminator(
        &mut self,
        state_identifier: &str,
        terminator: &HirTerminator,
    ) -> Result<(), CompilerError> {
        match terminator {
            HirTerminator::Jump { target, args } => {
                if !args.is_empty() {
                    return Err(CompilerError::compiler_error(
                        "JavaScript backend: Jump terminator args are not supported yet",
                    ));
                }

                self.emit_line(&format!("{} = {};", state_identifier, target.0));
                self.emit_line("continue;");
            }

            HirTerminator::If {
                condition,
                then_block,
                else_block,
            } => {
                let condition = self.lower_expr(condition)?;
                self.emit_line(&format!("if ({}) {{", condition));
                self.with_indent(|emitter| {
                    emitter.emit_line(&format!("{} = {};", state_identifier, then_block.0));
                });
                self.emit_line("} else {");
                self.with_indent(|emitter| {
                    emitter.emit_line(&format!("{} = {};", state_identifier, else_block.0));
                });
                self.emit_line("}");
                self.emit_line("continue;");
            }

            HirTerminator::Match { scrutinee, arms } => {
                if arms.is_empty() {
                    return Err(CompilerError::compiler_error(
                        "JavaScript backend: Match terminator has no arms",
                    ));
                }

                let scrutinee = self.lower_expr(scrutinee)?;
                let scrutinee_temp = self.next_temp_identifier("__match");
                self.emit_line(&format!("const {} = {};", scrutinee_temp, scrutinee));

                for (index, arm) in arms.iter().enumerate() {
                    let condition = self.lower_match_arm_condition(&scrutinee_temp, arm)?;
                    if index == 0 {
                        self.emit_line(&format!("if ({}) {{", condition));
                    } else {
                        self.emit_line(&format!("else if ({}) {{", condition));
                    }

                    self.with_indent(|emitter| {
                        emitter.emit_line(&format!("{} = {};", state_identifier, arm.body.0));
                    });
                    self.emit_line("}");
                }

                self.emit_line("else {");
                self.with_indent(|emitter| {
                    emitter.emit_line("throw new Error(\"No match arm selected\");");
                });
                self.emit_line("}");
                self.emit_line("continue;");
            }

            HirTerminator::Loop { body, .. } => {
                self.emit_line(&format!("{} = {};", state_identifier, body.0));
                self.emit_line("continue;");
            }

            HirTerminator::Break { target } | HirTerminator::Continue { target } => {
                self.emit_line(&format!("{} = {};", state_identifier, target.0));
                self.emit_line("continue;");
            }

            HirTerminator::Return(value) => {
                self.emit_return_terminator(value)?;
            }

            HirTerminator::Panic { message } => {
                self.emit_panic_terminator(message)?;
            }
        }

        Ok(())
    }

    pub(crate) fn lower_match_arm_condition(
        &mut self,
        scrutinee_expression: &str,
        arm: &HirMatchArm,
    ) -> Result<String, CompilerError> {
        let pattern_condition = match &arm.pattern {
            HirPattern::Literal(value) => {
                let literal = self.lower_expr(value)?;
                format!("{} === {}", scrutinee_expression, literal)
            }

            HirPattern::Wildcard => "true".to_owned(),

            unsupported => {
                return Err(CompilerError::compiler_error(format!(
                    "JavaScript backend: unsupported match pattern {:?}",
                    unsupported
                )));
            }
        };

        if let Some(guard) = &arm.guard {
            let guard = self.lower_expr(guard)?;
            Ok(format!("({}) && ({})", pattern_condition, guard))
        } else {
            Ok(pattern_condition)
        }
    }
}
