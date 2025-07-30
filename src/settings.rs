use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::parsers::ast_nodes::{Arg, NodeKind};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::expressions::expression::ExpressionKind;
use crate::compiler::parsers::tokens::TextLocation;
use crate::return_type_error;
use std::path::PathBuf;

pub const COMP_PAGE_KEYWORD: &str = "#page";
pub const GLOBAL_PAGE_KEYWORD: &str = "#global";
pub const INDEX_PAGE_NAME: &str = "index.html";
pub const CONFIG_FILE_NAME: &str = "#config.bs";
pub const BS_VAR_PREFIX: &str = "bs_";

// This is a guess about how much should be initially allocated for the token and node vecs.
// This should be a rough guess to help avoid too many allocations
// and is just a heuristic based on tests with rudimentary small snippets of code.
// Should be recalculated at a later point.
pub const SRC_TO_TOKEN_RATIO: usize = 5; // (Maybe) About 1/6 source code to tokens observed
pub const IMPORTS_CAPACITY: usize = 6; // (No Idea atm)
pub const EXPORTS_CAPACITY: usize = 6; // (No Idea atm)
pub const TOKEN_TO_NODE_RATIO: usize = 10; // (Maybe) About 1/10 tokens to AstNode ratio
pub const MINIMUM_LIKELY_DECLARATIONS: usize = 10; // (Maybe) How many symbols the smallest common Ast blocks will likely have

#[allow(dead_code)]
pub struct Config {
    pub name: String,
    pub project_type: String,
    pub entry_point: PathBuf,
    pub src: PathBuf,
    pub dev_folder: PathBuf,
    pub release_folder: PathBuf,
    pub version: String,
    pub author: String,
    pub license: String,
    pub html_meta: HTMLMeta,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            project_type: String::from("html"),
            entry_point: PathBuf::from("src/main.bs"),
            src: PathBuf::from("src"),
            dev_folder: PathBuf::from("dev"),
            release_folder: PathBuf::from("release"),
            name: String::from("html_project"),
            version: String::from("0.1.0"),
            author: String::new(),
            license: String::from("MIT"),
            html_meta: HTMLMeta::default(),
        }
    }
}

#[allow(dead_code)]
pub struct HTMLMeta {
    pub site_title: String,
    pub page_description: String,
    pub site_url: String,
    pub page_url: String,
    pub page_og_title: String,
    pub page_og_description: String,
    pub page_image_url: String,
    pub page_image_alt: String,
    pub page_locale: String,
    pub page_type: String,
    pub page_twitter_large_image: String,
    pub page_canonical_url: String,
    pub page_root_url: String,
    pub image_folder_url: String,
    pub favicons_folder_url: String,
    pub theme_color_light: String,
    pub theme_color_dark: String,
    pub auto_site_title: bool,
    pub release_build: bool,
}

impl Default for HTMLMeta {
    fn default() -> Self {
        HTMLMeta {
            site_title: String::from("Website Title"),
            page_description: String::from("Website Description"),
            site_url: String::from("localhost:6969"),
            page_url: String::from(""),
            page_og_title: String::from(""),
            page_og_description: String::from(""),
            page_image_url: String::from(""),
            page_image_alt: String::from(""),
            page_locale: String::from("en_US"),
            page_type: String::from("website"),
            page_twitter_large_image: String::from(""),
            page_canonical_url: String::from(""),
            page_root_url: String::from("./"),
            image_folder_url: String::from("images"),
            favicons_folder_url: String::from("images/favicons"),
            theme_color_light: String::from("#fafafa"),
            theme_color_dark: String::from("#101010"),
            auto_site_title: true,
            release_build: false,
        }
    }
}

