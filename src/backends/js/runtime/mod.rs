//! JS runtime helper emission.
//!
//! This module emits the JS helper functions that implement Beanstalk's runtime
//! semantics. All helper groups are declared as JS `function` declarations, which means
//! JS hoisting guarantees correct behaviour regardless of emission order.
//!
//! The top-level [`JsEmitter::emit_runtime_prelude`] only owns:
//! - helper emission order
//! - high-level comments about why these groups exist
//! - any tiny shared glue that genuinely belongs at orchestration level
//!
//! Individual helper groups live in focused submodules so semantic auditing and
//! targeted refactors are easier than with a single monolithic prelude file.

mod aliasing;
mod bindings;
mod casts;
mod cloning;
mod collections;
mod errors;
mod places;
mod results;
mod strings;

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    /// Emits the full JS runtime prelude.
    ///
    /// The JS backend preserves Beanstalk's aliasing semantics by modeling locals and computed
    /// places as explicit reference records. The prelude is the concrete JS model for those
    /// semantics — it is not incidental helper code.
    ///
    /// Helper groups and their responsibilities:
    ///   binding helpers         — reference record construction, parameter normalisation, slot
    ///                             read/write, and alias-chain resolution
    ///   alias helpers           — binding-mode transitions for borrow and value assignment
    ///   computed-place helpers  — closures capturing base reference + key for field/index access
    ///   clone helpers           — deep value copy for explicit `copy` semantics
    ///   error helpers           — normalises file paths, constructs canonical error records
    ///   result helpers          — `?` propagation and `or` fallback helpers
    ///   collection helpers      — guarded get/push/remove/length for ordered collections
    ///   string helpers          — value-to-string conversion and IO output
    ///   cast helpers            — numeric and string casting with Result-typed errors
    ///
    /// All groups use JS `function` declarations, which are hoisted by the JS engine.
    /// Ordering here is for readability only; correctness does not depend on it.
    pub(crate) fn emit_runtime_prelude(&mut self) {
        self.emit_runtime_binding_helpers();
        self.emit_runtime_alias_helpers();
        self.emit_runtime_computed_place_helpers();
        self.emit_runtime_clone_helpers();
        self.emit_runtime_error_helpers();
        self.emit_runtime_result_helpers();
        self.emit_runtime_collection_helpers();
        self.emit_runtime_string_helpers();
        self.emit_runtime_cast_helpers();
    }
}
