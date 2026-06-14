//! Syntax and statement diagnostic render helpers.
//!
//! WHAT: owns prose for syntax-shaped payloads and statement-position diagnostics.
//! WHY: these helpers are shared by terminal, terse, and dev-server renderers, but they do not
//! need to live in the root render module map.

use super::{DiagnosticRenderContext, diagnostic_type_name, token_kind_name};
use crate::compiler_frontend::compiler_messages::{
    CommonSyntaxMistakeReason, InvalidLoopHeaderReason, InvalidMatchArmReason,
    InvalidStandaloneStatementReason, InvalidStatementPositionReason, InvalidThisUsageReason,
    InvalidTypeAnnotationReason, NumberLiteralErrorReason, OperatorOperandPosition,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};

pub(crate) fn invalid_number_literal_message(
    literal_text: StringId,
    reason: NumberLiteralErrorReason,
    string_table: &StringTable,
) -> String {
    let literal = string_table.resolve(literal_text);

    match reason {
        NumberLiteralErrorReason::SeparatorNotBetweenDigits => {
            format!("Numeric literal '{literal}' has a separator not between digits.")
        }
        NumberLiteralErrorReason::MultipleDecimalPoints => {
            format!("Can't have more than one decimal point in numeric literal '{literal}'.")
        }
        NumberLiteralErrorReason::DecimalPointNotAfterDigit => {
            format!("Numeric literal '{literal}' has a decimal point not after a digit.")
        }
        NumberLiteralErrorReason::EndsWithSeparator => {
            format!("Numeric literal '{literal}' ends with a separator.")
        }
        NumberLiteralErrorReason::MissingFractionalDigits => {
            format!("Numeric literal '{literal}' is missing fractional digits.")
        }
        NumberLiteralErrorReason::ParseOverflow => {
            format!(
                "Invalid integer literal / Float literal '{literal}': value is too large to represent."
            )
        }
    }
}

pub(crate) fn invalid_style_directive_message(
    directive_name: StringId,
    supported_directives: StringId,
    string_table: &StringTable,
) -> String {
    let name = string_table.resolve(directive_name);
    let supported = string_table.resolve(supported_directives);

    format!("Style directive '${name}' is unsupported here. Registered directives are {supported}.")
}

pub(crate) fn invalid_type_annotation_message(
    reason: &InvalidTypeAnnotationReason,
    string_table: &StringTable,
) -> String {
    match reason {
        InvalidTypeAnnotationReason::NoneNotAllowed => {
            "`none` is not a valid type annotation. Use an explicit type such as `String` or `Int`."
                .to_string()
        }
        InvalidTypeAnnotationReason::ReservedTraitKeyword => {
            "Reserved trait keywords are not valid in type annotations.".to_string()
        }
        InvalidTypeAnnotationReason::TraitThisMustBeDirect => {
            "`This` must be used directly in trait requirements. Composed forms such as `This?`, `{This}`, and `This of T` are deferred.".to_string()
        }
        InvalidTypeAnnotationReason::AsNotValidHere => {
            "`as` is not valid here. It is only supported in type aliases, import clauses, and choice payload patterns.".to_string()
        }
        InvalidTypeAnnotationReason::UnexpectedColon => {
            "Unexpected ':' after declaration name. Beanstalk does not support bare labeled blocks or `name: Type` declarations. Use `block:` for a scoped block, or write declarations as `name Type = value`.".to_string()
        }
        InvalidTypeAnnotationReason::ReactiveAccessNotAllowed => {
            "`$Type` is reactive access syntax, not a standalone type annotation. Use it only on reactive declarations such as `name $Type = value` or function parameters such as `param $Type`."
                .to_string()
        }
        InvalidTypeAnnotationReason::InvalidTokenAfterName { token } => {
            format!(
                "Invalid token {} after declaration name. Expected a type or assignment operator.",
                token_kind_name(token, string_table)
            )
        }
        InvalidTypeAnnotationReason::ExpectedTypeAnnotation { found } => {
            format!(
                "Expected a type annotation but found {}.",
                token_kind_name(found, string_table)
            )
        }
        InvalidTypeAnnotationReason::DuplicateOptional => {
            "Duplicate optional marker '?'. Only one '?' suffix is allowed per type annotation."
                .to_string()
        }
        InvalidTypeAnnotationReason::NestedOptional => {
            "Nested optional types are not supported. Aliases that already expand to an optional type cannot be marked with '?' again."
                .to_string()
        }
    }
}

