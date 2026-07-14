//! Parser-local mutable build state for template construction.
//!
//! WHAT: `TemplateBuildState` is the mutable parser accumulator for template
//! head/body metadata — `kind`, `style`, direct-child wrapper refs, and
//! parser-local `id` — while a template is being parsed.
//!
//! WHY: `Template` is the durable AST value. The mutable parser accumulator is
//! shorter-lived: it exists only while syntax is being parsed, render units are
//! shaped, and parser-emitted TIR is finalized. Keeping mutable parse-time
//! fields on a dedicated build state means the durable `Template` is constructed
//! once, after authoritative TIR identity exists, instead of being mutated
//! throughout parsing.

use crate::compiler_frontend::ast::templates::template::{Style, TemplateType};
use crate::compiler_frontend::ast::templates::tir::{
    MaterializedTirTemplateClassification, TemplateWrapperReference,
};

/// Parser-local mutable state accumulated during template head and body parsing.
///
/// WHAT: carries `kind`, mutable `style`, direct-child wrapper refs and the
///       parser-local `id` that head and body parsing need to share without
///       threading `&mut Template`.
/// WHY: the durable `Template` is constructed once after authoritative TIR
///      identity exists; this build state is the single mutable owner during
///      parsing and render-unit preparation.
pub(crate) struct TemplateBuildState {
    pub(crate) kind: TemplateType,
    pub(crate) style: Style,
    pub(crate) child_wrappers: Vec<TemplateWrapperReference>,
    pub(crate) id: String,
}

impl TemplateBuildState {
    /// Creates a fresh build state with default kind, style, and no wrappers.
    pub(crate) fn new() -> Self {
        Self {
            kind: TemplateType::StringFunction,
            style: Style::default(),
            child_wrappers: vec![],
            id: String::new(),
        }
    }

    /// Applies generic String/StringFunction classification from an already
    /// materialized current-state TIR classification.
    pub(crate) fn refresh_kind_from_tir_classification(
        &mut self,
        classification: &MaterializedTirTemplateClassification,
    ) {
        if matches!(
            self.kind,
            TemplateType::SlotInsert(_)
                | TemplateType::SlotDefinition(_)
                | TemplateType::Comment(_)
        ) {
            return;
        }

        self.kind = if classification.shape_const_evaluable && !classification.has_slot_insertions {
            TemplateType::String
        } else {
            TemplateType::StringFunction
        };
    }
}
