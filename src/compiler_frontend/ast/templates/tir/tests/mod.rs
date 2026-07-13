//! TIR unit tests.
//!
//! WHAT: exercises the typed IDs, store, summary, validation and builder types
//! used by parser-emitted TIR.
//! WHY: TIR is a new internal representation; keeping its unit tests in a separate
//!      module follows the project rule that tests do not live in production files.

use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template_folding::TemplateEmission;

fn assert_slot_insert_fold_error(result: Result<TemplateEmission, TemplateError>) {
    let error = result.expect_err("escaped slot insertion should fail folding");
    let TemplateError::Infrastructure(error) = error else {
        panic!("escaped slot insertion should produce an infrastructure fold error");
    };
    assert!(
        error
            .msg
            .contains("unresolved slot insertions cannot be rendered directly"),
        "slot insertion should preserve the fold-specific diagnostic, got: {}",
        error.msg
    );
}

mod body_root_wrapper_tests;
mod builder_tests;
mod classification_tests;
mod cross_store_fold_tests;
mod expression_payload_walker_tests;
mod fold_cache_tests;
mod fold_final_view_tests;
mod foreign_slot_insert_proxy_tests;
mod hir_handoff_tests;
mod ids_tests;
mod overlays_tests;
mod refs_tests;
mod registry_tests;
mod render_unit_tests;
mod slot_composition_tests;
mod store_tests;
mod summary_tests;
mod template_reference_tests;
mod validation_tests;
mod view_tests;
mod wrapper_context_fold_tests;