pub(crate) fn common_syntax_mistake_message(
    reason: &CommonSyntaxMistakeReason,
    string_table: &StringTable,
) -> String {
    match reason {
        CommonSyntaxMistakeReason::EqualityOperator => {
            "Beanstalk uses the word `is` for equality, not `==`.".to_string()
        }
        CommonSyntaxMistakeReason::InequalityOperator => {
            "Beanstalk uses `is not` for inequality, not `!=`.".to_string()
        }
        CommonSyntaxMistakeReason::LogicalAndOperator => {
            "Beanstalk uses the word `and` for logical conjunction, not `&&`.".to_string()
        }
        CommonSyntaxMistakeReason::LogicalOrOperator => {
            "Beanstalk uses the word `or` for logical disjunction, not `||`.".to_string()
        }
        CommonSyntaxMistakeReason::BooleanBangNegation => {
            "Beanstalk uses the word `not` for boolean negation, not `!`.".to_string()
        }
        CommonSyntaxMistakeReason::ExpressionAssignment => {
            "Use `is` for comparison. `=` is for declarations and assignments.".to_string()
        }
        CommonSyntaxMistakeReason::RustBorrowPrefix => {
            "`&` marks inclusive ranges in Beanstalk. Borrowing is implicit; use `~` at call sites for mutation.".to_string()
        }
        CommonSyntaxMistakeReason::InvalidAsOperator => {
            "`as` is not a cast operator. It is only valid in type aliases, import clauses, and choice payload patterns.".to_string()
        }
        CommonSyntaxMistakeReason::StatementLineComment => {
            "`//` is integer division. Comments use `--`.".to_string()
        }
        CommonSyntaxMistakeReason::FunctionKeyword { keyword } => {
            let keyword = string_table.resolve(*keyword);
            format!("Functions don't use a keyword prefix like '{keyword}' in Beanstalk.")
        }
        CommonSyntaxMistakeReason::LetOrVarKeyword => {
            "Declarations don't use `let` or `var` in Beanstalk.".to_string()
        }
        CommonSyntaxMistakeReason::ConstKeyword => {
            "Constants don't use `const` in Beanstalk.".to_string()
        }
        CommonSyntaxMistakeReason::MatchKeyword => {
            "Use `if value is:` for pattern matching, not `match`.".to_string()
        }
        CommonSyntaxMistakeReason::StructKeyword { keyword } => {
            let keyword = string_table.resolve(*keyword);
            format!(
                "Structs are declared with `Name = | fields |` in Beanstalk, not with `{keyword}`."
            )
        }
        CommonSyntaxMistakeReason::SignatureParenthesisDelimiter => {
            "Parameters and struct fields are delimited with `|`, not `()`.".to_string()
        }
        CommonSyntaxMistakeReason::SignatureAsKeyword => {
            "`as` is not valid here. It is only supported in type aliases, import clauses, and choice payload patterns.".to_string()
        }
        CommonSyntaxMistakeReason::InvalidCompileTimeBindingSpacing => {
            "Invalid compile-time binding syntax. Use `name #= value` for inferred constants or `name #Type = value` for explicit constant types. For collection and option types, attach `#` to the first token of the type: `names #{String} = ...` or `value #String? = ...`.".to_string()
        }
        CommonSyntaxMistakeReason::InvalidMutableBindingSpacing => {
            "Invalid mutable binding syntax. Use `name ~= value` for inferred mutable bindings or `name ~Type = value` for explicit mutable types. For collection types, attach `~` to the first token of the type: `values ~{String} = ...`.".to_string()
        }
        CommonSyntaxMistakeReason::InvalidReactiveBindingSpacing => {
            "Invalid reactive binding syntax. Use `name $= value` for inferred reactive bindings or `name $Type = value` for explicit reactive types. For collection and option types, attach `$` to the first token of the type: `names ${String} = ...` or `value $String? = ...`.".to_string()
        }
    }
}

