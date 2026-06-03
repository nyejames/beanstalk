//! Expression lowering helpers for the JavaScript backend.
//!
//! These routines map HIR expressions into JS source strings while preserving the backend's
//! binding and alias helper conventions.

use crate::backends::js::JsEmitter;
use crate::backends::js::value_use::JsValueUse;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::hir::expressions::{
    HirBuiltinCastKind, HirExpression, HirExpressionKind, HirVariantCarrier,
};
use crate::compiler_frontend::hir::operators::{HirBinOp, HirUnaryOp};
use crate::compiler_frontend::hir::places::HirPlace;

#[derive(Clone, Copy)]
enum OptionComparisonSide {
    Option { inner_type: TypeId },
    NoneLiteral,
    Other,
}

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn lower_fallible_success_condition(
        &mut self,
        result: &HirExpression,
    ) -> Result<String, CompilerError> {
        let lowered_result = self.lower_expr(result)?;

        Ok(format!("(({lowered_result}).tag === \"ok\")"))
    }

    pub(crate) fn lower_expr(
        &mut self,
        expression: &HirExpression,
    ) -> Result<String, CompilerError> {
        // WHAT: dispatch lowering by the fully resolved HIR expression shape.
        // WHY: HIR has already linearized side effects, so expression lowering can stay a direct
        //      semantic mapping from each variant to the exact JS runtime helper sequence it needs.
        match &expression.kind {
            HirExpressionKind::Int(value) => Ok(value.to_string()),
            HirExpressionKind::VariantConstruct {
                carrier,
                variant_index,
                fields,
            } => self.lower_variant_construct(carrier, *variant_index, fields),
            HirExpressionKind::ConstructDynamicTraitValue {
                value, evidence_id, ..
            } => {
                let lowered_value = self.lower_expr(value)?;
                Ok(self.lower_dynamic_trait_construction(lowered_value, *evidence_id))
            }

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

            HirExpressionKind::Load(_) | HirExpressionKind::Copy(_) => {
                self.lower_expression_for_use(expression, JsValueUse::PlainExpression)
            }

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

            HirExpressionKind::FallibleUnwrapSuccess { result } => {
                let lowered_result = self.lower_expr(result)?;
                Ok(format!("(({lowered_result}).value)"))
            }

            HirExpressionKind::FallibleUnwrapError { result } => {
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

            HirExpressionKind::VariantPayloadGet {
                carrier,
                source,
                variant_index,
                field_index,
            } => self.lower_variant_payload_get(carrier, source, *variant_index, *field_index),
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
        self.lower_expression_for_use(expression, JsValueUse::ReturnValue)
    }

    // WHAT: lowers a variant construction into a JS object literal.
    // WHY: centralises tag policy and field-key escaping in one place.
    fn lower_variant_construct(
        &mut self,
        carrier: &HirVariantCarrier,
        variant_index: usize,
        fields: &[crate::compiler_frontend::hir::expressions::HirVariantField],
    ) -> Result<String, CompilerError> {
        let mut entries = vec![];
        for field in fields {
            let js_value = self.lower_expr(&field.value)?;
            if let Some(name) = field.name {
                let js_name = escape_js_string(self.string_table.resolve(name));
                entries.push(format!("{js_name}: {js_value}"));
            } else {
                entries.push(js_value);
            }
        }

        let tag_entry = match carrier {
            HirVariantCarrier::Choice { .. } => format!("tag: {variant_index}"),
            HirVariantCarrier::Option => {
                let tag = if variant_index == 0 { "none" } else { "some" };
                format!("tag: \"{tag}\"")
            }
            HirVariantCarrier::Fallible => {
                let tag = if variant_index == 0 { "ok" } else { "err" };
                format!("tag: \"{tag}\"")
            }
        };

        let mut all_entries = vec![tag_entry];
        all_entries.extend(entries);
        Ok(format!("{{ {} }}", all_entries.join(", ")))
    }

    // WHAT: lowers a variant payload field access into a JS bracket-access expression.
    // WHY: bracket access is safe for field names that collide with JS reserved words.
    fn lower_variant_payload_get(
        &mut self,
        carrier: &HirVariantCarrier,
        source: &HirExpression,
        variant_index: usize,
        field_index: usize,
    ) -> Result<String, CompilerError> {
        let source_js = self.lower_expr(source)?;
        let field_name_js = match carrier {
            HirVariantCarrier::Choice { choice_id } => {
                let choice = self.hir.choices.get(choice_id.0 as usize).ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "JavaScript backend: invalid ChoiceId {choice_id:?} in VariantPayloadGet"
                    ))
                })?;
                let variant = choice.variants.get(variant_index).ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "JavaScript backend: invalid variant index {variant_index} for choice {choice_id:?}"
                    ))
                })?;
                let field = variant.fields.get(field_index).ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "JavaScript backend: invalid field index {field_index} for variant {variant_index} of choice {choice_id:?}"
                    ))
                })?;
                escape_js_string(self.string_table.resolve(field.name))
            }
            HirVariantCarrier::Option | HirVariantCarrier::Fallible => "\"value\"".to_owned(),
        };
        Ok(format!("({source_js})[{field_name_js}]"))
    }

    pub(crate) fn is_unit_expression(&self, expression: &HirExpression) -> bool {
        if matches!(
            expression.kind,
            HirExpressionKind::TupleConstruct { ref elements } if elements.is_empty()
        ) {
            return true;
        }

        expression.ty == self.type_environment.builtins().none
    }

    /// Whether the expression's resolved type is a nominal choice type.
    ///
    /// WHY: choice carriers are object literals with a `tag` property, so equality
    /// must compare tags rather than using reference equality.
    fn is_choice_type(&self, expression: &HirExpression) -> bool {
        self.type_environment.variants_for(expression.ty).is_some()
    }

    fn is_choice_type_id(&self, type_id: TypeId) -> bool {
        self.type_environment.variants_for(type_id).is_some()
    }

    fn lower_bin_op(
        &mut self,
        left: &HirExpression,
        operator: HirBinOp,
        right: &HirExpression,
    ) -> Result<String, CompilerError> {
        let option_equality = if matches!(operator, HirBinOp::Eq | HirBinOp::Ne) {
            self.option_equality_sides(left, right)
        } else {
            None
        };

        // Unit choice equality compares variant tags because choice carriers are
        // object literals ({ tag: N }) and reference equality would be incorrect.
        let is_choice_equality = matches!(operator, HirBinOp::Eq | HirBinOp::Ne)
            && self.is_choice_type(left)
            && self.is_choice_type(right);

        let left = self.lower_expr(left)?;
        let right = self.lower_expr(right)?;

        if let Some((left_side, right_side)) = option_equality {
            return self.lower_option_equality(left, left_side, operator, right, right_side);
        }

        if is_choice_equality {
            self.used_choice_equality = true;
            let eq_expr = format!("__bs_choice_eq({left}, {right})");
            return match operator {
                HirBinOp::Eq => Ok(eq_expr),
                HirBinOp::Ne => Ok(format!("(!{eq_expr})")),
                _ => unreachable!(),
            };
        }

        let js_operator = match operator {
            HirBinOp::Add => "+",
            HirBinOp::Sub => "-",
            HirBinOp::Mul => "*",
            HirBinOp::Div => "/",
            HirBinOp::Mod => {
                return Ok(format!(
                    "(() => {{ const __lhs = {left}; const __rhs = {right}; if (__rhs === 0) {{ throw new Error(\"Modulus by zero\"); }} return ((__lhs % __rhs) + Math.abs(__rhs)) % Math.abs(__rhs); }})()"
                ));
            }
            HirBinOp::Eq => "===",
            HirBinOp::Ne => "!==",
            HirBinOp::Lt => "<",
            HirBinOp::Le => "<=",
            HirBinOp::Gt => ">",
            HirBinOp::Ge => ">=",
            // Short-circuit runtime semantics are guaranteed by HIR CFG lowering:
            // `and`/`or` may arrive here as plain BinOp only when both operands are side-effect
            // free at expression level. Branch-gated RHS evaluation is lowered earlier.
            HirBinOp::And => "&&",
            HirBinOp::Or => "||",
            HirBinOp::Exponent => "**",
            HirBinOp::IntDiv => {
                return Ok(format!(
                    "(() => {{ const __lhs = {left}; const __rhs = {right}; if (__rhs === 0) {{ throw new Error(\"Integer division by zero\"); }} return Math.trunc(__lhs / __rhs); }})()"
                ));
            }
        };

        Ok(format!("({left} {js_operator} {right})"))
    }

    fn option_equality_sides(
        &self,
        left: &HirExpression,
        right: &HirExpression,
    ) -> Option<(OptionComparisonSide, OptionComparisonSide)> {
        let left_side = self.classify_option_comparison_side(left.ty);
        let right_side = self.classify_option_comparison_side(right.ty);

        if matches!(left_side, OptionComparisonSide::Option { .. })
            || matches!(right_side, OptionComparisonSide::Option { .. })
        {
            Some((left_side, right_side))
        } else {
            None
        }
    }

    fn classify_option_comparison_side(&self, type_id: TypeId) -> OptionComparisonSide {
        let Some(inner_type) = self.type_environment.option_inner_type(type_id) else {
            return OptionComparisonSide::Other;
        };

        if inner_type == self.type_environment.builtins().none {
            return OptionComparisonSide::NoneLiteral;
        }

        OptionComparisonSide::Option { inner_type }
    }

    fn lower_option_equality(
        &mut self,
        left: String,
        left_side: OptionComparisonSide,
        operator: HirBinOp,
        right: String,
        right_side: OptionComparisonSide,
    ) -> Result<String, CompilerError> {
        let equality = match (left_side, right_side) {
            (OptionComparisonSide::Option { .. }, OptionComparisonSide::NoneLiteral) => {
                format!("(({left}).tag === \"none\")")
            }

            (OptionComparisonSide::NoneLiteral, OptionComparisonSide::Option { .. }) => {
                format!("(({right}).tag === \"none\")")
            }

            (OptionComparisonSide::Option { inner_type }, OptionComparisonSide::Option { .. }) => {
                let inner_equality = self.lower_option_inner_equality(
                    format!("({left}).value"),
                    inner_type,
                    format!("({right}).value"),
                );
                format!(
                    "((({left}).tag === ({right}).tag) && ((({left}).tag === \"none\") || {inner_equality}))"
                )
            }

            (OptionComparisonSide::Option { inner_type }, OptionComparisonSide::Other) => {
                let inner_equality =
                    self.lower_option_inner_equality(format!("({left}).value"), inner_type, right);
                format!("((({left}).tag === \"some\") && {inner_equality})")
            }

            (OptionComparisonSide::Other, OptionComparisonSide::Option { inner_type }) => {
                let inner_equality =
                    self.lower_option_inner_equality(left, inner_type, format!("({right}).value"));
                format!("((({right}).tag === \"some\") && {inner_equality})")
            }

            _ => {
                return Err(CompilerError::compiler_error(
                    "JavaScript backend received option equality without an option operand",
                ));
            }
        };

        match operator {
            HirBinOp::Eq => Ok(equality),
            HirBinOp::Ne => Ok(format!("(!{equality})")),
            _ => Err(CompilerError::compiler_error(
                "JavaScript backend received non-equality option comparison",
            )),
        }
    }

    pub(crate) fn lower_option_inner_equality(
        &mut self,
        left: String,
        inner_type: TypeId,
        right: String,
    ) -> String {
        if self.is_choice_type_id(inner_type) {
            self.used_choice_equality = true;
            return format!("__bs_choice_eq({left}, {right})");
        }

        format!("({left} === {right})")
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

pub(crate) fn escape_js_string(value: &str) -> String {
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
