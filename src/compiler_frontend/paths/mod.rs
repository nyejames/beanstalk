//! Compile-time path parsing, resolution, and usage-tracking modules.
//!
//! WHAT: normalizes language path syntax into canonical/public/runtime forms for frontend stages.

pub(crate) mod path_format;
pub(crate) mod path_resolution;
pub(crate) mod paths;
pub(crate) mod rendered_path_usage;
