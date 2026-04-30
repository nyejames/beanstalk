//! JavaScript helpers for `@core/math`.
//!
//! WHAT: previously wrapped JavaScript `Math` methods for `@core/math` external functions.
//! WHY: all current `@core/math` functions use `InlineExpression` lowering, so no runtime
//! helpers are emitted. This module is retained as a structural placeholder in case future
//! math surfaces need helper-based lowering.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_core_math_helpers(&mut self) {
        // All current @core/math functions lower via InlineExpression.
        // No runtime helpers are needed.
    }
}
