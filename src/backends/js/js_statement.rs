//! Statement lowering helpers for the JavaScript backend.
//!
//! These routines emit block-local statements after HIR has already made evaluation order and
//! control-flow edges explicit.

use crate::backends::js::JsEmitter;
use crate::backends::js::js_expr::escape_js_string;
use crate::compiler_frontend::analysis::borrow_checker::LocalMode;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::external_packages::CallTarget;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind};
use crate::compiler_frontend::hir::functions::HirFunction;
use crate::compiler_frontend::hir::ids::{BlockId, LocalId};
use crate::compiler_frontend::hir::patterns::{HirMatchArm, HirPattern, HirRelationalPatternOp};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};
use crate::compiler_frontend::hir::terminators::HirTerminator;

/// Result of lowering a call target for the JS backend.
///
/// WHAT: distinguishes between a regular function call (emit as `name(args)`) and an inline
/// expression (emit as a substituted expression template without call wrapping).
enum LoweredCallTarget {
    FunctionName(String),
    InlineExpression { template: String },
}

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_block_statements(
        &mut self,
        block: &crate::compiler_frontend::hir::blocks::HirBlock,
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
                let lowered_target = self.lower_call_target(target)?;
                let args = if matches!(target, CallTarget::ExternalFunction(_)) {
                    args.iter()
                        .map(|arg| self.lower_host_call_argument(arg))
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    args.iter()
                        .map(|arg| self.lower_call_argument(arg))
                        .collect::<Result<Vec<_>, _>>()?
                };

                let call = match &lowered_target {
                    LoweredCallTarget::FunctionName(name) => {
                        format!("{name}({})", args.join(", "))
                    }
                    LoweredCallTarget::InlineExpression { template } => {
                        substitute_inline_expression(template, &args)?
                    }
                };

                if let Some(result_local) = result {
                    let result_name = self.local_name(*result_local)?;
                    if self.call_returns_alias_reference(target) {
                        self.emit_line(&format!("__bs_assign_borrow({result_name}, {call});"));
                    } else {
                        self.emit_line(&format!("__bs_assign_value({result_name}, {call});"));
                    }
                } else {
                    self.emit_line(&format!("{call};"));
                }
            }

            HirStatementKind::CallDynamicTraitMethod {
                receiver,
                requirement_id,
                args,
                result,
                ..
            } => {
                let receiver = self.lower_expr(receiver)?;
                let args = args
                    .iter()
                    .map(|arg| self.lower_call_argument(&arg.value))
                    .collect::<Result<Vec<_>, _>>()?;
                let call = self.lower_dynamic_trait_dispatch(receiver, *requirement_id, args);

                if let Some(result_local) = result {
                    let result_name = self.local_name(*result_local)?;
                    self.emit_line(&format!("__bs_assign_value({result_name}, {call});"));
                } else {
                    self.emit_line(&format!("{call};"));
                }
            }

            HirStatementKind::Expr(expression) => {
                let expression = self.lower_expr(expression)?;
                self.emit_line(&format!("{expression};"));
            }

            HirStatementKind::Drop(_) => {
                // No-op for GC backend.
            }

            HirStatementKind::PushRuntimeFragment { vec_local, value } => {
                // WHAT: lower a fragment push into a JS vec push call against the unwrapped array.
                // WHY: locals are stored as binding wrappers `{ value: ... }` so `.push` cannot be
                //      called on the binding itself. __bs_read returns the underlying array.
                let vec_name = self.local_name(*vec_local)?.to_owned();
                let value_expr = self.lower_expr(value)?;
                self.emit_line(&format!("__bs_read({vec_name}).push({value_expr});"));
            }
        }

        Ok(())
    }

    fn lower_call_target(
        &mut self,
        target: &CallTarget,
    ) -> Result<LoweredCallTarget, CompilerError> {
        match target {
            CallTarget::UserFunction(function_id) => Ok(LoweredCallTarget::FunctionName(
                self.function_name(*function_id)?.to_owned(),
            )),
            CallTarget::ExternalFunction(id) => {
                self.referenced_external_functions.insert(*id);
                let function_def = self
                    .config
                    .external_package_registry
                    .get_function_by_id(*id)
                    .ok_or_else(|| {
                        CompilerError::compiler_error(format!(
                            "JavaScript backend: unknown external function '{}'",
                            id.name()
                        ))
                    })?;
                let lowering = function_def.lowerings.js.as_ref().ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "JavaScript backend: no JS lowering registered for external function '{}'",
                        id.name()
                    ))
                })?;
                match lowering {
                    crate::compiler_frontend::external_packages::ExternalJsLowering::RuntimeFunction(name) => {
                        Ok(LoweredCallTarget::FunctionName(name.clone()))
                    }
                    crate::compiler_frontend::external_packages::ExternalJsLowering::InlineExpression(template) => {
                        Ok(LoweredCallTarget::InlineExpression {
                            template: template.clone(),
                        })
                    }
                    crate::compiler_frontend::external_packages::ExternalJsLowering::ExternalModuleExport { export_name } => {
                        if self.config.external_module_export_glue_enabled {
                            let glue_name = crate::backends::js::external_module_export_glue_function_name(*id);
                            Ok(LoweredCallTarget::FunctionName(glue_name))
                        } else {
                            Err(CompilerError::compiler_error(format!(
                                "JavaScript backend: external module export '{}' requires generated HTML glue before lowering.",
                                export_name
                            )))
                        }
                    }
                }
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
                self.emit_line(&format!("__bs_write({target_ref}, {emitted_value});"));

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
                    self.emit_line(&format!("__bs_write({local_name}, __bs_read({source}));",));
                } else {
                    self.emit_line(&format!("__bs_assign_borrow({local_name}, {source});"));
                }
            }
            HirExpressionKind::Copy(place) => {
                let copied = format!("__bs_clone_value(__bs_read({}))", self.lower_place(place)?);
                if alias_only {
                    self.emit_line(&format!("__bs_write({local_name}, {copied});"));
                } else {
                    self.emit_line(&format!("__bs_assign_value({local_name}, {copied});"));
                }
            }
            _ => {
                let lowered = self.lower_expr(value)?;
                if alias_only {
                    self.emit_line(&format!("__bs_write({local_name}, {lowered});"));
                } else {
                    self.emit_line(&format!("__bs_assign_value({local_name}, {lowered});"));
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

        Self::snapshot_local_is_alias_only(local_snapshot.mode)
    }

    pub(crate) fn local_is_alias_only_at_block_entry(
        &self,
        block_id: BlockId,
        local_id: LocalId,
    ) -> bool {
        let Some(snapshot) = self
            .borrow_analysis
            .analysis
            .block_entry_states
            .get(&block_id)
        else {
            return false;
        };

        let Some(local_snapshot) = snapshot.locals.iter().find(|local| local.local == local_id)
        else {
            return false;
        };

        Self::snapshot_local_is_alias_only(local_snapshot.mode)
    }

    fn snapshot_local_is_alias_only(mode: LocalMode) -> bool {
        mode.contains(LocalMode::ALIAS) && !mode.contains(LocalMode::SLOT)
    }

    fn call_returns_alias_reference(&self, target: &CallTarget) -> bool {
        let CallTarget::UserFunction(function_id) = target else {
            return false;
        };

        self.hir
            .functions
            .iter()
            .find(|function| function.id == *function_id)
            .is_some_and(|function| {
                // Fallible calls return a fresh backend carrier. Any aliasing belongs to the
                // success payload inside that carrier, not to the carrier local itself.
                if self
                    .type_environment
                    .fallible_carrier_slots(function.return_type)
                    .is_some()
                {
                    return false;
                }

                function.return_aliases.len() == 1 && function.return_aliases[0].is_some()
            })
    }

    fn current_function_returns_alias_reference(&self) -> bool {
        let Some(function_id) = self.current_function else {
            return false;
        };

        self.hir
            .functions
            .iter()
            .find(|function| function.id == function_id)
            .is_some_and(|function| {
                function.return_aliases.len() == 1 && function.return_aliases[0].is_some()
            })
    }

    pub(crate) fn emit_return_terminator(
        &mut self,
        expression: &HirExpression,
    ) -> Result<(), CompilerError> {
        if self.is_unit_expression(expression) {
            self.emit_line("return;");
            return Ok(());
        }

        let value = if self.current_function_returns_alias_reference() {
            self.lower_return_value_expression(expression)?
        } else {
            self.lower_expr(expression)?
        };
        self.emit_line(&format!("return {value};"));
        Ok(())
    }

    pub(crate) fn emit_success_return_terminator(
        &mut self,
        expression: &HirExpression,
    ) -> Result<(), CompilerError> {
        let Some(function_id) = self.current_function else {
            return Err(CompilerError::compiler_error(
                "JavaScript backend: ReturnSuccess emitted outside a function",
            ));
        };
        let function = self
            .hir
            .functions
            .iter()
            .find(|function| function.id == function_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "JavaScript backend: current function {function_id:?} is missing"
                ))
            })?;
        let Some((success_type, _)) = self
            .type_environment
            .fallible_carrier_slots(function.return_type)
        else {
            return Err(CompilerError::compiler_error(
                "JavaScript backend: ReturnSuccess emitted in a non-fallible function",
            ));
        };
        if expression.ty != success_type {
            return Err(CompilerError::compiler_error(
                "JavaScript backend: ReturnSuccess value type does not match function success slot",
            ));
        }

        let value = if self.current_function_returns_alias_reference() {
            self.lower_return_value_expression(expression)?
        } else {
            self.lower_expr(expression)?
        };
        self.emit_line(&format!("return {{ tag: \"ok\", value: {value} }};"));
        Ok(())
    }

    pub(crate) fn emit_error_return_terminator(
        &mut self,
        expression: &HirExpression,
    ) -> Result<(), CompilerError> {
        let Some(function_id) = self.current_function else {
            return Err(CompilerError::compiler_error(
                "JavaScript backend: ReturnError emitted outside a function",
            ));
        };
        let function = self
            .hir
            .functions
            .iter()
            .find(|function| function.id == function_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "JavaScript backend: current function {function_id:?} is missing"
                ))
            })?;
        let Some((_, error_type)) = self
            .type_environment
            .fallible_carrier_slots(function.return_type)
        else {
            return Err(CompilerError::compiler_error(
                "JavaScript backend: ReturnError emitted in a non-fallible function",
            ));
        };
        if expression.ty != error_type {
            return Err(CompilerError::compiler_error(
                "JavaScript backend: ReturnError value type does not match function error slot",
            ));
        }

        let value = self.lower_expr(expression)?;
        self.emit_line(&format!("return {{ tag: \"err\", value: {value} }};"));
        Ok(())
    }

    pub(crate) fn emit_assert_failure_terminator(
        &mut self,
        message: &Option<String>,
    ) -> Result<(), CompilerError> {
        let js_message = match message {
            Some(text) => format!("throw new Error({});", escape_js_string(text)),
            None => "throw new Error(\"assertion failed\");".to_string(),
        };
        self.emit_line(&js_message);

        Ok(())
    }

    pub(crate) fn emit_runtime_failure_terminator(
        &mut self,
        message: &str,
    ) -> Result<(), CompilerError> {
        self.emit_line(&format!("throw new Error({});", escape_js_string(message)));

        Ok(())
    }

    pub(crate) fn emit_dispatcher_for_function(
        &mut self,
        function: &HirFunction,
        reachable_blocks: &[BlockId],
    ) -> Result<(), CompilerError> {
        let state_identifier = self.next_temp_identifier("__bb");

        self.emit_line(&format!("let {state_identifier} = {};", function.entry.0));
        self.emit_line("while (true) {");
        self.indent += 1;
        self.emit_line(&format!("switch ({state_identifier}) {{"));
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
                "throw new Error(\"Invalid control-flow block: \" + {state_identifier});",
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
                self.emit_jump_argument_transfer(*target, args)?;
                self.emit_line(&format!("{state_identifier} = {};", target.0));
                self.emit_line("continue;");
            }

            HirTerminator::If {
                condition,
                then_block,
                else_block,
            } => {
                let condition = self.lower_expr(condition)?;
                self.emit_line(&format!("if ({condition}) {{"));
                self.with_indent(|emitter| {
                    emitter.emit_line(&format!("{state_identifier} = {};", then_block.0));
                });
                self.emit_line("} else {");
                self.with_indent(|emitter| {
                    emitter.emit_line(&format!("{state_identifier} = {};", else_block.0));
                });
                self.emit_line("}");
                self.emit_line("continue;");
            }

            HirTerminator::FallibleBranch {
                result,
                success_block,
                error_block,
            } => {
                let condition = self.lower_fallible_success_condition(result)?;
                self.emit_line(&format!("if ({condition}) {{"));
                self.with_indent(|emitter| {
                    emitter.emit_line(&format!("{state_identifier} = {};", success_block.0));
                });
                self.emit_line("} else {");
                self.with_indent(|emitter| {
                    emitter.emit_line(&format!("{state_identifier} = {};", error_block.0));
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
                self.emit_line(&format!("const {scrutinee_temp} = {scrutinee};"));

                // If the last arm is an unguarded wildcard or capture, emit it as `else`
                // instead of `else if (true)` and skip the unreachable fallback throw.
                let has_unconditional_fallback = matches!(
                    arms.last(),
                    Some(HirMatchArm {
                        pattern: HirPattern::Wildcard | HirPattern::Capture,
                        guard: None,
                        ..
                    })
                );
                let emit_count = if has_unconditional_fallback {
                    arms.len() - 1
                } else {
                    arms.len()
                };

                for (index, arm) in arms.iter().enumerate().take(emit_count) {
                    let condition = self.lower_match_arm_condition(&scrutinee_temp, arm)?;
                    if index == 0 {
                        self.emit_line(&format!("if ({condition}) {{"));
                    } else {
                        self.emit_line(&format!("else if ({condition}) {{"));
                    }

                    self.with_indent(|emitter| {
                        emitter.emit_line(&format!("{state_identifier} = {};", arm.body.0));
                    });
                    self.emit_line("}");
                }

                if has_unconditional_fallback {
                    if let Some(wildcard_arm) = arms.last() {
                        self.emit_line("else {");
                        self.with_indent(|emitter| {
                            emitter.emit_line(&format!(
                                "{state_identifier} = {};",
                                wildcard_arm.body.0
                            ));
                        });
                        self.emit_line("}");
                    }
                } else {
                    self.emit_line("else {");
                    self.with_indent(|emitter| {
                        emitter.emit_line("throw new Error(\"No match arm selected\");");
                    });
                    self.emit_line("}");
                }
                self.emit_line("continue;");
            }

            HirTerminator::Break { target } | HirTerminator::Continue { target } => {
                self.emit_line(&format!("{state_identifier} = {};", target.0));
                self.emit_line("continue;");
            }

            HirTerminator::Return(value) => {
                self.emit_return_terminator(value)?;
            }

            HirTerminator::ReturnSuccess(value) => {
                self.emit_success_return_terminator(value)?;
            }

            HirTerminator::ReturnError(value) => {
                self.emit_error_return_terminator(value)?;
            }

            HirTerminator::Uninitialized => {
                return Err(CompilerError::compiler_error(
                    "Uninitialized terminator reached JS backend lowering",
                ));
            }

            HirTerminator::RuntimeFailure { message } => {
                self.emit_runtime_failure_terminator(message)?;
            }

            HirTerminator::AssertFailure { message } => {
                self.emit_assert_failure_terminator(message)?;
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
                format!("{scrutinee_expression} === {literal}")
            }
            HirPattern::OptionNone => {
                format!("({scrutinee_expression}).tag === \"none\"")
            }
            HirPattern::OptionValue { value } => {
                let literal = self.lower_expr(value)?;
                let inner_equality = self.lower_option_inner_equality(
                    format!("({scrutinee_expression}).value"),
                    value.ty,
                    literal,
                );
                format!("((({scrutinee_expression}).tag === \"some\") && {inner_equality})")
            }
            HirPattern::OptionRelational { op, value } => {
                let rhs = self.lower_expr(value)?;
                let js_op = match op {
                    HirRelationalPatternOp::LessThan => "<",
                    HirRelationalPatternOp::LessThanOrEqual => "<=",
                    HirRelationalPatternOp::GreaterThan => ">",
                    HirRelationalPatternOp::GreaterThanOrEqual => ">=",
                };
                format!(
                    "((({scrutinee_expression}).tag === \"some\") && (({scrutinee_expression}).value {js_op} {rhs}))"
                )
            }
            HirPattern::Wildcard => "true".to_owned(),
            HirPattern::Capture => "true".to_owned(),
            HirPattern::OptionPresent => {
                format!("({scrutinee_expression}).tag === \"some\"")
            }
            HirPattern::Relational { op, value } => {
                let rhs = self.lower_expr(value)?;
                let js_op = match op {
                    HirRelationalPatternOp::LessThan => "<",
                    HirRelationalPatternOp::LessThanOrEqual => "<=",
                    HirRelationalPatternOp::GreaterThan => ">",
                    HirRelationalPatternOp::GreaterThanOrEqual => ">=",
                };
                format!("{scrutinee_expression} {js_op} {rhs}")
            }
            HirPattern::ChoiceVariant { variant_index, .. } => {
                format!("{scrutinee_expression}.tag === {variant_index}")
            }
        };

        if let Some(guard) = &arm.guard {
            let guard = self.lower_expr(guard)?;
            Ok(format!("({pattern_condition}) && ({guard})"))
        } else {
            Ok(pattern_condition)
        }
    }
}

