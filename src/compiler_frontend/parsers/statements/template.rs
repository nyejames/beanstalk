use crate::compiler_frontend::parsers::expressions::expression::{Expression, ExpressionKind};

use crate::compiler_frontend::string_interning::InternedString;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum TemplateType {
    StringFunction,
    String,
    Slot,
    Comment,
}
#[derive(Clone, Debug)]
pub struct TemplateContent {
    pub before: Vec<Expression>,
    pub after: Vec<Expression>,
}
impl TemplateContent {
    pub fn new(content: Vec<Expression>) -> TemplateContent {
        TemplateContent {
            before: Vec::new(),
            after: content,
        }
    }

    pub fn default() -> Self {
        Self {
            before: Vec::new(),
            after: Vec::new(),
        }
    }
    pub fn add(&mut self, content: Expression, after_slot: bool) {
        if after_slot {
            self.after.push(content);
        } else {
            self.before.push(content);
        }
    }
    pub fn flatten(&self) -> Vec<&Expression> {
        let total_len = self.before.len() + self.after.len();
        let mut flattened = Vec::with_capacity(total_len);

        flattened.extend(&self.before);
        flattened.extend(&self.after);

        flattened
    }
    pub fn concat(&mut self, other: TemplateContent) {
        self.before.extend(other.before);
        self.after.extend(other.after);
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
impl Clone for Box<dyn TemplateFormatter> {
    fn clone(&self) -> Self {
        self.to_owned()
    }
}

#[derive(Clone, Debug)]
pub struct Formatter {
    pub id: &'static str,

    // This formatter will be skipped if there is already a formatter for the template
    pub skip_if_already_formatted: bool,
    pub formatter: Box<dyn TemplateFormatter>,
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

    // Passes a default style for any children to start with
    // Wrappers can be overridden with parent overrides
    // Or child wrappers that are higher precedence
    pub child_default: Option<Box<Style>>,

    // templates that this style will unlock
    // Basically a bunch of template declarations that are captured by this template
    // TODO: Styles and template unlocks as different things? Do full templates with styles being inherited suffice if they are empty?
    pub unlocked_templates: HashMap<InternedString, ExpressionKind>,

    // If this is true, no unlocked styles will be inherited from the parent
    pub unlocks_override: bool,
    pub strict: bool, // MAYBE - enforces only strings to be used in the template head, no dynamic behaviour
}

impl Style {
    pub fn default() -> Style {
        Style {
            id: "",
            formatter: None,
            formatter_precedence: -1,
            override_precedence: -1,
            child_default: None,
            unlocked_templates: HashMap::new(),
            unlocks_override: false,
            strict: false,
        }
    }

    pub fn has_no_unlocked_templates(&self) -> bool {
        self.unlocked_templates.is_empty()
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
