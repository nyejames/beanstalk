//! JavaScript helper emission for optional `@core/*` libraries.
//!
//! WHAT: emits JS helpers only for referenced core external functions.
//! WHY: optional core libraries are builder-provided surface; keeping helper emission here
//! prevents the generic runtime prelude from becoming a library implementation dump.

mod math;
mod random;
mod text;
mod time;

use crate::backends::js::JsEmitter;
use crate::compiler_frontend::external_packages::ExternalJsLowering;

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_core_library_helpers(&mut self) {
        self.emit_core_math_helpers();
        self.emit_core_text_helpers();
        self.emit_core_random_helpers();
        self.emit_core_time_helpers();
    }

    pub(super) fn referenced_external_runtime_function(&self, js_name: &str) -> bool {
        self.referenced_external_functions.iter().any(|id| {
            self.config
                .external_package_registry
                .get_function_by_id(*id)
                .and_then(|def| def.lowerings.js.as_ref())
                .is_some_and(|lowering| {
                    matches!(lowering, ExternalJsLowering::RuntimeFunction(name) if *name == js_name)
                })
        })
    }

    pub(super) fn emit_referenced_core_helpers(&mut self, helpers: &[(&str, &str)]) {
        for (js_name, body) in helpers {
            if self.referenced_external_runtime_function(js_name) {
                self.emit_line(body);
            }
        }
    }
}
