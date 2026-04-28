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

pub use collections::register_core_collections_package;
pub use error::register_core_error_package;
pub use io::register_core_io_package;
pub use math::register_core_math_package;
pub use prelude::register_core_prelude;
