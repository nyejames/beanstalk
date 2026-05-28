use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::styles::whitespace::TemplateWhitespacePassProfile;
use crate::compiler_frontend::ast::templates::template_render_plan::{
    FormatterInput, FormatterOutput,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap, StringTable};

use std::sync::Arc;

// -------------------------
//  Slot Keys
// -------------------------

/// Unique identifier for a template slot.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SlotKey {
    /// The default unnamed slot (`$slot`).
    Default,
    /// A named slot (`$slot("name")`).
    Named(StringId),
    /// A positional slot used in composition.
    Positional(usize),
}

impl SlotKey {
    pub fn named(name: StringId) -> Self {
        Self::Named(name)
    }

    /// Remap the named slot key, if any.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            SlotKey::Default | SlotKey::Positional(_) => {}

            SlotKey::Named(name) => {
                *name = remap.get(*name);
            }
        }
    }
}

// -------------------------
//  Directive Kinds
// -------------------------

/// Category of comment directive within a template.
#[derive(Clone, Debug, PartialEq)]
pub enum CommentDirectiveKind {
    Note,
    Todo,
    Doc,
}

/// High-level classification of a template node.
#[derive(Clone, Debug, PartialEq)]
pub enum TemplateType {
    /// A template that produces a string at runtime.
    StringFunction,

    /// Fully compile-time-resolved template content. This can still contain unresolved
    /// slots, which makes it a compile-time wrapper rather than a direct string value.
    String,

    /// `[$slot]` and `[$slot("name")]` parse as dedicated template nodes while body
    /// parsing, then become structural slot atoms in the parent template content.
    SlotDefinition(SlotKey),

    /// `[$insert("name"): ...]` helpers carry contribution content that only an
    /// immediate parent template can consume during slot composition.
    SlotInsert(SlotKey),

    /// A comment or documentation directive.
    Comment(CommentDirectiveKind),
}

impl TemplateType {
    /// Remap slot keys carried by template head directives.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            TemplateType::SlotDefinition(key) | TemplateType::SlotInsert(key) => {
                key.remap_string_ids(remap);
            }

            TemplateType::StringFunction | TemplateType::String | TemplateType::Comment(_) => {}
        }
    }
}

/// Classifies the context in which a template is being parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateParsingMode {
    /// Standard template parsing.
    Standard,
    /// Parsing inside a documentation comment (`$doc`), which has stricter constant requirements.
    DocComment,
}

/// Classifies how "constant" a template value is during AST evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateConstValueKind {
    /// Fully resolved final string value. Safe to materialize as a string slice before HIR.
    RenderableString,

    /// A template that wraps other content, such as unresolved slot placeholders.
    /// This is not automatically a backend-facing constant string in runtime paths.
    WrapperTemplate,

    /// AST composition helper (e.g., `$insert(...)`) that must not escape as a
    /// backend-facing runtime value. Helper identity alone is not sufficient to
    /// prove validity when nested under a wrapper-owned final template value.
    SlotInsertHelper,

    /// Final template value still depends on runtime expressions.
    NonConst,
}

impl TemplateConstValueKind {
    pub fn is_compile_time_value(self) -> bool {
        !matches!(self, Self::NonConst)
    }

    pub fn is_renderable_string(self) -> bool {
        matches!(self, Self::RenderableString)
    }
}

// -------------------------
//  Slot Placeholder
// -------------------------

/// A structural placeholder for a slot within template content.
#[derive(Clone, Debug)]
pub struct SlotPlaceholder {
    pub key: SlotKey,
    pub applied_child_wrappers: Vec<Template>,
    pub child_wrappers: Vec<Template>,
    pub skip_parent_child_wrappers: bool,
}

impl SlotPlaceholder {
    #[cfg(test)]
    pub fn new(key: SlotKey) -> Self {
        Self {
            key,
            applied_child_wrappers: Vec::new(),
            child_wrappers: Vec::new(),
            skip_parent_child_wrappers: false,
        }
    }

    pub fn with_wrappers(
        key: SlotKey,
        applied_child_wrappers: Vec<Template>,
        child_wrappers: Vec<Template>,
        skip_parent_child_wrappers: bool,
    ) -> Self {
        Self {
            key,
            applied_child_wrappers,
            child_wrappers,
            skip_parent_child_wrappers,
        }
    }

    /// Remap slot key and child wrapper templates recursively.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.key.remap_string_ids(remap);
        for wrapper in &mut self.applied_child_wrappers {
            wrapper.remap_string_ids(remap);
        }
        for wrapper in &mut self.child_wrappers {
            wrapper.remap_string_ids(remap);
        }
    }
}

// -------------------------
//  Template Content
// -------------------------

