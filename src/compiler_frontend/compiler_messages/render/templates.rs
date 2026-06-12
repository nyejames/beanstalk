//! Template and compile-time diagnostic text renderers.
//!
//! WHAT: renders diagnostics for template slot structure and compile-time evaluation failures.
//! WHY: template diagnostics are a broad payload family and keeping them out of `render::mod`
//! makes the render boundary easier to scan.

use super::context::DiagnosticRenderContext;
use super::named_value_or_default;
use crate::compiler_frontend::compiler_messages::{
    CompileTimeEvaluationErrorReason, InvalidTemplateSlotReason,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};

pub(crate) fn invalid_template_slot_message(
    reason: InvalidTemplateSlotReason,
    slot_name: Option<StringId>,
    string_table: &StringTable,
) -> String {
    let slot_text = named_value_or_default(slot_name, string_table, "this slot");

    match reason {
        InvalidTemplateSlotReason::InsertOutsideParentSlot => {
            "$insert(...) can only be used while filling an immediate parent template that defines matching $slot targets.".to_string()
        }
        InvalidTemplateSlotReason::ExtraLooseContentWithoutDefaultSlot => {
            "This template defines positional $slot(n) targets but no default $slot. There is more loose content than positional slots available.".to_string()
        }
        InvalidTemplateSlotReason::LooseContentWithoutDefaultSlot => {
            "This template defines named $slot(...) targets without a default $slot. Loose content is not allowed here; use $insert(\"name\").".to_string()
        }
        InvalidTemplateSlotReason::InsertCannotTargetDefaultSlot => {
            "$insert cannot target the default slot because the parent template does not define $slot.".to_string()
        }
        InvalidTemplateSlotReason::InsertTargetsUnknownNamedSlot => {
            format!("$insert({slot_text}) targets a named slot that does not exist on the immediate parent template.")
        }
        InvalidTemplateSlotReason::InsertTargetsUnknownPositionalSlot => {
            "$insert targets a positional slot that does not exist on the immediate parent template.".to_string()
        }
        InvalidTemplateSlotReason::MultipleDefaultSlots => {
            "Templates can only define one default $slot.".to_string()
        }
        InvalidTemplateSlotReason::SlotDefinitionOutsideTemplateBody => {
            "$slot markers are only valid as direct nested templates inside template bodies.".to_string()
        }
    }
}

pub(crate) fn compile_time_evaluation_error_message(
    reason: CompileTimeEvaluationErrorReason,
    operation: Option<StringId>,
    string_table: &StringTable,
) -> String {
    let operation_text = named_value_or_default(operation, string_table, "this expression");

    match reason {
        CompileTimeEvaluationErrorReason::IntegerOverflow => {
            format!("Compile-time integer overflow while evaluating {operation_text}.")
        }
        CompileTimeEvaluationErrorReason::FloatOverflow => {
            format!(
                "Compile-time float overflow or non-finite result while evaluating {operation_text}."
            )
        }
        CompileTimeEvaluationErrorReason::DivideByZero => "Cannot divide by zero.".to_string(),
        CompileTimeEvaluationErrorReason::InvalidOperatorForType => {
            format!("Cannot perform operation {operation_text} on this type.")
        }
        CompileTimeEvaluationErrorReason::IntegerDivisionOnlyIntInt => {
            "Integer division operator '//' only supports Int and Int operands.".to_string()
        }
        CompileTimeEvaluationErrorReason::ConstantSelfReference => {
            format!("Constant {operation_text} cannot reference itself in its initializer.")
        }
        CompileTimeEvaluationErrorReason::ConstantNotVisible => {
            format!("Constant {operation_text} is not visible in this file.")
        }
        CompileTimeEvaluationErrorReason::NonConstantReferenceInConstant => {
            format!(
                "Constants can only reference other constants. {operation_text} resolves to a non-constant value."
            )
        }
        CompileTimeEvaluationErrorReason::SameFileForwardConstantReference => {
            format!(
                "Constant initializer references same-file constant {operation_text} before it is declared."
            )
        }
        CompileTimeEvaluationErrorReason::ConstantInitializerNotFoldable => {
            format!("Constant {operation_text} is not compile-time resolvable.")
        }
        CompileTimeEvaluationErrorReason::ExternalNonScalarConstantInConstantContext => {
            format!(
                "External constant {operation_text} is not a scalar value and cannot be used in a constant context."
            )
        }
        CompileTimeEvaluationErrorReason::ExternalFunctionCallInConstantContext => {
            format!(
                "Constants cannot call external functions. {operation_text} is a runtime external call."
            )
        }
        CompileTimeEvaluationErrorReason::NonCompileTimeFieldInConstantContext => {
            format!(
                "Const coercion requires compile-time field values. {operation_text} is not compile-time constant."
            )
        }
        CompileTimeEvaluationErrorReason::NoneLiteralRequiresOptionalTypeContext => {
            "The 'none' literal requires an explicit optional type context.".to_string()
        }
        CompileTimeEvaluationErrorReason::ExternalTypeConstructionNotSupported => {
            format!(
                "Cannot construct external type {operation_text} with a struct literal. External types are opaque and can only be obtained from external function calls."
            )
        }
        CompileTimeEvaluationErrorReason::StructFieldDefaultNotFoldable => {
            format!("Struct field default value {operation_text} is not compile-time resolvable.")
        }
    }
}

