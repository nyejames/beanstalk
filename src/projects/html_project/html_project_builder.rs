// HTML/JS project builder
//
// Builds Beanstalk projects for web deployment, generating separate WASM files
// for different HTML pages and including JavaScript bindings for DOM interaction.
use crate::backends::js::{JsLoweringConfig, lower_hir_to_js};
use crate::build_system::build::{FileKind, Module, OutputFile, Project, ProjectBuilder};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::hir::hir_nodes::{HirModule, StartFragment};
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::Config;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct HtmlProjectBuilder {}

impl HtmlProjectBuilder {
    pub fn new() -> Self {
        Self {}
    }
}

impl ProjectBuilder for HtmlProjectBuilder {
    fn build_backend(
        &self,
        modules: Vec<Module>,
        _config: &Config,
        flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        let mut compiler_messages = CompilerMessages::new();
        if let Err(error) = self.validate_project_config(_config) {
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: Vec::new(),
            });
        }

        if modules.is_empty() {
            return Err(CompilerMessages {
                errors: vec![CompilerError::compiler_error(format!(
                    "HTML builder expected at least one compiled module but got {}.",
                    modules.len(),
                ))],
                warnings: Vec::new(),
            });
        }

        let release_build = flags.contains(&Flag::Release);
        let mut output_files = Vec::with_capacity(modules.len());
        let mut output_paths = HashSet::with_capacity(modules.len());

        for module in modules {
            match compile_html_module(
                &module.hir,
                &module.string_table,
                &module.entry_point,
                release_build,
            ) {
                Ok(output_file) => {
                    let output_path = output_file.relative_output_path().to_path_buf();
                    if !output_paths.insert(output_path.clone()) {
                        return Err(CompilerMessages {
                            errors: vec![CompilerError::compiler_error(format!(
                                "HTML builder produced duplicate output path '{}'. Ensure each '#*.bst' entry maps to a unique page output.",
                                output_path.display(),
                            ))],
                            warnings: Vec::new(),
                        });
                    }
                    output_files.push(output_file);
                }
                Err(error) => {
                    compiler_messages.errors.push(error);
                    return Err(compiler_messages);
                }
            }
        }

        Ok(Project {
            output_files,
            warnings: compiler_messages.warnings,
        })
    }

    fn validate_project_config(&self, _config: &Config) -> Result<(), CompilerError> {
        // Validate HTML-specific configuration

        // This used to just check that there was a dev / release folder set,
        // now we don't care
        // as not having it set means it just goes into the same directory as the entry path.

        Ok(())
    }
}

fn compile_html_module(
    hir_module: &HirModule,
    string_table: &StringTable,
    entry_point: &Path,
    release_build: bool,
) -> Result<OutputFile, CompilerError> {
    let js_lowering_config = JsLoweringConfig {
        pretty: !release_build,
        emit_locations: false,
        auto_invoke_start: false,
    };

    let js_module = lower_hir_to_js(hir_module, string_table, js_lowering_config)?;
    let html = render_html_document(
        hir_module,
        &js_module.source,
        &js_module.function_name_by_id,
    )?;

    Ok(OutputFile::new(
        html_output_path(entry_point),
        FileKind::Html(html),
    ))
}

fn html_output_path(entry_point: &Path) -> PathBuf {
    let file_stem = entry_point
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("main");

    if file_stem == "#page" {
        PathBuf::from("index.html")
    } else {
        let route_name = file_stem.strip_prefix('#').unwrap_or(file_stem);
        PathBuf::from(format!("{route_name}.html"))
    }
}

fn render_html_document(
    hir_module: &HirModule,
    js_bundle: &str,
    function_names: &std::collections::HashMap<
        crate::compiler_frontend::hir::hir_nodes::FunctionId,
        String,
    >,
) -> Result<String, CompilerError> {
    let mut html = String::new();
    let mut runtime_slots = Vec::new();
    let mut runtime_index = 0usize;

    for fragment in &hir_module.start_fragments {
        match fragment {
            StartFragment::ConstString(const_string_id) => {
                let string_index = const_string_id.0 as usize;
                let Some(const_string) = hir_module.const_string_pool.get(string_index) else {
                    return Err(CompilerError::compiler_error(format!(
                        "HTML builder could not resolve const fragment {}",
                        const_string_id.0
                    )));
                };
                // The HTML builder interprets const fragment strings as raw HTML.
                html.push_str(const_string);
                html.push('\n');
            }

            StartFragment::RuntimeStringFn(function_id) => {
                let Some(function_name) = function_names.get(function_id) else {
                    return Err(CompilerError::compiler_error(format!(
                        "HTML builder could not resolve runtime fragment function {:?}",
                        function_id
                    )));
                };
                let slot_id = format!("bst-slot-{runtime_index}");
                runtime_index += 1;
                html.push_str(&format!("<div id=\"{slot_id}\"></div>\n"));
                runtime_slots.push((slot_id, function_name.clone()));
            }
        }
    }

    html.push_str("<script>\n");
    html.push_str(js_bundle);
    html.push_str("\n</script>\n");
    html.push_str("<script>\n");
    html.push_str("(function () {\n");
    html.push_str("  const slots = [\n");
    for (slot_id, function_name) in &runtime_slots {
        let _ = writeln!(html, "    [\"{slot_id}\", {function_name}],");
    }
    html.push_str("  ];\n\n");
    // Hydrate runtime fragments in source order before running start().
    html.push_str("  for (const [id, fn] of slots) {\n");
    html.push_str("    const el = document.getElementById(id);\n");
    html.push_str("    if (!el) throw new Error(\"Missing runtime mount slot: \" + id);\n");
    html.push_str("    el.insertAdjacentHTML(\"beforeend\", fn());\n");
    html.push_str("  }\n\n");

    let Some(start_function_name) = function_names.get(&hir_module.start_function) else {
        return Err(CompilerError::compiler_error(format!(
            "HTML builder could not resolve start function {:?}",
            hir_module.start_function
        )));
    };

    let _ = writeln!(
        html,
        "  // start() remains the lifecycle entrypoint and runs after fragment hydration.\n  if (typeof {start_function_name} === \"function\") {start_function_name}();"
    );
    html.push_str("})();\n");
    html.push_str("</script>\n");

    Ok(html)
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

#[cfg(test)]
#[path = "html_project_builder_tests.rs"]
mod tests;