/// The structural content of a template: a sequence of atoms.
#[derive(Clone, Debug)]
pub struct TemplateContent {
    /// Atoms are stored in authored order.
    /// Slots are represented structurally, so template composition can preserve the
    /// authored order instead of juggling a fragile before/after split.
    pub atoms: Vec<TemplateAtom>,
}

impl TemplateContent {
    #[cfg(test)]
    pub fn new(content: Vec<Expression>) -> TemplateContent {
        TemplateContent {
            atoms: content
                .into_iter()
                .map(|expression| {
                    TemplateAtom::Content(TemplateSegment::new(
                        expression,
                        TemplateSegmentOrigin::Body,
                    ))
                })
                .collect(),
        }
    }

    pub fn default() -> Self {
        Self { atoms: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }

    pub fn add(&mut self, expression: Expression) {
        self.add_with_origin(expression, TemplateSegmentOrigin::Body);
    }

    pub fn add_with_origin(&mut self, expression: Expression, origin: TemplateSegmentOrigin) {
        self.atoms.push(TemplateAtom::Content(TemplateSegment::new(
            expression, origin,
        )));
    }

    pub fn push_slot_with_wrappers(
        &mut self,
        key: SlotKey,
        applied_child_wrappers: Vec<Template>,
        child_wrappers: Vec<Template>,
        skip_parent_child_wrappers: bool,
    ) {
        self.atoms
            .push(TemplateAtom::Slot(SlotPlaceholder::with_wrappers(
                key,
                applied_child_wrappers,
                child_wrappers,
                skip_parent_child_wrappers,
            )));
    }

    pub fn has_unresolved_slots(&self) -> bool {
        self.atoms.iter().any(TemplateAtom::has_unresolved_slots)
    }

    pub fn contains_slot_insertions(&self) -> bool {
        self.atoms
            .iter()
            .any(TemplateAtom::contains_slot_insertions)
    }

    pub fn is_const_evaluable_value(&self) -> bool {
        self.atoms
            .iter()
            .all(TemplateAtom::is_const_evaluable_value)
    }

    pub fn flatten_expressions(&self) -> Vec<Expression> {
        self.atoms
            .iter()
            .filter_map(|atom| match atom {
                TemplateAtom::Content(segment) => Some(segment.expression.clone()),
                TemplateAtom::Slot(_) => None,
            })
            .collect()
    }

    pub fn extend(&mut self, other: TemplateContent) {
        self.atoms.extend(other.atoms);
    }

    /// Remap all atoms in this template content.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        for atom in &mut self.atoms {
            atom.remap_string_ids(remap);
        }
    }
}

// -------------------------
//  Template Atoms
// -------------------------

/// A single structural unit of template content.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TemplateAtom {
    /// Literal text or a runtime expression.
    Content(TemplateSegment),
    /// A placeholder for content to be inserted later.
    Slot(SlotPlaceholder),
}

impl TemplateAtom {
    fn has_unresolved_slots(&self) -> bool {
        match self {
            TemplateAtom::Slot(_) => true,

            TemplateAtom::Content(segment) => match &segment.expression.kind {
                ExpressionKind::Template(template) => template.has_unresolved_slots(),

                _ => false,
            },
        }
    }

    fn contains_slot_insertions(&self) -> bool {
        match self {
            TemplateAtom::Slot(_) => false,

            TemplateAtom::Content(segment) => match &segment.expression.kind {
                ExpressionKind::Template(template) => {
                    matches!(template.kind, TemplateType::SlotInsert(_))
                        || template.content.contains_slot_insertions()
                }

                _ => false,
            },
        }
    }

    fn is_const_evaluable_value(&self) -> bool {
        match self {
            // Unresolved slots are allowed for compile-time wrapper values.
            // They only become invalid when a fully rendered string is required.
            TemplateAtom::Slot(_) => true,

            TemplateAtom::Content(segment) => match &segment.expression.kind {
                ExpressionKind::Template(template) => template.is_const_evaluable_value(),

                _ => segment.expression.is_compile_time_constant(),
            },
        }
    }

    /// Returns true if this atom is a direct child template in body position
    /// (either a folded child output or an unresolved template expression).
    pub(crate) fn is_direct_child_template_atom(&self) -> bool {
        let TemplateAtom::Content(segment) = self else {
            return false;
        };

        if segment.origin != TemplateSegmentOrigin::Body {
            return false;
        }

        if segment.is_child_template_output {
            return true;
        }

        match &segment.expression.kind {
            ExpressionKind::Template(template) => !template.has_unresolved_slots(),

            _ => false,
        }
    }

    /// Remap content segments and slot placeholders recursively.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            TemplateAtom::Content(segment) => {
                segment.remap_string_ids(remap);
            }

            TemplateAtom::Slot(placeholder) => {
                placeholder.remap_string_ids(remap);
            }
        }
    }
}

// -------------------------
//  Template Segments
// -------------------------

