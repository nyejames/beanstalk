use crate::compiler::codegen::js::js_statement::JsStmt;

/// Result of lowering a HIR expression to JS
///
/// JS expressions may require:
/// - a pure expression (`a + b`)
/// - or a sequence of statements + a final value
#[derive(Debug)]
pub struct JsExpr {
    /// Statements that must run before the value is available
    pub prelude: Vec<JsStmt>,

    /// JS expression string representing the value
    pub value: String,
}
