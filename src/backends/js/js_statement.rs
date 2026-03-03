use crate::backends::js::JsEmitter;
use crate::backends::js::js_host_functions::resolve_host_function_path;
use crate::compiler_frontend::analysis::borrow_checker::LocalMode;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirExpression, HirExpressionKind, HirFunction, HirMatchArm, HirPattern, HirPlace,
    HirStatement, HirStatementKind, HirTerminator, LocalId,
};
use crate::compiler_frontend::host_functions::CallTarget;

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
                self.emit_assignment(statement, target, value)?;
            }

            HirStatementKind::Call {
                target,
                args,
                result,
            } => {
                let target_name = self.lower_call_target(target)?;
                let args = if matches!(target, CallTarget::HostFunction(_)) {
                    args.iter()
                        .map(|arg| self.lower_host_call_argument(arg))
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    args.iter()
                        .map(|arg| self.lower_call_argument(arg))
                        .collect::<Result<Vec<_>, _>>()?
                };

                let call = format!("{}({})", target_name, args.join(", "));

                if let Some(result_local) = result {
                    let result_name = self.local_name(*result_local)?;
                    if self.call_returns_alias_reference(target) {
                        self.emit_line(&format!("__bs_assign_borrow({}, {});", result_name, call));
                    } else {
                        self.emit_line(&format!("__bs_assign_value({}, {});", result_name, call));
                    }
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

    fn emit_assignment(
        &mut self,
        statement: &HirStatement,
        target: &HirPlace,
        value: &HirExpression,
    ) -> Result<(), CompilerError> {
        match target {
            HirPlace::Local(local_id) => self.emit_local_assignment(statement, *local_id, value),
            _ => {
                let target_ref = self.lower_place(target)?;
                let emitted_value = match &value.kind {
                    HirExpressionKind::Load(place) => {
                        format!("__bs_read({})", self.lower_place(place)?)
                    }
                    HirExpressionKind::Copy(place) => {
                        format!("__bs_clone_value(__bs_read({}))", self.lower_place(place)?)
                    }
                    _ => self.lower_expr(value)?,
                };
                self.emit_line(&format!("__bs_write({}, {});", target_ref, emitted_value));

                Ok(())
            }
        }
    }

    fn emit_local_assignment(
        &mut self,
        statement: &HirStatement,
        local_id: LocalId,
        value: &HirExpression,
    ) -> Result<(), CompilerError> {
        let local_name = self.local_name(local_id)?.to_owned();
        let alias_only = self.local_is_alias_only_before_statement(statement, local_id);

        match &value.kind {
            HirExpressionKind::Load(place) => {
                let source = self.lower_place(place)?;
                if alias_only {
                    self.emit_line(&format!(
                        "__bs_write({}, __bs_read({}));",
                        local_name, source
                    ));
                } else {
                    self.emit_line(&format!("__bs_assign_borrow({}, {});", local_name, source));
                }
            }
            HirExpressionKind::Copy(place) => {
                let copied = format!("__bs_clone_value(__bs_read({}))", self.lower_place(place)?);
                if alias_only {
                    self.emit_line(&format!("__bs_write({}, {});", local_name, copied));
                } else {
                    self.emit_line(&format!("__bs_assign_value({}, {});", local_name, copied));
                }
            }
            _ => {
                let lowered = self.lower_expr(value)?;
                if alias_only {
                    self.emit_line(&format!("__bs_write({}, {});", local_name, lowered));
                } else {
                    self.emit_line(&format!("__bs_assign_value({}, {});", local_name, lowered));
                }
            }
        }

        Ok(())
    }

    fn local_is_alias_only_before_statement(
        &self,
        statement: &HirStatement,
        local_id: LocalId,
    ) -> bool {
        let Some(snapshot) = self
            .borrow_analysis
            .analysis
            .statement_entry_states
            .get(&statement.id)
        else {
            return false;
        };

        let Some(local_snapshot) = snapshot.locals.iter().find(|local| local.local == local_id)
        else {
            return false;
        };

        local_snapshot.mode.contains(LocalMode::ALIAS)
            && !local_snapshot.mode.contains(LocalMode::SLOT)
    }

    fn call_returns_alias_reference(&self, target: &CallTarget) -> bool {
        let CallTarget::UserFunction(path) = target else {
            return false;
        };

        self.hir.functions.iter().any(|function| {
            self.hir
                .side_table
                .function_name_path(function.id)
                .map(|candidate| candidate == path)
                .unwrap_or(false)
                && function.return_aliases.len() == 1
                && function.return_aliases[0].is_some()
        })
    }

    pub(crate) fn emit_return_terminator(
        &mut self,
        expression: &crate::compiler_frontend::hir::hir_nodes::HirExpression,
    ) -> Result<(), CompilerError> {
        if self.is_unit_expression(expression) {
            self.emit_line("return;");
            return Ok(());
        }

        let value = self.lower_return_value_expression(expression)?;
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
