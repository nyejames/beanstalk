//! Lowering Context and Local Allocation
//!
//! This module contains the main context struct for HIR to LIR lowering
//! and the local variable allocator for WASM locals.

use std::collections::HashMap;

use crate::backends::lir::nodes::{LirInst, LirType};
use crate::compiler_frontend::ast::ast_nodes::Var;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::nodes::BlockId;
use crate::compiler_frontend::hir::nodes::{HirExpr, HirExprKind};
use crate::compiler_frontend::host_functions::registry::HostFunctionId;
use crate::compiler_frontend::string_interning::{InternedString, StringId};

use super::types::{FieldLayout, StructLayout, build_struct_layout};

// ============================================================================
// Lowering Context
// ============================================================================

/// The main context struct that maintains state during the HIR to LIR lowering process.
///
/// This context is passed through all lowering functions and tracks:
/// - Local variable allocation and reuse
/// - Variable to WASM local index mappings
/// - Struct layout information for field access
/// - Loop nesting for break/continue handling
/// - Function index mappings for call instructions
/// - Error accumulation for comprehensive error reporting
#[derive(Debug)]
pub struct LoweringContext {
    /// Manages WASM local allocation with type-based reuse optimization
    pub local_allocator: LocalAllocator,

    /// The name of the function currently being lowered (None when lowering module-level items)
    pub current_function: Option<InternedString>,

    /// Maps HIR BlockIds to their lowered LIR instruction sequences
    pub block_map: HashMap<BlockId, Vec<LirInst>>,

    /// Maps variable names to their allocated WASM local indices
    pub var_to_local: HashMap<InternedString, u32>,

    /// Stores computed struct layouts for field offset calculations
    pub struct_layouts: HashMap<InternedString, StructLayout>,

    /// Stack of active loops for break/continue target resolution
    pub loop_stack: Vec<LoopContext>,

    /// Accumulated errors during lowering (allows continuing after non-fatal errors)
    pub errors: Vec<CompilerError>,

    /// Maps function names to their WASM function indices
    pub function_indices: HashMap<StringId, u32>,

    /// Maps host function names to their import indices
    pub host_function_indices: HashMap<HostFunctionId, u32>,

    /// The next available function index for allocation
    next_function_index: u32,

    /// The next available host function index for allocation
    next_host_function_index: u32,

    /// Tracks which variables are at their last use in the current scope
    pub last_use_vars: HashMap<InternedString, bool>,
}

impl LoweringContext {
    /// Creates a new LoweringContext with default initial state.
    pub fn new() -> Self {
        Self {
            local_allocator: LocalAllocator::new(),
            current_function: None,
            block_map: HashMap::new(),
            var_to_local: HashMap::new(),
            struct_layouts: HashMap::new(),
            loop_stack: Vec::new(),
            errors: Vec::new(),
            function_indices: HashMap::new(),
            host_function_indices: HashMap::new(),
            next_function_index: 0,
            next_host_function_index: 0,
            last_use_vars: HashMap::new(),
        }
    }

    /// Resets the context for lowering a new function.
    ///
    /// This clears function-specific state while preserving module-level
    /// information like struct layouts and function indices.
    pub fn reset_for_function(&mut self, function_name: InternedString) {
        self.local_allocator = LocalAllocator::new();
        self.current_function = Some(function_name);
        self.block_map.clear();
        self.var_to_local.clear();
        self.loop_stack.clear();
        self.last_use_vars.clear();
    }

    /// Registers a function and assigns it a WASM function index.
    pub fn register_function(&mut self, target: StringId) -> u32 {
        if let Some(&idx) = self.function_indices.get(&target) {
            return idx;
        }
        let idx = self.next_function_index;
        self.next_function_index += 1;
        self.function_indices.insert(target, idx);
        idx
    }

    /// Retrieves the function index for a function by name.
    pub fn get_function_index(&self, target: StringId) -> Option<u32> {
        self.function_indices.get(&target).copied()
    }

    /// Registers a host function and assigns it an import index.
    pub fn register_host_function(&mut self, id: HostFunctionId) -> u32 {
        if let Some(&idx) = self.host_function_indices.get(&id) {
            return idx;
        }
        let idx = self.next_host_function_index;
        self.next_host_function_index += 1;
        self.host_function_indices.insert(id, idx);
        idx
    }

    /// Retrieves the host function index for a host function by name.
    pub fn get_host_function_index(&self, id: HostFunctionId) -> Option<u32> {
        self.host_function_indices.get(&id).copied()
    }

    /// Marks a variable as being at its last use.
    pub fn mark_last_use(&mut self, var_name: InternedString) {
        self.last_use_vars.insert(var_name, true);
    }

    /// Checks if a variable is at its last use.
    pub fn is_last_use(&self, var_name: InternedString) -> bool {
        self.last_use_vars.get(&var_name).copied().unwrap_or(false)
    }

