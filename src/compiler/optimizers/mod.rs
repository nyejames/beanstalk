/// Optimization modules for the Beanstalk compiler
///
/// This module contains optimization systems that are not part of the core
/// compilation pipeline. These optimizations can be added incrementally
/// after the basic functionality is working.
///
/// ## Organization
/// - `constant_folding`: Compile-time constant evaluation
/// - `optimized_dataflow`: Struct-of-arrays dataflow analysis optimizations
/// - `streamlined_diagnostics`: Fast-path error generation for borrow checking
/// - `place_interner`: Place ID interning for memory and performance optimization
///
/// ## Usage
/// These modules are intended to be used as optional optimizations that can
/// be enabled or disabled based on compilation requirements. The core compiler
/// should work without these optimizations.

pub mod constant_folding;
pub mod optimized_dataflow;
pub mod place_interner;
pub mod streamlined_diagnostics;