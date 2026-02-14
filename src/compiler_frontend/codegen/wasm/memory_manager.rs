//! Memory Manager
//!
//! This module handles WASM linear memory integration for Beanstalk's memory model.
//! It provides:
//! - WASM linear memory section creation with proper configuration
//! - Simple bump allocator using WASM globals for heap pointer tracking
//! - Conditional drop operations based on ownership flags
//! - Integration with Beanstalk's tagged pointer ownership system
//!
//! ## Memory Layout
//!
//! The WASM linear memory is organized as follows:
//! ```text
//! +------------------+
//! | Stack (grows ↓)  |  (managed by WASM runtime)
//! +------------------+
//! | Heap (grows ↑)   |  (managed by bump allocator)
//! +------------------+
//! | Static Data      |  (constants, string literals)
//! +------------------+
//! | Reserved         |  (first 64KB for safety)
//! +------------------+
//! ```
//!
//! ## Bump Allocator
//!
//! The bump allocator is a simple, fast allocator that:
//! - Maintains a heap pointer in a WASM global
//! - Allocates by incrementing the heap pointer
//! - Ensures all allocations are at least 2-byte aligned for tagged pointers
//! - Does not support individual deallocation (memory is freed in bulk)

#![allow(dead_code)]

// Re-export constants for backward compatibility
pub use crate::compiler_frontend::codegen::wasm::constants::{
    DEFAULT_MAX_PAGES, DEFAULT_MIN_PAGES, HEAP_START_OFFSET, MIN_ALLOCATION_ALIGNMENT,
};

use crate::compiler_frontend::codegen::wasm::constants::{ALIGNMENT_MASK, OWNERSHIP_BIT};
use crate::compiler_frontend::codegen::wasm::error::WasmGenerationError;
use crate::compiler_frontend::codegen::wasm::module_builder::WasmModuleBuilder;
use crate::compiler_frontend::codegen::wasm::ownership_manager::OwnershipManager;
use crate::compiler_frontend::compiler_errors::CompilerError;
use wasm_encoder::{Function, Instruction, ValType};

/// Configuration for WASM memory setup
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Minimum number of memory pages (64KB each)
    pub min_pages: u32,
    /// Maximum number of memory pages (None = unlimited up to WASM max)
    pub max_pages: Option<u32>,
    /// Initial heap pointer offset
    pub heap_start: i32,
    /// Whether to export memory
    pub export_memory: bool,
    /// Memory export name
    pub memory_export_name: String,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        MemoryConfig {
            min_pages: DEFAULT_MIN_PAGES,
            max_pages: Some(DEFAULT_MAX_PAGES),
            heap_start: HEAP_START_OFFSET,
            export_memory: true,
            memory_export_name: "memory".to_string(),
        }
    }
}

impl MemoryConfig {
    /// Create a minimal memory configuration for testing
    pub fn minimal() -> Self {
        MemoryConfig {
            min_pages: 1,
            max_pages: Some(16), // 1MB max
            heap_start: HEAP_START_OFFSET,
            export_memory: false,
            memory_export_name: "memory".to_string(),
        }
    }

    /// Create a configuration with specific page limits
    pub fn with_pages(min_pages: u32, max_pages: Option<u32>) -> Self {
        MemoryConfig {
            min_pages,
            max_pages,
            heap_start: HEAP_START_OFFSET,
            export_memory: true,
            memory_export_name: "memory".to_string(),
        }
    }
}

/// Indices for memory-related globals and functions
#[derive(Debug, Clone, Copy)]
pub struct MemoryIndices {
    /// Index of the memory section
    pub memory_index: u32,
    /// Index of the heap pointer global
    pub heap_ptr_global: u32,
    /// Index of the allocation function
    pub alloc_func_index: u32,
    /// Index of the free function (no-op for bump allocator)
    pub free_func_index: u32,
}

/// Manages WASM linear memory for Beanstalk's memory model.
///
/// The MemoryManager handles:
/// - Memory section creation with proper configuration
/// - Bump allocator implementation using WASM globals
/// - Allocation and deallocation function generation
/// - Integration with the ownership system
pub struct MemoryManager {
    /// Memory configuration
    config: MemoryConfig,
    /// Memory indices after setup
    indices: Option<MemoryIndices>,
}

impl MemoryManager {
    /// Create a new memory manager with default configuration
    pub fn new() -> Self {
        MemoryManager {
            config: MemoryConfig::default(),
            indices: None,
        }
    }

    /// Create a new memory manager with custom configuration
    pub fn with_config(config: MemoryConfig) -> Self {
        MemoryManager {
            config,
            indices: None,
        }
    }

    /// Get the memory configuration
    pub fn config(&self) -> &MemoryConfig {
        &self.config
    }

    /// Get the memory indices (after setup)
    pub fn indices(&self) -> Option<&MemoryIndices> {
        self.indices.as_ref()
    }

    /// Check if memory has been set up
    pub fn is_setup(&self) -> bool {
        self.indices.is_some()
    }

