use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::styles::whitespace::TemplateWhitespacePassProfile;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::string_interning::StringId;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SlotKey {
    Default,
    Named(StringId),
    Positional(usize),
}

impl SlotKey {
    pub fn named(name: StringId) -> Self {
        Self::Named(name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommentDirectiveKind {
    Note,
    Todo,
    Doc,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TemplateType {
    StringFunction,
    // Fully compile-time-resolved template content. This can still contain unresolved
    // slots, which makes it a compile-time wrapper rather than a direct string value.
    String,
    // `[$slot]` and `[$slot("name")]` parse as dedicated template nodes while body
    // parsing, then become structural slot atoms in the parent template content.
    SlotDefinition(SlotKey),
    // `[$insert("name"): ...]` helpers carry contribution content that only an
    // immediate parent template can consume during slot composition.
    SlotInsert(SlotKey),
    Comment(CommentDirectiveKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateConstValueKind {
    RenderableString,
    WrapperTemplate,
    SlotInsertHelper,
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

#[derive(Clone, Debug)]
pub struct SlotPlaceholder {
    pub key: SlotKey,
    pub applied_child_wrappers: Vec<Template>,
    pub child_wrappers: Vec<Template>,
    pub skip_parent_child_wrappers: bool,
}

impl SlotPlaceholder {
    #[allow(dead_code)] // Used only in tests
    pub fn new(key: SlotKey) -> Self {
        Self {
            key,
            applied_child_wrappers: Vec::new(),
            child_wrappers: Vec::new(),
            skip_parent_child_wrappers: false,
        }
    }

    pub fn with_child_wrappers(
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
}

#[derive(Clone, Debug)]
pub struct TemplateContent {
    // Slots are represented structurally, so template composition can preserve the
    // authored order instead of juggling a fragile before/after split.
    pub atoms: Vec<TemplateAtom>,
}

impl TemplateContent {
    #[allow(dead_code)] // Used only in tests and planned constructors
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

    pub fn add(&mut self, content: Expression) {
        self.add_with_origin(content, TemplateSegmentOrigin::Body);
    }

    pub fn add_with_origin(&mut self, content: Expression, origin: TemplateSegmentOrigin) {
        self.atoms
            .push(TemplateAtom::Content(TemplateSegment::new(content, origin)));
    }

    pub fn push_slot_with_child_wrappers(
        &mut self,
        key: SlotKey,
        applied_child_wrappers: Vec<Template>,
        child_wrappers: Vec<Template>,
        skip_parent_child_wrappers: bool,
    ) {
        self.atoms
            .push(TemplateAtom::Slot(SlotPlaceholder::with_child_wrappers(
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
}

#[derive(Clone, Debug)]
pub enum TemplateAtom {
    Content(TemplateSegment),
    Slot(SlotPlaceholder),
}

impl TemplateAtom {
    fn has_unresolved_slots(&self) -> bool {
        match self {
            TemplateAtom::Slot(_) => true,
            TemplateAtom::Content(segment) => match &segment.expression.kind {
                crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Template(
                    template,
                ) => template.has_unresolved_slots(),
                _ => false,
            },
        }
    }

    fn contains_slot_insertions(&self) -> bool {
        match self {
            TemplateAtom::Slot(_) => false,
            TemplateAtom::Content(segment) => match &segment.expression.kind {
                crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Template(
                    template,
                ) => {
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
                crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Template(
                    template,
                ) => template.is_const_evaluable_value(),
                _ => segment.expression.is_compile_time_constant(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateSegmentOrigin {
    // Head segments are values/configuration injected before the body starts.
    // They must never be reformatted by the current template style.
    Head,
    // Body segments are literal body content, so they are eligible for style
    // formatters such as markdown when they are compile-time-known strings.
    Body,
}

#[derive(Clone, Debug)]
pub struct TemplateSegment {
    pub expression: Expression,
    pub origin: TemplateSegmentOrigin,
    pub is_child_template_output: bool,
    pub source_child_template: Option<Box<Template>>,
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
            source_child_template: Some(Box::new(source_child_template)),
        }
    }
}

pub trait TemplateFormatter {
    fn format(
        &self,
        input: crate::compiler_frontend::ast::templates::template_render_plan::FormatterInput,
        string_table: &mut crate::compiler_frontend::string_interning::StringTable,
    ) -> Result<FormatterResult, CompilerMessages>;
}

impl std::fmt::Debug for dyn TemplateFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TemplateFormatter")
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // Some fields (id, skip_if_already_formatted) are planned but not yet read
pub struct Formatter {
    pub id: &'static str,

    // This formatter will be skipped if there is already a formatter for the template
    pub skip_if_already_formatted: bool,
    // Pre-format whitespace passes are run before parser-specific formatting.
    // This allows directive-owned formatters (for example `$markdown`) to opt into
    // shared dedent/trim behavior while still operating over raw template body text.
    pub pre_format_whitespace_passes: Vec<TemplateWhitespacePassProfile>,
    // Shared ownership keeps formatters cheap to clone when template styles are
    // copied or explicitly inherited during AST construction.
    pub formatter: Arc<dyn TemplateFormatter>,
    // Post-format passes run after formatter output is generated.
    pub post_format_whitespace_passes: Vec<TemplateWhitespacePassProfile>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyWhitespacePolicy {
    // Plain templates (no style directive) keep the historical default dedent/trim flow.
    DefaultTemplateBehavior,
    // Style directives own body whitespace behavior and receive raw body text unless
    // their formatter explicitly opts into shared whitespace passes.
    StyleDirectiveControlled,
}

#[derive(Clone, Debug)]
pub struct FormatterResult {
    pub output: crate::compiler_frontend::ast::templates::template_render_plan::FormatterOutput,
    pub warnings: Vec<CompilerWarning>,
}

// Template Config Type
// This is passed into a template head to configure how it should be parsed
#[derive(Clone, Debug)]
pub struct Style {
    // The name of the style,
    // For helping other styles check compatibility with this style
    pub id: &'static str,

    // A callback function for how the string content of the template should be parsed
    // If at all. Compiler will determine if this can be run at compile-time, or need a runtime call.
    pub formatter: Option<Formatter>,

    // Passes templates into the head of each direct child template of this template.
    // These wrappers do not automatically flow into grandchildren.
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
