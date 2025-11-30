// Memory utilities for host function implementations
//
// Provides safe and efficient utilities for reading and writing data between
// WASM linear memory and host functions, with proper error handling.

use crate::compiler::compiler_errors::CompilerError;
use wasmer::MemoryView;

/// Utilities for reading and writing WASM memory from host functions
pub struct MemoryUtils;

impl MemoryUtils {
    /// Read a string from WASM memory using pointer and length
    ///
    /// # Arguments
    /// * `memory` - The WASM memory view
    /// * `ptr` - Pointer to the start of the string in WASM memory
    /// * `len` - Length of the string in bytes
    ///
    /// # Returns
    /// * `Ok(String)` - The decoded UTF-8 string
    /// * `Err(CompileError)` - If memory access fails or string is invalid UTF-8
    pub fn read_string_from_memory(
        memory: &MemoryView,
        ptr: u32,
        len: u32,
    ) -> Result<String, CompilerError> {
        // Validate parameters
        if len == 0 {
            return Ok(String::new());
        }

        // Check for potential overflow
        let end_ptr = ptr.checked_add(len).ok_or_else(|| {
            CompilerError::compiler_error(&format!(
                "Memory access overflow: ptr={}, len={}",
                ptr, len
            ))
        })?;

        // Check memory bounds
        let memory_size = memory.size().bytes().0 as u32;
        if end_ptr > memory_size {
            return Err(CompilerError::compiler_error(&format!(
                "Memory access out of bounds: trying to read {}..{}, memory size is {}",
                ptr, end_ptr, memory_size
            )));
        }

        // Read bytes from memory
        let mut bytes = vec![0u8; len as usize];
        memory.read(ptr as u64, &mut bytes).map_err(|e| {
            CompilerError::compiler_error(&format!(
                "Failed to read from WASM memory at {}: {}",
                ptr, e
            ))
        })?;

        // Convert to UTF-8 string
        String::from_utf8(bytes).map_err(|e| {
            CompilerError::compiler_error(&format!(
                "Invalid UTF-8 string in WASM memory at {}: {}",
                ptr, e
            ))
        })
    }

    /// Write a string to WASM memory at the specified location
    ///
    /// # Arguments
    /// * `memory` - The WASM memory view
    /// * `ptr` - Pointer to write the string to
    /// * `data` - The string data to write
    ///
    /// # Returns
    /// * `Ok(u32)` - Number of bytes written
    /// * `Err(CompileError)` - If memory access fails
    pub fn write_string_to_memory(
        memory: &MemoryView,
        ptr: u32,
        data: &str,
    ) -> Result<u32, CompilerError> {
        let bytes = data.as_bytes();
        let len = bytes.len() as u32;

        if len == 0 {
            return Ok(0);
        }

        // Check for potential overflow
        let end_ptr = ptr.checked_add(len).ok_or_else(|| {
            CompilerError::compiler_error(&format!(
                "Memory write overflow: ptr={}, len={}",
                ptr, len
            ))
        })?;

        // Check memory bounds
        let memory_size = memory.size().bytes().0 as u32;
        if end_ptr > memory_size {
            return Err(CompilerError::compiler_error(&format!(
                "Memory write out of bounds: trying to write {}..{}, memory size is {}",
                ptr, end_ptr, memory_size
            )));
        }

        // Write bytes to memory
        memory.write(ptr as u64, bytes).map_err(|e| {
            CompilerError::compiler_error(&format!(
                "Failed to write to WASM memory at {}: {}",
                ptr, e
            ))
        })?;

        Ok(len)
    }

    /// Read a null-terminated string from WASM memory
    ///
    /// # Arguments
    /// * `memory` - The WASM memory view
    /// * `ptr` - Pointer to the start of the null-terminated string
    /// * `max_len` - Maximum length to read (safety limit)
    ///
    /// # Returns
    /// * `Ok(String)` - The decoded UTF-8 string (without null terminator)
    /// * `Err(CompileError)` - If memory access fails or no null terminator found
    pub fn read_cstring_from_memory(
        memory: &MemoryView,
        ptr: u32,
        max_len: u32,
    ) -> Result<String, CompilerError> {
        let memory_size = memory.size().bytes().0 as u32;

        // Find the null terminator
        let mut len = 0u32;
        while len < max_len && (ptr + len) < memory_size {
            let mut byte = [0u8; 1];
            memory.read((ptr + len) as u64, &mut byte).map_err(|e| {
                CompilerError::compiler_error(&format!(
                    "Failed to read from WASM memory at {}: {}",
                    ptr + len,
                    e
                ))
            })?;

            if byte[0] == 0 {
                break;
            }
            len += 1;
        }

        if len == max_len {
            return Err(CompilerError::compiler_error(&format!(
                "No null terminator found within {} bytes at {}",
                max_len, ptr
            )));
        }

        // Read the string (excluding null terminator)
        Self::read_string_from_memory(memory, ptr, len)
    }

