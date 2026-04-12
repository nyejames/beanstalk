//! Template struct and core type definitions.
//!
//! WHAT: Houses the central `Template` struct and its associated methods —
//! queries (constness, slots), construction helpers, and style application.
//!
//! WHY: Separates the `Template` data type from the parsing/composition logic
//! so other modules can depend on the struct without pulling in the full parser.

use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateConstValueKind, TemplateContent, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_render_plan::TemplateRenderPlan;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// The central template representation in the AST.
///
/// A `Template` carries its parsed content (body atoms + head atoms), style
/// configuration, constness classification, and source location. It is the
/// primary data structure passed between parsing, composition, formatting,
/// folding, and HIR lowering.
#[derive(Clone, Debug)]
pub struct Template {
    pub content: TemplateContent,
    pub unformatted_content: TemplateContent,
    pub content_needs_formatting: bool,
    pub render_plan:
        Option<crate::compiler_frontend::ast::templates::template_render_plan::TemplateRenderPlan>,
    pub kind: TemplateType,
    pub doc_children: Vec<Template>,
    pub style: Style,
    pub explicit_style: Style,
    pub id: String,
    pub location: SourceLocation,
}

/// Inheritance state passed from parent to nested child templates.
/// Carries only direct child wrappers from `$children(..)` directives.
#[derive(Clone, Debug, Default)]
pub(crate) struct TemplateInheritance {
    pub(crate) direct_child_wrappers: Vec<Template>,
}

impl TemplateInheritance {
    /// Builds inheritance state from wrapper templates passed down by a parent.
    /// Nested templates do not inherit formatter/style state automatically.
    pub(crate) fn from_parent_wrappers(templates: Vec<Template>) -> Self {
        Self {
            direct_child_wrappers: templates,
        }
    }
}

impl Template {
    /// Creates a default template with no inherited formatter/style state.
    pub fn create_default(templates: Vec<Template>) -> Template {
        let _inheritance = TemplateInheritance::from_parent_wrappers(templates);

        Template {
            content: TemplateContent::default(),
            unformatted_content: TemplateContent::default(),
            content_needs_formatting: false,
            render_plan: None,
            kind: TemplateType::StringFunction,
            doc_children: vec![],
            style: Style::default(),
            explicit_style: Style::default(),
            id: String::new(),
            location: SourceLocation::default(),
        }
    }

    /// Replaces both the effective and explicit style with the given style.
    pub(crate) fn apply_style(&mut self, style: Style) {
        self.style = style.to_owned();
        self.explicit_style = style;
    }

    /// Applies an update function to both the effective and explicit style.
    pub(crate) fn apply_style_updates(&mut self, mut update: impl FnMut(&mut Style)) {
        update(&mut self.style);
        update(&mut self.explicit_style);
    }

    /// Returns true if this template's content contains unresolved slot placeholders.
    pub fn has_unresolved_slots(&self) -> bool {
        self.content.has_unresolved_slots()
    }

    /// Returns true if this template can be fully evaluated at compile time.
    pub fn is_const_evaluable_value(&self) -> bool {
        self.const_value_kind().is_compile_time_value()
    }

    /// Returns true if this template is a compile-time string (not a wrapper).
    pub fn is_const_renderable_string(&self) -> bool {
        self.const_value_kind().is_renderable_string()
    }

    /// Rebuilds the derived metadata that must stay aligned with `content`.
    ///
    /// WHAT:
    /// - refreshes the pre-format snapshot
    /// - clears deferred-formatting state
    /// - reclassifies non-special template kinds
    /// - rebuilds the final render plan from the current content stream
    ///
    /// WHY:
    /// - wrapper/slot composition mutates template content after parsing, and HIR must only
    ///   receive templates whose runtime metadata is already authoritative.
    pub(crate) fn resync_runtime_metadata(&mut self) {
        self.unformatted_content = self.content.to_owned();
        self.content_needs_formatting = false;
        self.refresh_kind_from_content();
        self.render_plan = Some(TemplateRenderPlan::from_content(&self.content));
    }

    /// Refreshes the non-special string/string-function classification from current content.
    ///
    /// WHY:
    /// - slot/comment helper kinds are semantic markers and must not be rewritten by generic
    ///   post-composition cleanup.
    pub(crate) fn refresh_kind_from_content(&mut self) {
        if matches!(
            self.kind,
            TemplateType::SlotInsert(_)
                | TemplateType::SlotDefinition(_)
                | TemplateType::Comment(_)
        ) {
            return;
        }

        self.kind = if self.content.is_const_evaluable_value()
            && !self.content.contains_slot_insertions()
        {
            TemplateType::String
        } else {
            TemplateType::StringFunction
        };
    }

    /// Classifies template const-ness in one place.
    /// AST constant checks and render-required paths need consistent rules.
    ///
    /// IMPORTANT:
    /// - `WrapperTemplate` means "compile-time wrapper value" (unresolved slots).
    /// - It does NOT mean "always fold to backend-facing const string" in runtime paths.
    /// - Runtime-vs-const lowering decisions must use the final template value shape.
    pub fn const_value_kind(&self) -> TemplateConstValueKind {
        if !self.content.is_const_evaluable_value() {
            return TemplateConstValueKind::NonConst;
        }

        if matches!(self.kind, TemplateType::SlotInsert(_)) {
            // Slot insertion templates are compile-time helper values and are only
            // valid when consumed by an active wrapper fill site.
            if self.content.contains_slot_insertions() {
                return TemplateConstValueKind::NonConst;
            }
            return TemplateConstValueKind::SlotInsertHelper;
        }

        if matches!(self.kind, TemplateType::SlotDefinition(_)) {
            return TemplateConstValueKind::NonConst;
        }

        if !matches!(self.kind, TemplateType::String) {
            return TemplateConstValueKind::NonConst;
        }

        if self.has_unresolved_slots() {
            return TemplateConstValueKind::WrapperTemplate;
        }

        if self.content.contains_slot_insertions() {
            return TemplateConstValueKind::NonConst;
        }

        TemplateConstValueKind::RenderableString
    }
}
