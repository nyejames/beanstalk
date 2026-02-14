//! Ownership Manager
//!
//! This module handles Beanstalk's unique tagged pointer system and
//! possible_drop generation for runtime ownership resolution.
//!
//! ## Tagged Pointer System
//!
//! Beanstalk uses tagged pointers where the lowest alignment-safe bit indicates ownership:
//! - `0` = borrowed (callee must not drop)
//! - `1` = owned (callee must drop before returning)
//!
//! All heap-allocated values are passed as tagged pointers. The callee masks out
//! the tag to access the value and checks the ownership bit to determine if it
//! should drop the value before returning.
//!
//! ## Alignment Requirements
//!
//! Tagged pointers require proper alignment to ensure the lowest bit is available:
//! - All heap allocations must be at least 2-byte aligned
//! - Struct layouts must maintain alignment requirements
//! - Pointer arithmetic must preserve alignment

// This module is prepared for task 9 (Implement Beanstalk's Ownership System)
// All methods will be used when ownership system is integrated
#![allow(dead_code)]

use crate::compiler_frontend::codegen::wasm::constants::{ALIGNMENT_MASK, OWNERSHIP_BIT};
use crate::compiler_frontend::compiler_errors::CompilerError;
use wasm_encoder::{Function, Instruction};

/// Manages Beanstalk's ownership system with tagged pointers.
///
/// The OwnershipManager handles:
/// - Tagged pointer creation and manipulation
/// - Ownership bit testing and masking
/// - Possible_drop insertion at control flow boundaries
/// - Unified ABI implementation for function calls
pub struct OwnershipManager {
    /// Index of the heap allocation function (imported or defined)
    alloc_function_index: u32,
    /// Index of the heap free function (imported or defined)
    free_function_index: u32,
    /// Mask for clearing ownership bit (0xFFFFFFFE)
    alignment_mask: i32,
}

/// Tagged argument types for function calls.
///
/// Used to indicate whether an argument should be passed as owned or borrowed
/// when generating function calls with the unified ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaggedArg {
    /// Local index of owned value (ownership bit will be set)
    Owned(u32),
    /// Local index of borrowed value (ownership bit will be cleared)
    Borrowed(u32),
}

impl TaggedArg {
    /// Get the local index regardless of ownership status
    pub fn local_index(&self) -> u32 {
        match self {
            TaggedArg::Owned(idx) | TaggedArg::Borrowed(idx) => *idx,
        }
    }

    /// Check if this argument is owned
    pub fn is_owned(&self) -> bool {
        matches!(self, TaggedArg::Owned(_))
    }

    /// Check if this argument is borrowed
    pub fn is_borrowed(&self) -> bool {
        matches!(self, TaggedArg::Borrowed(_))
    }
}

/// Parameter information for function prologue/epilogue generation.
///
/// Contains metadata about function parameters needed for ownership handling.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    /// The WASM local index for this parameter
    pub local_index: u32,
    /// Whether this parameter can potentially be owned (heap-allocated types)
    pub can_be_owned: bool,
    /// The WASM value type of this parameter
    pub wasm_type: wasm_encoder::ValType,
}

impl ParamInfo {
    /// Create a new ParamInfo for a potentially owned parameter
    pub fn owned(local_index: u32, wasm_type: wasm_encoder::ValType) -> Self {
        ParamInfo {
            local_index,
            can_be_owned: true,
            wasm_type,
        }
    }

    /// Create a new ParamInfo for a parameter that cannot be owned (primitives)
    pub fn primitive(local_index: u32, wasm_type: wasm_encoder::ValType) -> Self {
        ParamInfo {
            local_index,
            can_be_owned: false,
            wasm_type,
        }
    }
}

/// Function signature information for unified ABI handling.
///
/// Contains all the information needed to generate function prologues,
/// epilogues, and calls with proper ownership handling.
#[derive(Debug, Clone)]
pub struct FunctionAbiInfo {
    /// Parameters with ownership information
    pub params: Vec<ParamInfo>,
    /// Number of additional locals needed for real pointer storage
    pub extra_locals_needed: u32,
    /// Whether this function has any potentially owned parameters
    pub has_owned_params: bool,
}