pub(crate) fn compile_time_evaluation_error_suggestion(
    reason: CompileTimeEvaluationErrorReason,
) -> &'static str {
    match reason {
        CompileTimeEvaluationErrorReason::IntegerOverflow
        | CompileTimeEvaluationErrorReason::FloatOverflow => {
            "Use smaller values or avoid compile-time evaluation of large expressions"
        }
        CompileTimeEvaluationErrorReason::DivideByZero => {
            "Avoid division by zero in compile-time expressions"
        }
        CompileTimeEvaluationErrorReason::InvalidOperatorForType => {
            "Use an operator that is valid for the operand types"
        }
        CompileTimeEvaluationErrorReason::IntegerDivisionOnlyIntInt => {
            "Use '//' only with two Int operands"
        }
        CompileTimeEvaluationErrorReason::ConstantSelfReference => {
            "A constant cannot depend on itself. Use a different value or compute it differently."
        }
        CompileTimeEvaluationErrorReason::ConstantNotVisible => {
            "Import the compile-time constant before using it in this constant initializer."
        }
        CompileTimeEvaluationErrorReason::NonConstantReferenceInConstant => {
            "Only reference constants in constant declarations and const templates."
        }
        CompileTimeEvaluationErrorReason::SameFileForwardConstantReference => {
            "Move the referenced constant above this declaration, or import it from another file."
        }
        CompileTimeEvaluationErrorReason::ConstantInitializerNotFoldable => {
            "Constants may only contain compile-time values and constant references."
        }
        CompileTimeEvaluationErrorReason::ExternalNonScalarConstantInConstantContext => {
            "Only scalar external constants (Int, Float, Bool) are supported in constant declarations and const templates"
        }
        CompileTimeEvaluationErrorReason::ExternalFunctionCallInConstantContext => {
            "Use only compile-time constant values inside constants and const templates"
        }
        CompileTimeEvaluationErrorReason::NonCompileTimeFieldInConstantContext => {
            "Use only compile-time values when constructing records or choices for top-level '#' constants"
        }
        CompileTimeEvaluationErrorReason::NoneLiteralRequiresOptionalTypeContext => {
            "Add an explicit optional type annotation (e.g., 'value Option<Type> = none')"
        }
        CompileTimeEvaluationErrorReason::ExternalTypeConstructionNotSupported => {
            "Use an external function that returns this type instead"
        }
        CompileTimeEvaluationErrorReason::StructFieldDefaultNotFoldable => {
            "Struct field defaults may only contain compile-time values and constant references."
        }
    }
}

