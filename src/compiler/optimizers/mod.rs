/// Optimization modules for the Beanstalk compiler
///
/// This module contains optimization systems that are not part of the core
/// compilation pipeline. These optimizations can be added incrementally
/// after the basic functionality is working.
///
/// ## Organization
/// - `constant_folding`: Compile-time constant evaluation
///
/// ## Usage
/// These modules are intended to be used as optional optimizations that can
/// be enabled or disabled based on compilation requirements. The core compiler
/// should work without these optimizations.

pub mod constant_folding;