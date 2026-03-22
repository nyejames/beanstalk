//! Template struct and core type definitions.
//!
//! WHAT: Houses the central `Template` struct and its associated methods —
//! queries (constness, slots), construction helpers, and style application.
//!
//! WHY: Separates the `Template` data type from the parsing/composition logic
//! so other modules can depend on the struct without pulling in the full parser.

use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateConstValueKind, TemplateContent, TemplateControlFlow, TemplateType,
};
use crate::compiler_frontend::tokenizer::tokens::TextLocation;

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
    #[allow(dead_code)] // Planned for template-level if/loop control flow
    pub control_flow: TemplateControlFlow,
    pub id: String,
    pub location: TextLocation,
}

/// Inherited style state passed from parent to nested child templates.
/// Carries the recursive style (formatter/mode) and direct child wrappers
/// from `$children(..)` directives.
#[derive(Clone, Debug, Default)]
pub(crate) struct TemplateInheritance {
    pub(crate) recursive_style: Option<Style>,
    pub(crate) direct_child_wrappers: Vec<Template>,
}

impl TemplateInheritance {
    /// Builds inheritance state from wrapper templates passed down by a parent
    /// (e.g. `$children(..)` wrappers or inherited style directives).
    pub(crate) fn from_parent_wrappers(templates: Vec<Template>) -> Self {
        let recursive_style = templates
            .last()
            .and_then(|template| recursive_inherited_style(&template.style));

        Self {
            recursive_style,
            direct_child_wrappers: templates,
        }
    }
}

impl Template {
    /// Creates a default template pre-populated with inherited style state.
    pub fn create_default(templates: Vec<Template>) -> Template {
        let inheritance = TemplateInheritance::from_parent_wrappers(templates);
        Self::create_default_with_inherited_style(inheritance.recursive_style)
    }

    /// Creates a default template with an optional pre-inherited style.
    pub(crate) fn create_default_with_inherited_style(inherited_style: Option<Style>) -> Template {
        let mut style = inherited_style.unwrap_or_else(Style::default);
        style.child_templates.clear();

        Template {
            content: TemplateContent::default(),
            unformatted_content: TemplateContent::default(),
            content_needs_formatting: false,
            render_plan: None,
            kind: TemplateType::StringFunction,
            doc_children: vec![],
            style,
            explicit_style: Style::default(),
            control_flow: TemplateControlFlow::None,
            id: String::new(),
            location: TextLocation::default(),
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

    /// Classifies template const-ness in one place.
    /// AST constant checks and render-required paths need consistent rules.
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

/// Extracts the inheritable recursive style from a given style. Returns `None`
/// if the style has no meaningful state worth inheriting (empty ID, no formatter,
/// no mode flags).
pub(crate) fn recursive_inherited_style(style: &Style) -> Option<Style> {
    use crate::compiler_frontend::ast::templates::template::BodyWhitespacePolicy;

    let mut inherited = style.to_owned();
    inherited.child_templates.clear();

    if inherited.formatter.is_none()
        && inherited.css_mode.is_none()
        && inherited.formatter_precedence == -1
        && inherited.override_precedence == -1
        && inherited.id.is_empty()
        && !inherited.clear_inherited
        && inherited.body_whitespace_policy == BodyWhitespacePolicy::DefaultTemplateBehavior
        && !inherited.html_mode
    {
        return None;
    }

    Some(inherited)
}
