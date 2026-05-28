//! Optional custom name-section emission.
//!
//! The current emitter keeps this lightweight: it emits a valid `name` custom-section envelope so
//! debug tooling can detect that names were intentionally requested.

use wasm_encoder::CustomSection;

pub(crate) fn build_name_custom_section() -> CustomSection<'static> {
    // WHAT: emit a valid `name` custom-section envelope.
    // WHY: keeping this as a minimal stub lets tooling detect the intent now while leaving
    // detailed function/local naming for later Wasm maturity work.
    CustomSection {
        name: "name".into(),
        data: Vec::<u8>::new().into(),
    }
}