    /// Set up WASM linear memory in the module builder.
    ///
    /// This method:
    /// 1. Adds the memory section with configured limits
    /// 2. Adds a global for the heap pointer
    /// 3. Generates the allocation function
    /// 4. Generates the free function (no-op for bump allocator)
    /// 5. Optionally exports the memory
    ///
    /// Returns the memory indices for use by other components.
    pub fn setup_memory(
        &mut self,
        module_builder: &mut WasmModuleBuilder,
    ) -> Result<MemoryIndices, CompilerError> {
        // 1. Add memory section
        let memory_index = module_builder.add_memory(self.config.min_pages, self.config.max_pages);

        // 2. Add heap pointer global (mutable i32)
        let heap_ptr_global = module_builder.add_global_i32(
            self.config.heap_start,
            true, // mutable
        );

        // 3. Generate allocation function type: (size: i32) -> i32
        let alloc_type_index =
            module_builder.add_function_type(vec![ValType::I32], vec![ValType::I32]);

        // 4. Generate free function type: (ptr: i32) -> ()
        let free_type_index = module_builder.add_function_type(vec![ValType::I32], vec![]);

        // 5. Generate allocation function body
        let alloc_func = self.generate_alloc_function(heap_ptr_global)?;
        let alloc_func_index =
            module_builder.add_named_function("__bst_alloc", alloc_type_index, alloc_func);

        // 6. Generate free function body (no-op for bump allocator)
        let free_func = self.generate_free_function()?;
        let free_func_index =
            module_builder.add_named_function("__bst_free", free_type_index, free_func);

        // 7. Export memory if configured
        if self.config.export_memory {
            module_builder.add_memory_export(&self.config.memory_export_name, memory_index);
        }

        let indices = MemoryIndices {
            memory_index,
            heap_ptr_global,
            alloc_func_index,
            free_func_index,
        };

        self.indices = Some(indices);
        Ok(indices)
    }