/// Identifies where a template segment originated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateSegmentOrigin {
    /// Head segments are values/configuration injected before the body starts.
    /// They must never be reformatted by the current template style.
    Head,
    /// Body segments are literal body content, so they are eligible for style
    /// formatters such as markdown when they are compile-time-known strings.
    Body,
}

/// A wrapped expression representing a piece of template content.
#[derive(Clone, Debug)]
pub struct TemplateSegment {
    pub expression: Expression,
    pub origin: TemplateSegmentOrigin,
    pub is_child_template_output: bool,
    /// Thread-safe sharing keeps template values legal inside style directive registries that are
    /// read by parallel per-file frontend preparation workers.
    pub source_child_template: Option<Arc<Template>>,
}

impl TemplateSegment {
    pub fn new(expression: Expression, origin: TemplateSegmentOrigin) -> Self {
        Self {
            expression,
            origin,
            is_child_template_output: false,
            source_child_template: None,
        }
    }

    pub fn from_child_template_output(
        expression: Expression,
        origin: TemplateSegmentOrigin,
        source_child_template: Template,
    ) -> Self {
        Self {
            expression,
            origin,
            is_child_template_output: true,
            source_child_template: Some(Arc::new(source_child_template)),
        }
    }

    /// Remap expression and source child template recursively.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.expression.remap_string_ids(remap);
        if let Some(source_child) = &mut self.source_child_template {
            Arc::make_mut(source_child).remap_string_ids(remap);
        }
    }
}

// -------------------------
//  Formatting Traits & Types
// -------------------------

/// Trait for directive-owned output formatters (e.g. `$markdown`).
///
/// Formatters are stored in style directive registries, which are shared read-only across parallel
/// tokenization/header parsing workers.
pub trait TemplateFormatter: Send + Sync {
    fn format(
        &self,
        input: FormatterInput,
        string_table: &mut StringTable,
    ) -> Result<FormatterResult, CompilerMessages>;
}

impl std::fmt::Debug for dyn TemplateFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TemplateFormatter")
    }
}

/// Bundles a core formatter with pre- and post-format whitespace passes.
#[derive(Clone, Debug)]
pub struct Formatter {
    /// Pre-format whitespace passes are run before parser-specific formatting.
    /// This allows directive-owned formatters (for example, `$markdown`) to opt into
    /// shared dedent/trim behavior while still operating over raw template body text.
    pub(crate) pre_format_whitespace_passes: Vec<TemplateWhitespacePassProfile>,

    /// Shared ownership keeps formatters cheap to clone when template styles are
    /// copied or explicitly inherited during AST construction.
    pub formatter: Arc<dyn TemplateFormatter>,

    /// Post-format passes run after formatter output is generated.
    pub(crate) post_format_whitespace_passes: Vec<TemplateWhitespacePassProfile>,
}

/// Controls how whitespace in the template body is handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyWhitespacePolicy {
    /// Plain templates (no style directive) keep the historical default dedent/trim flow.
    DefaultTemplateBehavior,
    /// Style directives own body whitespace behavior and receive raw body text unless
    /// their formatter explicitly opts into shared whitespace passes.
    StyleDirectiveControlled,
}

/// Result of a successful formatting pass.
#[derive(Clone, Debug)]
pub struct FormatterResult {
    pub output: FormatterOutput,
    pub warnings: Vec<CompilerDiagnostic>,
}

// -------------------------
//  Template Style Configuration
// -------------------------

/// Configuration passed into a template head to define how it should be parsed and rendered.
#[derive(Clone, Debug)]
pub struct Style {
    /// Semantic style label for this parsed template. Set by directive effects
    /// (`StyleDirectiveEffects.style_id`) or built-in directive handlers.
    pub id: &'static str,

    /// A callback function for how the string content of the template should be parsed
    /// If at all. Compiler will determine if this can be run at compile-time, or need a runtime call.
    pub formatter: Option<Formatter>,

    /// Passes templates into the head of each direct child template of this template.
    /// These wrappers do not automatically flow into grandchildren.
    pub child_templates: Vec<Template>,

    /// When true, nested child templates skip the parent-applied `$children(..)`
    /// wrappers while still allowing wrappers declared on the child itself.
    pub skip_parent_child_wrappers: bool,

    pub body_whitespace_policy: BodyWhitespacePolicy,

    /// When true, `[...]` brackets in the template body are treated as balanced
    /// literal text rather than parsed as nested child templates.
    pub suppress_child_templates: bool,
}

impl Style {
    pub fn default() -> Style {
        Style {
            id: "",
            formatter: None,
            child_templates: vec![],
            skip_parent_child_wrappers: false,
            body_whitespace_policy: BodyWhitespacePolicy::DefaultTemplateBehavior,
            suppress_child_templates: false,
        }
    }
}