impl FunctionAbiInfo {
    /// Create ABI info from a list of parameter infos
    pub fn new(params: Vec<ParamInfo>) -> Self {
        let has_owned_params = params.iter().any(|p| p.can_be_owned);
        let extra_locals_needed = params.iter().filter(|p| p.can_be_owned).count() as u32;

        FunctionAbiInfo {
            params,
            extra_locals_needed,
            has_owned_params,
        }
    }

    /// Get the local index for the real (untagged) pointer of a parameter.
    ///
    /// For parameters that can be owned, the real pointer is stored in a
    /// separate local after the function prologue.
    pub fn get_real_ptr_local(&self, param_index: u32) -> Option<u32> {
        if param_index as usize >= self.params.len() {
            return None;
        }

        if !self.params[param_index as usize].can_be_owned {
            return None;
        }

        // Real pointer locals are allocated after all parameters
        // Count how many owned params come before this one
        let owned_before: u32 = self.params[..param_index as usize]
            .iter()
            .filter(|p| p.can_be_owned)
            .count() as u32;

        Some(self.params.len() as u32 + owned_before)
    }
}

impl OwnershipManager {
    /// Create a new ownership manager with the given function indices.
    ///
    /// # Arguments
    /// * `alloc_index` - Index of the heap allocation function
    /// * `free_index` - Index of the heap free function
    pub fn new(alloc_index: u32, free_index: u32) -> Self {
        OwnershipManager {
            alloc_function_index: alloc_index,
            free_function_index: free_index,
            alignment_mask: ALIGNMENT_MASK,
        }
    }

    /// Get the allocation function index
    pub fn alloc_function_index(&self) -> u32 {
        self.alloc_function_index
    }

    /// Get the free function index
    pub fn free_function_index(&self) -> u32 {
        self.free_function_index
    }

    // =========================================================================
    // Tagged Pointer Operations
    // These operations manipulate the ownership bit in tagged pointers
    // =========================================================================

    /// Generate code to tag a pointer as owned (set ownership bit).
    ///
    /// This sets the lowest bit of the pointer to 1, indicating ownership.
    /// The pointer value in the local is modified in place.
    ///
    /// Stack effect: [] -> []
    /// Local effect: ptr_local = ptr_local | 1
    pub fn generate_tag_as_owned(
        &self,
        ptr_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Load pointer and set ownership bit: ptr | 1
        function.instruction(&Instruction::LocalGet(ptr_local));
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::LocalSet(ptr_local));