    /// Registers a struct layout computed from a HIR struct definition.
    pub fn register_struct_layout(&mut self, name: InternedString, fields: &[Var]) {
        let layout = build_struct_layout(name, fields);
        self.struct_layouts.insert(name, layout);
    }

    /// Retrieves the layout for a struct type by name.
    pub fn get_struct_layout(&self, name: InternedString) -> Option<&StructLayout> {
        self.struct_layouts.get(&name)
    }

    /// Retrieves the field layout for a specific field within a struct.
    pub fn get_field_layout(
        &self,
        struct_name: InternedString,
        field_name: InternedString,
    ) -> Option<&FieldLayout> {
        self.struct_layouts
            .get(&struct_name)
            .and_then(|layout| layout.get_field(field_name))
    }

    /// Checks if an HIR expression kind represents a heap-allocated type that needs ownership tagging.
    pub fn is_heap_allocated_expr(&self, expr: &HirExpr) -> bool {
        match &expr.kind {
            HirExprKind::HeapString(_)
            | HirExprKind::StructConstruct { .. }
            | HirExprKind::Collection(_)
            | HirExprKind::Range { .. } => true,
            // Load operations may be heap-allocated depending on what they load
            HirExprKind::Load(_) => true,
            // Scalar types are not heap-allocated
            HirExprKind::Int(_)
            | HirExprKind::Float(_)
            | HirExprKind::Bool(_)
            | HirExprKind::Char(_) => false,
            // Binary and unary operations inherit the heap allocation status of their operands
            HirExprKind::BinOp { left, .. } => self.is_heap_allocated_expr(left),
            HirExprKind::UnaryOp { operand, .. } => self.is_heap_allocated_expr(operand),
            _ => false,
        }
    }

    /// Checks if a DataType is heap-allocated and needs ownership tagging.
    /// This is used for function signatures and other contexts where we still have DataType.
    pub fn is_heap_allocated_type(&self, data_type: &DataType) -> bool {
        match data_type {
            DataType::String
            | DataType::Struct(_, _)
            | DataType::Collection(_, _)
            | DataType::Template
            | DataType::Parameters(_)
            | DataType::Option(_)
            | DataType::Choices(_) => true,
            DataType::Reference(inner) => self.is_heap_allocated_type(inner),
            _ => false,
        }
    }
}

impl Default for LoweringContext {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Local Allocator
// ============================================================================

/// Manages WASM local variable allocation with type-based reuse optimization.
///
/// WASM locals are typed, and this allocator tracks:
/// - The next available local index
/// - The type of each allocated local
/// - A free list for reusing locals by type when they go out of scope
#[derive(Debug, Clone)]
pub struct LocalAllocator {
    /// The next available local index for allocation
    next_local: u32,

    /// The type of each allocated local, indexed by local index
    local_types: Vec<LirType>,

    /// Free lists for reusing locals, organized by type
    free_locals: HashMap<LirType, Vec<u32>>,
}

impl LocalAllocator {
    /// Creates a new LocalAllocator with no allocated locals.
    pub fn new() -> Self {
        Self {
            next_local: 0,
            local_types: Vec::new(),
            free_locals: HashMap::new(),
        }
    }

    /// Allocates a new local of the specified type.
    ///
    /// If a free local of the same type exists, it will be reused.
    /// Otherwise, a new local index is allocated.
    pub fn allocate(&mut self, ty: LirType) -> u32 {
        // Try to reuse a free local of the same type
        if let Some(free_list) = self.free_locals.get_mut(&ty) {
            if let Some(local_idx) = free_list.pop() {
                return local_idx;
            }
        }

        // Allocate a new local
        let idx = self.next_local;
        self.next_local += 1;
        self.local_types.push(ty);
        idx
    }

    /// Marks a local as free for potential reuse.
    pub fn free(&mut self, local_idx: u32) {
        if let Some(&ty) = self.local_types.get(local_idx as usize) {
            self.free_locals.entry(ty).or_default().push(local_idx);
        }
    }

    /// Returns the types of all allocated locals.
    pub fn get_local_types(&self) -> &[LirType] {
        &self.local_types
    }

    /// Returns the total number of locals that have been allocated.
    pub fn local_count(&self) -> u32 {
        self.next_local
    }
}

impl Default for LocalAllocator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Loop Context
// ============================================================================

/// Tracks information about an active loop for break/continue handling.
#[derive(Debug, Clone)]
pub struct LoopContext {
    /// The HIR BlockId that identifies this loop
    pub label: BlockId,

    /// The nesting depth of this loop (0 for outermost loop in a function)
    pub depth: u32,
}

impl LoopContext {
    /// Creates a new LoopContext with the given label and depth.
    pub fn new(label: BlockId, depth: u32) -> Self {
        Self { label, depth }
    }
}
