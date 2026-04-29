//! JavaScript implementations of builder-provided external libraries.
//!
//! WHAT: maps external package lowering metadata to concrete JS helper emission.
//! WHY: backend-specific library code belongs beside the JS backend, while shared package
//! identity and signatures stay in `src/libraries`.

mod core;