    /// Generate the bump allocator function.
    ///
    /// The allocator:
    /// 1. Gets the current heap pointer
    /// 2. Aligns the size to MIN_ALLOCATION_ALIGNMENT
    /// 3. Calculates the new heap pointer
    /// 4. Stores the new heap pointer
    /// 5. Returns the old heap pointer (allocation address)
    ///
    /// Function signature: (size: i32) -> i32
    fn generate_alloc_function(&self, heap_ptr_global: u32) -> Result<Function, CompilerError> {
        let mut func = Function::new(vec![
            (1, ValType::I32), // local 1: aligned_size
            (1, ValType::I32), // local 2: result (old heap ptr)
        ]);

        // Parameter 0: size
        // Local 1: aligned_size
        // Local 2: result

        // Step 1: Align size to MIN_ALLOCATION_ALIGNMENT
        // aligned_size = (size + (alignment - 1)) & ~(alignment - 1)
        // For alignment = 2: aligned_size = (size + 1) & ~1
        func.instruction(&Instruction::LocalGet(0)); // size
        func.instruction(&Instruction::I32Const(
            (MIN_ALLOCATION_ALIGNMENT - 1) as i32,
        ));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(
            !((MIN_ALLOCATION_ALIGNMENT - 1) as i32),
        ));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::LocalSet(1)); // aligned_size

        // Step 2: Get current heap pointer and save as result
        func.instruction(&Instruction::GlobalGet(heap_ptr_global));
        func.instruction(&Instruction::LocalSet(2)); // result = heap_ptr

        // Step 3: Calculate new heap pointer
        // new_heap_ptr = heap_ptr + aligned_size
        func.instruction(&Instruction::LocalGet(2)); // result (old heap_ptr)
        func.instruction(&Instruction::LocalGet(1)); // aligned_size
        func.instruction(&Instruction::I32Add);

        // Step 4: Store new heap pointer
        func.instruction(&Instruction::GlobalSet(heap_ptr_global));

        // Step 5: Return the old heap pointer (allocation address)
        func.instruction(&Instruction::LocalGet(2));
        func.instruction(&Instruction::End);

        Ok(func)
    }

    /// Generate the free function (no-op for bump allocator).
    ///
    /// The bump allocator does not support individual deallocation.
    /// Memory is freed in bulk when the module is reset or reloaded.
    ///
    /// Function signature: (ptr: i32) -> ()
    fn generate_free_function(&self) -> Result<Function, CompilerError> {
        let mut func = Function::new(vec![]);

        // No-op: bump allocator doesn't support individual deallocation
        // The pointer parameter is ignored
        func.instruction(&Instruction::End);

        Ok(func)
    }

    /// Create an OwnershipManager configured with the memory indices.
    ///
    /// This should be called after setup_memory() to get an ownership manager
    /// that uses the correct allocation and free function indices.
    pub fn create_ownership_manager(&self) -> Result<OwnershipManager, WasmGenerationError> {
        let indices = self.indices.as_ref().ok_or_else(|| {
            WasmGenerationError::memory_layout(
                "Memory not set up",
                "MemoryManager",
                Some("Call setup_memory() before create_ownership_manager()".to_string()),
            )
        })?;

        Ok(OwnershipManager::new(
            indices.alloc_func_index,
            indices.free_func_index,
        ))
    }

    /// Generate code to allocate memory with a specific size.
    ///
    /// This generates a call to the allocation function.
    /// The result is an untagged pointer on the stack.
    ///
    /// Stack effect: [] -> [ptr]
    pub fn generate_allocation(
        &self,
        size: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        let indices = self.indices.as_ref().ok_or_else(|| {
            WasmGenerationError::memory_layout(
                "Memory not set up",
                "MemoryManager",
                Some("Call setup_memory() before generate_allocation()".to_string()),
            )
            .to_compiler_error(crate::compiler_frontend::compiler_errors::ErrorLocation::default())
        })?;

        // Push size and call allocator
        function.instruction(&Instruction::I32Const(size as i32));
        function.instruction(&Instruction::Call(indices.alloc_func_index));

        Ok(())
    }

    /// Generate code to allocate memory with size on stack.
    ///
    /// This generates a call to the allocation function with the size
    /// already on the stack.
    ///
    /// Stack effect: [size] -> [ptr]
    pub fn generate_allocation_dynamic(
        &self,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        let indices = self.indices.as_ref().ok_or_else(|| {
            WasmGenerationError::memory_layout(
                "Memory not set up",
                "MemoryManager",
                Some("Call setup_memory() before generate_allocation_dynamic()".to_string()),
            )
            .to_compiler_error(crate::compiler_frontend::compiler_errors::ErrorLocation::default())
        })?;

        // Size is already on stack, just call allocator
        function.instruction(&Instruction::Call(indices.alloc_func_index));

        Ok(())
    }

    /// Generate code to allocate memory and tag as owned.
    ///
    /// This allocates memory and sets the ownership bit.
    ///
    /// Stack effect: [] -> [tagged_ptr]
    pub fn generate_allocation_owned(
        &self,
        size: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Allocate memory
        self.generate_allocation(size, function)?;

        // Tag as owned (set lowest bit)
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32Or);

        Ok(())
    }

    /// Generate code to free memory (no-op for bump allocator).
    ///
    /// This generates a call to the free function, which is a no-op.
    /// The pointer should be untagged before calling this.
    ///
    /// Stack effect: [ptr] -> []
    pub fn generate_free(&self, function: &mut Function) -> Result<(), CompilerError> {
        let indices = self.indices.as_ref().ok_or_else(|| {
            WasmGenerationError::memory_layout(
                "Memory not set up",
                "MemoryManager",
                Some("Call setup_memory() before generate_free()".to_string()),
            )
            .to_compiler_error(crate::compiler_frontend::compiler_errors::ErrorLocation::default())
        })?;

        // Pointer is on stack, call free function
        function.instruction(&Instruction::Call(indices.free_func_index));

        Ok(())
    }

    /// Generate conditional drop based on ownership flag.
    ///
    /// This checks the ownership bit and only frees if owned.
    /// Uses the ownership manager for the actual implementation.
    ///
    /// Stack effect: [] -> []
    pub fn generate_conditional_drop(
        &self,
        ptr_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        let indices = self.indices.as_ref().ok_or_else(|| {
            WasmGenerationError::memory_layout(
                "Memory not set up",
                "MemoryManager",
                Some("Call setup_memory() before generate_conditional_drop()".to_string()),
            )
            .to_compiler_error(crate::compiler_frontend::compiler_errors::ErrorLocation::default())
        })?;

        // Load the tagged pointer
        function.instruction(&Instruction::LocalGet(ptr_local));

        // Test ownership bit
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32And);

        // Conditional drop based on ownership
        function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));

        // If owned (bit = 1), free the memory
        // First, get the real pointer by masking out the ownership bit
        function.instruction(&Instruction::LocalGet(ptr_local));
        function.instruction(&Instruction::I32Const(ALIGNMENT_MASK));
        function.instruction(&Instruction::I32And);
        // Call the free function
        function.instruction(&Instruction::Call(indices.free_func_index));

        function.instruction(&Instruction::End); // End if block

        Ok(())
    }

    /// Generate conditional drops for multiple locals at scope exit.
    ///
    /// Stack effect: [] -> []
    pub fn generate_scope_exit_drops(
        &self,
        owned_locals: &[u32],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        for &local in owned_locals {
            self.generate_conditional_drop(local, function)?;
        }
        Ok(())
    }

    /// Get the heap pointer global index.
    pub fn heap_ptr_global(&self) -> Option<u32> {
        self.indices.as_ref().map(|i| i.heap_ptr_global)
    }

    /// Get the allocation function index.
    pub fn alloc_func_index(&self) -> Option<u32> {
        self.indices.as_ref().map(|i| i.alloc_func_index)
    }

    /// Get the free function index.
    pub fn free_func_index(&self) -> Option<u32> {
        self.indices.as_ref().map(|i| i.free_func_index)
    }

    /// Get the memory index.
    pub fn memory_index(&self) -> Option<u32> {
        self.indices.as_ref().map(|i| i.memory_index)
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new()
    }
}
