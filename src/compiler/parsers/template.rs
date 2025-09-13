use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum TemplateType {
    FunctionTemplate,
    FoldedString,
    Slot,
    Comment,
}
#[derive(Clone, Debug)]
pub struct TemplateContent {
    pub before: Vec<Expression>,
    pub after: Vec<Expression>,
}
impl<'a> TemplateContent {
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

// Template Config Type
// This is passed into a template head to configure how it should be parsed
#[derive(Clone, Debug)]
pub struct Style {
    // The name of the style,
    // For helping other styles check compatibility with this style
    pub id: &'static str,

    // A callback function for how the string content of the template should be parsed
    // If at all.
    // It has a precedence that determines whether it takes priority over any inherited styles.
    // A high precedence combined with 'None' will prevent the parent from parsing the content with a formatter.
    pub compile_time_parser: Option<Box<dyn TemplateFormatter>>,
    pub formatter_precedence: i32,

    // Overrides any inherited styles that have a lower precedence
    pub override_precedence: i32,

    // Passes a default style for any children to start with
    // Wrappers can be overridden with parent overrides
    // Or child wrappers that are higher precedence
    pub child_default: Option<Box<Style>>,

    pub compatibility: TemplateCompatibility,

    // templates that this style will unlock
    // Basically a bunch of template declarations that are captured by this template
    // TODO: Styles and template unlocks as different things? Do full templates with styles being inherited suffice if they are empty?
    pub unlocked_templates: HashMap<String, ExpressionKind>,

    // If this is true, no unlocked styles will be inherited from the parent
    pub unlocks_override: bool,
}

impl Style {
    pub fn default() -> Style {
        Style {
            id: "",
            compile_time_parser: None,
            formatter_precedence: -1,
            override_precedence: -1,
            child_default: None,
            compatibility: TemplateCompatibility::All,
            unlocked_templates: HashMap::new(),
            unlocks_override: false,
        }
    }

    pub fn has_no_unlocked_templates(&self) -> bool {
        self.unlocked_templates.is_empty()
    }
}

// A trait for how the content of a template should be parsed
// This is used for Markdown, codeblocks, comments
// THESE ARE ORDERED BY PRECEDENCE (LOWEST TO HIGHEST)
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
