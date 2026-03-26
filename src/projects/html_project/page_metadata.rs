//! Entry-page metadata extraction for the HTML builder.
//!
//! WHAT: reads a strict reserved subset of top-level module constants from HIR metadata.
//! WHY: page metadata should stay builder-local and deterministic without introducing new
//!      language surface area or hidden imports.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation, ErrorType};
use crate::compiler_frontend::hir::hir_nodes::{HirConstValue, HirModule};
use crate::compiler_frontend::string_interning::StringTable;
use std::path::PathBuf;

const PAGE_TITLE: &str = "page_title";
const PAGE_DESCRIPTION: &str = "page_description";
const PAGE_LANG: &str = "page_lang";
const PAGE_FAVICON: &str = "page_favicon";
const PAGE_BODY_STYLE: &str = "page_body_style";
const PAGE_HEAD: &str = "page_head";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HtmlPageMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub lang: Option<String>,
    pub favicon: Option<String>,
    pub body_style: Option<String>,
    pub head_html: Option<String>,
}

pub(crate) fn extract_html_page_metadata(
    hir_module: &HirModule,
    string_table: &StringTable,
) -> Result<HtmlPageMetadata, CompilerError> {
    let entry_scope = hir_module
        .side_table
        .function_name_path(hir_module.start_function)
        .and_then(|path| path.parent());

    let entry_scope_prefix = entry_scope
        .as_ref()
        .map(|path| path.to_portable_string(string_table));

    let error_location = entry_scope
        .as_ref()
        .map(|path| {
            ErrorLocation::new(
                path.to_path_buf(string_table),
                Default::default(),
                Default::default(),
            )
        })
        .unwrap_or_else(|| {
            ErrorLocation::new(
                PathBuf::from("#page.bst"),
                Default::default(),
                Default::default(),
            )
        });

    let mut metadata = HtmlPageMetadata::default();

    for module_constant in &hir_module.module_constants {
        let Some(reserved_name) =
            reserved_metadata_name(&module_constant.name, entry_scope_prefix.as_deref())
        else {
            continue;
        };

        let value = match &module_constant.value {
            HirConstValue::String(value) => value.to_owned(),
            _ => {
                return Err(metadata_error(
                    &error_location,
                    reserved_name,
                    "must fold to a string",
                ));
            }
        };

        let target_slot = match reserved_name {
            PAGE_TITLE => &mut metadata.title,
            PAGE_DESCRIPTION => &mut metadata.description,
            PAGE_LANG => &mut metadata.lang,
            PAGE_FAVICON => &mut metadata.favicon,
            PAGE_BODY_STYLE => &mut metadata.body_style,
            PAGE_HEAD => &mut metadata.head_html,
            _ => continue,
        };

        if target_slot.is_some() {
            return Err(metadata_error(
                &error_location,
                reserved_name,
                "is declared more than once for this entry page",
            ));
        }

        *target_slot = Some(value);
    }

    Ok(metadata)
}

fn reserved_metadata_name<'a>(
    raw_name: &'a str,
    entry_scope_prefix: Option<&str>,
) -> Option<&'a str> {
    if is_reserved_page_key(raw_name) {
        return Some(raw_name);
    }

    let entry_scope_prefix = entry_scope_prefix?;
    let leaf_name = raw_name
        .strip_prefix(entry_scope_prefix)?
        .strip_prefix('/')?;
    is_reserved_page_key(leaf_name).then_some(leaf_name)
}

fn is_reserved_page_key(name: &str) -> bool {
    matches!(
        name,
        PAGE_TITLE | PAGE_DESCRIPTION | PAGE_LANG | PAGE_FAVICON | PAGE_BODY_STYLE | PAGE_HEAD
    )
}

fn metadata_error(location: &ErrorLocation, key: &str, reason: &str) -> CompilerError {
    let mut error = CompilerError::new(
        format!("Reserved HTML page metadata constant '{key}' {reason}."),
        location.clone(),
        ErrorType::Rule,
    );
    error.metadata.insert(
        crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
        format!("Declare '# {key} = \"...\"' as a compile-time string in the entry module"),
    );
    error
}

#[cfg(test)]
#[path = "tests/page_metadata_tests.rs"]
mod tests;
