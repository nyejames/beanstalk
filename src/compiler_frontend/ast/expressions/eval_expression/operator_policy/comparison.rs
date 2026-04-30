//! Comparison operator typing policy.
//!
//! WHAT: decides the result type of comparison operators and rejects invalid operand combinations.
//! WHY: structural equality rules for choices, scalar ordering, and mixed numeric comparisons
//! must be enforced consistently before backend lowering.

use super::diagnostics::invalid_comparison_types;
use super::shared::is_mixed_int_float;
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::return_type_error;

pub(super) fn is_comparison_operator(op: &Operator) -> bool {
    matches!(
        op,
        Operator::Equality
            | Operator::NotEqual
            | Operator::GreaterThan
            | Operator::GreaterThanOrEqual
            | Operator::LessThan
            | Operator::LessThanOrEqual
    )
}

pub(super) fn resolve_comparison_operator_type(
    lhs: &DataType,
    rhs: &DataType,
    op: &Operator,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    if lhs == rhs {
        return match (lhs, op) {
            (
                DataType::Int | DataType::Float | DataType::Decimal,
                Operator::Equality
                | Operator::NotEqual
                | Operator::GreaterThan
                | Operator::GreaterThanOrEqual
                | Operator::LessThan
                | Operator::LessThanOrEqual,
            ) => Ok(DataType::Bool),
            (DataType::Bool, Operator::Equality | Operator::NotEqual) => Ok(DataType::Bool),
            (DataType::StringSlice, Operator::Equality | Operator::NotEqual) => Ok(DataType::Bool),
            (
                DataType::Char,
                Operator::Equality
                | Operator::NotEqual
                | Operator::LessThan
                | Operator::LessThanOrEqual
                | Operator::GreaterThan
                | Operator::GreaterThanOrEqual,
            ) => Ok(DataType::Bool),
            (DataType::Choices { variants, .. }, Operator::Equality | Operator::NotEqual) => {
                // Phase 1: define the structural equality contract by checking payload fields.
                // If any payload field type does not support structural equality, emit a
                // specific diagnostic so the user knows why the comparison is rejected.
                for variant in variants {
                    if let ChoiceVariantPayload::Record { fields } = &variant.payload {
                        for field in fields {
                            if !field.value.data_type.supports_structural_equality() {
                                let field_name = field
                                    .id
                                    .name_str(string_table)
                                    .unwrap_or("<unknown>")
                                    .to_owned();
                                let field_type =
                                    field.value.data_type.display_with_table(string_table);
                                return_type_error!(
                                    format!(
                                        "Choice payload equality is not supported because field '{field_name}' has type '{field_type}', which does not support equality."
                                    ),
                                    location.clone(),
                                    {
                                        CompilationStage => "Expression Evaluation",
                                        PrimarySuggestion => "Use pattern matching and compare fields individually, or use a type that supports equality",
                                    }
                                );
                            }
                        }
                    }
                }

                // Phase 2: unit choice equality is supported.
                // Phase 3: payload structural equality is supported when all payload fields
                // support structural equality (verified above).
                Ok(DataType::Bool)
            }
            _ => invalid_comparison_types(lhs, rhs, op, location, string_table),
        };
    }

    if is_mixed_int_float(lhs, rhs) {
        return Ok(DataType::Bool);
    }

    // Two choice values of different nominal types are never comparable.
    if let (DataType::Choices { .. }, DataType::Choices { .. }) = (lhs, rhs) {
        let left_name = lhs.display_with_table(string_table);
        let right_name = rhs.display_with_table(string_table);
        return_type_error!(
            format!("Cannot compare choices of different types: '{left_name}' and '{right_name}'."),
            location.clone(),
            {
                CompilationStage => "Expression Evaluation",
                PrimarySuggestion => "Compare values of the same choice type, or use pattern matching to compare variants",
            }
        );
    }

    // A choice value can only be compared with another value of the same choice type.
    let exactly_one_is_choice =
        matches!(lhs, DataType::Choices { .. }) != matches!(rhs, DataType::Choices { .. });
    if exactly_one_is_choice {
        let (choice, other) = if matches!(lhs, DataType::Choices { .. }) {
            (lhs, rhs)
        } else {
            (rhs, lhs)
        };
        let choice_name = choice.display_with_table(string_table);
        let other_name = other.display_with_table(string_table);
        return_type_error!(
            format!(
                "Cannot compare choice '{choice_name}' with '{other_name}'. Choices can only be compared with values of the same choice type."
            ),
            location.clone(),
            {
                CompilationStage => "Expression Evaluation",
                PrimarySuggestion => "Compare choice values with the same choice type, or use pattern matching to inspect the variant",
            }
        );
    }

    invalid_comparison_types(lhs, rhs, op, location, string_table)
}
