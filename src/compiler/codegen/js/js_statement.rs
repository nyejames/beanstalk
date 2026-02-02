use crate::compiler::codegen::js::JsIdent;

/// Internal representation of simple JS statements
#[derive(Debug)]
pub enum JsStmt {
    /// `let x = expr;`
    Let { name: JsIdent, value: String },

    /// `x = expr;`
    Assign { name: JsIdent, value: String },

    /// Standalone expression statement
    Expr(String),
}
