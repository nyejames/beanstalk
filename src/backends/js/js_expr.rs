//! Expression lowering helpers for the JavaScript backend.
//!
//! These routines map HIR expressions into JS source strings while preserving the backend's
//! binding and alias helper conventions.

use crate::backends::js::JsEmitter;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_datatypes::HirTypeKind;
use crate::compiler_frontend::hir::hir_nodes::{
    HirBinOp, HirBuiltinCastKind, HirExpression, HirExpressionKind, HirPlace, HirUnaryOp,
    OptionVariant, ResultVariant,
};

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn lower_expr(
        &mut self,
        expression: &HirExpression,
    ) -> Result<String, CompilerError> {
        // WHAT: dispatch lowering by the fully resolved HIR expression shape.
        // WHY: HIR has already linearized side effects, so expression lowering can stay a direct
        //      semantic mapping from each variant to the exact JS runtime helper sequence it needs.
        match &expression.kind {
            HirExpressionKind::Int(value) => Ok(value.to_string()),

            HirExpressionKind::Float(value) => {
                if value.is_nan() {
                    Ok("NaN".to_owned())
                } else if value.is_infinite() {
                    if value.is_sign_positive() {
                        Ok("Infinity".to_owned())
                    } else {
                        Ok("-Infinity".to_owned())
                    }
                } else {
                    Ok(value.to_string())
                }
            }

            HirExpressionKind::Bool(value) => Ok(value.to_string()),
            HirExpressionKind::Char(value) => Ok(escape_js_char(*value)),
            HirExpressionKind::StringLiteral(value) => Ok(escape_js_string(value)),

            HirExpressionKind::Load(place) => {
                Ok(format!("__bs_read({})", self.lower_place(place)?))
            }
            HirExpressionKind::Copy(place) => Ok(format!(
                "__bs_clone_value(__bs_read({}))",
                self.lower_place(place)?
            )),

            HirExpressionKind::BinOp { left, op, right } => self.lower_bin_op(left, *op, right),
            HirExpressionKind::UnaryOp { op, operand } => self.lower_unary_op(*op, operand),

            HirExpressionKind::StructConstruct { fields, .. } => {
                let mut pairs = Vec::with_capacity(fields.len());
                for (field_id, value) in fields {
                    let field_name = self.field_name(*field_id)?.to_owned();
                    let field_value = self.lower_expr(value)?;
                    pairs.push(format!("{field_name}: {field_value}"));
                }

                Ok(format!("{{ {} }}", pairs.join(", ")))
            }

            HirExpressionKind::Collection(elements) => {
                let lowered = elements
                    .iter()
                    .map(|element| self.lower_expr(element))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(format!("[{}]", lowered.join(", ")))
            }

            HirExpressionKind::Range { start, end } => {
                let start = self.lower_expr(start)?;
                let end = self.lower_expr(end)?;
                Ok(format!("{{ start: {start}, end: {end} }}"))
            }

            HirExpressionKind::TupleConstruct { elements } => {
                if elements.is_empty() {
                    Ok("undefined".to_owned())
                } else {
                    let lowered = elements
                        .iter()
                        .map(|element| self.lower_expr(element))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(format!("[{}]", lowered.join(", ")))
                }
            }

            HirExpressionKind::TupleGet { tuple, index } => {
                let tuple = self.lower_expr(tuple)?;
                Ok(format!("({tuple})[{index}]"))
            }

            HirExpressionKind::OptionConstruct { variant, value } => match (variant, value) {
                (OptionVariant::None, None) => Ok("{ tag: \"none\" }".to_owned()),
                (OptionVariant::Some, Some(value)) => Ok(format!(
                    "{{ tag: \"some\", value: {} }}",
                    self.lower_expr(value)?
                )),
                (OptionVariant::Some, None) => Err(CompilerError::compiler_error(
                    "JavaScript backend: OptionConstruct(Some) missing value",
                )),
                (OptionVariant::None, Some(_)) => Err(CompilerError::compiler_error(
                    "JavaScript backend: OptionConstruct(None) should not carry a value",
                )),
            },

            HirExpressionKind::ResultConstruct { variant, value } => {
                let lowered_value = self.lower_expr(value)?;
                let tag = match variant {
                    ResultVariant::Ok => "ok",
                    ResultVariant::Err => "err",
                };
                Ok(format!("{{ tag: \"{tag}\", value: {lowered_value} }}"))
            }

            HirExpressionKind::ResultPropagate { result } => {
                let lowered_result = self.lower_expr(result)?;
                Ok(format!("__bs_result_propagate({lowered_result})"))
            }

            HirExpressionKind::ResultIsOk { result } => {
                let lowered_result = self.lower_expr(result)?;
                Ok(format!("(({lowered_result}).tag === \"ok\")"))
            }

            HirExpressionKind::ResultUnwrapOk { result } => {
                let lowered_result = self.lower_expr(result)?;
                Ok(format!("(({lowered_result}).value)"))
            }

            HirExpressionKind::ResultUnwrapErr { result } => {
                let lowered_result = self.lower_expr(result)?;
                Ok(format!("(({lowered_result}).value)"))
            }

            HirExpressionKind::BuiltinCast { kind, value } => {
                let lowered_value = self.lower_expr(value)?;
                let helper = match kind {
                    HirBuiltinCastKind::Int => "__bs_cast_int",
                    HirBuiltinCastKind::Float => "__bs_cast_float",
                };
                Ok(format!("{helper}({lowered_value})"))
            }
        }
    }

    pub(crate) fn lower_place(&mut self, place: &HirPlace) -> Result<String, CompilerError> {
        match place {
            HirPlace::Local(local_id) => Ok(self.local_name(*local_id)?.to_owned()),

            HirPlace::Field { base, field } => {
                let base = self.lower_place(base)?;
                let field = escape_js_string(self.field_name(*field)?);
                Ok(format!("__bs_field({base}, {field})"))
            }

            HirPlace::Index { base, index } => {
                let base = self.lower_place(base)?;
                let index = self.lower_expr(index)?;
                Ok(format!("__bs_index({base}, {index})"))
            }
        }
    }

    pub(crate) fn lower_return_value_expression(
        &mut self,
        expression: &HirExpression,
    ) -> Result<String, CompilerError> {
        match &expression.kind {
            HirExpressionKind::Load(place) => self.lower_place(place),
            HirExpressionKind::Copy(place) => Ok(format!(
                "__bs_clone_value(__bs_read({}))",
                self.lower_place(place)?
            )),
            HirExpressionKind::TupleConstruct { elements } => {
                let lowered = elements
                    .iter()
                    .map(|element| self.lower_return_value_expression(element))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("[{}]", lowered.join(", ")))
            }
            _ => self.lower_expr(expression),
        }
    }

    pub(crate) fn lower_call_argument(
        &mut self,
        expression: &HirExpression,
    ) -> Result<String, CompilerError> {
        // User-defined Beanstalk functions speak the reference ABI: places stay as refs and
        // rvalues are wrapped into fresh bindings so the callee can alias or overwrite them.
        match &expression.kind {
            HirExpressionKind::Load(place) => self.lower_place(place),
            HirExpressionKind::Copy(place) => Ok(format!(
                "__bs_binding(__bs_clone_value(__bs_read({})))",
                self.lower_place(place)?
            )),
            _ => Ok(format!("__bs_binding({})", self.lower_expr(expression)?)),
        }
    }

    pub(crate) fn lower_host_call_argument(
        &mut self,
        expression: &HirExpression,
    ) -> Result<String, CompilerError> {
        // Host calls are raw JavaScript boundaries, not Beanstalk functions, so they need the
        // concrete JS value rather than a binding wrapper.
        self.lower_expr(expression)
    }

    pub(crate) fn is_unit_expression(&self, expression: &HirExpression) -> bool {
        if matches!(
            expression.kind,
            HirExpressionKind::TupleConstruct { ref elements } if elements.is_empty()
        ) {
            return true;
        }

        matches!(
            self.hir.type_context.get(expression.ty).kind,
            HirTypeKind::Unit
        )
    }

    fn lower_bin_op(
        &mut self,
        left: &HirExpression,
        operator: HirBinOp,
        right: &HirExpression,
    ) -> Result<String, CompilerError> {
        let left = self.lower_expr(left)?;
        let right = self.lower_expr(right)?;

        let js_operator = match operator {
            HirBinOp::Add => "+",
            HirBinOp::Sub => "-",
            HirBinOp::Mul => "*",
            HirBinOp::Div => "/",
            HirBinOp::Mod => "%",
            HirBinOp::Eq => "===",
            HirBinOp::Ne => "!==",
            HirBinOp::Lt => "<",
            HirBinOp::Le => "<=",
            HirBinOp::Gt => ">",
            HirBinOp::Ge => ">=",
            HirBinOp::And => "&&",
            HirBinOp::Or => "||",
            HirBinOp::Exponent => "**",
            HirBinOp::Root => {
                return Ok(format!("Math.pow({right}, 1 / {left})"));
            }
        };

        Ok(format!("({left} {js_operator} {right})"))
    }

    fn lower_unary_op(
        &mut self,
        operator: HirUnaryOp,
        operand: &HirExpression,
    ) -> Result<String, CompilerError> {
        let operand = self.lower_expr(operand)?;
        let js_operator = match operator {
            HirUnaryOp::Neg => "-",
            HirUnaryOp::Not => "!",
        };

        Ok(format!("({js_operator}{operand})"))
    }
}

fn escape_js_string(value: &str) -> String {
    let mut escaped = String::from("\"");

    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\0"),
            control if control.is_control() => {
                escaped.push_str(&format!("\\u{:04X}", control as u32));
            }
            normal => escaped.push(normal),
        }
    }

    escaped.push('"');
    escaped
}

fn escape_js_char(value: char) -> String {
    let mut escaped = String::from("\"");

    match value {
        '\\' => escaped.push_str("\\\\"),
        '"' => escaped.push_str("\\\""),
        '\n' => escaped.push_str("\\n"),
        '\r' => escaped.push_str("\\r"),
        '\t' => escaped.push_str("\\t"),
        '\0' => escaped.push_str("\\0"),
        control if control.is_control() => {
            escaped.push_str(&format!("\\u{:04X}", control as u32));
        }
        normal => escaped.push(normal),
    }

    escaped.push('"');
    escaped
}
