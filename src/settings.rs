use crate::CompilerFrontend;
use crate::build::BuildTarget;
use crate::compiler::basic_utility_functions::check_if_valid_file_path;
use crate::compiler::compiler_errors::CompilerError;
use std::env;
use std::path::{Path, PathBuf};

pub const BEANSTALK_FILE_EXTENSION: &str = "bst";
pub const COMP_PAGE_KEYWORD: &str = "#page";
pub const GLOBAL_PAGE_KEYWORD: &str = "#global";
pub const INDEX_PAGE_NAME: &str = "index.html";
pub const CONFIG_FILE_NAME: &str = "#config.bst";
pub const BS_VAR_PREFIX: &str = "bst_";

pub const IMPLICIT_START_FUNC_NAME: &str = "start";

// This is a guess about how much should be initially allocated for the token and node vecs.
// This should be a rough guess to help avoid too many allocations
// and is just a heuristic based on tests with rudimentary small snippets of code.
// Should be recalculated at a later point.
pub const SRC_TO_TOKEN_RATIO: usize = 5; // (Maybe) About 1/6 source code to tokens observed
pub const IMPORTS_CAPACITY: usize = 6; // (No Idea atm)
pub const EXPORTS_CAPACITY: usize = 6; // (No Idea atm)
pub const TOKEN_TO_HEADER_RATIO: usize = 35; // (Maybe) About 1/35 tokens to AstNode ratio
pub const TOKEN_TO_NODE_RATIO: usize = 10; // (Maybe) About 1/10 tokens to AstNode ratio
pub const MINIMUM_LIKELY_DECLARATIONS: usize = 10; // (Maybe) How many symbols the smallest common Ast blocks will likely have

#[allow(dead_code)]
#[derive(Clone)]
pub struct Config {
    pub project_name: String,
    pub build_target: BuildTarget,
    pub entry_dir: PathBuf,
    pub src: PathBuf,
    pub dev_folder: PathBuf,
    pub release_folder: PathBuf,
    pub libraries: Vec<PathBuf>, // All folders that any file in this project can import from
    pub version: String,
    pub author: String,
    pub license: String,
    pub html_meta: HTMLMeta,
    pub hot_reload: bool,
}

impl Config {
    pub fn new(user_specified_path: PathBuf, build_target: BuildTarget) -> Self {
        Config {
            build_target,
            entry_dir: user_specified_path,

            // These Default to the same directory as the project
            src: PathBuf::from(""),
            dev_folder: PathBuf::from(""),
            release_folder: PathBuf::from(""),

            libraries: Vec::new(), // All folders that are visible to all other files in this project
            project_name: String::new(),
            version: String::from("0.1.0"),
            author: String::new(),
            license: String::from("MIT"),
            html_meta: HTMLMeta::default(),
            hot_reload: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            build_target: BuildTarget::HtmlJSProject,
            entry_dir: PathBuf::from("src/main.bst"),
            src: PathBuf::from("src"),
            dev_folder: PathBuf::from("dev"),
            release_folder: PathBuf::from("release"),
            libraries: Vec::new(),
            project_name: String::from("html_project"),
            version: String::from("0.1.0"),
            author: String::new(),
            license: String::from("MIT"),
            html_meta: HTMLMeta::default(),
            hot_reload: false,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
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
