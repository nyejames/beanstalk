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
///
/// ## Metadata lifecycle invariants
///
/// - **`content`** is authoritative after parsing and after every composition
///   or formatting pass. It always reflects the current composed atom stream.
///
/// - **`render_plan`** is authoritative only when `content_needs_formatting` is
///   `false`. HIR lowering requires runtime templates to carry a render plan
///   that matches their content so the backend can emit the correct fragment
///   sequence without re-parsing atoms.
///
/// - **`unformatted_content`** is a snapshot of `content` taken *before* body
///   formatting runs. It exists so diagnostics and future re-format workflows
///   can refer to the original composed shape. It is not updated by later
///   composition passes — only by `resync_runtime_metadata` when formatting is
///   deferred.
///
/// - **`kind`** is derived from `content` (via `refresh_kind_from_content`) but
///   preserves semantic markers such as `SlotDefinition`, `SlotInsert`, and
///   `Comment` that must not be overwritten by generic cleanup.
#[derive(Clone, Debug)]
pub struct Template {
    pub content: TemplateContent,
    pub unformatted_content: TemplateContent,
    pub content_needs_formatting: bool,
    pub render_plan: Option<TemplateRenderPlan>,
    pub kind: TemplateType,
    pub doc_children: Vec<Template>,
    pub style: Style,

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
    /// Creates an empty template with default style and no content.
    pub fn empty() -> Template {
        Template {
            content: TemplateContent::default(),
            unformatted_content: TemplateContent::default(),
            content_needs_formatting: false,
            render_plan: None,
            kind: TemplateType::StringFunction,
            doc_children: vec![],
            style: Style::default(),
            id: String::new(),
            location: SourceLocation::default(),
        }
    }

    /// Replaces the effective style with the given style.
    pub(crate) fn apply_style(&mut self, style: Style) {
        self.style = style;
    }

    /// Applies an update function to the effective style.
    pub(crate) fn apply_style_updates(&mut self, mut update: impl FnMut(&mut Style)) {
        update(&mut self.style);
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

    /// Rebuilds full runtime metadata from current `content`.
    ///
    /// WHAT:
    /// - refreshes pre-format snapshot and kind classification
    /// - always materializes a render plan from the current content stream
    ///
    /// WHY:
    /// - HIR lowering requires runtime templates to already carry an authoritative render plan.
    pub(crate) fn resync_runtime_metadata(&mut self) {
        self.resync_metadata_with_plan_policy(true);
    }

    /// Rebuilds composition metadata while only materializing plans needed by runtime templates.
    ///
    /// WHAT:
    /// - refreshes pre-format snapshot and kind classification
    /// - materializes render plans only when the template remains `StringFunction`
    ///
    /// WHY:
    /// - template composition creates many temporary compile-time wrapper/string templates.
    ///   Building full render plans for those intermediates causes avoidable clone churn.
    pub(crate) fn resync_composition_metadata(&mut self) {
        self.resync_metadata_with_plan_policy(false);
    }

    fn resync_metadata_with_plan_policy(&mut self, force_full_plan: bool) {
        // Only snapshot pre-format content when formatting is actually deferred.
        // After Template::new(), content_needs_formatting is false, so this clone
        // is unnecessary for the common case.
        if self.content_needs_formatting {
            self.unformatted_content = self.content.to_owned();
        }
        self.content_needs_formatting = false;
        self.refresh_kind_from_content();

        let should_materialize_plan =
            force_full_plan || matches!(self.kind, TemplateType::StringFunction);
        self.render_plan =
            should_materialize_plan.then(|| TemplateRenderPlan::from_content(&self.content));
    }

    /// Clones this template for AST composition work without carrying stale render-plan payload.
    ///
    /// WHY:
    /// - composition frequently clones wrapper/intermediate templates for structural rewrites.
    ///   Carrying cloned render plans in those intermediates is unnecessary and expensive.
    pub(crate) fn clone_for_composition(&self) -> Template {
        Template {
            content: self.content.to_owned(),
            unformatted_content: self.unformatted_content.to_owned(),
            content_needs_formatting: self.content_needs_formatting,
            render_plan: None,
            kind: self.kind.to_owned(),
            doc_children: self.doc_children.to_owned(),
            style: self.style.to_owned(),

            id: self.id.to_owned(),
            location: self.location.to_owned(),
        }
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
    /// - `SlotInsertHelper` identifies helper shape, but nested helper-owned content can
    ///   still be legal inside a reusable wrapper value.
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
