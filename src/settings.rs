use crate::CompilerFrontend;
use crate::build::BuildTarget;
use crate::compiler::basic_utility_functions::check_if_valid_path;
use crate::compiler::compiler_errors::CompilerError;
use std::collections::HashMap;
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
    pub settings: HashMap<String, String>,
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
        }
    }
}
