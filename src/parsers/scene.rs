use crate::CompileError;
use crate::bs_types::DataType;
use crate::html_output::js_parser::create_reference_in_js;
use crate::parsers::ast_nodes::{Arg, AstNode, Expr};
use crate::parsers::markdown::to_markdown;
use crate::settings::BS_VAR_PREFIX;
use crate::settings::HTMLMeta;
use colour::red_ln;
use std::collections::HashMap;

pub enum SceneType {
    Scene(Expr),
    Slot,
    Comment,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SceneBody {
    pub before: Vec<Expr>,
    pub after: Vec<Expr>,
}
impl SceneBody {
    pub fn default() -> Self {
        Self {
            before: Vec::new(),
            after: Vec::new(),
        }
    }
    pub fn flatten(&self) -> Vec<&Expr> {
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
    pub format: i32,

    // Overrides scenes trying to place this in a slot
    pub parent_override: i32,

    // // Rules for adding this string to the wrapper
    // pub groups: &'static [u32],
    // pub incompatible_groups: &'static [u32],
    // pub required_groups: &'static [u32],

    // If compatible, should this overwrite everything else in the vec.
    // pub overwrite: bool,
    pub neighbour_rule: NeighbourRule,

    // Passes a default style for any children to start with
    // Wrappers can be overridden with parent overrides
    // Or child wrappers that are higher precedence
    pub child_default: Option<Box<PrecedenceStyle>>,

    pub compatibility: SceneCompatibility,

    // Scenes that this style will unlock
    pub unlocked_scenes: HashMap<String, Expr>,

    // If this is true, no unlocked styles will be inherited from the parent
    pub unlocks_override: bool,
}

impl Style {
    pub fn default() -> Style {
        Style {
            format: StyleFormat::None as i32,
            parent_override: -1,
            neighbour_rule: NeighbourRule::None,
            child_default: None,
            compatibility: SceneCompatibility::All,
            unlocked_scenes: HashMap::new(),
            unlocks_override: false,
        }
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrecedenceStyle {
    pub style: Style,
    pub precedence: i32,
}

impl PrecedenceStyle {
    pub(crate) fn new() -> PrecedenceStyle {
        PrecedenceStyle {
            style: Style::default(),
            precedence: -1,
        }
    }
}

// This will be important for Markdown parsing and how scenes might modify neighbouring scenes
#[derive(Clone, Debug, PartialEq)]
pub enum NeighbourRule {
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SceneCompatibility {
    None, // No other styles can be used with this style
    Incompatible(Vec<String>),
    Compatible(Vec<String>),
    All, // All other styles can be used with this style
}

pub struct SceneIngredients<'a> {
    pub scene_body: &'a SceneBody,
    pub scene_head: &'a Vec<AstNode>,
    pub scene_styles: &'a Vec<Style>,
    pub inherited_style: PrecedenceStyle,
    pub scene_id: String,
    pub format_context: i32,
}

// Returns a regular string containing the parsed scene
pub fn parse_scene(
    scene_ingredients: SceneIngredients,
    js: &mut String,
    css: &mut String,
    declarations: &mut Vec<Arg>,
    class_id: &mut usize,
    exp_id: &mut usize,
    config: &HTMLMeta,
) -> Result<String, CompileError> {
    let SceneIngredients {
        scene_body,
        scene_styles,
        scene_head,
        inherited_style,
        scene_id,
        format_context,
    } = scene_ingredients;

    // Set everything apart from the wrappers for the new style
    let mut final_style = Style {
        format: inherited_style.style.format,
        neighbour_rule: inherited_style.style.neighbour_rule,
        child_default: inherited_style.style.child_default,
        compatibility: inherited_style.style.compatibility,
        unlocks_override: inherited_style.style.unlocks_override,
        ..Style::default()
    };

    for style in scene_styles {
        // Format. How will the content be parsed?
        // Each format has a different precedence, using the highest precedence
        if style.format > final_style.format {
            final_style.format = style.format.to_owned();
        }

        // Child default
        // If the child default is higher precedence than what is set currently,
        // Then replace it with this new child default
        // >= means that later declared styles take priority over earlier ones (this can change)
        if let Some(child_default) = &style.child_default {
            match &final_style.child_default {
                Some(final_child_default) => {
                    if child_default.precedence > final_child_default.precedence {
                        final_style.child_default = style.child_default.to_owned();
                    }
                }
                None => {
                    final_style.child_default = style.child_default.to_owned();
                }
            }
        }

        // Compatibility
        // More restrictive compatibility takes precedence over less restrictive ones
        match style.compatibility {
            SceneCompatibility::None => {
                if final_style.compatibility != SceneCompatibility::None {
                    final_style.compatibility = SceneCompatibility::None;
                }
            }
            // TODO: check compatibility of scenes
            _ => {}
        }

        // Inlining rule
        // TODO: what the hell is this?
        // Something to do with how surrounding scenes are parsed with this one.
        final_style.neighbour_rule = style.neighbour_rule.to_owned();
    }

    // Now we start combining everything into one string
    let mut final_string = String::new();

    // Everything inserted into the body
    // This needs to be done now
    // so Markdown will parse any added literals correctly
    let mut content = String::new();

    // Scene content
    for value in scene_body.flatten() {
        match value {
            Expr::String(string) => {
                content.push_str(string);
            }

            Expr::Float(float) => {
                content.push_str(&float.to_string());
            }

            Expr::Int(int) => {
                content.push_str(&int.to_string());
            }

            // Add the string representation of the bool
            Expr::Bool(value) => {
                content.push_str(&value.to_string());
            }

            Expr::Scene(new_scene_nodes, new_scene_styles, new_scene_head, _) => {
                let child_default_style = match &final_style.child_default {
                    Some(child_default) => *child_default.to_owned(),
                    None => PrecedenceStyle::new(),
                };

                let new_scene = parse_scene(
                    SceneIngredients {
                        scene_body: new_scene_nodes,
                        scene_styles: new_scene_styles,
                        scene_head: new_scene_head,
                        inherited_style: child_default_style,
                        scene_id: scene_id.to_owned(),
                        format_context: final_style.format,
                    },
                    js,
                    css,
                    declarations,
                    class_id,
                    exp_id,
                    config,
                )?;

                content.push_str(&new_scene);
            }

            Expr::Reference(name, data_type, argument_accessed) => {
                // Create a span in the HTML with a class that can be referenced by JS
                // TO DO: Should be reactive in future so this can change at runtime

                // TODO: should only do this in markdown mode
                content.push_str(&format!("<span class=\"{name}\"></span>"));

                if !declarations.iter().any(|a| &a.name == name) {
                    match &data_type {
                        DataType::Structure(items) => {
                            // Automatically unpack all items in the tuple into the scene
                            // If no items accessed
                            if argument_accessed.is_empty() {
                                let mut elements = String::new();

                                for (index, _) in (**items).iter().enumerate() {
                                    elements.push_str(&format!("{BS_VAR_PREFIX}{name}[{index}],"));
                                }

                                js.push_str(&format!("uInnerHTML(\"{name}\",[{elements}]);"));
                            } else {
                                js.push_str(&create_reference_in_js(
                                    name,
                                    data_type,
                                    argument_accessed,
                                ));
                            }
                        }

                        _ => {
                            js.push_str(&create_reference_in_js(
                                name,
                                data_type,
                                argument_accessed,
                            ));
                        }
                    }
                }
            }

            Expr::None => {
                // Ignore this
                // Currently 'ignored' or hidden scenes result in a None value being added to a scene,
                // So it's not an error
                // Hopefully the compiler will always catch unintended use of None in scenes
            }

            // TODO - add / test remaining types, some of them might need unpacking
            Expr::Runtime(..) => {
                red_ln!("adding a runtime value to a scene. Not yet supported.");
            }
            Expr::Function(..) => {
                red_ln!("adding a function to a scene. Not yet supported.");
            }

            // At this point, if this structure was a style, those fields and inner scene would have been parsed in scene_node.rs
            // So we can just unpack any other public fields into the scene as strings
            Expr::StructLiteral(_) => {
                red_ln!("adding a struct literal to a scene. Not yet supported.");
            }

            // Collections will be unpacked into a scene
            Expr::Collection(_, _) => {
                red_ln!("adding a collection to a scene. Not yet supported.");
            }
        }
    }

    // If this is a Markdown scene, and the parent isn't one,
    // parse the content into Markdown
    // If the parent is parsing the Markdown already,
    // skip this as it should be done at the highest level possible
    if final_style.format == StyleFormat::Markdown as i32
        && format_context == StyleFormat::None as i32
    {
        let default_tag = "p";

        final_string.push_str(&to_markdown(&content, default_tag));

    // TODO - add parsers for each format
    } else if final_style.format == StyleFormat::Codeblock as i32
        && format_context == StyleFormat::Markdown as i32
    {
        // Add a special object replace character to signal to parent that this tag should not be parsed into Markdown
        final_string.push_str(&format!(
            "\u{FFFC}<pre><code>{}</code></pre>\u{FFFC}",
            content
        ));
    } else {
        final_string.push_str(&content);
    }

    // After wrappers
    // final_string.push_str(
    //     &final_style
    //         .wrapper
    //         .after
    //         .iter()
    //         .map(|s| s.string.to_owned())
    //         .collect::<String>(),
    // );

    Ok(final_string)
}
