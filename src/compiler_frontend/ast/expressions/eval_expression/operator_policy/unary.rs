//! Unary operator typing policy.

use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::string_interning::StringTable;
use crate::return_type_error;

pub(super) fn resolve_unary_operator_type(
    op: &Operator,
    operand: &DataType,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    match op {
        Operator::Not => {
            if operand == &DataType::Bool {
                Ok(DataType::Bool)
            } else {
                let found_type = operand.display_with_table(string_table);
                return_type_error!(
                    format!(
                        "Unary operator '{}' requires Bool, found '{}'.",
                        op.to_str(),
                        found_type
                    ),
                    location.clone(),
                    {
                        CompilationStage => "Expression Evaluation",
                        PrimarySuggestion => "Use 'not' only with Bool values",
                        ExpectedType => "Bool",
                        FoundType => found_type,
                    }
                )
            }
        }
        // Unary minus preserves the numeric payload type. The tokenizer/parser already own the
        // distinction between negative literals and a runtime unary subtraction operator.
        Operator::Subtract => Ok(operand.to_owned()),
        _ => Ok(operand.to_owned()),
    }
}
