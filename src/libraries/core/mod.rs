//! Core library package registrations.
//!
//! WHAT: registers the builtin core packages that builders may provide.
//! WHY: keeps package definitions in one place so frontend and backend
//! can reference the same canonical metadata.

mod collections;
mod error;
mod io;
mod math;
mod prelude;
mod random;
mod text;
mod time;

/// Optional core packages that builders may expose explicitly.
///
/// WHAT: these are compiler-known package identities, but not mandatory builder surface.
/// WHY: Stage 0 can report "unsupported by builder" for these paths instead of treating them
/// as missing source files.
pub const OPTIONAL_CORE_PACKAGE_PATHS: &[&str] =
    &["@core/math", "@core/text", "@core/random", "@core/time"];

pub use collections::register_core_collections_package;
pub use error::register_core_error_package;
pub use io::register_core_io_package;
pub use math::register_core_math_package;
pub use prelude::register_core_prelude;
pub use random::register_core_random_package;
pub use text::register_core_text_package;
pub use time::register_core_time_package;
