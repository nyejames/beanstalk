use crate::compiler::compiler_errors::CompileError;
use crate::return_file_errors;
use crate::runtime::io::js_bindings::JsBindingsGenerator;
use crate::settings::HTMLMeta;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code)]
pub fn create_html_boilerplate(
    meta_tags: &HTMLMeta,
    release_build: bool,
) -> Result<String, Vec<CompileError>> {
    // Add basic HTML boilerplate to output
    let file = match release_build {
        true => fs::read_to_string("/boilerplate-release.html"),
        false => fs::read_to_string("/boilerplate.html"),
    };

    match file {
        Ok(html) => Ok(html
            .replace("page-description", &meta_tags.page_description)
            .replace("site-url", &meta_tags.site_url)
            .replace("page-url", &meta_tags.page_url)
            .replace("page-og-title", &meta_tags.page_og_title)
            .replace("page-og-description", &meta_tags.page_og_description)
            .replace("page-image-url", &meta_tags.page_image_url)
            .replace("page-image-alt", &meta_tags.page_image_alt)
            .replace("page-locale", &meta_tags.page_locale)
            .replace("page-type", &meta_tags.page_type)
            .replace(
                "page-twitter-large-image",
                &meta_tags.page_twitter_large_image,
            )
            .replace("page-dist-url/", &meta_tags.page_root_url)
            .replace("page-canonical-url", &meta_tags.page_canonical_url)
            .replace("site-favicons-folder-url", &meta_tags.favicons_folder_url)
            .replace("theme-color-light", &meta_tags.theme_color_light)
            .replace("theme-color-dark", &meta_tags.theme_color_dark)),

        Err(ref err) => return_file_errors!(
            PathBuf::new(),
            "Error reading boilerplate HTML file: {:?}",
            err
        ),
    }
}

/// Create HTML boilerplate with integrated JS bindings for WASM
#[allow(dead_code)]
pub fn create_html_with_js_bindings(
    meta_tags: &HTMLMeta,
    wasm_module_name: &str,
    release_build: bool,
) -> Result<String, Vec<CompileError>> {
    // Generate the JS bindings
    let generator = JsBindingsGenerator::new(wasm_module_name.to_string())
        .with_dom_functions(true)
        .with_dev_features(!release_build);

    let js_bindings = generator.generate_js_bindings();

    // Get the base HTML boilerplate
    let base_html = create_html_boilerplate(meta_tags, release_build)?;

    // Replace the JS modules placeholder with our comprehensive JS bindings
    let html_with_js = base_html
        .replace(
            "<!--//js-modules-->",
            &format!("<script type=\"module\">\n{}\n</script>", js_bindings),
        )
        .replace("wasm-module-name", wasm_module_name);

    Ok(html_with_js)
}

/// Create a standalone HTML file with embedded JS bindings (for simple projects)
#[allow(dead_code)]
pub fn create_standalone_html(title: &str, wasm_module_name: &str, release_build: bool) -> String {
    let generator = JsBindingsGenerator::new(wasm_module_name.to_string())
        .with_dom_functions(true)
        .with_dev_features(!release_build);

    let js_bindings = generator.generate_js_bindings();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 0;
            padding: 20px;
            background-color: #f5f5f5;
        }}
        #app {{
            max-width: 800px;
            margin: 0 auto;
            background: white;
            padding: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
        }}
        .loading {{
            text-align: center;
            color: #666;
            font-style: italic;
        }}
        .error {{
            color: #d32f2f;
            background: #ffebee;
            padding: 10px;
            border-radius: 4px;
            margin: 10px 0;
        }}
    </style>
</head>
<body>
    <div id="app">
        <div class="loading">Loading Beanstalk application...</div>
    </div>
    
    <script type="module">
        {}
    </script>
</body>
</html>"#,
        title, js_bindings
    )
}
