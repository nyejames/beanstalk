//! HIR display regression tests.
//!
//! WHAT: pins debug-display rendering for HIR-only constructs.
//! WHY: display output is used while auditing lowering and borrow behavior, so embedded message
//! text must remain unambiguous when it contains quotes or control characters.

use crate::compiler_frontend::hir::hir_display::HirDisplayContext;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::symbols::string_interning::StringTable;

#[test]
fn assertion_failure_message_display_escapes_debug_text() {
    let string_table = StringTable::new();
    let display = HirDisplayContext::new(&string_table);

    let rendered = display.render_terminator(&HirTerminator::AssertFailure {
        message: Some("quoted \"message\"\nnext".to_owned()),
    });

    assert_eq!(rendered, "assert_failure \"quoted \\\"message\\\"\\nnext\"");
}

#[test]
fn runtime_failure_message_display_escapes_debug_text() {
    let string_table = StringTable::new();
    let display = HirDisplayContext::new(&string_table);

    let rendered = display.render_terminator(&HirTerminator::RuntimeFailure {
        message: "quoted \"message\"\nnext".to_owned(),
    });

    assert_eq!(
        rendered,
        "runtime_failure \"quoted \\\"message\\\"\\nnext\""
    );
}
