//! Scoped-block parsing regression tests.
//!
//! WHAT: validates `block:` statement parsing, local shadowing rules, and block-boundary
//!       diagnostics.
//! WHY: blocks create explicit lexical regions; parser drift here affects borrow scope and
//!      local lifetime analysis.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
#[cfg(any(feature = "async_blocks", feature = "checked_blocks"))]
use crate::compiler_frontend::compiler_messages::DeferredFeatureReason;
use crate::compiler_frontend::compiler_messages::{DiagnosticPayload, ReservedNameOwner};
use crate::compiler_frontend::tests::ast_fixture_support::start_function_body;
use crate::compiler_frontend::tests::parse_support::{
    parse_single_file_ast, parse_single_file_ast_diagnostic,
};

#[test]
fn parses_keyword_scoped_block_as_own_ast_node() {
    let (ast, string_table) = parse_single_file_ast("block:\n    value = \"inside\"\n;\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::ScopedBlock { body: block_body } = &body[0].kind else {
        panic!("expected scoped block node");
    };

    assert_eq!(block_body.len(), 1);
    assert!(matches!(
        block_body[0].kind,
        NodeKind::VariableDeclaration(_)
    ));
}

#[test]
fn rejects_block_keyword_as_declaration_name() {
    let diagnostic = parse_single_file_ast_diagnostic("block = 1\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::ReservedNameCollision {
            reserved_by: ReservedNameOwner::Keyword,
            ..
        }
    ));
}

#[cfg(feature = "checked_blocks")]
#[test]
fn checked_block_feature_still_reports_unimplemented() {
    let diagnostic = parse_single_file_ast_diagnostic("checked:\n    value = 1\n;\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::DeferredFeature {
            reason: DeferredFeatureReason::CheckedBlock
        }
    ));
}

#[cfg(feature = "async_blocks")]
#[test]
fn async_block_feature_still_reports_unimplemented() {
    let diagnostic = parse_single_file_ast_diagnostic("async:\n    value = 1\n;\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::DeferredFeature {
            reason: DeferredFeatureReason::AsyncBlock
        }
    ));
}
