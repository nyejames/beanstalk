use crate::compiler::compiler_errors::ErrorType;
#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::markdown::to_markdown;
use crate::compiler::parsers::tokens::TextLocation;
use crate::{return_compiler_error, return_rule_error};
use std::collections::HashMap;
use crate::compiler::html5_codegen::code_block_highlighting::highlight_html_code_block;

#[derive(Debug)]
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
    pub fn new(content: Vec<Expression>) -> TemplateContent{
        TemplateContent { before: Vec::new(), after: content }
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
// This is passed into a scene head to configure how it should be parsed
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    // pub slot: Wrapper,

    // A callback functions for how the string content of the scene should be parsed
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

    // Scenes that this style will unlock
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

// A trait for how the content of a scene should be parsed
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

pub struct TemplateIngredients<'a> {
    pub template_body: &'a TemplateContent,
    pub template_style: &'a Style,
    pub inherited_style: &'a Option<Style>,
    pub template_id: String,
    pub format_context: StyleFormat,
}

// Returns a regular string containing the parsed template
pub fn parse_template(
    template_ingredients: TemplateIngredients,
    code: &mut String,
    position: &TextLocation,
) -> Result<String, CompileError> {
    let TemplateIngredients {
        template_body,
        template_style,
        inherited_style,
        template_id: _,
        format_context,
    } = template_ingredients;

    // Set everything apart from the wrappers for the new style
    let mut final_style = match inherited_style {
        Some(style) => style.to_owned(),
        None => Style::default(),
    };

    // Format. How will the content be parsed?
    // Each format has a different precedence, using the highest precedence
    if template_style.format > final_style.format {
        final_style.format = template_style.format.to_owned();
    }

    // Compatibility
    // More restrictive compatibility takes precedence over less restrictive ones
    // match style.compatibility {
    //     SceneCompatibility::None => {
    //         if final_style.compatibility != SceneCompatibility::None {
    //             final_style.compatibility = SceneCompatibility::None;
    //         }
    //     }
    //     // TODO: check compatibility of templates
    //     _ => {}
    // }

    // Inlining rule
    // TODO: what the hell is this?
    // Something to do with how surrounding scenes are parsed with this one.
    // final_style.neighbour_rule = style.neighbour_rule.to_owned();

    // Now we start combining everything into one string
    let mut final_string = String::new();

    // Everything inserted into the body
    // This needs to be done now
    // so Markdown will parse any added literals correctly
    let mut content = String::new();

    // Scene content
    for value in template_body.flatten() {
        match &value.kind {
            ExpressionKind::String(string) => {
                content.push_str(string);
            }

            ExpressionKind::Float(float) => {
                content.push_str(&float.to_string());
            }

            ExpressionKind::Int(int) => {
                content.push_str(&int.to_string());
            }

            // Add the string representation of the bool
            ExpressionKind::Bool(value) => {
                content.push_str(&value.to_string());
            }

            ExpressionKind::Template(new_template_nodes, new_template_style, new_template_id) => {
                let new_template = parse_template(
                    TemplateIngredients {
                        template_body: new_template_nodes,
                        template_style: new_template_style,
                        inherited_style: &final_style.child_default.to_owned().map(|b| *b),
                        template_id: new_template_id.to_owned(),
                        format_context: final_style.format.to_owned(),
                    },
                    code,
                    position,
                )?;

                content.push_str(&new_template);
            }

            ExpressionKind::None => {
                // Ignore this
                // Currently 'ignored' or hidden scenes result in a None value being added to a scene,
                // So it's not an error
                // Hopefully the compiler will always catch unintended use of None in scenes.
                // May emit a warning in future if this is possible from user error.
            }

            ExpressionKind::Runtime(_nodes) => {
                // TODO
            },

            ExpressionKind::Reference(name) => {
                // TODO: Variable references in templates - if reference can't be folded at compile time,
                // evaluation and string coercion must happen at runtime
                content.push_str(&format!("${}", name));
            },

            ExpressionKind::Function(..) => {
                return_rule_error!(
                    position.to_owned(),
                    "Functions are not supported in Template Heads"
                )
            }

            // At this point, if this structure was a style, those fields and inner scene would have been parsed in scene_node.rs
            // So we can just unpack any other public fields into the scene as strings
            ExpressionKind::Struct(..) => {
                return_rule_error!(
                    position.to_owned(),
                    "You can't declare new variables inside of Template Heads"
                )
            }

            // Collections will be unpacked into a scene
            ExpressionKind::Collection(_) => {
                return_compiler_error!(
                    "Collections inside template heads not yet implemented in the compiler."
                )
            }
        }
    }

    // If this is a Markdown scene, and the parent isn't one,
    // parse the content into Markdown
    // If the parent is parsing the Markdown already,
    // skip this as it should be done at the highest level possible
    if final_style.format == StyleFormat::Markdown && format_context != StyleFormat::Markdown {
        let default_tag = "p";

        final_string.push_str(&to_markdown(&content, default_tag));

    // If the parent is outputting Markdown and the style is now a Codeblock
    // Codeblocks can't have children, so there's no need to check that like above
    } else if final_style.format == StyleFormat::Codeblock
        && format_context == StyleFormat::Markdown
    {
        // Add a special object replace character to signal to the parent that this tag should not be parsed into Markdown
        final_string.push_str(&format!(
            "\u{FFFC}<pre><code>{}</code></pre>\u{FFFC}",
            highlight_html_code_block(&content, "bs")
        ));

    // No need to do any additional parsing to the content
    // Might already be parsed by the parent, or just raw
    } else {
        final_string.push_str(&content);
    }

    Ok(final_string)
}
