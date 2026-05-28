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
use crate::libraries::LibrarySet;
use crate::projects::settings::{self, Config};

use std::path::Path;

// -------------------------
//  Config Parse Services
// -------------------------

/// Focused frontend services passed into config parsing so `#config.bst` can import from core and
/// builder-provided libraries.
///
/// WHAT: bundles the style directives and the complete library set (external packages, source
/// libraries, and config keys) that config parsing needs.
/// WHY: `bootstrap_project_build` already computes `LibrarySet` before config loading; threading
/// it through config parsing lets imports resolve against builder/core surfaces instead of an
/// empty default registry.
pub(crate) struct ProjectConfigParseServices<'a> {
    pub style_directives: &'a StyleDirectiveRegistry,
    pub libraries: &'a LibrarySet,
}

// -------------------------
//  Public API
// -------------------------

/// Load and validate the project config from `#config.bst` before compilation begins (Stage 0).
///
/// Config files are optional. When present this delegates to the parser/validator pipeline and
/// applies all accepted settings directly to `config`.
pub fn load_project_config(
    config: &mut Config,
    services: &ProjectConfigParseServices<'_>,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    let config_path = config.entry_dir.join(settings::CONFIG_FILE_NAME);

    if !config_path.exists() {
        return Ok(());
    }

    parse_project_config_file(config, &config_path, services, string_table)
}

// -------------------------
//  Internal Orchestration
// -------------------------

/// Parse `#config.bst` and extract top-level constant declarations into the `Config` struct.
///
/// WHY: config uses normal Beanstalk syntax, so Stage 0 keeps the tokenizer/header parser in the
/// loop and then applies a dedicated config-only validation pass.
pub(crate) fn parse_project_config_file(
    config: &mut Config,
    config_path: &Path,
    services: &ProjectConfigParseServices<'_>,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    // 1. Run the specialized config parser.
    let mut parsed_config = parsing::parse_config_file(config_path, services, string_table)?;
    let mut errors = std::mem::take(&mut parsed_config.errors);

    // 2. Validate and apply the folded AST to the live Config object.
    if let Err(mut validation_errors) = validation::validate_and_apply_config_ast(
        config,
        &parsed_config,
        &services.libraries.config_keys,
        string_table,
    ) {
        errors.append(&mut validation_errors);
    }

    // 3. Aggregate all errors into one CompilerMessages payload.
    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompilerMessages::from_diagnostics(
            errors,
            string_table.clone(),
        ))
    }
}
