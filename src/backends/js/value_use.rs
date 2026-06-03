//! Value-use context lowering for the JavaScript backend.
//!
//! WHAT: centralizes how HIR `Load` and `Copy` expressions are lowered depending on the JS
//! consumption context.
//! WHY: Beanstalk calls, host/external calls, assignments, returns, and plain expressions each
//! use a different value policy. Explicit contexts prevent duplicated `Load`/`Copy` branches
//! across expression and statement lowering, and make the ABI boundary between Beanstalk
//! reference bindings and raw JS values explicit.

use crate::backends::js::JsEmitter;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind};

/// Context in which a lowered JS expression will be consumed.
///
/// WHAT: names the value-use site so the emitter can apply the correct ABI policy.
/// WHY: Beanstalk functions speak a reference ABI (places are passed as binding refs, rvalues
/// are wrapped in `__bs_binding`), while host/external JS calls cross into raw JS and must
/// receive concrete values without binding wrappers. Returns may preserve alias references.
/// Assignments need concrete values suitable for write-through or rebinding.
pub(crate) enum JsValueUse {
    /// Ordinary expression position: produces a concrete JS value.
    PlainExpression,

    /// Assignment or write target: produces the concrete JS value to store.
    AssignmentValue,

    /// Argument to a Beanstalk (user-defined or source-library) function call.
    /// Places are passed as binding references; rvalues are wrapped in `__bs_binding(...)`.
    BeanstalkCallArgument,

    /// Argument to a host or external JS function call.
    /// These cross the Beanstalk-reference ABI boundary and receive raw JS values.
    HostCallArgument,

    /// Return value from a function.
    /// Preserves alias-return behavior: `Load` passes the binding ref, `Copy` clones.
    ReturnValue,
}

impl<'hir> JsEmitter<'hir> {
    /// Lower a HIR expression according to the consumption context.
    ///
    /// WHAT: selects the correct lowering for `Load`, `Copy`, and recursive contexts
    /// (such as tuple returns) based on how the resulting JS value will be used.
    pub(crate) fn lower_expression_for_use(
        &mut self,
        expression: &HirExpression,
        use_context: JsValueUse,
    ) -> Result<String, CompilerError> {
        match use_context {
            JsValueUse::PlainExpression
            | JsValueUse::AssignmentValue
            | JsValueUse::HostCallArgument => self.lower_concrete_value(expression),

            JsValueUse::BeanstalkCallArgument => self.lower_call_argument_value(expression),

            JsValueUse::ReturnValue => self.lower_return_value(expression),
        }
    }

    fn lower_concrete_value(
        &mut self,
        expression: &HirExpression,
    ) -> Result<String, CompilerError> {
        match &expression.kind {
            HirExpressionKind::Load(place) => {
                Ok(format!("__bs_read({})", self.lower_place(place)?))
            }
            HirExpressionKind::Copy(place) => Ok(format!(
                "__bs_clone_value(__bs_read({}))",
                self.lower_place(place)?
            )),
            _ => self.lower_expr(expression),
        }
    }

    fn lower_call_argument_value(
        &mut self,
        expression: &HirExpression,
    ) -> Result<String, CompilerError> {
        match &expression.kind {
            HirExpressionKind::Load(place) => self.lower_place(place),
            HirExpressionKind::Copy(place) => Ok(format!(
                "__bs_binding(__bs_clone_value(__bs_read({})))",
                self.lower_place(place)?
            )),
            _ => Ok(format!("__bs_binding({})", self.lower_expr(expression)?)),
        }
    }

    fn lower_return_value(&mut self, expression: &HirExpression) -> Result<String, CompilerError> {
        match &expression.kind {
            HirExpressionKind::Load(place) => self.lower_place(place),
            HirExpressionKind::Copy(place) => Ok(format!(
                "__bs_clone_value(__bs_read({}))",
                self.lower_place(place)?
            )),
            HirExpressionKind::TupleConstruct { elements } => {
                let lowered = elements
                    .iter()
                    .map(|element| self.lower_return_value(element))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("[{}]", lowered.join(", ")))
            }
            _ => self.lower_expr(expression),
        }
    }
}
