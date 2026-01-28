use crate::compiler::host_functions::registry::RuntimeBackend;
use std::path::PathBuf;

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

#[derive(Clone)]
pub enum ProjectType {
    HTML,     // Will start off as pure JS, then become JS + Wasm as the compiler matures
    Embedded, // Using Wasmer/Rust to embed Beanstalk into projects
    Jit,      // Don't create output files, just run the code
    Repl,     // Start in a string template head
}

impl Default for ProjectType {
    fn default() -> Self {
        ProjectType::HTML
    }
}
#[allow(dead_code)]
#[derive(Clone)]
pub struct Config {
    pub name: String,
    pub project_type: ProjectType,
    pub entry_point: PathBuf,
    pub src: PathBuf,
    pub dev_folder: PathBuf,
    pub release_folder: PathBuf,
    pub version: String,
    pub author: String,
    pub license: String,
    pub html_meta: HTMLMeta,
    pub hot_reload: bool,
}

impl Config {
    pub fn new(entry_point: PathBuf) -> Self {
        Config {
            project_type: ProjectType::default(),
            entry_point,
            src: PathBuf::from("src"),
            dev_folder: PathBuf::from("dev"),
            release_folder: PathBuf::from("release"),
            name: String::from("html_project"),
            version: String::from("0.1.0"),
            author: String::new(),
            license: String::from("MIT"),
            html_meta: HTMLMeta::default(),
            hot_reload: false,
        }
    }