pub(crate) fn common_syntax_mistake_suggestion(reason: &CommonSyntaxMistakeReason) -> &'static str {
    match reason {
        CommonSyntaxMistakeReason::EqualityOperator => "Replace `==` with `is`",
        CommonSyntaxMistakeReason::InequalityOperator => "Replace `!=` with `is not`",
        CommonSyntaxMistakeReason::LogicalAndOperator => "Replace `&&` with `and`",
        CommonSyntaxMistakeReason::LogicalOrOperator => "Replace `||` with `or`",
        CommonSyntaxMistakeReason::BooleanBangNegation => "Replace `!` with `not`",
        CommonSyntaxMistakeReason::ExpressionAssignment => {
            "Replace `=` with `is` for equality, or move the assignment to statement position"
        }
        CommonSyntaxMistakeReason::RustBorrowPrefix => {
            "Remove `&`; shared borrows are automatic. For mutation, prefix the place with `~` at the call site."
        }
        CommonSyntaxMistakeReason::InvalidAsOperator => {
            "Use `cast` at an explicitly typed boundary for supported conversions, or use `as` only in a supported renaming context"
        }
        CommonSyntaxMistakeReason::SignatureAsKeyword => {
            "Remove `as` or use it only in a supported renaming context"
        }
        CommonSyntaxMistakeReason::StatementLineComment => "Replace `//` with `--` for a comment",
        CommonSyntaxMistakeReason::FunctionKeyword { .. } => "Write `name |args| -> Type:` instead",
        CommonSyntaxMistakeReason::LetOrVarKeyword => {
            "Write `name Type = value` for an immutable binding, or `name ~Type = value` for a mutable one"
        }
        CommonSyntaxMistakeReason::ConstKeyword => {
            "Write `name #= value` for an inferred compile-time constant or `name #Type = value` for an explicit constant type"
        }
        CommonSyntaxMistakeReason::MatchKeyword => {
            "Replace `match value {` with `if value is:` and use `<pattern> =>` arms"
        }
        CommonSyntaxMistakeReason::StructKeyword { .. } => {
            "Write `Name = | field Type, |` instead of `struct Name { ... }`"
        }
        CommonSyntaxMistakeReason::SignatureParenthesisDelimiter => {
            "Replace `(` with `|` and `)` with `|`"
        }
        CommonSyntaxMistakeReason::InvalidCompileTimeBindingSpacing => {
            "Remove the space after `#` so it is immediately followed by `=` or the type annotation"
        }
        CommonSyntaxMistakeReason::InvalidMutableBindingSpacing => {
            "Remove the space after `~` so it is immediately followed by `=` or the type annotation"
        }
        CommonSyntaxMistakeReason::InvalidReactiveBindingSpacing => {
            "Remove the space after `$` so it is immediately followed by `=` or the type annotation"
        }
    }
}

pub(crate) fn missing_operator_operand_message(
    operator: StringId,
    position: OperatorOperandPosition,
    string_table: &StringTable,
) -> String {
    let op_str = string_table.resolve(operator);
    match position {
        OperatorOperandPosition::Unary => {
            format!("Missing operand for unary operator '{op_str}'.")
        }
        OperatorOperandPosition::BinaryLeft => {
            format!("Missing left-hand operand for operator '{op_str}'.")
        }
        OperatorOperandPosition::BinaryRight => {
            format!("Missing right-hand operand for operator '{op_str}'.")
        }
    }
}

pub(crate) fn invalid_standalone_statement_message(
    reason: InvalidStandaloneStatementReason,
) -> String {
    match reason {
        InvalidStandaloneStatementReason::FieldRead => {
            "Field reads are not valid standalone statements.".to_string()
        }
        InvalidStandaloneStatementReason::Expression => {
            "Standalone expression is not a valid statement in this position.".to_string()
        }
    }
}

