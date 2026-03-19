#![allow(dead_code)]

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::string_interning::StringId;
use crate::return_rule_error;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SlotKey {
    Default,
    Named(StringId),
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

impl CommentDirectiveKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CommentDirectiveKind::Note => "note",
            CommentDirectiveKind::Todo => "todo",
            CommentDirectiveKind::Doc => "doc",
        }
    }
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
    pub clear_inherited_style: bool,
}

impl SlotPlaceholder {
    pub fn new(key: SlotKey) -> Self {
        Self {
            key,
            applied_child_wrappers: Vec::new(),
            child_wrappers: Vec::new(),
            clear_inherited_style: false,
        }
    }

    pub fn with_child_wrappers(
        key: SlotKey,
        applied_child_wrappers: Vec<Template>,
        child_wrappers: Vec<Template>,
        clear_inherited_style: bool,
    ) -> Self {
        Self {
            key,
            applied_child_wrappers,
            child_wrappers,
            clear_inherited_style,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TemplateContent {
    // Slots are represented structurally so template composition can preserve the
    // authored order instead of juggling a fragile before/after split.
    pub atoms: Vec<TemplateAtom>,
}

impl TemplateContent {
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

    pub fn push_slot(&mut self, key: SlotKey) {
        self.atoms
            .push(TemplateAtom::Slot(SlotPlaceholder::new(key)));
    }

    pub fn push_slot_with_child_wrappers(
        &mut self,
        key: SlotKey,
        applied_child_wrappers: Vec<Template>,
        child_wrappers: Vec<Template>,
        clear_inherited_style: bool,
    ) {
        self.atoms
            .push(TemplateAtom::Slot(SlotPlaceholder::with_child_wrappers(
                key,
                applied_child_wrappers,
                child_wrappers,
                clear_inherited_style,
            )));
    }

    pub fn slot_count(&self) -> usize {
        self.atoms
            .iter()
            .filter(|atom| matches!(atom, TemplateAtom::Slot(_)))
            .count()
    }

    pub fn has_default_slot(&self) -> bool {
        self.atoms
            .iter()
            .any(|atom| matches!(atom, TemplateAtom::Slot(slot) if matches!(&slot.key, SlotKey::Default)))
    }

    pub fn has_named_slots(&self) -> bool {
        self.atoms
            .iter()
            .any(|atom| matches!(atom, TemplateAtom::Slot(slot) if matches!(&slot.key, SlotKey::Named(_))))
    }

    /// Count every unresolved slot marker reachable inside this content,
    /// including slots nested in child templates.
    pub fn total_slot_count(&self) -> usize {
        self.atoms.iter().map(TemplateAtom::total_slot_count).sum()
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

    pub fn split_by_slots(&self) -> Vec<Vec<TemplateAtom>> {
        let mut segments = vec![Vec::new()];

        for atom in &self.atoms {
            match atom {
                TemplateAtom::Slot(_) => segments.push(Vec::new()),
                TemplateAtom::Content(_) => segments
                    .last_mut()
                    .expect("slot splitting should always keep at least one output segment")
                    .push(atom.clone()),
            }
        }

        segments
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

    pub fn flatten_renderable_segments(&self) -> Result<Vec<Expression>, CompilerError> {
        let mut flattened = Vec::with_capacity(self.atoms.len());

        for atom in &self.atoms {
            match atom {
                TemplateAtom::Slot(_) => {
                    return Err(CompilerError::compiler_error(
                        "Template still contains unresolved '$slot' directives and cannot be rendered directly.",
                    ));
                }
                TemplateAtom::Content(segment) => {
                    if let crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Template(
                        template,
                    ) = &segment.expression.kind
                    {
                        if matches!(template.kind, TemplateType::SlotInsert(_)) {
                            return_rule_error!(
                                "'$insert(...)' can only be used while filling an immediate parent template that defines matching slots.",
                                segment.expression.location.to_owned().to_error_location_without_table()
                            );
                        }

                        if template.has_unresolved_slots() {
                            return_rule_error!(
                                "Template still contains unresolved '$slot' directives and cannot be rendered directly.",
                                segment.expression.location.to_owned().to_error_location_without_table()
                            );
                        }
                    }

                    flattened.push(segment.expression.clone())
                }
            }
        }

        Ok(flattened)
    }

    pub fn extend(&mut self, other: TemplateContent) {
        self.atoms.extend(other.atoms);
    }

    pub fn extend_retagged(&mut self, other: TemplateContent, origin: TemplateSegmentOrigin) {
        // When a template is unpacked into another template head, its content should
        // behave like head content in the receiving template, even if it originally
        // came from a body. Retagging preserves the new formatter boundary rules.
        self.atoms
            .extend(other.atoms.into_iter().map(|atom| match atom {
                TemplateAtom::Content(segment) => {
                    TemplateAtom::Content(segment.with_origin(origin))
                }
                TemplateAtom::Slot(slot) => TemplateAtom::Slot(slot),
            }));
    }
}

#[derive(Clone, Debug)]
pub enum TemplateAtom {
    Content(TemplateSegment),
    Slot(SlotPlaceholder),
}

impl TemplateAtom {
    fn total_slot_count(&self) -> usize {
        match self {
            TemplateAtom::Slot(_) => 1,
            TemplateAtom::Content(segment) => match &segment.expression.kind {
                crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Template(
                    template,
                ) => template.content.total_slot_count(),
                _ => 0,
            },
        }
    }

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

    pub fn with_origin(mut self, origin: TemplateSegmentOrigin) -> Self {
        self.origin = origin;
        self
    }
}

pub trait TemplateFormatter {
    fn format(&self, content: &mut String);
}

impl std::fmt::Debug for dyn TemplateFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TemplateFormatter")
    }
}

#[derive(Clone, Debug)]
pub struct Formatter {
    pub id: &'static str,

    // This formatter will be skipped if there is already a formatter for the template
    pub skip_if_already_formatted: bool,
    // Shared ownership keeps formatters cheap to clone as styles are inherited or
    // copied into nested templates during AST construction.
    pub formatter: Arc<dyn TemplateFormatter>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CssDirectiveMode {
    Block,
    Inline,
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
    pub formatter_precedence: i32,

    // Overrides any inherited styles that have a lower precedence
    pub override_precedence: i32,

    // Passes templates into the head of each direct child template of this template.
    // These wrappers do not automatically flow into grandchildren.
    pub child_templates: Vec<Template>,
    pub css_mode: Option<CssDirectiveMode>,
    pub clear_inherited: bool,
}

impl Style {
    pub fn default() -> Style {
        Style {
            id: "",
            formatter: None,
            formatter_precedence: -1,
            override_precedence: -1,
            child_templates: vec![],
            css_mode: None,
            clear_inherited: false,
        }
    }
}

// A trait for how the content of a template should be parsed
// This is used for Markdown, codeblocks, comments
// THESE ARE ORDERED BY PRECEDENCE (LOWEST TO HIGHEST)
#[allow(dead_code)] // Will be used by frontend template system
#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum StyleFormat {
    Markdown = 0,
    WasmString = 1,
    None = 2, // This is an explicit override of the parent style
    Codeblock = 3,
    Metadata = 4,
    Raw = 5,
    Comment = 6,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TemplateCompatibility {
    None, // No other styles can be used with this style
    Incompatible(Vec<String>),
    Compatible(Vec<String>),
    All, // All other styles can be used with this style
}

#[derive(Clone, Debug, PartialEq)]
pub enum TemplateControlFlow {
    None,
    If,
    Loop,
}
