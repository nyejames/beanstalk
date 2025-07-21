use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::parsers::tokens::TextLocation;
use crate::return_file_errors;
use crate::settings::HTMLMeta;
use std::fs;
use std::path::PathBuf;

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