/// Substitute lowered argument expressions into an inline expression template.
///
/// WHAT: replaces positional placeholders `#0`, `#1`, ... in the template with the corresponding
/// lowered argument string.
/// WHY: inline expressions are raw JS snippets; arguments are spliced in positionally so the
/// backend emits a single expression instead of a helper call.
pub(super) fn substitute_inline_expression(
    template: &str,
    args: &[String],
) -> Result<String, CompilerError> {
    let mut result = String::new();
    let mut seen_placeholders = vec![0usize; args.len()];
    let mut last_copied_byte = 0usize;
    let mut chars = template.char_indices().peekable();

    while let Some((start_byte, character)) = chars.next() {
        if character != '#' {
            continue;
        }

        let digit_start_byte = start_byte + character.len_utf8();
        let mut digit_end_byte = digit_start_byte;
        while let Some(&(next_byte, next_character)) = chars.peek() {
            if !next_character.is_ascii_digit() {
                break;
            }

            digit_end_byte = next_byte + next_character.len_utf8();
            chars.next();
        }

        if digit_end_byte == digit_start_byte {
            continue;
        }

        let placeholder = &template[start_byte..digit_end_byte];
        let argument_index = template[digit_start_byte..digit_end_byte]
            .parse::<usize>()
            .map_err(|_| {
                CompilerError::compiler_error(format!(
                    "JavaScript backend: inline expression template contains invalid placeholder '{placeholder}'"
                ))
            })?;

        let Some(argument) = args.get(argument_index) else {
            return Err(CompilerError::compiler_error(format!(
                "JavaScript backend: inline expression template contains placeholder '{placeholder}' but only {} argument(s) were provided.",
                args.len()
            )));
        };

        seen_placeholders[argument_index] += 1;
        if seen_placeholders[argument_index] > 1 {
            return Err(CompilerError::compiler_error(format!(
                "JavaScript backend: inline expression template contains duplicate placeholder '{placeholder}'. Each argument must be referenced at most once."
            )));
        }

        result.push_str(&template[last_copied_byte..start_byte]);
        result.push_str(argument);
        last_copied_byte = digit_end_byte;
    }

    result.push_str(&template[last_copied_byte..]);

    for (index, count) in seen_placeholders.iter().enumerate() {
        if *count == 0 {
            let placeholder = format!("#{index}");
            return Err(CompilerError::compiler_error(format!(
                "JavaScript backend: inline expression template is missing placeholder '{placeholder}'"
            )));
        }
    }

    Ok(result)
}