pub fn get_config_from_ast(
    config_exports: Vec<Arg>,
    project_config: &mut Config,
) -> Result<(), CompileError> {
    for arg in config_exports {
        match arg.name.as_str() {
            "project" => {
                project_config.project_type = match &arg.value.kind {
                    ExpressionKind::String(value) => value.to_owned(),
                    _ => {
                        return_type_error!(TextLocation::default(), "Project name must be a string")
                    }
                };
            }

            "entry_point" => {
                project_config.entry_point = match &arg.value.kind {
                    ExpressionKind::String(value) => PathBuf::from(value),
                    _ => {
                        return_type_error!(TextLocation::default(), "Entry point must be a string")
                    }
                };
            }

            "src" => {
                project_config.src = match &arg.value.kind {
                    ExpressionKind::String(value) => PathBuf::from(value),
                    _ => return_type_error!(
                        TextLocation::default(),
                        "Source folder must be a string"
                    ),
                };
            }

            "dev" => {
                project_config.dev_folder = match &arg.value.kind {
                    ExpressionKind::String(value) => PathBuf::from(value),
                    _ => return_type_error!(TextLocation::default(), "Dev folder must be a string"),
                };
            }

            "release" => {
                project_config.release_folder = match &arg.value.kind {
                    ExpressionKind::String(value) => PathBuf::from(value),
                    _ => return_type_error!(
                        TextLocation::default(),
                        "Release folder must be a string"
                    ),
                };
            }

            "name" => {
                project_config.name = match &arg.value.kind {
                    ExpressionKind::String(value) => value.to_owned(),
                    _ => {
                        return_type_error!(TextLocation::default(), "Name must be a string")
                    }
                };
            }

            "version" => {
                project_config.version = match &arg.value.kind {
                    ExpressionKind::String(value) => value.to_owned(),
                    _ => {
                        return_type_error!(TextLocation::default(), "Version must be a string")
                    }
                };
            }

            "author" => {
                project_config.author = match &arg.value.kind {
                    ExpressionKind::String(value) => value.to_owned(),
                    _ => return_type_error!(TextLocation::default(), "Author must be a string"),
                };
            }

            "license" => {
                project_config.license = match &arg.value.kind {
                    ExpressionKind::String(value) => value.to_owned(),
                    _ => {
                        return_type_error!(TextLocation::default(), "License must be a string")
                    }
                };
            }

            "html_settings" => {
                match &arg.value.kind {
                    ExpressionKind::Struct(args) => {
                        for arg in args {
                            match arg.name.as_str() {
                                "site_title" => {
                                    project_config.html_meta.site_title = match &arg.value.kind {
                                        ExpressionKind::String(value) => value.to_owned(),
                                        _ => return_type_error!(
                                            TextLocation::default(),
                                            "Site title must be a string"
                                        ),
                                    };
                                }

                                "page_description" => {
                                    project_config.html_meta.page_description =
                                        match &arg.value.kind {
                                            ExpressionKind::String(value) => value.to_owned(),
                                            _ => return_type_error!(
                                                TextLocation::default(),
                                                "Page description must be a string"
                                            ),
                                        };
                                }

                                "site_url" => {
                                    project_config.html_meta.site_url = match &arg.value.kind {
                                        ExpressionKind::String(value) => value.to_owned(),
                                        _ => return_type_error!(
                                            TextLocation::default(),
                                            "Site url must be a string"
                                        ),
                                    };
                                }

                                "page_url" => {
                                    project_config.html_meta.page_url = match &arg.value.kind {
                                        ExpressionKind::String(value) => value.to_owned(),
                                        _ => return_type_error!(
                                            TextLocation::default(),
                                            "Page url must be a string"
                                        ),
                                    };
                                }

                                "page_og_title" => {
                                    project_config.html_meta.page_og_title = match &arg.value.kind {
                                        ExpressionKind::String(value) => value.to_owned(),
                                        _ => return_type_error!(
                                            TextLocation::default(),
                                            "Page og title must be a string"
                                        ),
                                    };
                                }

                                "page_og_description" => {
                                    project_config.html_meta.page_og_description =
                                        match &arg.value.kind {
                                            ExpressionKind::String(value) => value.to_owned(),
                                            _ => return_type_error!(
                                                TextLocation::default(),
                                                "Page og description must be a string"
                                            ),
                                        };
                                }

                                "page_image_url" => {
                                    project_config.html_meta.page_image_url = match &arg.value.kind
                                    {
                                        ExpressionKind::String(value) => value.to_owned(),
                                        _ => return_type_error!(
                                            TextLocation::default(),
                                            "Page image url must be a string"
                                        ),
                                    };
                                }

                                "page_image_alt" => {
                                    project_config.html_meta.page_image_alt = match &arg.value.kind
                                    {
                                        ExpressionKind::String(value) => value.to_owned(),
                                        _ => return_type_error!(
                                            TextLocation::default(),
                                            "Page image alt must be a string"
                                        ),
                                    };
                                }

                                "page_locale" => {
                                    project_config.html_meta.page_locale = match &arg.value.kind {
                                        ExpressionKind::String(value) => value.to_owned(),
                                        _ => return_type_error!(
                                            TextLocation::default(),
                                            "Page locale must be a string"
                                        ),
                                    };
                                }

                                "page_type" => {
                                    project_config.html_meta.page_type = match &arg.value.kind {
                                        ExpressionKind::String(value) => value.to_owned(),
                                        _ => return_type_error!(
                                            TextLocation::default(),
                                            "Page type must be a string"
                                        ),
                                    };
                                }

                                "page_twitter_large_image" => {
                                    project_config.html_meta.page_twitter_large_image =
                                        match &arg.value.kind {
                                            ExpressionKind::String(value) => value.to_owned(),
                                            _ => return_type_error!(
                                                TextLocation::default(),
                                                "Page twitter large image must be a string"
                                            ),
                                        };
                                }

                                "page_canonical_url" => {
                                    project_config.html_meta.page_canonical_url =
                                        match &arg.value.kind {
                                            ExpressionKind::String(value) => value.to_owned(),
                                            _ => return_type_error!(
                                                TextLocation::default(),
                                                "Page canonical url must be a string"
                                            ),
                                        };
                                }

                                "page_root_url" => {
                                    project_config.html_meta.page_root_url = match &arg.value.kind {
                                        ExpressionKind::String(value) => value.to_owned(),
                                        _ => return_type_error!(
                                            TextLocation::default(),
                                            "Page root url must be a string"
                                        ),
                                    };
                                }

                                "image_folder_url" => {
                                    project_config.html_meta.image_folder_url =
                                        match &arg.value.kind {
                                            ExpressionKind::String(value) => value.to_owned(),
                                            _ => return_type_error!(
                                                TextLocation::default(),
                                                "Image folder url must be a string"
                                            ),
                                        };
                                }

                                _ => return_type_error!(
                                    TextLocation::default(),
                                    "Unknown HTML setting"
                                ),
                            }
                        }
                    }

                    _ => return_type_error!(
                        TextLocation::default(),
                        "HTML settings must be a struct"
                    ),
                };
            }

            _ => {}
        }

        // if *is_exported {
        //     exported_variables.push(Arg {
        //         name: name.to_owned(),
        //         data_type: data_type.to_owned(),
        //         value: value.to_owned(),
        //     });
        // }
    }

    Ok(())
}