pub(crate) fn invalid_template_structure_message(
    reason: crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason,
    context: DiagnosticRenderContext<'_>,
) -> String {
    match reason {
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingClosingBracket => {
            "Template is missing a closing bracket.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::SlotInHead => {
            "Slot insertions cannot appear in template heads.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingHandlerBody => {
            "Template handler is missing a body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::InvalidChildDirective => {
            "Invalid child directive in template.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::NestedTemplateNotAllowed => {
            "Nested templates are not allowed here.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::HelperInConstTemplate => {
            "Top-level const templates cannot evaluate to '$insert(...)' helpers.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::NonFoldableConstTemplate => {
            "Top-level const templates must be fully foldable at compile time.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::NonFoldableDocComment => {
            "'$doc' comments can only contain compile-time values.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::ResultInTemplateHead => {
            "Template head expressions do not implicitly unwrap Result values.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::UnsupportedTypeInTemplateHead { type_id } => {
            let type_name = super::context::diagnostic_type_name(type_id, context);
            format!(
                "Template head expressions only accept final scalar or textual values. Found: {}",
                type_name
            )
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::RuntimeTemplateInConst => {
            "Const templates can only capture compile-time templates.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::RuntimeValueInConstTemplateHead => {
            "Const templates can only capture compile-time values in the template head.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::EmptyPathInTemplateHead => {
            "Path token in template head cannot be empty.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::PathAliasInTemplateHead => {
            "Path aliases are only valid in import clauses.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::IncompatibleHeadItem => {
            "This template head item is incompatible with other meaningful items in this template head.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::HelperOutsideWrapperSlot => {
            "Template helper reached AST finalization outside immediate wrapper-slot composition.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedSlot => {
            "Runtime template control-flow bodies cannot leave unresolved `$slot` placeholders.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedInsert => {
            "Runtime template control-flow bodies cannot leave unresolved `$insert(...)` helpers.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingCommaBeforeControlFlowSuffix => {
            "Template control-flow suffixes must be separated from earlier head items with a comma.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::ControlFlowSuffixNotFinal => {
            "Template control-flow suffixes must be the final item in the template head.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingTemplateIfCondition => {
            "Template `if` suffix is missing a condition.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingTemplateLoopHeader => {
            "Template `loop` suffix is missing a range or collection header.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::ElseInTemplateHead => {
            "`else` is only valid as a standalone template body sentinel `[else]` inside a template `if`.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::OrphanTemplateElse => {
            "Template `[else]` is only valid inside a template `if` body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::OrphanTemplateElseIf => {
            "Template `[else if ...]` is only valid inside a template `if` body before the final `[else]`.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::OrphanTemplateBreak => {
            "Template `[break]` is only valid inside a template `loop` body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::OrphanTemplateContinue => {
            "Template `[continue]` is only valid inside a template `loop` body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::DuplicateTemplateElse => {
            "Template `if` bodies can only contain one direct `[else]` sentinel.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateElseIfAfterElse => {
            "Template `[else if ...]` must appear before the final `[else]` branch.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MalformedTemplateElse => {
            "Template `else` must use the exact standalone form `[else]`.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MalformedTemplateElseIf => {
            "Template `else if` must use the standalone form `[else if condition]` without a body colon.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MalformedTemplateBreak => {
            "Template loop control must use the exact standalone form `[break]`.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MalformedTemplateContinue => {
            "Template loop control must use the exact standalone form `[continue]`.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingTemplateElseIfCondition => {
            "Template `[else if ...]` is missing a condition.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::InlineTemplateElse => {
            "Template `[else]` must be standalone, with no meaningful same-line body text beside it.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::InlineTemplateElseIf => {
            "Template `[else if ...]` must be standalone, with no meaningful same-line body text beside it.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::InlineTemplateBreak => {
            "Template `[break]` must be standalone, with no meaningful same-line body text beside it.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::InlineTemplateContinue => {
            "Template `[continue]` must be standalone, with no meaningful same-line body text beside it.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateElseInLiteralBody => {
            "Template `[else]` cannot split a template body whose directive treats bracketed content as literal text.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateElseIfInLiteralBody => {
            "Template `[else if ...]` cannot split a template body whose directive treats bracketed content as literal text.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateLoopControlInLiteralBody => {
            "Template `[break]` and `[continue]` cannot control a template body whose directive treats bracketed content as literal text.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateElseInLoopBody => {
            "Template `[else]` cannot appear directly inside a template `loop` body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateElseIfInLoopBody => {
            "Template `[else if ...]` cannot appear directly inside a template `loop` body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::UnexpectedTokenAfterControlFlowSuffix => {
            "Unexpected token after template control-flow suffix.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateMatchStyleControlFlowUnsupported => {
            "Template `if` heads support Bool conditions and option-present capture only. Use ordinary statement/value `if value is:` blocks for pattern matching outside templates.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateIfConditionNotConst => {
            "Template `if` condition in a const-required template must fold to a Bool at compile time.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateIfBranchNotConst => {
            "Both branches of a const-required template `if` must be fully foldable, even when one branch is inactive.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateOptionCaptureConstDeferred => {
            "Option-present template `if` folding in const-required templates is deferred because the current const value model cannot decide option presence here.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateLoopRangeBoundsNotConst => {
            "Template range loop bounds in a const-required template must fold to numeric values at compile time.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateLoopSourceNotConst => {
            "Template collection loop source in a const-required template must fold to a collection at compile time.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateLoopConditionNotConst => {
            "Template conditional loop condition in a const-required template must fold to a Bool at compile time.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateConditionalLoopConstTrue => {
            "Const-required template conditional loops with a true condition are rejected because they may not terminate.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateLoopBodyNotConst => {
            "Template loop body in a const-required template must be fully foldable for every iteration.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateConstLoopExpansionLimitExceeded { limit } => {
            format!(
                "Const template loop expansion is limited to {} iterations.",
                limit
            )
        }
    }
}