    /// Write a null-terminated string to WASM memory
    ///
    /// # Arguments
    /// * `memory` - The WASM memory view
    /// * `ptr` - Pointer to write the string to
    /// * `data` - The string data to write (null terminator will be added)
    ///
    /// # Returns
    /// * `Ok(u32)` - Number of bytes written (including null terminator)
    /// * `Err(CompileError)` - If memory access fails
    pub fn write_cstring_to_memory(
        memory: &MemoryView,
        ptr: u32,
        data: &str,
    ) -> Result<u32, CompilerError> {
        let mut bytes = data.as_bytes().to_vec();
        bytes.push(0); // Add null terminator
        let len = bytes.len() as u32;

        // Check for potential overflow
        let end_ptr = ptr.checked_add(len).ok_or_else(|| {
            CompilerError::compiler_error(&format!(
                "Memory write overflow: ptr={}, len={}",
                ptr, len
            ))
        })?;

        // Check memory bounds
        let memory_size = memory.size().bytes().0 as u32;
        if end_ptr > memory_size {
            return Err(CompilerError::compiler_error(&format!(
                "Memory write out of bounds: trying to write {}..{}, memory size is {}",
                ptr, end_ptr, memory_size
            )));
        }

        // Write bytes to memory
        memory.write(ptr as u64, &bytes).map_err(|e| {
            CompilerError::compiler_error(&format!(
                "Failed to write to WASM memory at {}: {}",
                ptr, e
            ))
        })?;

        Ok(len)
    }

    /// Read a 32-bit integer from WASM memory
    ///
    /// # Arguments
    /// * `memory` - The WASM memory view
    /// * `ptr` - Pointer to the 32-bit integer
    ///
    /// # Returns
    /// * `Ok(i32)` - The integer value
    /// * `Err(CompileError)` - If memory access fails
    pub fn read_i32_from_memory(memory: &MemoryView, ptr: u32) -> Result<i32, CompilerError> {
        let mut bytes = [0u8; 4];
        memory.read(ptr as u64, &mut bytes).map_err(|e| {
            CompilerError::compiler_error(&format!(
                "Failed to read i32 from WASM memory at {}: {}",
                ptr, e
            ))
        })?;

        Ok(i32::from_le_bytes(bytes))
    }

    /// Write a 32-bit integer to WASM memory
    ///
    /// # Arguments
    /// * `memory` - The WASM memory view
    /// * `ptr` - Pointer to write the integer to
    /// * `value` - The integer value to write
    ///
    /// # Returns
    /// * `Ok(())` - Success
    /// * `Err(CompileError)` - If memory access fails
    pub fn write_i32_to_memory(
        memory: &MemoryView,
        ptr: u32,
        value: i32,
    ) -> Result<(), CompilerError> {
        let bytes = value.to_le_bytes();
        memory.write(ptr as u64, &bytes).map_err(|e| {
            CompilerError::compiler_error(&format!(
                "Failed to write i32 to WASM memory at {}: {}",
                ptr, e
            ))
        })?;

        Ok(())
    }

    /// Validate that a memory range is accessible
    ///
    /// # Arguments
    /// * `memory` - The WASM memory view
    /// * `ptr` - Starting pointer
    /// * `len` - Length of the range
    ///
    /// # Returns
    /// * `Ok(())` - Range is valid
    /// * `Err(CompileError)` - Range is invalid or out of bounds
    pub fn validate_memory_range(
        memory: &MemoryView,
        ptr: u32,
        len: u32,
    ) -> Result<(), CompilerError> {
        if len == 0 {
            return Ok(());
        }

        // Check for overflow
        let end_ptr = ptr.checked_add(len).ok_or_else(|| {
            CompilerError::compiler_error(&format!(
                "Memory range overflow: ptr={}, len={}",
                ptr, len
            ))
        })?;

        // Check bounds
        let memory_size = memory.size().bytes().0 as u32;
        if end_ptr > memory_size {
            return Err(CompilerError::compiler_error(&format!(
                "Memory range out of bounds: {}..{}, memory size is {}",
                ptr, end_ptr, memory_size
            )));
        }

        Ok(())
    }
}
