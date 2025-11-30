//! HIR builder (scaffold)
//!
//! Converts AST into a structured HIR representation. For now, this is a
//! minimal placeholder that returns an empty module, so the pipeline can be
//! wired up incrementally.

use crate::compiler::hir::nodes::HirModule;
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::compiler_messages::compiler_warnings::CompilerWarning;
use crate::compiler::string_interning::StringTable;
use crate::hir_log;

/// Build a HIR module from the AST.
///
/// This is a scaffold: it currently returns an empty module.
///
/// Note: HIR needs access to the StringTable and a warnings sink, and should
/// return CompilerMessages so it can surface warnings alongside errors.
pub fn build_hir(
    _ast: &crate::compiler::parsers::ast::Ast,
    _string_table: &mut StringTable,
    _warnings: &mut Vec<CompilerWarning>,
) -> Result<HirModule, CompilerMessages> {
    // macro exported at crate root
    hir_log!("build_hir(): starting placeholder HIR construction");
    Ok(HirModule::default())
}
