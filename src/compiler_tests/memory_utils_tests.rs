//! Tests for runtime memory utilities
//!
//! These tests verify the safe reading and writing of data between WASM linear
//! memory and host functions, with proper error handling and bounds checking.

#[cfg(test)]
use crate::runtime::memory_utils::MemoryUtils;
#[cfg(test)]
use wasmer::{Memory, MemoryType, Pages, Store};

#[cfg(test)]
fn create_test_memory() -> (Store, Memory) {
    let mut store = Store::default();
    let memory_type = MemoryType::new(Pages(1), None, false);
    let memory = Memory::new(&mut store, memory_type).unwrap();
    (store, memory)
}

#[test]
fn test_read_write_string() {
    let (store, memory) = create_test_memory();
    let memory_view = memory.view(&store);

    let test_string = "Hello, Beanstalk!";
    let ptr = 100u32;

    // Write string
    let bytes_written = MemoryUtils::write_string_to_memory(&memory_view, ptr, test_string)
        .expect("Failed to write string");
    assert_eq!(bytes_written, test_string.len() as u32);

    // Read string back
    let read_string = MemoryUtils::read_string_from_memory(&memory_view, ptr, bytes_written)
        .expect("Failed to read string");
    assert_eq!(read_string, test_string);
}

#[test]
fn test_read_write_cstring() {
    let (store, memory) = create_test_memory();
    let memory_view = memory.view(&store);

    let test_string = "Hello, World!";
    let ptr = 200u32;

    // Write C-string
    let bytes_written = MemoryUtils::write_cstring_to_memory(&memory_view, ptr, test_string)
        .expect("Failed to write C-string");
    assert_eq!(bytes_written, test_string.len() as u32 + 1); // +1 for null terminator

    // Read C-string back
    let read_string = MemoryUtils::read_cstring_from_memory(&memory_view, ptr, 100)
        .expect("Failed to read C-string");
    assert_eq!(read_string, test_string);
}

#[test]
fn test_read_write_i32() {
    let (store, memory) = create_test_memory();
    let memory_view = memory.view(&store);

    let test_value = 0x12345678i32;
    let ptr = 300u32;

    // Write i32
    MemoryUtils::write_i32_to_memory(&memory_view, ptr, test_value)
        .expect("Failed to write i32");

    // Read i32 back
    let read_value =
        MemoryUtils::read_i32_from_memory(&memory_view, ptr).expect("Failed to read i32");
    assert_eq!(read_value, test_value);
}

#[test]
fn test_empty_string() {
    let (store, memory) = create_test_memory();
    let memory_view = memory.view(&store);

    let empty_string = "";
    let ptr = 400u32;

    // Write empty string
    let bytes_written = MemoryUtils::write_string_to_memory(&memory_view, ptr, empty_string)
        .expect("Failed to write empty string");
    assert_eq!(bytes_written, 0);

    // Read empty string back
    let read_string = MemoryUtils::read_string_from_memory(&memory_view, ptr, 0)
        .expect("Failed to read empty string");
    assert_eq!(read_string, empty_string);
}

#[test]
fn test_memory_bounds_checking() {
    let (store, memory) = create_test_memory();
    let memory_view = memory.view(&store);

    let memory_size = memory_view.size().bytes().0 as u32;
    println!("Memory size: {}", memory_size);

    // Try to read beyond memory bounds - ensure we actually go beyond bounds
    let result = MemoryUtils::read_string_from_memory(&memory_view, memory_size - 10, 20);
    assert!(
        result.is_err(),
        "Expected error when reading beyond memory bounds"
    );

    // Try to write beyond memory bounds - ensure we actually go beyond bounds
    let result =
        MemoryUtils::write_string_to_memory(&memory_view, memory_size - 5, "Hello World");
    assert!(
        result.is_err(),
        "Expected error when writing beyond memory bounds"
    );

    // Test exact boundary (should succeed)
    let result = MemoryUtils::read_string_from_memory(&memory_view, memory_size - 10, 10);
    assert!(
        result.is_ok(),
        "Should succeed when reading exactly at boundary"
    );
}

#[test]
fn test_validate_memory_range() {
    let (store, memory) = create_test_memory();
    let memory_view = memory.view(&store);

    // Valid range
    assert!(MemoryUtils::validate_memory_range(&memory_view, 0, 100).is_ok());

    // Empty range
    assert!(MemoryUtils::validate_memory_range(&memory_view, 100, 0).is_ok());

    // Invalid range (out of bounds)
    let memory_size = memory_view.size().bytes().0 as u32;
    assert!(MemoryUtils::validate_memory_range(&memory_view, memory_size - 10, 20).is_err());

    // Overflow
    assert!(MemoryUtils::validate_memory_range(&memory_view, u32::MAX - 10, 20).is_err());
}
