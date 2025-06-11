use crate::parsers::ast_nodes::{AstNode, Expr};
use crate::tokenizer::TokenPosition;
use crate::{Error, ErrorType};
use std::path::PathBuf;

pub const COMP_PAGE_KEYWORD: &str = "#page";
pub const GLOBAL_PAGE_KEYWORD: &str = "#global";
pub const INDEX_PAGE_NAME: &str = "index.html";
pub const CONFIG_FILE_NAME: &str = "#config.bs";
pub const BS_VAR_PREFIX: &str = "bs_";

#[allow(dead_code)]
pub struct Config {
    pub project: String,
    pub src: PathBuf,
    pub dev_folder: PathBuf,
    pub release_folder: PathBuf,
    pub name: String,
    pub version: String,
    pub author: String,
    pub license: String,
    pub html_meta: HTMLMeta,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            project: String::from("html"),
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

pub fn get_config_from_ast(ast: &[AstNode], project_config: &mut Config) -> Result<(), Error> {
    for node in ast {
        if let AstNode::Settings(args, ..) = node {
            for arg in args {
                match arg.name.as_str() {
                    "project" => {
                        project_config.project = match &arg.default_value {
                            Expr::String(value) => value.to_owned(),
                            _ => {
                                return Err(Error {
                                    msg: "Project name must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "src" => {
                        project_config.src = match &arg.default_value {
                            Expr::String(value) => PathBuf::from(value),
                            _ => {
                                return Err(Error {
                                    msg: "Source folder must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "dev" => {
                        project_config.dev_folder = match &arg.default_value {
                            Expr::String(value) => PathBuf::from(value),
                            _ => {
                                return Err(Error {
                                    msg: "Dev folder must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "release" => {
                        project_config.release_folder = match &arg.default_value {
                            Expr::String(value) => PathBuf::from(value),
                            _ => {
                                return Err(Error {
                                    msg: "Release folder must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "name" => {
                        project_config.name = match &arg.default_value {
                            Expr::String(value) => value.to_owned(),
                            _ => {
                                return Err(Error {
                                    msg: "Name must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "version" => {
                        project_config.version = match &arg.default_value {
                            Expr::String(value) => value.to_owned(),
                            _ => {
                                return Err(Error {
                                    msg: "Version must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "author" => {
                        project_config.author = match &arg.default_value {
                            Expr::String(value) => value.to_owned(),
                            _ => {
                                return Err(Error {
                                    msg: "Author must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "license" => {
                        project_config.license = match &arg.default_value {
                            Expr::String(value) => value.to_owned(),
                            _ => {
                                return Err(Error {
                                    msg: "License must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "html_settings" => {
                        return match &arg.default_value {
                            Expr::Args(args) => {
                                for arg in args {
                                    match arg.name.as_str() {
                                        "site_title" => {
                                            project_config.html_meta.site_title =
                                                match &arg.default_value {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Site title must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_description" => {
                                            project_config.html_meta.page_description = match &arg
                                                .default_value
                                            {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Page description must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                }
                                            };
                                        }

                                        "site_url" => {
                                            project_config.html_meta.site_url =
                                                match &arg.default_value {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Site url must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_url" => {
                                            project_config.html_meta.page_url =
                                                match &arg.default_value {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Page url must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_og_title" => {
                                            project_config.html_meta.page_og_title =
                                                match &arg.default_value {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Page og title must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_og_description" => {
                                            project_config.html_meta.page_og_description =
                                                match &arg.default_value {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Page og description must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_image_url" => {
                                            project_config.html_meta.page_image_url =
                                                match &arg.default_value {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Page image url must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_image_alt" => {
                                            project_config.html_meta.page_image_alt =
                                                match &arg.default_value {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Page image alt must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_locale" => {
                                            project_config.html_meta.page_locale =
                                                match &arg.default_value {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Page locale must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_type" => {
                                            project_config.html_meta.page_type =
                                                match &arg.default_value {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Page type must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_twitter_large_image" => {
                                            project_config.html_meta.page_twitter_large_image =
                                                match &arg.default_value {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => return Err(Error {
                                                        msg:
                                                        "Page twitter large image must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    }),
                                                };
                                        }

                                        "page_canonical_url" => {
                                            project_config.html_meta.page_canonical_url = match &arg
                                                .default_value
                                            {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Page canonical url must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                }
                                            };
                                        }

                                        "page_root_url" => {
                                            project_config.html_meta.page_root_url =
                                                match &arg.default_value {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Page root url must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "image_folder_url" => {
                                            project_config.html_meta.image_folder_url = match &arg
                                                .default_value
                                            {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Image folder url must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                }
                                            };
                                        }
                                        _ => {}
                                    }
                                }
                                Ok(())
                            }
                            _ => Err(Error {
                                msg: "HTML settings must be a struct".to_string(),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path: PathBuf::from("#config.bs"),
                                error_type: ErrorType::Type,
                            }),
                        };
                    }

                    _ => {}
                }
            }

            // if *is_exported {
            //     exported_variables.push(Arg {
            //         name: name.to_owned(),
            //         data_type: data_type.to_owned(),
            //         value: value.to_owned(),
            //     });
            // }
        }
    }

    Ok(())
}