pub(crate) fn expected_symbol_statement_message() -> String {
    "Expected a symbol-led statement.".to_string()
}

pub(crate) fn missing_collection_item_message() -> String {
    "Expected a collection item after the comma".to_string()
}

pub(crate) fn invalid_match_arm_message(reason: InvalidMatchArmReason) -> String {
    match reason {
        InvalidMatchArmReason::SemicolonDelimiter => {
            "Match arms are not closed with semicolons. Use the next line-initial arm, 'else', or the final match ';' to delimit arms.".to_string()
        }
        InvalidMatchArmReason::LegacyColonSyntax => {
            "Legacy match arm syntax is no longer supported. Use `<pattern> => <body>`.".to_string()
        }
        InvalidMatchArmReason::LegacyElseSyntax => {
            "Legacy default-arm syntax 'else:' is no longer supported. Use 'else =>'.".to_string()
        }
        InvalidMatchArmReason::InvalidArrow => {
            "Unexpected '->' in match arm. Match arms use '=>'.".to_string()
        }
        InvalidMatchArmReason::ArmMustStartNewLine => {
            "Match arms must start at the beginning of a logical line.".to_string()
        }
        InvalidMatchArmReason::ExpectedArmHeader => {
            "Expected a match arm like `<pattern> => <body>` or `else => <body>`.".to_string()
        }
    }
}

pub(crate) fn invalid_loop_header_message(
    reason: InvalidLoopHeaderReason,
    context: DiagnosticRenderContext<'_>,
) -> String {
    match reason {
        InvalidLoopHeaderReason::EmptyHeader => {
            "Loop header is empty. Expected a condition or iteration source after 'loop'.".to_string()
        }
        InvalidLoopHeaderReason::MissingColon => {
            "A loop must have ':' after the loop header.".to_string()
        }
        InvalidLoopHeaderReason::RemovedInSyntax => {
            "Old loop syntax 'loop <binder> in ...' was removed. Use 'loop 0 to 10 |i|:' or 'loop items |item|:'.".to_string()
        }
        InvalidLoopHeaderReason::MissingClosingPipe => {
            "Missing closing pipe in loop bindings.".to_string()
        }
        InvalidLoopHeaderReason::MalformedBindingPipes => {
            "Malformed loop binding pipes.".to_string()
        }
        InvalidLoopHeaderReason::MissingSourceBeforeBindings => {
            "Loop header is missing a condition or iteration source before bindings.".to_string()
        }
        InvalidLoopHeaderReason::EmptyBindingList => {
            "Loop binding list cannot be empty.".to_string()
        }
        InvalidLoopHeaderReason::ThisBinding => {
            "'this' is reserved for method receiver parameters and cannot be used as a loop binding.".to_string()
        }
        InvalidLoopHeaderReason::BindingMustBeSymbol => {
            "Loop bindings must be symbol names.".to_string()
        }
        InvalidLoopHeaderReason::MissingBindingComma => {
            "Missing comma between loop bindings.".to_string()
        }
        InvalidLoopHeaderReason::TrailingBindingComma => {
            "Loop binding list cannot end with a comma.".to_string()
        }
        InvalidLoopHeaderReason::BareSingleBinding => {
            "Loop bindings must use `|...|` after the loop source or range.".to_string()
        }
        InvalidLoopHeaderReason::BareDualBinding => {
            "Loop bindings must use `|item, index|` form.".to_string()
        }
        InvalidLoopHeaderReason::TooManyBindings => {
            "Loop bindings support at most two names.".to_string()
        }
        InvalidLoopHeaderReason::DuplicateBindingName => {
            "Duplicate loop binding name in the same loop header.".to_string()
        }
        InvalidLoopHeaderReason::BindingAlreadyDeclared => {
            "Loop binding is already declared in this scope.".to_string()
        }
        InvalidLoopHeaderReason::CollectionSourceNotCollection { found_type } => {
            let found_type = diagnostic_type_name(found_type, context);
            format!("Collection loop source must be a collection. Found '{found_type}'.")
        }
        InvalidLoopHeaderReason::MissingRangeSeparator => {
            "Range loops must include 'to' between bounds.".to_string()
        }
        InvalidLoopHeaderReason::MissingRangeEndBound => {
            "Range loop is missing an end bound.".to_string()
        }
        InvalidLoopHeaderReason::MissingRangeStep => {
            "Range loop uses 'by' without a step value.".to_string()
        }
        InvalidLoopHeaderReason::FloatRangeMissingStep => {
            "Float ranges require an explicit 'by' step.".to_string()
        }
        InvalidLoopHeaderReason::ZeroRangeStep => {
            "Range step cannot be zero.".to_string()
        }
        InvalidLoopHeaderReason::ExpectedHeaderExpression => {
            "Expected an expression in loop header.".to_string()
        }
    }
}

