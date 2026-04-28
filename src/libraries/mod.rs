//! Library identity and shared package metadata.
//!
//! WHAT: defines library identity and shared package metadata for core,
//! builder, and source libraries.
//! WHY: separates library definition from frontend parsing and backend
//! lowering so each stage has one clear responsibility.

pub mod core;