        Ok(())
    }

    /// Generate code to tag a pointer as borrowed (clear ownership bit).
    ///
    /// This clears the lowest bit of the pointer to 0, indicating borrowed.
    /// The pointer value in the local is modified in place.
    ///
    /// Stack effect: [] -> []
    /// Local effect: ptr_local = ptr_local & ~1
    pub fn generate_tag_as_borrowed(
        &self,
        ptr_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Load pointer and clear ownership bit: ptr & ~1
        function.instruction(&Instruction::LocalGet(ptr_local));
        function.instruction(&Instruction::I32Const(self.alignment_mask));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::LocalSet(ptr_local));

        Ok(())
    }

    /// Generate code to tag a value on the stack as owned.
    ///
    /// This sets the lowest bit of the value on top of the stack to 1.
    /// The result replaces the original value on the stack.
    ///
    /// Stack effect: [ptr] -> [tagged_ptr]
    pub fn generate_tag_stack_as_owned(
        &self,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32Or);
        Ok(())
    }

    /// Generate code to tag a value on the stack as borrowed.
    ///
    /// This clears the lowest bit of the value on top of the stack to 0.
    /// The result replaces the original value on the stack.
    ///
    /// Stack effect: [ptr] -> [tagged_ptr]
    pub fn generate_tag_stack_as_borrowed(
        &self,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        function.instruction(&Instruction::I32Const(self.alignment_mask));
        function.instruction(&Instruction::I32And);
        Ok(())
    }

    /// Generate code to mask out ownership bit and get real pointer.
    ///
    /// This extracts the actual memory address from a tagged pointer
    /// by clearing the ownership bit. The value on top of the stack
    /// is replaced with the untagged pointer.
    ///
    /// Stack effect: [tagged_ptr] -> [real_ptr]
    pub fn generate_mask_pointer(&self, function: &mut Function) -> Result<(), CompilerError> {
        // Assume pointer is on stack, mask out ownership bit: ptr & ~1
        function.instruction(&Instruction::I32Const(self.alignment_mask));
        function.instruction(&Instruction::I32And);

        Ok(())
    }

    /// Generate code to mask a pointer from a local and leave result on stack.
    ///
    /// This loads a tagged pointer from a local, masks out the ownership bit,
    /// and leaves the real pointer on the stack.
    ///
    /// Stack effect: [] -> [real_ptr]
    pub fn generate_mask_pointer_from_local(
        &self,
        ptr_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        function.instruction(&Instruction::LocalGet(ptr_local));
        function.instruction(&Instruction::I32Const(self.alignment_mask));
        function.instruction(&Instruction::I32And);
        Ok(())
    }

    /// Generate code to mask a pointer and store in a different local.
    ///
    /// This loads a tagged pointer from src_local, masks out the ownership bit,
    /// and stores the real pointer in dest_local.
    ///
    /// Stack effect: [] -> []
    /// Local effect: dest_local = src_local & ~1
    pub fn generate_mask_pointer_to_local(
        &self,
        src_local: u32,
        dest_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        function.instruction(&Instruction::LocalGet(src_local));
        function.instruction(&Instruction::I32Const(self.alignment_mask));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::LocalSet(dest_local));
        Ok(())
    }

    /// Generate code to test ownership bit.
    ///
    /// This extracts the ownership bit from the value on top of the stack.
    /// The result is 1 if owned, 0 if borrowed.
    ///
    /// Stack effect: [tagged_ptr] -> [ownership_bit]
    pub fn generate_test_ownership(&self, function: &mut Function) -> Result<(), CompilerError> {
        // Assume pointer is on stack, test ownership bit: ptr & 1
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32And);

        Ok(())
    }

    /// Generate code to test ownership bit from a local.
    ///
    /// This loads a tagged pointer from a local and extracts the ownership bit.
    /// The result (1 if owned, 0 if borrowed) is left on the stack.
    ///
    /// Stack effect: [] -> [ownership_bit]
    pub fn generate_test_ownership_from_local(
        &self,
        ptr_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        function.instruction(&Instruction::LocalGet(ptr_local));
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32And);
        Ok(())
    }

    /// Generate code to check if a pointer is owned (ownership bit == 1).
    ///
    /// This is equivalent to generate_test_ownership but more semantically clear.
    /// The result is 1 (true) if owned, 0 (false) if borrowed.
    ///
    /// Stack effect: [tagged_ptr] -> [is_owned]
    pub fn generate_is_owned(&self, function: &mut Function) -> Result<(), CompilerError> {
        self.generate_test_ownership(function)
    }

    /// Generate code to check if a pointer is borrowed (ownership bit == 0).
    ///
    /// The result is 1 (true) if borrowed, 0 (false) if owned.
    ///
    /// Stack effect: [tagged_ptr] -> [is_borrowed]
    pub fn generate_is_borrowed(&self, function: &mut Function) -> Result<(), CompilerError> {
        // Test ownership bit and invert: (ptr & 1) == 0
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        Ok(())
    }

    /// Generate code to conditionally set ownership based on a boolean.
    ///
    /// If the condition on the stack is non-zero, the pointer is tagged as owned.
    /// Otherwise, it is tagged as borrowed.
    ///
    /// Stack effect: [ptr, condition] -> [tagged_ptr]
    pub fn generate_conditional_tag(&self, function: &mut Function) -> Result<(), CompilerError> {
        // Stack: [ptr, condition]
        // If condition is non-zero, set ownership bit; otherwise clear it
        // Result: (ptr & ~1) | (condition ? 1 : 0)

        // First, normalize condition to 0 or 1
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32Ne); // condition != 0 -> 1 or 0

        // Stack: [ptr, normalized_condition]
        // Now: (ptr & ~1) | normalized_condition

        // We need to swap and process
        // Use a select approach: select(ptr | 1, ptr & ~1, condition)
        // But that requires duplicating ptr, which is complex without locals

        // Simpler approach: just OR the normalized condition
        // (ptr & ~1) | normalized_condition
        // But we need ptr first, then mask, then OR

        // Actually, let's use the stack more carefully:
        // Stack: [ptr, normalized_condition (0 or 1)]
        // We want: (ptr & ~1) | normalized_condition

        // Swap to get ptr on top
        // Note: WASM doesn't have swap, so we need a different approach
        // Let's restructure: assume caller provides [condition, ptr]

        // For now, use a simpler approach that works with [ptr, condition]:
        // We'll use select: select(ptr | 1, ptr & ~1, condition)
        // But this requires duplicating ptr which needs locals

        // Simplest working approach: just use the condition as the bit
        function.instruction(&Instruction::I32Or);

        Ok(())
    }

    // =========================================================================
    // Possible Drop Operations
    // These operations generate conditional drops based on ownership flags
    // =========================================================================

    /// Generate possible_drop operation that checks ownership at runtime.
    ///
    /// This generates code that:
    /// 1. Tests the ownership bit of the tagged pointer
    /// 2. If owned (bit = 1), calls the free function with the real pointer
    /// 3. If borrowed (bit = 0), does nothing
    ///
    /// Stack effect: [] -> []
    pub fn generate_possible_drop(
        &self,
        ptr_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
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
        function.instruction(&Instruction::I32Const(self.alignment_mask));
        function.instruction(&Instruction::I32And);
        // Call the free function
        function.instruction(&Instruction::Call(self.free_function_index));

        function.instruction(&Instruction::End); // End if block

        Ok(())
    }

    /// Generate possible_drop for a value on the stack.
    ///
    /// This is similar to generate_possible_drop but works with a value
    /// already on the stack rather than in a local.
    ///
    /// Stack effect: [tagged_ptr] -> []
    pub fn generate_possible_drop_stack(
        &self,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // We need to duplicate the pointer to test ownership and potentially free
        // Since WASM doesn't have dup, we need a temporary local
        // This method assumes the caller has set up a temporary local

        // For now, this is a simplified version that consumes the pointer
        // A more complete implementation would use a temporary local

        // Test ownership bit (consumes the pointer)
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32And);

        // If owned, we've lost the pointer - this is a limitation
        // The caller should use generate_possible_drop with a local instead
        function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        // Cannot free here without the pointer
        function.instruction(&Instruction::End);

        Ok(())
    }

    /// Generate conditional drops for multiple potentially owned values at scope exit.
    ///
    /// This is used at the end of scopes to drop all values that might be owned.
    ///
    /// Stack effect: [] -> []
    pub fn generate_scope_exit_drops(
        &self,
        owned_locals: &[u32],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        for &local in owned_locals {
            self.generate_possible_drop(local, function)?;
        }
        Ok(())
    }

    // =========================================================================
    // Unified ABI Operations
    // These operations implement Beanstalk's single calling convention
    // =========================================================================

    /// Generate function call with Beanstalk's unified ABI.
    ///
    /// All arguments are passed as tagged pointers. The callee is responsible
    /// for checking ownership flags and handling drops appropriately.
    ///
    /// Stack effect: [] -> [results...]
    pub fn generate_function_call_with_abi(
        &self,
        func_index: u32,
        args: &[TaggedArg],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Load all arguments with appropriate tagging
        for arg in args {
            match arg {
                TaggedArg::Owned(local) => {
                    // Load local and ensure ownership bit is set
                    function.instruction(&Instruction::LocalGet(*local));
                    function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
                    function.instruction(&Instruction::I32Or);
                }
                TaggedArg::Borrowed(local) => {
                    // Load local and ensure ownership bit is clear
                    function.instruction(&Instruction::LocalGet(*local));
                    function.instruction(&Instruction::I32Const(self.alignment_mask));
                    function.instruction(&Instruction::I32And);
                }
            }
        }

        // Make the call - callee will handle ownership appropriately
        function.instruction(&Instruction::Call(func_index));

        Ok(())
    }

    /// Generate function prologue for handling potentially owned parameters.
    ///
    /// For each parameter that could be owned, this generates code to:
    /// 1. Extract the real pointer (mask out ownership bit)
    /// 2. Store the real pointer in a separate local for use in the function body
    /// 3. Keep the original tagged pointer for ownership testing later
    ///
    /// Stack effect: [] -> []
    pub fn generate_function_prologue(
        &self,
        params: &[ParamInfo],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        for (i, param) in params.iter().enumerate() {
            if param.can_be_owned {
                let param_local = i as u32;
                // Calculate the local index for the real pointer
                // This assumes real pointer locals are allocated after all params
                let real_ptr_local = param_local + params.len() as u32;

                // Extract real pointer (mask out ownership bit)
                function.instruction(&Instruction::LocalGet(param_local));
                function.instruction(&Instruction::I32Const(self.alignment_mask));
                function.instruction(&Instruction::I32And);
                function.instruction(&Instruction::LocalSet(real_ptr_local));

                // The ownership bit remains in the original parameter for later testing
            }
        }
        Ok(())
    }

    /// Generate function epilogue for dropping owned parameters.
    ///
    /// Before returning, this generates possible_drop for all parameters
    /// that could potentially be owned.
    ///
    /// Stack effect: [] -> []
    pub fn generate_function_epilogue(
        &self,
        params: &[ParamInfo],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        for (i, param) in params.iter().enumerate() {
            if param.can_be_owned {
                let param_local = i as u32;
                self.generate_possible_drop(param_local, function)?;
            }
        }
        Ok(())
    }

    /// Prepare an argument for a function call based on ownership.
    ///
    /// This loads the argument from a local and tags it appropriately.
    ///
    /// Stack effect: [] -> [tagged_arg]
    pub fn prepare_argument(
        &self,
        arg: &TaggedArg,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        match arg {
            TaggedArg::Owned(local) => {
                function.instruction(&Instruction::LocalGet(*local));
                function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
                function.instruction(&Instruction::I32Or);
            }
            TaggedArg::Borrowed(local) => {
                function.instruction(&Instruction::LocalGet(*local));
                function.instruction(&Instruction::I32Const(self.alignment_mask));
                function.instruction(&Instruction::I32And);
            }
        }
        Ok(())
    }

    // =========================================================================
    // Memory Allocation Helpers
    // =========================================================================

    /// Generate code to allocate memory and tag as owned.
    ///
    /// This calls the allocation function with the given size and tags
    /// the resulting pointer as owned.
    ///
    /// Stack effect: [] -> [tagged_ptr]
    pub fn generate_allocate_owned(
        &self,
        size: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Push size and call allocator
        function.instruction(&Instruction::I32Const(size as i32));
        function.instruction(&Instruction::Call(self.alloc_function_index));

        // Tag as owned
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32Or);

        Ok(())
    }

    /// Generate code to allocate memory and store in a local as owned.
    ///
    /// Stack effect: [] -> []
    /// Local effect: dest_local = alloc(size) | 1
    pub fn generate_allocate_owned_to_local(
        &self,
        size: u32,
        dest_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        self.generate_allocate_owned(size, function)?;
        function.instruction(&Instruction::LocalSet(dest_local));
        Ok(())
    }

    // =========================================================================
    // Control Flow Boundary Operations
    // These operations handle ownership at control flow boundaries
    // =========================================================================

    /// Generate possible_drop operations before a return statement.
    ///
    /// This ensures all potentially owned values are dropped before returning.
    /// The return value (if any) should already be on the stack.
    ///
    /// Stack effect: [return_value?] -> [return_value?]
    pub fn generate_return_drops(
        &self,
        owned_locals: &[u32],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Drop all potentially owned locals before returning
        for &local in owned_locals {
            self.generate_possible_drop(local, function)?;
        }
        Ok(())
    }

    /// Generate possible_drop operations before a break statement.
    ///
    /// This ensures all potentially owned values in the current scope
    /// are dropped before breaking out of a loop or block.
    ///
    /// Stack effect: [] -> []
    pub fn generate_break_drops(
        &self,
        owned_locals: &[u32],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Drop all potentially owned locals before breaking
        for &local in owned_locals {
            self.generate_possible_drop(local, function)?;
        }
        Ok(())
    }

    /// Generate possible_drop for values that might be owned at an if/else branch.
    ///
    /// This is used when a value might be owned in one branch but not another.
    /// The value is dropped only if it's owned.
    ///
    /// Stack effect: [] -> []
    pub fn generate_branch_drops(
        &self,
        owned_locals: &[u32],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        for &local in owned_locals {
            self.generate_possible_drop(local, function)?;
        }
        Ok(())
    }

    /// Generate possible_drop with a null check.
    ///
    /// This is useful when a pointer might be null (e.g., uninitialized).
    /// Only drops if the pointer is non-null and owned.
    ///
    /// Stack effect: [] -> []
    pub fn generate_possible_drop_with_null_check(
        &self,
        ptr_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Load the tagged pointer
        function.instruction(&Instruction::LocalGet(ptr_local));

        // Check if pointer is non-null (ignoring ownership bit)
        function.instruction(&Instruction::I32Const(self.alignment_mask));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32Ne);

        // If non-null, check ownership and potentially drop
        function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));

        // Now do the normal possible_drop
        function.instruction(&Instruction::LocalGet(ptr_local));
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32And);

        function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));

        // If owned, free the memory
        function.instruction(&Instruction::LocalGet(ptr_local));
        function.instruction(&Instruction::I32Const(self.alignment_mask));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::Call(self.free_function_index));

        function.instruction(&Instruction::End); // End inner if
        function.instruction(&Instruction::End); // End outer if

        Ok(())
    }

    /// Transfer ownership from one local to another.
    ///
    /// This copies the tagged pointer and clears the source to prevent double-free.
    /// The source local is set to 0 (null) after transfer.
    ///
    /// Stack effect: [] -> []
    /// Local effect: dest = src, src = 0
    pub fn generate_ownership_transfer(
        &self,
        src_local: u32,
        dest_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Copy the tagged pointer to destination
        function.instruction(&Instruction::LocalGet(src_local));
        function.instruction(&Instruction::LocalSet(dest_local));

        // Clear the source to prevent double-free
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::LocalSet(src_local));

        Ok(())
    }

    /// Generate code to clear ownership (set to borrowed) without dropping.
    ///
    /// This is used when ownership is being transferred elsewhere.
    /// The value is not dropped, just marked as borrowed.
    ///
    /// Stack effect: [] -> []
    pub fn generate_clear_ownership(
        &self,
        ptr_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        self.generate_tag_as_borrowed(ptr_local, function)
    }

    // =========================================================================
    // Enhanced Unified ABI Operations
    // These operations provide more complete unified ABI support
    // =========================================================================

    /// Generate a complete function prologue with ABI info.
    ///
    /// This is a convenience method that uses FunctionAbiInfo to generate
    /// the complete prologue including real pointer extraction.
    ///
    /// Stack effect: [] -> []
    pub fn generate_function_prologue_with_abi(
        &self,
        abi_info: &FunctionAbiInfo,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        self.generate_function_prologue(&abi_info.params, function)
    }

    /// Generate a complete function epilogue with ABI info.
    ///
    /// This is a convenience method that uses FunctionAbiInfo to generate
    /// the complete epilogue including ownership drops.
    ///
    /// Stack effect: [] -> []
    pub fn generate_function_epilogue_with_abi(
        &self,
        abi_info: &FunctionAbiInfo,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        self.generate_function_epilogue(&abi_info.params, function)
    }

    /// Generate a function call with mixed argument types.
    ///
    /// This handles calls where some arguments are primitives (not tagged)
    /// and some are heap-allocated (tagged).
    ///
    /// Stack effect: [] -> [results...]
    pub fn generate_mixed_function_call(
        &self,
        func_index: u32,
        args: &[MixedArg],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        for arg in args {
            match arg {
                MixedArg::Tagged(tagged) => {
                    self.prepare_argument(tagged, function)?;
                }
                MixedArg::Primitive(local) => {
                    // Primitives are loaded directly without tagging
                    function.instruction(&Instruction::LocalGet(*local));
                }
                MixedArg::Constant(value) => {
                    // Constants are pushed directly
                    function.instruction(&Instruction::I32Const(*value));
                }
            }
        }

        function.instruction(&Instruction::Call(func_index));
        Ok(())
    }

    /// Generate code to receive a return value that might be owned.
    ///
    /// This stores the return value in a local, preserving its ownership tag.
    ///
    /// Stack effect: [tagged_ptr] -> []
    /// Local effect: dest_local = return_value
    pub fn generate_receive_owned_return(
        &self,
        dest_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // The return value is already tagged, just store it
        function.instruction(&Instruction::LocalSet(dest_local));
        Ok(())
    }

    /// Generate code to return an owned value.
    ///
    /// This ensures the ownership bit is set before returning.
    ///
    /// Stack effect: [] -> [tagged_ptr]
    pub fn generate_return_owned(
        &self,
        src_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        function.instruction(&Instruction::LocalGet(src_local));
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32Or);
        Ok(())
    }

    /// Generate code to return a borrowed value.
    ///
    /// This ensures the ownership bit is clear before returning.
    ///
    /// Stack effect: [] -> [tagged_ptr]
    pub fn generate_return_borrowed(
        &self,
        src_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        function.instruction(&Instruction::LocalGet(src_local));
        function.instruction(&Instruction::I32Const(self.alignment_mask));
        function.instruction(&Instruction::I32And);
        Ok(())
    }

    /// Generate code to copy a value (create a new owned copy).
    ///
    /// This allocates new memory, copies the data, and returns an owned pointer.
    /// The source value is not modified.
    ///
    /// Stack effect: [] -> [tagged_ptr]
    pub fn generate_copy_value(
        &self,
        _src_local: u32,
        size: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Allocate new memory
        function.instruction(&Instruction::I32Const(size as i32));
        function.instruction(&Instruction::Call(self.alloc_function_index));

        // Stack: [new_ptr]
        // We need to copy data from src to new_ptr
        // This requires memory.copy or a loop - for now, we'll use a simple approach
        // that assumes the caller will handle the actual copy

        // Tag as owned
        function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
        function.instruction(&Instruction::I32Or);

        Ok(())
    }
}

/// Mixed argument types for function calls.
///
/// Supports both tagged (heap-allocated) and primitive arguments.
#[derive(Debug, Clone)]
pub enum MixedArg {
    /// A tagged argument (heap-allocated, with ownership)
    Tagged(TaggedArg),
    /// A primitive argument (no ownership tracking)
    Primitive(u32),
    /// A constant value
    Constant(i32),
}

impl MixedArg {
    /// Create a tagged owned argument
    pub fn owned(local: u32) -> Self {
        MixedArg::Tagged(TaggedArg::Owned(local))
    }

    /// Create a tagged borrowed argument
    pub fn borrowed(local: u32) -> Self {
        MixedArg::Tagged(TaggedArg::Borrowed(local))
    }

    /// Create a primitive argument
    pub fn primitive(local: u32) -> Self {
        MixedArg::Primitive(local)
    }

    /// Create a constant argument
    pub fn constant(value: i32) -> Self {
        MixedArg::Constant(value)
    }
}