pub(crate) fn invalid_statement_position_message(reason: InvalidStatementPositionReason) -> String {
    match reason {
        InvalidStatementPositionReason::UnexpectedComma => {
            "Unexpected ',' in function body. Commas only separate items in lists, arguments, or return declarations.".to_string()
        }
        InvalidStatementPositionReason::UnexpectedCloseParenthesis => {
            "Unexpected ')' in function body. This usually means an earlier '(' was not parsed in a valid expression or call.".to_string()
        }
        InvalidStatementPositionReason::UnexpectedCloseCurly => {
            "Unexpected '}' in function body. Curly braces are only valid for collection syntax.".to_string()
        }
        InvalidStatementPositionReason::UnexpectedPipe => {
            "Unexpected '|' in function body. '|' is valid in function signatures, struct field/type declarations, and loop binding headers.".to_string()
        }
        InvalidStatementPositionReason::UnexpectedArrow => {
            "Unexpected '->' in function body. Arrow syntax is only valid in function signatures.".to_string()
        }
        InvalidStatementPositionReason::UnexpectedWildcard => {
            "Unexpected wildcard '_' in function body. Wildcards are not standalone statements.".to_string()
        }
        InvalidStatementPositionReason::ReservedGenericDeclaration => {
            "Generic declarations using `type` are reserved but not implemented yet.".to_string()
        }
        InvalidStatementPositionReason::UnexpectedOf => {
            "Unexpected `of` in statement position. `of` is reserved for future generic type application syntax.".to_string()
        }
        InvalidStatementPositionReason::UnexpectedScopeCloseInExpression => {
            "Unexpected scope close. Expressions are not terminated like this.".to_string()
        }
        InvalidStatementPositionReason::UnexpectedScopeCloseInTemplate => {
            "Unexpected use of ';' inside a template. Templates are not closed with ';'.".to_string()
        }
    }
}

pub(crate) fn invalid_this_usage_message(
    reason: InvalidThisUsageReason,
    string_table: &StringTable,
) -> String {
    match reason {
        InvalidThisUsageReason::NotInReceiverMethod => {
            "'this' can only be used inside the body of a receiver method.".to_string()
        }
        InvalidThisUsageReason::Reassignment => {
            "'this' is a reserved receiver parameter and cannot be reassigned.".to_string()
        }
        InvalidThisUsageReason::LoopBinding => {
            "'this' cannot be used as a loop variable name.".to_string()
        }
        InvalidThisUsageReason::DeclarationBinding => {
            "'this' cannot be used as a declaration name.".to_string()
        }
        InvalidThisUsageReason::DuplicateThis { function_name } => {
            format!(
                "Function '{}' declares 'this' more than once. Receiver parameters can only appear once.",
                string_table.resolve(function_name)
            )
        }
        InvalidThisUsageReason::NotFirstParameter { function_name } => {
            format!(
                "Function '{}' uses 'this' as a receiver parameter, but it is not the first parameter.",
                string_table.resolve(function_name)
            )
        }
        InvalidThisUsageReason::OutsideTraitDeclaration => {
            "'This' is only valid inside trait declarations. Use a concrete type name here."
                .to_string()
        }
    }
}
