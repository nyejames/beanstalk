use crate::compiler::compiler_errors::CompileError;
use crate::runtime::io::js_bindings::JsBindingsGenerator;
use crate::settings::HTMLMeta;
use std::fs;

pub fn create_html_boilerplate(
    meta_tags: &HTMLMeta,
    release_build: bool,
) -> Result<String, CompileError> {
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

        Err(ref err) => Err(CompileError::compiler_error(&format!(
            "Error reading boilerplate HTML file: {:?}",
            err
        ))),
    }
}

/// Create the HTML boilerplate with integrated JS bindings for WASM
pub fn create_html_with_js_bindings(
    meta_tags: &HTMLMeta,
    wasm_module_name: &str,
    release_build: bool,
) -> Result<String, CompileError> {
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
