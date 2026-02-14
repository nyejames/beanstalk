//! Shared Constants for WASM Codegen
//!
//! This module centralizes all constants used across the WASM codegen system.
//! Having constants in one place ensures consistency and makes maintenance easier.

// =========================================================================
// Ownership System Constants
// These constants implement Beanstalk's tagged pointer ownership system
// =========================================================================

/// The ownership bit value (1 = owned, 0 = borrowed).
///
/// In Beanstalk's memory model, all heap-allocated values are passed as tagged
/// pointers where the lowest alignment-safe bit indicates ownership:
/// - `1` = owned (callee must drop before returning)
/// - `0` = borrowed (callee must not drop)
pub const OWNERSHIP_BIT: i32 = 1;

/// The mask for clearing the ownership bit (0xFFFFFFFE).
///
/// Used to extract the real pointer address from a tagged pointer by
/// masking out the ownership bit: `real_ptr = tagged_ptr & ALIGNMENT_MASK`
pub const ALIGNMENT_MASK: i32 = !1;

// =========================================================================
// Memory Configuration Constants
// These constants configure WASM linear memory layout
// =========================================================================

/// Default minimum memory pages (64KB each).
///
/// WASM memory is allocated in pages of 64KB. This is the minimum number
/// of pages allocated when a module is instantiated.
pub const DEFAULT_MIN_PAGES: u32 = 1;

/// Default maximum memory pages (4GB max for 32-bit WASM).
///
/// This is the maximum number of pages the memory can grow to.
/// 65536 pages * 64KB = 4GB (the maximum addressable by 32-bit pointers).
pub const DEFAULT_MAX_PAGES: u32 = 65536;

/// Initial heap start offset (after reserved space).
///
/// We reserve the first 64KB (1 page) for safety and static data.
/// The heap starts at this offset and grows upward.
pub const HEAP_START_OFFSET: i32 = 65536;

/// Minimum allocation alignment for tagged pointers.
///
/// All heap allocations must be at least 2-byte aligned to ensure
/// the lowest bit is available for the ownership tag.
pub const MIN_ALLOCATION_ALIGNMENT: u32 = 2;

// =========================================================================
// Memory Index Constants
// Default indices for memory-related sections
// =========================================================================

/// Default memory index (WASM modules typically have one memory at index 0).
pub const DEFAULT_MEMORY_INDEX: u32 = 0;

// =========================================================================
// Alignment Constants for WASM Types
// Natural alignment values (as log2) for MemArg
// =========================================================================

/// Natural alignment for i32/f32 (log2(4) = 2).
pub const ALIGN_32: u32 = 2;

/// Natural alignment for i64/f64 (log2(8) = 3).
pub const ALIGN_64: u32 = 3;

/// Byte alignment (log2(1) = 0).
pub const ALIGN_8: u32 = 0;

/// 16-bit alignment (log2(2) = 1).
pub const ALIGN_16: u32 = 1;
