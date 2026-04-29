use crate::compiler_frontend::compiler_errors::{
    CompilerError, ErrorMetaDataKey, ErrorType, SourceLocation,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::collections::HashMap;
use std::path::PathBuf;

pub const BEANSTALK_FILE_EXTENSION: &str = "bst";
pub const COMP_PAGE_KEYWORD: &str = "#page";
pub const GLOBAL_PAGE_KEYWORD: &str = "#global";
pub const INDEX_PAGE_NAME: &str = "index.html";
pub const CONFIG_FILE_NAME: &str = "#config.bst";
pub const BS_VAR_PREFIX: &str = "bst_";

/// Special reserved names for functions and variables created by the compiler
pub const TOP_LEVEL_TEMPLATE_NAME: &str = "#template";
pub const TOP_LEVEL_CONST_TEMPLATE_NAME: &str = "#const_template";
pub const IMPLICIT_START_FUNC_NAME: &str = "start";

// This is a guess about how much should be initially allocated for vecs in the compiler.
// This should be a rough guess to help avoid too many allocations
// and is just a heuristic based on tests with rudimentary small snippets of code.
// Should be recalculated at a later point.
pub const MINIMUM_STRING_TABLE_CAPACITY: usize = 32;
pub const SRC_TO_TOKEN_RATIO: usize = 5; // (Maybe) About 1/6 source code to tokens observed
pub const IMPORTS_CAPACITY: usize = 6; // (No Idea atm)
pub const EXPORTS_CAPACITY: usize = 6; // (No Idea atm)
pub const TOKEN_TO_HEADER_RATIO: usize = 35; // (Maybe) About 1/35 tokens to AstNode ratio
pub const TOKEN_TO_DECLARATION_RATIO: usize = 20; // (Maybe) About 1/20 tokens for each new declaration symbol
pub const TOKEN_TO_NODE_RATIO: usize = 10; // (Maybe) About 1/10 tokens to AstNode ratio
pub const MINIMUM_LIKELY_DECLARATIONS: usize = 10; // (Maybe) How many symbols the smallest common Ast blocks will likely have

/// WHAT: project configuration loaded from #config.bst that controls build behavior.
/// WHY: config is the control plane for the build system; it must be validated early
///      and provide precise error locations for all settings.
///
/// Design Principles:
/// - Config is loaded in Stage 0 before any compilation work begins
/// - All config keys are validated early so backends can reject invalid settings
/// - Source locations are tracked for precise error reporting
/// - Multi-error collection helps developers fix all issues in one iteration
///
/// Standard Config Keys:
/// - `#entry_root`: The root directory for source files (default: "")
/// - `#dev_folder`: Output directory for development builds (default: "dev")
/// - `#output_folder`: Output directory for release builds (default: "release")
/// - `#root_folders`: Top-level project folders for explicit imports (default: [])
/// - `#library_folders`: Top-level folders scanned for project-local source libraries (default: ["lib"])
/// - `#project_name` or `#name`: The project name
/// - `#version`: The project version (default: "0.1.0")
/// - `#author`: The project author
/// - `#license`: The project license (default: "MIT")
///
/// Custom Keys:
/// - Backend-specific keys are stored in the `settings` HashMap
/// - Backends validate their own keys through `BackendBuilder::validate_project_config`
#[derive(Clone)]
pub struct Config {
    pub project_name: String,
    pub entry_dir: PathBuf,
    pub entry_root: PathBuf,
    pub dev_folder: PathBuf,
    pub release_folder: PathBuf,
    /// Top-level project folders that non-relative imports can target explicitly
    pub root_folders: Vec<PathBuf>,
    /// Top-level project folders scanned for project-local source libraries.
    pub library_folders: Vec<PathBuf>,
    /// Whether `#library_folders` was explicitly configured in `#config.bst`.
    pub has_explicit_library_folders: bool,
    pub version: String,
    pub author: String,
    pub license: String,
    /// Custom settings for any project builder to use
    pub settings: HashMap<String, String>,
    /// Source locations for each config key, used for precise error reporting
    pub setting_locations: HashMap<String, SourceLocation>,
}

impl Config {
    pub fn new(user_specified_path: PathBuf) -> Self {
        Config {
            entry_dir: user_specified_path,
            // These Default to the same directory as the project
            entry_root: PathBuf::from(""),
            dev_folder: PathBuf::from("dev"),
            release_folder: PathBuf::from("release"),

            root_folders: Vec::new(), // Explicitly-visible top-level project folders for imports and future path validation
            library_folders: vec![PathBuf::from("lib")], // Default convention for project-local source libraries
            has_explicit_library_folders: false,
            project_name: String::new(),
            version: String::from("0.1.0"),
            author: String::new(),
            license: String::from("MIT"),

            // Custom settings for any project builder to use
            settings: HashMap::new(),
            setting_locations: HashMap::new(),
        }
    }

    /// Resolve the most specific location for a config key, falling back to `#config.bst`.
    ///
    /// WHAT: uses the recorded setting location when available, otherwise creates a file-level
    /// location for the config file itself.
    /// WHY: config parsers should not duplicate fallback logic every time they report a bad value.
    pub fn setting_location_or_config_file(
        &self,
        key: &str,
        string_table: &mut StringTable,
    ) -> SourceLocation {
        self.setting_locations
            .get(key)
            .cloned()
            .unwrap_or_else(|| SourceLocation::from_path(&self.config_file_path(), string_table))
    }

    /// Build a config error with the standard location and suggestion wiring.
    ///
    /// WHAT: centralizes config-setting diagnostics on `Config`.
    /// WHY: parsers for routing/document/html settings should only define value semantics, not
    /// duplicate location lookup and metadata plumbing.
    pub fn config_error_with_suggestion(
        &self,
        key: &str,
        message: impl Into<String>,
        suggestion: impl Into<String>,
        string_table: &mut StringTable,
    ) -> CompilerError {
        let mut error = CompilerError::new(
            message.into(),
            self.setting_location_or_config_file(key, string_table),
            ErrorType::Config,
        );
        error
            .metadata
            .insert(ErrorMetaDataKey::PrimarySuggestion, suggestion.into());
        error
    }

    pub fn config_file_path(&self) -> PathBuf {
        self.entry_dir.join(CONFIG_FILE_NAME)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            entry_dir: PathBuf::new(),
            entry_root: PathBuf::from("src"),
            dev_folder: PathBuf::from("dev"),
            release_folder: PathBuf::from("release"),
            root_folders: Vec::new(),
            library_folders: vec![PathBuf::from("lib")],
            has_explicit_library_folders: false,
            project_name: String::from("html_project"),
            version: String::from("0.1.0"),
            author: String::new(),
            license: String::from("MIT"),
            settings: HashMap::new(),
            setting_locations: HashMap::new(),
        }
    }
}
