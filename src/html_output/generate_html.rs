use crate::settings::HTMLMeta;
use std::fs;
use crate::CompileError;

pub fn create_html_boilerplate(meta_tags: &HTMLMeta, release_build: bool) -> Result<String, CompileError> {

    // Add basic HTML boilerplate to output
    let file = match release_build {
        true => fs::read_to_string("src/html_output/boilerplate-release.html"),
        false => fs::read_to_string("src/html_output/boilerplate.html"),
    };

    match file {
        Ok(html) => {
            Ok(html
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
                .replace("theme-color-dark", &meta_tags.theme_color_dark)
            )
        }

        Err(err) => {
            Err(CompileError {
                msg: format!("Error reading boilerplate HTML file: {:?}", err),
                line_number: 0,
            })
        }
    }

}
