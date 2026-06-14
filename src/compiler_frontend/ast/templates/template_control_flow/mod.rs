//! Structured template control-flow metadata and AST-stage helpers.
//!
//! Template control flow is parsed in the template head/body pipeline, then
//! kept structured so const folding and runtime HIR lowering can preserve lazy
//! branch and loop semantics. The sibling modules keep the model, validation,
//! const-evaluability checks, const-loop folding mechanics, and string-id
//! remapping separate because later roadmap slices will extend those concerns
//! independently.

mod const_eval;
mod const_folding;
mod remap;
mod types;
mod validation;

pub(crate) use const_eval::{
    inline_source_consts_for_const_required_expression,
    inline_source_consts_for_const_required_if_condition,
};
pub(crate) use const_folding::{
    ConstRangeCursor, TemplateFoldBinding, build_collection_iteration_bindings,
    build_range_iteration_bindings, const_collection_items,
};
pub(crate) use types::{
    TemplateAggregatePiece, TemplateAggregateRenderPlan, TemplateBodyEmission,
    TemplateBodyParseMode, TemplateBranchChain, TemplateBranchSelector, TemplateConditionalBranch,
    TemplateControlFlow, TemplateControlFlowValidationMode, TemplateFallbackBranch,
    TemplateIfBodyParseInput, TemplateLoopBodyParseInput, TemplateLoopControlFlow,
    TemplateLoopControlKind, TemplateLoopControlSignal, TemplateLoopHeader,
};
pub(crate) use validation::{
    validate_const_required_template_control_flow,
    validate_runtime_template_control_flow_slot_artifacts,
};
