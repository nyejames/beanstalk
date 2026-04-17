//! Stage 0 config loading, parsing, and validation for Beanstalk projects.
//!
//! WHAT: owns the public entry points for loading `#config.bst` before compilation starts.
//! WHY: callers only need one stable surface while parsing and validation details stay split by
//! concern in dedicated helpers.

mod parsing;
mod validation;

use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::{self, Config};
use std::path::Path;

/// Load and validate the project config from `#config.bst` before compilation begins (Stage 0).
///
/// Config files are optional. When present this delegates to the parser/validator pipeline and
/// applies all accepted settings directly to `config`.
pub fn load_project_config(
    config: &mut Config,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    let config_path = config.entry_dir.join(settings::CONFIG_FILE_NAME);

    if !config_path.exists() {
        return Ok(());
    }

    parse_project_config_file(config, &config_path, style_directives, string_table)
}

/// Parse `#config.bst` and extract top-level constant declarations into the `Config` struct.
///
/// WHY: config uses normal Beanstalk syntax, so Stage 0 keeps the tokenizer/header parser in the
/// loop and then applies a dedicated config-only validation pass.
pub(crate) fn parse_project_config_file(
    config: &mut Config,
    config_path: &Path,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    let parsed_config = parsing::parse_config_file(config_path, style_directives, string_table)?;
    let mut errors = parsed_config.errors;

    if let Err(mut validation_errors) = validation::validate_and_apply_config_headers(
        config,
        &parsed_config.headers,
        string_table,
        config_path,
    ) {
        errors.append(&mut validation_errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompilerMessages {
            errors,
            warnings: Vec::new(),
            string_table: string_table.clone(),
        })
    }
}
