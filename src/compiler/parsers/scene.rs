use crate::compiler::compiler_errors::ErrorType;
#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::html5_codegen::code_block_highlighting::highlight_html_code_block;
use crate::compiler::html5_codegen::js_parser::create_reactive_reference;
use crate::compiler::html5_codegen::web_parser::{Target, parse};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::markdown::to_markdown;
use crate::compiler::parsers::tokens::TextLocation;
use crate::settings::BS_VAR_PREFIX;
use crate::{return_compiler_error, return_rule_error};
use std::collections::HashMap;

#[derive(Debug)]
pub enum SceneType {
    Scene(Expression),
    Slot,
    Comment,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SceneContent {
    pub before: Vec<Expression>,
    pub after: Vec<Expression>,
}
impl SceneContent {
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
}

// Scene Config Type
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

    pub compatibility: SceneCompatibility,

    // Scenes that this style will unlock
    pub unlocked_scenes: HashMap<String, ExpressionKind>,

    // If this is true, no unlocked styles will be inherited from the parent
    pub unlocks_override: bool,
}

impl Style {
    pub fn default() -> Style {
        Style {
            format: StyleFormat::None,
            precedence: -1,
            child_default: None,
            compatibility: SceneCompatibility::All,
            unlocked_scenes: HashMap::new(),
            unlocks_override: false,
        }
    }

    pub fn has_no_unlocked_scenes(&self) -> bool {
        self.unlocked_scenes.is_empty()
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
pub enum SceneCompatibility {
    None, // No other styles can be used with this style
    Incompatible(Vec<String>),
    Compatible(Vec<String>),
    All, // All other styles can be used with this style
}

pub struct SceneIngredients<'a> {
    pub scene_body: &'a SceneContent,
    pub scene_style: &'a Style,
    pub inherited_style: &'a Option<Style>,
    pub scene_id: String,
    pub format_context: StyleFormat,
}

// Returns a regular string containing the parsed scene
pub fn parse_scene(
    scene_ingredients: SceneIngredients,
    code: &mut String,
    position: &TextLocation,
) -> Result<String, CompileError> {
    let SceneIngredients {
        scene_body,
        scene_style,
        inherited_style,
        scene_id,
        format_context,
    } = scene_ingredients;

    // Set everything apart from the wrappers for the new style
    let mut final_style = match inherited_style {
        Some(style) => style.to_owned(),
        None => Style::default(),
    };

    // Format. How will the content be parsed?
    // Each format has a different precedence, using the highest precedence
    if scene_style.format > final_style.format {
        final_style.format = scene_style.format.to_owned();
    }

    // Compatibility
    // More restrictive compatibility takes precedence over less restrictive ones
    // match style.compatibility {
    //     SceneCompatibility::None => {
    //         if final_style.compatibility != SceneCompatibility::None {
    //             final_style.compatibility = SceneCompatibility::None;
    //         }
    //     }
    //     // TODO: check compatibility of scenes
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
    for value in scene_body.flatten() {
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

            ExpressionKind::Scene(new_scene_nodes, new_scene_style, new_scene_id) => {
                let new_scene = parse_scene(
                    SceneIngredients {
                        scene_body: new_scene_nodes,
                        scene_style: new_scene_style,
                        inherited_style: &final_style.child_default.to_owned().map(|b| *b),
                        scene_id: new_scene_id.to_owned(),
                        format_context: final_style.format.to_owned(),
                    },
                    code,
                    position,
                )?;

                content.push_str(&new_scene);
            }

            ExpressionKind::Reference(name) => {
                // Create a span in the HTML with a class that JS can reference
                // TO DO: Should be reactive in future so this can change at runtime

                match format_context {
                    StyleFormat::Markdown => {
                        content.push_str(&format!("<span class=\"{BS_VAR_PREFIX}{name}\"></span>"));
                        code.push_str(&create_reactive_reference(name, &value.data_type));
                    }

                    StyleFormat::JSString => {
                        content.push_str(&format!("${{{BS_VAR_PREFIX}{name}}}"));
                    }

                    _ => {
                        content.push_str(&format!("{BS_VAR_PREFIX}{name}"));
                    }
                }
            }

            ExpressionKind::None => {
                // Ignore this
                // Currently 'ignored' or hidden scenes result in a None value being added to a scene,
                // So it's not an error
                // Hopefully the compiler will always catch unintended use of None in scenes
            }

            ExpressionKind::Runtime(nodes) => match format_context {
                StyleFormat::Markdown => {
                    let new_parsed = parse(nodes, "", &Target::JS)?;

                    content.push_str(&format!("<span class=\"{scene_id}\"></span>"));

                    code.push_str(&format!("let {scene_id} = {}", new_parsed.code_module));
                    code.push_str(&create_reactive_reference(&scene_id, &value.data_type));
                }

                StyleFormat::JSString => {
                    let new_parsed = parse(nodes, "", &Target::JS)?;
                    content.push_str(&format!("`${{{}}}`", new_parsed.content_output));
                    code.push_str(&new_parsed.code_module);
                }

                _ => {
                    let new_parsed = parse(nodes, "", &Target::Raw)?;

                    content.push_str(&new_parsed.content_output.to_string());
                    code.push_str(&new_parsed.code_module);
                }
            },

            ExpressionKind::Function(..) => {
                return_rule_error!(
                    position.to_owned(),
                    "Functions are not supported in Scene Heads"
                )
            }

            // At this point, if this structure was a style, those fields and inner scene would have been parsed in scene_node.rs
            // So we can just unpack any other public fields into the scene as strings
            ExpressionKind::Struct(..) => {
                return_rule_error!(
                    position.to_owned(),
                    "You can't declare new variables inside of Scene Heads"
                )
            }

            // Collections will be unpacked into a scene
            ExpressionKind::Collection(_) => {
                return_compiler_error!(
                    "Collections inside scene heads not yet implemented in the compiler."
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
