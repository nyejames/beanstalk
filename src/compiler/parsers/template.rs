use crate::settings::BEANSTALK_FILE_EXTENSION;
#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::html5_codegen::code_block_highlighting::highlight_html_code_block;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::markdown::to_markdown;
use crate::compiler::parsers::tokens::TextLocation;
use crate::{return_compiler_error, return_rule_error};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum TemplateType {
    StringTemplate,
    Slot,
    Comment,
}
#[derive(Clone, Debug, PartialEq)]
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

// Template Config Type
// This is passed into a template head to configure how it should be parsed
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    // pub slot: Wrapper,

    // A callback functions for how the string content of the template should be parsed
    // If at all
    pub format: StyleFormat,

    // Overrides other styles
    pub precedence: i32,

    // // Rules for adding this string to the wrapper
    // pub groups: &'static [u32],
    // pub incompatible_groups: &'static [u32],
    // pub required_groups: &'static [u32],

    // If compatible, should this overwrite everything else in the vec.
    // pub overwrite: bool,

    // Passes a default style for any children to start with
    // Wrappers can be overridden with parent overrides
    // Or child wrappers that are higher precedence
    pub child_default: Option<Box<Style>>,

    pub compatibility: TemplateCompatibility,

    // templates that this style will unlock
    pub unlocked_templates: HashMap<String, ExpressionKind>,

    // If this is true, no unlocked styles will be inherited from the parent
    pub unlocks_override: bool,
}

impl Style {
    pub fn default() -> Style {
        Style {
            format: StyleFormat::None,
            precedence: -1,
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
    None = 0,
    Markdown = 1,
    Metadata = 2,
    Codeblock = 3,
    Comment = 4,
    Raw = 5,
    JSString = 6,
    WasmString = 7,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TemplateCompatibility {
    None, // No other styles can be used with this style
    Incompatible(Vec<String>),
    Compatible(Vec<String>),
    All, // All other styles can be used with this style
}