    pub fn runtime_backend(&self) -> RuntimeBackend {
        match self.project_type {
            ProjectType::HTML | ProjectType::Jit | ProjectType::Repl => RuntimeBackend::JavaScript,
            ProjectType::Embedded => RuntimeBackend::Rust,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            project_type: ProjectType::default(),
            entry_point: PathBuf::from("src/main.bst"),
            src: PathBuf::from("src"),
            dev_folder: PathBuf::from("dev"),
            release_folder: PathBuf::from("release"),
            name: String::from("html_project"),
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

// pub fn get_config_from_ast(
//     config_ast: Ast,
//     project_config: &mut Config,
// ) -> Result<(), CompileError> {
//     // TODO: Maybe these should be compiler directives again instead of explicit exports with determined names?
//     for arg in config_ast.external_exports {
//         match arg.name.as_str() {
//             "project" => {
//                 project_config.project_type = match &arg.value.kind {
//                     ExpressionKind::StringSlice(value) => match value.as_str() {
//                         "" => ProjectType::default(),
//                         _ => return_type_error!(
//                             "Project type must be a string",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "String",
//                                 PrimarySuggestion => "Use a valid project type string",
//                             }
//                         ),
//                     },
//                     _ => {
//                         let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                         return_type_error!(
//                             "Project type must be a string",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "String",
//                                 FoundType => found_type,
//                                 PrimarySuggestion => "Provide a string value for project type",
//                             }
//                         )
//                     }
//                 };
//             }

//             "runtime_backend" => {
//                 project_config.runtime = match &arg.value.kind {
//                     ExpressionKind::StringSlice(value) => match value.as_str() {
//                         "web" => RuntimeConfig::for_html_release(),
//                         "" => {
//                             // Default backend for the project type
//                             RuntimeConfig::for_native_release()
//                         }
//                         _ => {
//                             let backend_value: &'static str = Box::leak(value.clone().into_boxed_str());
//                             return_config_error!(
//                                 format!("Invalid runtime backend: '{}'", value),
//                                 TextLocation::default(),
//                                 {
//                                     CompilationStage => "Configuration",
//                                     FoundType => backend_value,
//                                     PrimarySuggestion => "Use 'web' for HTML projects or leave empty for native",
//                                     AlternativeSuggestion => "Valid backends: 'web' (HTML/JS), '' (native default)",
//                                 }
//                             )
//                         }
//                     },
//                     _ => {
//                         let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                         return_type_error!(
//                             "Runtime backend must be a string",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "String",
//                                 FoundType => found_type,
//                                 PrimarySuggestion => "Provide a string value for runtime_backend",
//                             }
//                         )
//                     }
//                 };
//             }

//             "entry_point" => {
//                 project_config.entry_point = match &arg.value.kind {
//                     ExpressionKind::StringSlice(value) => PathBuf::from(value),
//                     _ => {
//                         let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                         return_type_error!(
//                             "Entry point must be a string",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "String",
//                                 FoundType => found_type,
//                                 PrimarySuggestion => "Provide a file path string for entry_point",
//                                 SuggestedInsertion => "entry_point = \"src/main.bst\"",
//                             }
//                         )
//                     }
//                 };
//             }

//             "src" => {
//                 project_config.src = match &arg.value.kind {
//                     ExpressionKind::StringSlice(value) => PathBuf::from(value),
//                     _ => {
//                         let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                         return_type_error!(
//                             "Source folder must be a string",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "String",
//                                 FoundType => found_type,
//                                 PrimarySuggestion => "Provide a directory path string for src",
//                                 SuggestedInsertion => "src = \"src\"",
//                             }
//                         )
//                     },
//                 };
//             }

//             "dev" => {
//                 project_config.dev_folder = match &arg.value.kind {
//                     ExpressionKind::StringSlice(value) => PathBuf::from(value),
//                     _ => {
//                         let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                         return_type_error!(
//                             "Dev folder must be a string",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "String",
//                                 FoundType => found_type,
//                                 PrimarySuggestion => "Provide a directory path string for dev",
//                                 SuggestedInsertion => "dev = \"dev\"",
//                             }
//                         )
//                     },
//                 };
//             }

//             "release" => {
//                 project_config.release_folder = match &arg.value.kind {
//                     ExpressionKind::StringSlice(value) => PathBuf::from(value),
//                     _ => {
//                         let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                         return_type_error!(
//                             "Release folder must be a string",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "String",
//                                 FoundType => found_type,
//                                 PrimarySuggestion => "Provide a directory path string for release",
//                                 SuggestedInsertion => "release = \"release\"",
//                             }
//                         )
//                     },
//                 };
//             }

//             "name" => {
//                 project_config.name = match &arg.value.kind {
//                     ExpressionKind::StringSlice(value) => value.to_owned(),
//                     _ => {
//                         let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                         return_type_error!(
//                             "Name must be a string",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "String",
//                                 FoundType => found_type,
//                                 PrimarySuggestion => "Provide a string value for name",
//                                 SuggestedInsertion => "name = \"my_project\"",
//                             }
//                         )
//                     }
//                 };
//             }

//             "version" => {
//                 project_config.version = match &arg.value.kind {
//                     ExpressionKind::StringSlice(value) => value.to_owned(),
//                     _ => {
//                         let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                         return_type_error!(
//                             "Version must be a string",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "String",
//                                 FoundType => found_type,
//                                 PrimarySuggestion => "Provide a version string",
//                                 SuggestedInsertion => "version = \"0.1.0\"",
//                             }
//                         )
//                     }
//                 };
//             }

//             "author" => {
//                 project_config.author = match &arg.value.kind {
//                     ExpressionKind::StringSlice(value) => value.to_owned(),
//                     _ => {
//                         let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                         return_type_error!(
//                             "Author must be a string",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "String",
//                                 FoundType => found_type,
//                                 PrimarySuggestion => "Provide a string value for author",
//                                 SuggestedInsertion => "author = \"Your Name\"",
//                             }
//                         )
//                     },
//                 };
//             }

//             "license" => {
//                 project_config.license = match &arg.value.kind {
//                     ExpressionKind::StringSlice(value) => value.to_owned(),
//                     _ => {
//                         let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                         return_type_error!(
//                             "License must be a string",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "String",
//                                 FoundType => found_type,
//                                 PrimarySuggestion => "Provide a license string",
//                                 SuggestedInsertion => "license = \"MIT\"",
//                             }
//                         )
//                     }
//                 };
//             }

//             "html_settings" => {
//                 match &arg.value.kind {
//                     ExpressionKind::StructInstance(args) => {
//                         for arg in args {
//                             match arg.name.as_str() {
//                                 "site_title" => {
//                                     project_config.html_meta.site_title = match &arg.value.kind {
//                                         ExpressionKind::StringSlice(value) => value.to_owned(),
//                                         _ => {
//                                             let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                             return_type_error!(
//                                                 "Site title must be a string",
//                                                 TextLocation::default(),
//                                                 {
//                                                     CompilationStage => "Configuration",
//                                                     ExpectedType => "String",
//                                                     FoundType => found_type,
//                                                     PrimarySuggestion => "Provide a string value for site_title in html_settings",
//                                                 }
//                                             )
//                                         },
//                                     };
//                                 }

//                                 "page_description" => {
//                                     project_config.html_meta.page_description =
//                                         match &arg.value.kind {
//                                             ExpressionKind::StringSlice(value) => value.to_owned(),
//                                             _ => {
//                                                 let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                                 return_type_error!(
//                                                     "Page description must be a string",
//                                                     TextLocation::default(),
//                                                     {
//                                                         CompilationStage => "Configuration",
//                                                         ExpectedType => "String",
//                                                         FoundType => found_type,
//                                                         PrimarySuggestion => "Provide a string value for page_description in html_settings",
//                                                     }
//                                                 )
//                                             },
//                                         };
//                                 }

//                                 "site_url" => {
//                                     project_config.html_meta.site_url = match &arg.value.kind {
//                                         ExpressionKind::StringSlice(value) => value.to_owned(),
//                                         _ => {
//                                             let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                             return_type_error!(
//                                                 "Site url must be a string",
//                                                 TextLocation::default(),
//                                                 {
//                                                     CompilationStage => "Configuration",
//                                                     ExpectedType => "String",
//                                                     FoundType => found_type,
//                                                     PrimarySuggestion => "Provide a string value for site_url in html_settings",
//                                                 }
//                                             )
//                                         },
//                                     };
//                                 }

//                                 "page_url" => {
//                                     project_config.html_meta.page_url = match &arg.value.kind {
//                                         ExpressionKind::StringSlice(value) => value.to_owned(),
//                                         _ => {
//                                             let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                             return_type_error!(
//                                                 "Page url must be a string",
//                                                 TextLocation::default(),
//                                                 {
//                                                     CompilationStage => "Configuration",
//                                                     ExpectedType => "String",
//                                                     FoundType => found_type,
//                                                     PrimarySuggestion => "Provide a string value for page_url in html_settings",
//                                                 }
//                                             )
//                                         },
//                                     };
//                                 }

//                                 "page_og_title" => {
//                                     project_config.html_meta.page_og_title = match &arg.value.kind {
//                                         ExpressionKind::StringSlice(value) => value.to_owned(),
//                                         _ => {
//                                             let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                             return_type_error!(
//                                                 "Page og title must be a string",
//                                                 TextLocation::default(),
//                                                 {
//                                                     CompilationStage => "Configuration",
//                                                     ExpectedType => "String",
//                                                     FoundType => found_type,
//                                                     PrimarySuggestion => "Provide a string value for page_og_title in html_settings",
//                                                 }
//                                             )
//                                         },
//                                     };
//                                 }

//                                 "page_og_description" => {
//                                     project_config.html_meta.page_og_description =
//                                         match &arg.value.kind {
//                                             ExpressionKind::StringSlice(value) => value.to_owned(),
//                                             _ => {
//                                                 let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                                 return_type_error!(
//                                                     "Page og description must be a string",
//                                                     TextLocation::default(),
//                                                     {
//                                                         CompilationStage => "Configuration",
//                                                         ExpectedType => "String",
//                                                         FoundType => found_type,
//                                                         PrimarySuggestion => "Provide a string value for page_og_description in html_settings",
//                                                     }
//                                                 )
//                                             },
//                                         };
//                                 }

//                                 "page_image_url" => {
//                                     project_config.html_meta.page_image_url = match &arg.value.kind
//                                     {
//                                         ExpressionKind::StringSlice(value) => value.to_owned(),
//                                         _ => {
//                                             let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                             return_type_error!(
//                                                 "Page image url must be a string",
//                                                 TextLocation::default(),
//                                                 {
//                                                     CompilationStage => "Configuration",
//                                                     ExpectedType => "String",
//                                                     FoundType => found_type,
//                                                     PrimarySuggestion => "Provide a string value for page_image_url in html_settings",
//                                                 }
//                                             )
//                                         },
//                                     };
//                                 }

//                                 "page_image_alt" => {
//                                     project_config.html_meta.page_image_alt = match &arg.value.kind
//                                     {
//                                         ExpressionKind::StringSlice(value) => value.to_owned(),
//                                         _ => {
//                                             let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                             return_type_error!(
//                                                 "Page image alt must be a string",
//                                                 TextLocation::default(),
//                                                 {
//                                                     CompilationStage => "Configuration",
//                                                     ExpectedType => "String",
//                                                     FoundType => found_type,
//                                                     PrimarySuggestion => "Provide a string value for page_image_alt in html_settings",
//                                                 }
//                                             )
//                                         },
//                                     };
//                                 }

//                                 "page_locale" => {
//                                     project_config.html_meta.page_locale = match &arg.value.kind {
//                                         ExpressionKind::StringSlice(value) => value.to_owned(),
//                                         _ => {
//                                             let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                             return_type_error!(
//                                                 "Page locale must be a string",
//                                                 TextLocation::default(),
//                                                 {
//                                                     CompilationStage => "Configuration",
//                                                     ExpectedType => "String",
//                                                     FoundType => found_type,
//                                                     PrimarySuggestion => "Provide a string value for page_locale in html_settings",
//                                                 }
//                                             )
//                                         },
//                                     };
//                                 }

//                                 "page_type" => {
//                                     project_config.html_meta.page_type = match &arg.value.kind {
//                                         ExpressionKind::StringSlice(value) => value.to_owned(),
//                                         _ => {
//                                             let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                             return_type_error!(
//                                                 "Page type must be a string",
//                                                 TextLocation::default(),
//                                                 {
//                                                     CompilationStage => "Configuration",
//                                                     ExpectedType => "String",
//                                                     FoundType => found_type,
//                                                     PrimarySuggestion => "Provide a string value for page_type in html_settings",
//                                                 }
//                                             )
//                                         },
//                                     };
//                                 }

//                                 "page_twitter_large_image" => {
//                                     project_config.html_meta.page_twitter_large_image =
//                                         match &arg.value.kind {
//                                             ExpressionKind::StringSlice(value) => value.to_owned(),
//                                             _ => {
//                                                 let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                                 return_type_error!(
//                                                     "Page twitter large image must be a string",
//                                                     TextLocation::default(),
//                                                     {
//                                                         CompilationStage => "Configuration",
//                                                         ExpectedType => "String",
//                                                         FoundType => found_type,
//                                                         PrimarySuggestion => "Provide a string value for page_twitter_large_image in html_settings",
//                                                     }
//                                                 )
//                                             },
//                                         };
//                                 }

//                                 "page_canonical_url" => {
//                                     project_config.html_meta.page_canonical_url =
//                                         match &arg.value.kind {
//                                             ExpressionKind::StringSlice(value) => value.to_owned(),
//                                             _ => {
//                                                 let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                                 return_type_error!(
//                                                     "Page canonical url must be a string",
//                                                     TextLocation::default(),
//                                                     {
//                                                         CompilationStage => "Configuration",
//                                                         ExpectedType => "String",
//                                                         FoundType => found_type,
//                                                         PrimarySuggestion => "Provide a string value for page_canonical_url in html_settings",
//                                                     }
//                                                 )
//                                             },
//                                         };
//                                 }

//                                 "page_root_url" => {
//                                     project_config.html_meta.page_root_url = match &arg.value.kind {
//                                         ExpressionKind::StringSlice(value) => value.to_owned(),
//                                         _ => {
//                                             let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                             return_type_error!(
//                                                 "Page root url must be a string",
//                                                 TextLocation::default(),
//                                                 {
//                                                     CompilationStage => "Configuration",
//                                                     ExpectedType => "String",
//                                                     FoundType => found_type,
//                                                     PrimarySuggestion => "Provide a string value for page_root_url in html_settings",
//                                                 }
//                                             )
//                                         },
//                                     };
//                                 }

//                                 "image_folder_url" => {
//                                     project_config.html_meta.image_folder_url =
//                                         match &arg.value.kind {
//                                             ExpressionKind::StringSlice(value) => value.to_owned(),
//                                             _ => {
//                                                 let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                                                 return_type_error!(
//                                                     "Image folder url must be a string",
//                                                     TextLocation::default(),
//                                                     {
//                                                         CompilationStage => "Configuration",
//                                                         ExpectedType => "String",
//                                                         FoundType => found_type,
//                                                         PrimarySuggestion => "Provide a string value for image_folder_url in html_settings",
//                                                     }
//                                                 )
//                                             },
//                                         };
//                                 }

//                                 _ => {
//                                     let field_name: &'static str = Box::leak(arg.name.clone().into_boxed_str());
//                                     return_type_error!(
//                                         format!("Unknown HTML setting: '{}'", arg.name),
//                                         TextLocation::default(),
//                                         {
//                                             CompilationStage => "Configuration",
//                                             VariableName => field_name,
//                                             PrimarySuggestion => "Check the field name against valid HTML settings",
//                                             AlternativeSuggestion => "Valid fields: site_title, page_description, site_url, page_url, etc.",
//                                         }
//                                     )
//                                 },
//                             }
//                         }
//                     }

//                     _ => {
//                         let found_type: &'static str = Box::leak(format!("{:?}", arg.value.kind).into_boxed_str());
//                         return_type_error!(
//                             "HTML settings must be a struct",
//                             TextLocation::default(),
//                             {
//                                 CompilationStage => "Configuration",
//                                 ExpectedType => "Struct",
//                                 FoundType => found_type,
//                                 PrimarySuggestion => "Provide a struct instance for html_settings",
//                                 SuggestedInsertion => "html_settings = { site_title = \"My Site\", ... }",
//                             }
//                         )
//                     },
//                 };
//             }

//             _ => {}
//         }

//         // if *is_exported {
//         //     exported_variables.push(Arg {
//         //         name: name.to_owned(),
//         //         data_type: data_type.to_owned(),
//         //         value: value.to_owned(),
//         //     });
//         // }
//     }

//     Ok(())
// }
