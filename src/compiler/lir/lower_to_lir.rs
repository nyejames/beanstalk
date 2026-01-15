//! HIR to LIR Lowering
//!
//! This module transforms the high-level, language-shaped HIR (High-Level Intermediate
//! Representation) into the low-level, WASM-shaped LIR (Low-Level Intermediate Representation).
//!
//! The lowering process:
//! - Resolves ownership decisions using runtime tagged pointers
//! - Lowers control flow structures to WASM-compatible blocks
//! - Converts RPN-ordered expressions into stack-based LIR instructions
//! - Lowers struct field access and collection operations to concrete memory offsets
//! - Allocates and tracks WASM locals for variables and temporaries

use std::collections::HashMap;

use crate::compiler::compiler_messages::compiler_errors::{
    CompilerError, ErrorLocation, ErrorMetaDataKey, ErrorType,
};
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::BlockId;
use crate::compiler::lir::nodes::{LirField, LirFunction, LirInst, LirStruct, LirType};
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::InternedString;

// ============================================================================
// Constants for Collection Layout
// ============================================================================

/// Collection header size in bytes.
/// Layout: [length: i32, capacity: i32, element_size: i32]
const COLLECTION_HEADER_SIZE: u32 = 12;

/// Default element size for I64 elements (8 bytes).
const ELEMENT_SIZE_I64: u32 = 8;

/// Placeholder function index for bounds checking.
/// This will be resolved to the actual runtime function during codegen.
const BOUNDS_CHECK_FUNC_INDEX: u32 = 0;

// ============================================================================
// Core Types - Ordered from highest to lowest abstraction
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
    /// This is populated during the initial pass over function definitions
    pub function_indices: HashMap<InternedString, u32>,

    /// Maps host function names to their import indices
    /// Host functions are imported from the host environment (e.g., `io`)
    pub host_function_indices: HashMap<InternedString, u32>,

    /// The next available function index for allocation
    next_function_index: u32,

    /// The next available host function index for allocation
    next_host_function_index: u32,

    /// Tracks which variables are at their last use in the current scope
    /// Used to determine ownership transfer vs borrow for function arguments
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
    ///
    /// This should be called during the initial pass over function definitions
    /// before lowering function bodies that may call these functions.
    ///
    /// # Arguments
    /// * `name` - The name of the function
    ///
    /// # Returns
    /// The assigned function index
    pub fn register_function(&mut self, name: InternedString) -> u32 {
        if let Some(&idx) = self.function_indices.get(&name) {
            return idx;
        }
        let idx = self.next_function_index;
        self.next_function_index += 1;
        self.function_indices.insert(name, idx);
        idx
    }

    /// Retrieves the function index for a function by name.
    ///
    /// Returns None if the function has not been registered.
    pub fn get_function_index(&self, name: InternedString) -> Option<u32> {
        self.function_indices.get(&name).copied()
    }

    /// Registers a host function and assigns it an import index.
    ///
    /// Host functions are imported from the host environment (e.g., `io`).
    /// They are indexed separately from regular functions.
    ///
    /// # Arguments
    /// * `name` - The name of the host function
    ///
    /// # Returns
    /// The assigned host function index
    pub fn register_host_function(&mut self, name: InternedString) -> u32 {
        if let Some(&idx) = self.host_function_indices.get(&name) {
            return idx;
        }
        let idx = self.next_host_function_index;
        self.next_host_function_index += 1;
        self.host_function_indices.insert(name, idx);
        idx
    }

    /// Retrieves the host function index for a host function by name.
    ///
    /// Returns None if the host function has not been registered.
    pub fn get_host_function_index(&self, name: InternedString) -> Option<u32> {
        self.host_function_indices.get(&name).copied()
    }

    /// Marks a variable as being at its last use.
    ///
    /// This is used to determine ownership transfer vs borrow for function arguments.
    /// When a variable is at its last use, ownership can be transferred to the callee.
    pub fn mark_last_use(&mut self, var_name: InternedString) {
        self.last_use_vars.insert(var_name, true);
    }

    /// Checks if a variable is at its last use.
    ///
    /// Returns true if the variable has been marked as being at its last use.
    pub fn is_last_use(&self, var_name: InternedString) -> bool {
        self.last_use_vars.get(&var_name).copied().unwrap_or(false)
    }

    /// Registers a struct layout computed from a HIR struct definition.
    ///
    /// This should be called during the initial pass over struct definitions
    /// before lowering function bodies that may reference these structs.
    ///
    /// # Arguments
    /// * `name` - The name of the struct type
    /// * `fields` - The struct fields from the HIR struct definition
    pub fn register_struct_layout(&mut self, name: InternedString, fields: &[Arg]) {
        let layout = build_struct_layout(name, fields);
        self.struct_layouts.insert(name, layout);
    }

    /// Retrieves the layout for a struct type by name.
    ///
    /// Returns None if the struct has not been registered.
    pub fn get_struct_layout(&self, name: InternedString) -> Option<&StructLayout> {
        self.struct_layouts.get(&name)
    }

    /// Retrieves the field layout for a specific field within a struct.
    ///
    /// Returns None if the struct or field is not found.
    pub fn get_field_layout(
        &self,
        struct_name: InternedString,
        field_name: InternedString,
    ) -> Option<&FieldLayout> {
        self.struct_layouts
            .get(&struct_name)
            .and_then(|layout| layout.get_field(field_name))
    }
}

impl Default for LoweringContext {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Local Allocation
// ============================================================================

/// Manages WASM local variable allocation with type-based reuse optimization.
///
/// WASM locals are typed, and this allocator tracks:
/// - The next available local index
/// - The type of each allocated local
/// - A free list for reusing locals by type when they go out of scope
///
/// This optimization reduces the total number of locals needed in the
/// generated WASM, which can improve performance and reduce binary size.
#[derive(Debug, Clone)]
pub struct LocalAllocator {
    /// The next available local index for allocation
    next_local: u32,

    /// The type of each allocated local, indexed by local index
    local_types: Vec<LirType>,

    /// Free lists for reusing locals, organized by type
    /// When a local is freed, it's added to the appropriate type's list
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
    ///
    /// Returns the local index that can be used in LocalGet/LocalSet instructions.
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
    ///
    /// The local will be added to the free list for its type and may be
    /// returned by a future call to `allocate()` with the same type.
    pub fn free(&mut self, local_idx: u32) {
        if let Some(&ty) = self.local_types.get(local_idx as usize) {
            self.free_locals.entry(ty).or_default().push(local_idx);
        }
    }

    /// Returns the types of all allocated locals.
    ///
    /// This is used when building the final LirFunction to specify
    /// the local variable types in the WASM function signature.
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
// Struct Layout
// ============================================================================

/// Describes the memory layout of a struct type.
///
/// This information is computed from HIR struct definitions and used
/// during lowering to calculate field offsets for memory load/store operations.
#[derive(Debug, Clone)]
pub struct StructLayout {
    /// The name of the struct type
    pub name: InternedString,

    /// Layout information for each field, in declaration order
    pub fields: Vec<FieldLayout>,

    /// Total size of the struct in bytes, including any padding
    pub total_size: u32,
}

impl StructLayout {
    /// Creates a new StructLayout with the given name and no fields.
    pub fn new(name: InternedString) -> Self {
        Self {
            name,
            fields: Vec::new(),
            total_size: 0,
        }
    }

    /// Looks up a field by name and returns its layout information.
    pub fn get_field(&self, field_name: InternedString) -> Option<&FieldLayout> {
        self.fields.iter().find(|f| f.name == field_name)
    }
}

// ============================================================================
// Struct Layout Computation
// ============================================================================

/// Computes the size in bytes for a given LIR type.
///
/// Type sizes follow WASM conventions:
/// - I32: 4 bytes (used for Bool, Char, String pointers, Struct pointers, Collection pointers)
/// - I64: 8 bytes (used for Int)
/// - F32: 4 bytes
/// - F64: 8 bytes (used for Float)
pub fn size_of_lir_type(ty: LirType) -> u32 {
    match ty {
        LirType::I32 => 4,
        LirType::I64 => 8,
        LirType::F32 => 4,
        LirType::F64 => 8,
    }
}

/// Computes the alignment requirement in bytes for a given LIR type.
///
/// Alignment follows natural alignment rules:
/// - Types are aligned to their size
/// - This ensures efficient memory access on most architectures
pub fn alignment_of_lir_type(ty: LirType) -> u32 {
    // Natural alignment: align to the size of the type
    size_of_lir_type(ty)
}

/// Maps a Beanstalk DataType to its corresponding LIR type.
///
/// Type mapping follows the design document:
/// - Int -> I64 (64-bit signed integer)
/// - Float -> F64 (64-bit floating point)
/// - Bool -> I32 (0 = false, 1 = true)
/// - String -> I32 (pointer to string data in linear memory)
/// - Struct -> I32 (pointer to struct data in linear memory)
/// - Collection -> I32 (pointer to collection data in linear memory)
/// - Char -> I32 (Unicode code point)
pub fn data_type_to_lir_type(data_type: &DataType) -> LirType {
    match data_type {
        DataType::Int => LirType::I64,
        DataType::Float => LirType::F64,
        DataType::Decimal => LirType::F64, // Decimal uses F64 for now
        DataType::Bool => LirType::I32,
        DataType::True => LirType::I32,
        DataType::False => LirType::I32,
        DataType::Char => LirType::I32,
        DataType::String => LirType::I32, // Pointer to string data
        DataType::Struct(_, _) => LirType::I32, // Pointer to struct data
        DataType::Collection(_, _) => LirType::I32, // Pointer to collection data
        DataType::Parameters(_) => LirType::I32, // Pointer to struct data
        DataType::Template => LirType::I32, // Pointer to template data
        DataType::Function(_, _) => LirType::I32, // Function reference
        DataType::Option(_) => LirType::I32, // Pointer to optional data
        DataType::Choices(_) => LirType::I32, // Pointer to choice data
        DataType::Range => LirType::I32, // Pointer to range data
        DataType::Reference(inner, _) => data_type_to_lir_type(inner), // Follow the reference
        DataType::CoerceToString => LirType::I32, // Pointer to string
        DataType::None => LirType::I32, // Null pointer representation
        DataType::Inferred => LirType::I32, // Default to I32 for unresolved types
    }
}

/// Aligns an offset to the specified alignment boundary.
///
/// Returns the smallest value >= offset that is a multiple of alignment.
/// Alignment must be a power of 2.
fn align_to(offset: u32, alignment: u32) -> u32 {
    // Round up to the next multiple of alignment
    // This works because alignment is a power of 2
    let mask = alignment - 1;
    (offset + mask) & !mask
}

/// Computes field offsets with proper alignment for a list of struct fields.
///
/// This function calculates the byte offset for each field, ensuring proper
/// alignment based on the field's type. Fields are laid out in declaration order.
///
/// # Arguments
/// * `fields` - The struct fields as Arg values (from HIR struct definition)
///
/// # Returns
/// A vector of FieldLayout containing name, offset, and type for each field
pub fn compute_field_offsets(fields: &[Arg]) -> Vec<FieldLayout> {
    let mut field_layouts = Vec::with_capacity(fields.len());
    let mut current_offset: u32 = 0;

    for field in fields {
        let lir_type = data_type_to_lir_type(&field.value.data_type);
        let alignment = alignment_of_lir_type(lir_type);
        let size = size_of_lir_type(lir_type);

        // Align the current offset to the field's alignment requirement
        let aligned_offset = align_to(current_offset, alignment);

        field_layouts.push(FieldLayout {
            name: field.id,
            offset: aligned_offset,
            ty: lir_type,
        });

        // Move past this field
        current_offset = aligned_offset + size;
    }

    field_layouts
}

/// Calculates the total size of a struct including any trailing padding.
///
/// The total size is aligned to the struct's overall alignment requirement,
/// which is the maximum alignment of any field. This ensures that arrays
/// of structs maintain proper alignment for all fields.
///
/// # Arguments
/// * `field_layouts` - The computed field layouts with offsets
///
/// # Returns
/// The total size of the struct in bytes, including trailing padding
pub fn calculate_struct_size(field_layouts: &[FieldLayout]) -> u32 {
    if field_layouts.is_empty() {
        // Empty structs have size 0 (or could be 1 for addressability)
        return 0;
    }

    // Find the maximum alignment requirement among all fields
    let max_alignment = field_layouts
        .iter()
        .map(|f| alignment_of_lir_type(f.ty))
        .max()
        .unwrap_or(1);

    // Find the end of the last field
    let last_field = field_layouts.last().unwrap();
    let end_of_last_field = last_field.offset + size_of_lir_type(last_field.ty);

    // Align the total size to the struct's alignment
    // This ensures arrays of structs maintain proper alignment
    align_to(end_of_last_field, max_alignment)
}

/// Builds a complete StructLayout from a HIR struct definition.
///
/// This function computes field offsets with proper alignment and calculates
/// the total struct size including any necessary padding.
///
/// # Arguments
/// * `name` - The name of the struct type
/// * `fields` - The struct fields from the HIR struct definition
///
/// # Returns
/// A complete StructLayout ready for use in memory operations
pub fn build_struct_layout(name: InternedString, fields: &[Arg]) -> StructLayout {
    let field_layouts = compute_field_offsets(fields);
    let total_size = calculate_struct_size(&field_layouts);

    StructLayout {
        name,
        fields: field_layouts,
        total_size,
    }
}

/// Describes the layout of a single struct field.
///
/// Contains the information needed to generate memory load/store
/// instructions for accessing this field.
#[derive(Debug, Clone)]
pub struct FieldLayout {
    /// The name of the field
    pub name: InternedString,

    /// The byte offset of this field from the start of the struct
    pub offset: u32,

    /// The LIR type of this field (determines load/store instruction variant)
    pub ty: LirType,
}

impl FieldLayout {
    /// Creates a new FieldLayout with the given properties.
    pub fn new(name: InternedString, offset: u32, ty: LirType) -> Self {
        Self { name, offset, ty }
    }
}

// ============================================================================
// Loop Context
// ============================================================================

/// Tracks information about an active loop for break/continue handling.
///
/// When lowering nested loops, we maintain a stack of LoopContext values
/// to resolve break and continue statements to the correct WASM branch targets.
#[derive(Debug, Clone)]
pub struct LoopContext {
    /// The HIR BlockId that identifies this loop (used by break/continue to reference it)
    pub label: BlockId,

    /// The nesting depth of this loop (0 for outermost loop in a function)
    /// Used to calculate the correct branch depth for WASM br instructions
    pub depth: u32,
}

impl LoopContext {
    /// Creates a new LoopContext with the given label and depth.
    pub fn new(label: BlockId, depth: u32) -> Self {
        Self { label, depth }
    }
}

// ============================================================================
// Expression Lowering
// ============================================================================

use crate::compiler::hir::nodes::{
    BinOp, HirBlock, HirExpr, HirExprKind, HirKind, HirMatchArm, HirNode, HirPattern, HirPlace,
    HirStmt, HirTerminator, UnaryOp,
};

impl LoweringContext {
    /// Lowers a HIR expression to a sequence of LIR instructions.
    ///
    /// Expressions are lowered to stack-based operations following their RPN order
    /// from the AST stage. The resulting instructions, when executed, will leave
    /// the expression's value on the WASM operand stack.
    ///
    /// # Arguments
    /// * `expr` - The HIR expression to lower
    ///
    /// # Returns
    /// A vector of LIR instructions that compute the expression value
    pub fn lower_expr(&mut self, expr: &HirExpr) -> Result<Vec<LirInst>, CompilerError> {
        match &expr.kind {
            // === Literals ===
            HirExprKind::Int(val) => self.lower_int_literal(*val),
            HirExprKind::Float(val) => self.lower_float_literal(*val),
            HirExprKind::Bool(val) => self.lower_bool_literal(*val),
            HirExprKind::Char(val) => self.lower_char_literal(*val),

            // === Variable Access ===
            HirExprKind::Load(place) => self.lower_place_load(place),

            // === Binary Operations ===
            HirExprKind::BinOp { left, op, right } => {
                self.lower_binary_op(left, *op, right, &expr.data_type)
            }

            // === Unary Operations ===
            HirExprKind::UnaryOp { op, operand } => {
                self.lower_unary_op(*op, operand, &expr.data_type)
            }

            // === String Literals ===
            HirExprKind::StringLiteral(_) | HirExprKind::HeapString(_) => {
                // String literals will be handled in a later task (memory operations)
                Err(CompilerError::lir_transformation(
                    "String literal lowering not yet implemented",
                ))
            }

            // === Field Access ===
            HirExprKind::Field { base, field } => {
                // Field access on a variable - create a HirPlace and lower it
                let place = HirPlace::Field {
                    base: Box::new(HirPlace::Var(*base)),
                    field: *field,
                };
                self.lower_place_load(&place)
            }

            // === Move ===
            HirExprKind::Move(place) => {
                // Move is similar to load but with ownership transfer
                // For now, treat it the same as load
                self.lower_place_load(place)
            }

            // === Function Calls ===
            HirExprKind::Call { target, args } => {
                // Lower function call expression
                self.lower_call_expr(*target, args)
            }

            HirExprKind::MethodCall {
                receiver,
                method,
                args,
            } => {
                // Lower method call expression
                self.lower_method_call(receiver, *method, args)
            }

            // === Constructors ===
            HirExprKind::StructConstruct {
                type_name,
                fields: _,
            } => {
                // Struct construction will be handled in task 6
                Err(CompilerError::lir_transformation(format!(
                    "Struct construction lowering not yet implemented: {}",
                    type_name
                )))
            }

            HirExprKind::Collection(_) => {
                // Collection construction will be handled in task 6
                Err(CompilerError::lir_transformation(
                    "Collection construction lowering not yet implemented",
                ))
            }

            HirExprKind::Range { start: _, end: _ } => {
                // Range construction will be handled in task 6
                Err(CompilerError::lir_transformation(
                    "Range construction lowering not yet implemented",
                ))
            }
        }
    }

    // ========================================================================
    // Literal Lowering (Task 4.1)
    // ========================================================================

    /// Lowers an integer literal to an I64Const instruction.
    ///
    /// **Validates: Requirements 6.3, 9.2**
    fn lower_int_literal(&self, value: i64) -> Result<Vec<LirInst>, CompilerError> {
        Ok(vec![LirInst::I64Const(value)])
    }

    /// Lowers a float literal to an F64Const instruction.
    ///
    /// **Validates: Requirements 6.3, 9.3**
    fn lower_float_literal(&self, value: f64) -> Result<Vec<LirInst>, CompilerError> {
        Ok(vec![LirInst::F64Const(value)])
    }

    /// Lowers a boolean literal to an I32Const instruction.
    ///
    /// Booleans are represented as I32 in WASM: 0 = false, 1 = true.
    ///
    /// **Validates: Requirements 6.3, 9.2**
    fn lower_bool_literal(&self, value: bool) -> Result<Vec<LirInst>, CompilerError> {
        let int_value = if value { 1 } else { 0 };
        Ok(vec![LirInst::I32Const(int_value)])
    }

    /// Lowers a character literal to an I32Const instruction.
    ///
    /// Characters are represented as I32 Unicode code points in WASM.
    ///
    /// **Validates: Requirements 6.3, 9.2**
    fn lower_char_literal(&self, value: char) -> Result<Vec<LirInst>, CompilerError> {
        Ok(vec![LirInst::I32Const(value as i32)])
    }

    // ========================================================================
    // Variable Load Lowering (Task 4.2)
    // ========================================================================

    /// Lowers a place load to LIR instructions.
    ///
    /// Handles three cases:
    /// - Variable: emits LocalGet instruction
    /// - Field access: emits pointer load, mask, and field load with offset
    /// - Index access: emits pointer load, mask, bounds check, and element load
    ///
    /// **Validates: Requirements 6.4, 1.3, 4.1, 4.3, 9.4**
    fn lower_place_load(&mut self, place: &HirPlace) -> Result<Vec<LirInst>, CompilerError> {
        match place {
            HirPlace::Var(name) => {
                // Look up the variable in the var_to_local map
                let local_idx = self.var_to_local.get(name).ok_or_else(|| {
                    CompilerError::lir_transformation(format!("Undefined variable: {}", name))
                })?;
                Ok(vec![LirInst::LocalGet(*local_idx)])
            }
            HirPlace::Field { base, field } => {
                // Field access: load base pointer, mask, and load field with offset
                self.lower_field_access_load(base, *field)
            }
            HirPlace::Index { base, index } => {
                // Index access: load base pointer, mask, bounds check, and load element
                self.lower_collection_element_load(base, index)
            }
        }
    }

    /// Helper function to convert a HirPlace to a string for error messages.
    fn place_to_string(&self, place: &HirPlace) -> String {
        match place {
            HirPlace::Var(name) => format!("{}", name),
            HirPlace::Field { base, field } => {
                format!("{}.{}", self.place_to_string(base), field)
            }
            HirPlace::Index { base, .. } => {
                format!("{}[...]", self.place_to_string(base))
            }
        }
    }

    // ========================================================================
    // Field Access Lowering (Task 6.1)
    // ========================================================================

    /// Lowers a struct field access to LIR instructions.
    ///
    /// The lowering process:
    /// 1. Load the base pointer (recursively handle nested field access)
    /// 2. Emit MaskPointer to remove the ownership tag
    /// 3. Look up the field offset from the struct layout
    /// 4. Emit the appropriate load instruction with the field offset
    ///
    /// **Validates: Requirements 4.1, 9.4**
    fn lower_field_access_load(
        &mut self,
        base: &HirPlace,
        field: InternedString,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Step 1: Load the base pointer
        insts.extend(self.lower_place_load(base)?);

        // Step 2: Mask out the ownership bit to get the real pointer
        insts.push(LirInst::MaskPointer);

        // Step 3: Get the struct type and look up the field layout
        let struct_type = self.get_place_struct_type(base)?;
        let field_layout = self.get_field_layout(struct_type, field).ok_or_else(|| {
            CompilerError::lir_transformation(format!(
                "Unknown field '{}' in struct '{}'",
                field, struct_type
            ))
        })?;

        // Clone the values we need before the borrow ends
        let field_offset = field_layout.offset;
        let field_ty = field_layout.ty;

        // Step 4: Emit the appropriate load instruction with the field offset
        let load_inst = self.emit_load_instruction(field_ty, field_offset);
        insts.push(load_inst);

        Ok(insts)
    }

    /// Gets the struct type name for a place.
    ///
    /// For a variable, looks up its type in the context.
    /// For nested field access, recursively determines the type.
    fn get_place_struct_type(&self, place: &HirPlace) -> Result<InternedString, CompilerError> {
        match place {
            HirPlace::Var(name) => {
                // Look up the variable's type
                // For now, we need to track variable types in the context
                // This is a simplified implementation that assumes the variable name
                // matches a struct type name (will be improved with proper type tracking)
                self.get_variable_struct_type(*name)
            }
            HirPlace::Field { base, field } => {
                // Get the base struct type, then look up the field's type
                let base_struct_type = self.get_place_struct_type(base)?;
                let field_layout =
                    self.get_field_layout(base_struct_type, *field)
                        .ok_or_else(|| {
                            CompilerError::lir_transformation(format!(
                                "Unknown field '{}' in struct '{}'",
                                field, base_struct_type
                            ))
                        })?;

                // If the field is a struct pointer (I32), we need to determine its struct type
                // This requires additional type information that we don't have yet
                // For now, return an error for nested struct field access
                if field_layout.ty == LirType::I32 {
                    Err(CompilerError::lir_transformation(format!(
                        "Nested struct field access type resolution not yet implemented for field '{}'",
                        field
                    )))
                } else {
                    Err(CompilerError::lir_transformation(format!(
                        "Field '{}' is not a struct type",
                        field
                    )))
                }
            }
            HirPlace::Index { .. } => Err(CompilerError::lir_transformation(
                "Cannot determine struct type for indexed place",
            )),
        }
    }

    /// Gets the struct type for a variable by name.
    ///
    /// This is a placeholder that will be improved with proper type tracking.
    /// For now, it looks for a struct layout with a matching name pattern.
    fn get_variable_struct_type(
        &self,
        _var_name: InternedString,
    ) -> Result<InternedString, CompilerError> {
        // In a complete implementation, we would track variable types in the context
        // For now, we'll need to rely on the caller providing type information
        // or use a naming convention

        // Return the first struct layout as a fallback (this is temporary)
        // A proper implementation would track variable -> type mappings
        if let Some(struct_name) = self.struct_layouts.keys().next() {
            Ok(*struct_name)
        } else {
            Err(CompilerError::lir_transformation(
                "No struct layouts registered - cannot determine variable struct type",
            ))
        }
    }

    /// Emits the appropriate load instruction for a given LIR type and offset.
    ///
    /// **Validates: Requirements 4.1, 9.4**
    fn emit_load_instruction(&self, ty: LirType, offset: u32) -> LirInst {
        match ty {
            LirType::I32 => LirInst::I32Load { offset, align: 4 },
            LirType::I64 => LirInst::I64Load { offset, align: 8 },
            LirType::F32 => LirInst::F32Load { offset, align: 4 },
            LirType::F64 => LirInst::F64Load { offset, align: 8 },
        }
    }

    /// Emits the appropriate store instruction for a given LIR type and offset.
    ///
    /// **Validates: Requirements 4.2, 9.4**
    fn emit_store_instruction(&self, ty: LirType, offset: u32) -> LirInst {
        match ty {
            LirType::I32 => LirInst::I32Store { offset, align: 4 },
            LirType::I64 => LirInst::I64Store { offset, align: 8 },
            LirType::F32 => LirInst::F32Store { offset, align: 4 },
            LirType::F64 => LirInst::F64Store { offset, align: 8 },
        }
    }

    // ========================================================================
    // Field Assignment Lowering (Task 6.2)
    // ========================================================================

    /// Lowers a struct field assignment to LIR instructions.
    ///
    /// The lowering process:
    /// 1. Load the base pointer
    /// 2. Mask the pointer to remove ownership tag
    /// 3. Lower the value expression
    /// 4. Look up the field offset from the struct layout
    /// 5. Emit the appropriate store instruction with the field offset
    ///
    /// Note: WASM store instructions expect [address, value] on the stack,
    /// so we need to ensure the address is pushed before the value.
    ///
    /// **Validates: Requirements 4.2, 9.4**
    pub fn lower_field_assignment(
        &mut self,
        base: &HirPlace,
        field: InternedString,
        value: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Step 1: Load the base pointer
        insts.extend(self.lower_place_load(base)?);

        // Step 2: Mask out the ownership bit to get the real pointer
        insts.push(LirInst::MaskPointer);

        // Step 3: Get the struct type and look up the field layout
        let struct_type = self.get_place_struct_type(base)?;
        let field_layout = self.get_field_layout(struct_type, field).ok_or_else(|| {
            CompilerError::lir_transformation(format!(
                "Unknown field '{}' in struct '{}'",
                field, struct_type
            ))
        })?;

        // Clone the values we need before the borrow ends
        let field_offset = field_layout.offset;
        let field_ty = field_layout.ty;

        // Step 4: Lower the value expression
        // The value will be pushed onto the stack after the address
        insts.extend(self.lower_expr(value)?);

        // Step 5: Emit the appropriate store instruction with the field offset
        let store_inst = self.emit_store_instruction(field_ty, field_offset);
        insts.push(store_inst);

        Ok(insts)
    }

    // ========================================================================
    // Collection Element Access Lowering (Task 6.3)
    // ========================================================================

    /// Lowers a collection element access to LIR instructions.
    ///
    /// The lowering process:
    /// 1. Load the base collection pointer
    /// 2. Mask the pointer to remove ownership tag
    /// 3. Lower the index expression
    /// 4. Emit bounds check call (runtime function)
    /// 5. Calculate element offset and emit load instruction
    ///
    /// Collection layout (from design.md):
    /// - [0-3]: length (i32)
    /// - [4-7]: capacity (i32)
    /// - [8-11]: element_size (i32)
    /// - [12+]: element data
    ///
    /// **Validates: Requirements 4.3**
    fn lower_collection_element_load(
        &mut self,
        base: &HirPlace,
        index: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Step 1: Load the base collection pointer
        insts.extend(self.lower_place_load(base)?);

        // Step 2: Mask out the ownership bit to get the real pointer
        insts.push(LirInst::MaskPointer);

        // Store the base pointer in a temporary local for reuse
        let base_ptr_local = self.local_allocator.allocate(LirType::I32);
        insts.push(LirInst::LocalTee(base_ptr_local));

        // Step 3: Lower the index expression
        insts.extend(self.lower_expr(index)?);

        // Store the index in a temporary local
        let index_local = self.local_allocator.allocate(LirType::I64);
        insts.push(LirInst::LocalSet(index_local));

        // Step 4: Emit bounds check
        // Load base pointer and index for bounds check
        insts.push(LirInst::LocalGet(base_ptr_local));
        insts.push(LirInst::LocalGet(index_local));

        // Call bounds check function (this will be a runtime function)
        // The bounds check function takes (collection_ptr, index) and returns the validated index
        // or traps if out of bounds
        insts.push(LirInst::Call(BOUNDS_CHECK_FUNC_INDEX));

        // Step 5: Calculate element offset and emit load
        // Element offset = COLLECTION_HEADER_SIZE + (index * element_size)
        // For simplicity, we assume I64 elements (8 bytes each)
        // A more complete implementation would look up the element type

        // Load base pointer again
        insts.push(LirInst::LocalGet(base_ptr_local));

        // Load index
        insts.push(LirInst::LocalGet(index_local));

        // Calculate offset: index * 8 (assuming I64 elements)
        insts.push(LirInst::I64Const(ELEMENT_SIZE_I64 as i64));
        insts.push(LirInst::I64Mul);

        // Add header offset
        insts.push(LirInst::I64Const(COLLECTION_HEADER_SIZE as i64));
        insts.push(LirInst::I64Add);

        // Convert to I32 for address calculation
        // Note: This is a simplification; proper implementation would handle this better
        insts.push(LirInst::I32Const(0)); // Placeholder for i64 to i32 conversion

        // Add to base pointer and load
        insts.push(LirInst::I32Add);

        // Emit load instruction for I64 element
        insts.push(LirInst::I64Load { offset: 0, align: 8 });

        // Free temporary locals
        self.local_allocator.free(base_ptr_local);
        self.local_allocator.free(index_local);

        Ok(insts)
    }

    // ========================================================================
    // Collection Element Assignment Lowering (Task 6.4)
    // ========================================================================

    /// Lowers a collection element assignment to LIR instructions.
    ///
    /// The lowering process:
    /// 1. Load the base collection pointer
    /// 2. Mask the pointer to remove ownership tag
    /// 3. Lower the index expression
    /// 4. Emit bounds check call
    /// 5. Lower the value expression
    /// 6. Emit element store instruction
    ///
    /// **Validates: Requirements 4.4**
    pub fn lower_collection_element_assignment(
        &mut self,
        base: &HirPlace,
        index: &HirExpr,
        value: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Step 1: Load the base collection pointer
        insts.extend(self.lower_place_load(base)?);

        // Step 2: Mask out the ownership bit to get the real pointer
        insts.push(LirInst::MaskPointer);

        // Store the base pointer in a temporary local for reuse
        let base_ptr_local = self.local_allocator.allocate(LirType::I32);
        insts.push(LirInst::LocalTee(base_ptr_local));

        // Step 3: Lower the index expression
        insts.extend(self.lower_expr(index)?);

        // Store the index in a temporary local
        let index_local = self.local_allocator.allocate(LirType::I64);
        insts.push(LirInst::LocalSet(index_local));

        // Step 4: Emit bounds check
        insts.push(LirInst::LocalGet(base_ptr_local));
        insts.push(LirInst::LocalGet(index_local));
        insts.push(LirInst::Call(BOUNDS_CHECK_FUNC_INDEX));

        // Step 5: Calculate element address
        // Load base pointer
        insts.push(LirInst::LocalGet(base_ptr_local));

        // Load index and calculate offset
        insts.push(LirInst::LocalGet(index_local));
        insts.push(LirInst::I64Const(ELEMENT_SIZE_I64 as i64));
        insts.push(LirInst::I64Mul);
        insts.push(LirInst::I64Const(COLLECTION_HEADER_SIZE as i64));
        insts.push(LirInst::I64Add);

        // Convert to I32 for address calculation (simplified)
        insts.push(LirInst::I32Const(0)); // Placeholder for i64 to i32 conversion
        insts.push(LirInst::I32Add);

        // Store the calculated address in a temporary
        let addr_local = self.local_allocator.allocate(LirType::I32);
        insts.push(LirInst::LocalSet(addr_local));

        // Step 6: Lower the value expression
        insts.extend(self.lower_expr(value)?);

        // Load the address back onto the stack (WASM store expects [addr, value])
        // We need to swap the order, so we store value temporarily
        let value_local = self.local_allocator.allocate(LirType::I64);
        insts.push(LirInst::LocalSet(value_local));
        insts.push(LirInst::LocalGet(addr_local));
        insts.push(LirInst::LocalGet(value_local));

        // Emit store instruction for I64 element
        insts.push(LirInst::I64Store { offset: 0, align: 8 });

        // Free temporary locals
        self.local_allocator.free(base_ptr_local);
        self.local_allocator.free(index_local);
        self.local_allocator.free(addr_local);
        self.local_allocator.free(value_local);

        Ok(insts)
    }

    // ========================================================================
    // Binary Operation Lowering (Task 4.3)
    // ========================================================================

    /// Lowers a binary operation to LIR instructions.
    ///
    /// Binary operations are lowered in RPN order:
    /// 1. Lower left operand (pushes value onto stack)
    /// 2. Lower right operand (pushes value onto stack)
    /// 3. Emit the operation instruction (consumes two values, produces one)
    ///
    /// **Validates: Requirements 6.1, 9.1, 9.2, 9.3**
    fn lower_binary_op(
        &mut self,
        left: &HirExpr,
        op: BinOp,
        right: &HirExpr,
        _result_type: &DataType,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower left operand (recursive call)
        insts.extend(self.lower_expr(left)?);

        // Lower right operand (recursive call)
        insts.extend(self.lower_expr(right)?);

        // Emit the appropriate operation instruction based on type
        // Use the left operand's type to determine the instruction variant
        let op_inst = self.lower_binop_instruction(op, &left.data_type)?;
        insts.push(op_inst);

        Ok(insts)
    }

    /// Maps a BinOp variant to the appropriate LIR instruction based on type.
    ///
    /// **Validates: Requirements 9.1, 9.2, 9.3**
    fn lower_binop_instruction(
        &self,
        op: BinOp,
        operand_type: &DataType,
    ) -> Result<LirInst, CompilerError> {
        let lir_type = data_type_to_lir_type(operand_type);

        match (op, lir_type) {
            // Integer operations (I64)
            (BinOp::Add, LirType::I64) => Ok(LirInst::I64Add),
            (BinOp::Sub, LirType::I64) => Ok(LirInst::I64Sub),
            (BinOp::Mul, LirType::I64) => Ok(LirInst::I64Mul),
            (BinOp::Div, LirType::I64) => Ok(LirInst::I64DivS),
            (BinOp::Eq, LirType::I64) => Ok(LirInst::I64Eq),
            (BinOp::Ne, LirType::I64) => Ok(LirInst::I64Ne),
            (BinOp::Lt, LirType::I64) => Ok(LirInst::I64LtS),
            (BinOp::Gt, LirType::I64) => Ok(LirInst::I64GtS),

            // Integer operations (I32) - for Bool comparisons
            (BinOp::Add, LirType::I32) => Ok(LirInst::I32Add),
            (BinOp::Sub, LirType::I32) => Ok(LirInst::I32Sub),
            (BinOp::Mul, LirType::I32) => Ok(LirInst::I32Mul),
            (BinOp::Div, LirType::I32) => Ok(LirInst::I32DivS),
            (BinOp::Eq, LirType::I32) => Ok(LirInst::I32Eq),
            (BinOp::Ne, LirType::I32) => Ok(LirInst::I32Ne),
            (BinOp::Lt, LirType::I32) => Ok(LirInst::I32LtS),
            (BinOp::Gt, LirType::I32) => Ok(LirInst::I32GtS),

            // Float operations (F64)
            (BinOp::Add, LirType::F64) => Ok(LirInst::F64Add),
            (BinOp::Sub, LirType::F64) => Ok(LirInst::F64Sub),
            (BinOp::Mul, LirType::F64) => Ok(LirInst::F64Mul),
            (BinOp::Div, LirType::F64) => Ok(LirInst::F64Div),
            (BinOp::Eq, LirType::F64) => Ok(LirInst::F64Eq),
            (BinOp::Ne, LirType::F64) => Ok(LirInst::F64Ne),

            // Logical operations (And, Or) - work on I32 (booleans)
            (BinOp::And, LirType::I32) => {
                // Logical AND: both operands must be non-zero
                // In WASM, we can use i32.and for bitwise AND which works for 0/1 booleans
                Ok(LirInst::I32Mul) // 1 * 1 = 1, 1 * 0 = 0, 0 * 0 = 0
            }
            (BinOp::Or, LirType::I32) => {
                // Logical OR: at least one operand must be non-zero
                // We use addition and then compare to 0 for proper boolean OR
                // But for simplicity with 0/1 booleans, we can use: (a + b) != 0
                // However, WASM doesn't have a direct OR that works this way
                // For now, we'll use I32Add and rely on the result being > 0
                // A more correct implementation would be: (a | b) != 0
                Ok(LirInst::I32Add) // This works for 0/1 booleans: 0+0=0, 0+1=1, 1+0=1, 1+1=2 (truthy)
            }

            // Modulo operation
            (BinOp::Mod, LirType::I64) => {
                // WASM doesn't have a direct modulo instruction
                // We need to use i64.rem_s (signed remainder)
                // For now, return an error as we need to add I64RemS to LirInst
                Err(CompilerError::lir_transformation(
                    "Modulo operation not yet supported for I64",
                ))
            }
            (BinOp::Mod, LirType::I32) => Err(CompilerError::lir_transformation(
                "Modulo operation not yet supported for I32",
            )),

            // Less than or equal, Greater than or equal
            (BinOp::Le, LirType::I64) => {
                // a <= b is equivalent to !(a > b)
                // We'll need to emit multiple instructions for this
                // For now, return an error as we need a more complex lowering
                Err(CompilerError::lir_transformation(
                    "Less than or equal operation not yet supported for I64",
                ))
            }
            (BinOp::Ge, LirType::I64) => Err(CompilerError::lir_transformation(
                "Greater than or equal operation not yet supported for I64",
            )),
            (BinOp::Le, LirType::I32) => Err(CompilerError::lir_transformation(
                "Less than or equal operation not yet supported for I32",
            )),
            (BinOp::Ge, LirType::I32) => Err(CompilerError::lir_transformation(
                "Greater than or equal operation not yet supported for I32",
            )),

            // Float comparisons that aren't directly supported
            (BinOp::Lt, LirType::F64) | (BinOp::Gt, LirType::F64) => {
                Err(CompilerError::lir_transformation(format!(
                    "Float comparison {:?} not yet supported",
                    op
                )))
            }
            (BinOp::Le, LirType::F64) | (BinOp::Ge, LirType::F64) => {
                Err(CompilerError::lir_transformation(format!(
                    "Float comparison {:?} not yet supported",
                    op
                )))
            }

            // Exponent and Root operations
            (BinOp::Exponent, _) => Err(CompilerError::lir_transformation(
                "Exponent operation not yet supported",
            )),
            (BinOp::Root, _) => Err(CompilerError::lir_transformation(
                "Root operation not yet supported",
            )),

            // F32 operations (not commonly used in Beanstalk but included for completeness)
            (BinOp::Add, LirType::F32)
            | (BinOp::Sub, LirType::F32)
            | (BinOp::Mul, LirType::F32)
            | (BinOp::Div, LirType::F32) => {
                Err(CompilerError::lir_transformation("F32 operations not yet supported"))
            }

            // Catch-all for unsupported combinations
            _ => Err(CompilerError::lir_transformation(format!(
                "Unsupported binary operation {:?} for type {:?}",
                op, operand_type
            ))),
        }
    }

    // ========================================================================
    // Unary Operation Lowering (Task 4.4)
    // ========================================================================

    /// Lowers a unary operation to LIR instructions.
    ///
    /// Unary operations are lowered as:
    /// 1. Lower operand (pushes value onto stack)
    /// 2. Emit the operation instruction (consumes one value, produces one)
    ///
    /// **Validates: Requirements 6.2**
    fn lower_unary_op(
        &mut self,
        op: UnaryOp,
        operand: &HirExpr,
        _result_type: &DataType,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower operand (recursive call)
        insts.extend(self.lower_expr(operand)?);

        // Emit the appropriate operation instruction
        let op_insts = self.lower_unaryop_instructions(op, &operand.data_type)?;
        insts.extend(op_insts);

        Ok(insts)
    }

    /// Maps a UnaryOp variant to the appropriate LIR instructions based on type.
    ///
    /// **Validates: Requirements 6.2**
    fn lower_unaryop_instructions(
        &self,
        op: UnaryOp,
        operand_type: &DataType,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let lir_type = data_type_to_lir_type(operand_type);

        match (op, lir_type) {
            // Negation for integers (I64)
            // -x is equivalent to 0 - x
            (UnaryOp::Neg, LirType::I64) => Ok(vec![
                LirInst::I64Const(-1),
                LirInst::I64Mul, // x * -1 = -x
            ]),

            // Negation for integers (I32)
            (UnaryOp::Neg, LirType::I32) => Ok(vec![
                LirInst::I32Const(-1),
                LirInst::I32Mul, // x * -1 = -x
            ]),

            // Negation for floats (F64)
            // WASM has f64.neg instruction, but we don't have it in LirInst yet
            // Use multiplication by -1.0 as a workaround
            (UnaryOp::Neg, LirType::F64) => Ok(vec![
                LirInst::F64Const(-1.0),
                LirInst::F64Mul, // x * -1.0 = -x
            ]),

            // Logical NOT for booleans (I32)
            // !x is equivalent to x == 0 (flips 0 to 1 and non-zero to 0)
            (UnaryOp::Not, LirType::I32) => Ok(vec![
                LirInst::I32Const(0),
                LirInst::I32Eq, // x == 0 gives 1 if x is 0, 0 otherwise
            ]),

            // Unsupported combinations
            (UnaryOp::Not, _) => Err(CompilerError::lir_transformation(format!(
                "Logical NOT only supported for boolean (I32) types, got {:?}",
                operand_type
            ))),
            (UnaryOp::Neg, LirType::F32) => {
                Err(CompilerError::lir_transformation("F32 negation not yet supported"))
            }
        }
    }

    // ========================================================================
    // Ownership Operations (Task 7)
    // ========================================================================

    // ------------------------------------------------------------------------
    // Task 7.1: Ownership Tagging Helpers
    // ------------------------------------------------------------------------

    /// Emits a `TagAsOwned` instruction for the given local.
    ///
    /// This sets the ownership bit (bit 0) to 1 in the tagged pointer,
    /// indicating that the value is owned and the holder is responsible
    /// for dropping it.
    ///
    /// Tagged pointer encoding:
    /// - Bit 0: Ownership flag (1 = owned, 0 = borrowed)
    /// - Bits 1-31: Actual pointer address
    ///
    /// Operation: `local = local | 1`
    ///
    /// **Validates: Requirements 3.1, 3.4**
    pub fn emit_tag_as_owned(&self, local_idx: u32) -> LirInst {
        LirInst::TagAsOwned(local_idx)
    }

    /// Emits a `TagAsBorrowed` instruction for the given local.
    ///
    /// This clears the ownership bit (bit 0) to 0 in the tagged pointer,
    /// indicating that the value is borrowed and the holder must not drop it.
    ///
    /// Tagged pointer encoding:
    /// - Bit 0: Ownership flag (1 = owned, 0 = borrowed)
    /// - Bits 1-31: Actual pointer address
    ///
    /// Operation: `local = local & ~1`
    ///
    /// **Validates: Requirements 3.5**
    pub fn emit_tag_as_borrowed(&self, local_idx: u32) -> LirInst {
        LirInst::TagAsBorrowed(local_idx)
    }

    /// Emits a `MaskPointer` instruction.
    ///
    /// This extracts the real pointer address from a tagged pointer by
    /// masking out the ownership bit. The result is left on the stack.
    ///
    /// Stack effect: [tagged_ptr] -> [real_ptr]
    /// Operation: `result = tagged_ptr & ~1`
    ///
    /// This is used when accessing the actual memory location pointed to
    /// by a tagged pointer, such as when loading struct fields or
    /// collection elements.
    ///
    /// **Validates: Requirements 3.2**
    pub fn emit_mask_pointer(&self) -> LirInst {
        LirInst::MaskPointer
    }

    /// Emits a `TestOwnership` instruction.
    ///
    /// This tests the ownership bit of a tagged pointer and leaves the
    /// result on the stack (1 = owned, 0 = borrowed).
    ///
    /// Stack effect: [tagged_ptr] -> [ownership_bit]
    /// Operation: `result = tagged_ptr & 1`
    ///
    /// This is used in conditional drop logic to determine whether a
    /// value should be freed.
    ///
    /// **Validates: Requirements 3.3**
    pub fn emit_test_ownership(&self) -> LirInst {
        LirInst::TestOwnership
    }

    // ------------------------------------------------------------------------
    // Task 7.2: Possible Drop Lowering
    // ------------------------------------------------------------------------

    /// Lowers a `HirStmt::PossibleDrop` to LIR instructions.
    ///
    /// This emits a conditional drop instruction that will free the value
    /// only if it is owned at runtime. The ownership is determined by
    /// testing the ownership bit in the tagged pointer.
    ///
    /// The `PossibleDrop` instruction in LIR will:
    /// 1. Load the local's value
    /// 2. Test the ownership bit
    /// 3. If owned (bit = 1), call the free function
    /// 4. If borrowed (bit = 0), do nothing
    ///
    /// **Validates: Requirements 3.3**
    pub fn lower_possible_drop(&mut self, place: &HirPlace) -> Result<Vec<LirInst>, CompilerError> {
        let local_idx = self.get_local_for_place(place)?;
        Ok(vec![LirInst::PossibleDrop(local_idx)])
    }

    /// Gets the local index for a HirPlace.
    ///
    /// For simple variables, this looks up the variable in the var_to_local map.
    /// For field and index access, this returns an error as those require
    /// more complex handling (the base pointer local should be used instead).
    fn get_local_for_place(&self, place: &HirPlace) -> Result<u32, CompilerError> {
        match place {
            HirPlace::Var(name) => {
                self.var_to_local.get(name).copied().ok_or_else(|| {
                    CompilerError::lir_transformation(format!(
                        "Cannot get local for undefined variable: {}",
                        name
                    ))
                })
            }
            HirPlace::Field { base, field } => {
                // For field access, we need the base pointer's local
                // The field itself doesn't have a dedicated local
                Err(CompilerError::lir_transformation(format!(
                    "Cannot get local for field access: {}.{} - use base pointer instead",
                    self.place_to_string(base),
                    field
                )))
            }
            HirPlace::Index { base, .. } => {
                // For index access, we need the base pointer's local
                Err(CompilerError::lir_transformation(format!(
                    "Cannot get local for index access: {}[...] - use base pointer instead",
                    self.place_to_string(base)
                )))
            }
        }
    }

    // ------------------------------------------------------------------------
    // Task 7.3: Mutable Assignment with Ownership
    // ------------------------------------------------------------------------

    /// Lowers a mutable assignment with ownership tagging.
    ///
    /// This handles the `~=` assignment operator in Beanstalk, which indicates
    /// mutable access and potential ownership transfer.
    ///
    /// The lowering process:
    /// 1. Lower the value expression (pushes result onto stack)
    /// 2. Get or allocate a local for the target variable
    /// 3. Emit `LocalSet` to store the value
    /// 4. Emit `TagAsOwned` to mark the value as owned
    ///
    /// After this operation, the target variable holds an owned value and
    /// is responsible for dropping it when it goes out of scope.
    ///
    /// **Validates: Requirements 3.1, 3.4**
    pub fn lower_mutable_assign(
        &mut self,
        target: &HirPlace,
        value: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Step 1: Lower the value expression
        insts.extend(self.lower_expr(value)?);

        // Step 2: Get or allocate the target local
        let target_local = self.get_or_allocate_local(target, &value.data_type)?;

        // Step 3: Store the value in the local
        insts.push(LirInst::LocalSet(target_local));

        // Step 4: Tag the local as owned
        insts.push(self.emit_tag_as_owned(target_local));

        Ok(insts)
    }

    /// Gets an existing local for a place, or allocates a new one if needed.
    ///
    /// For simple variables, this looks up or creates a mapping in var_to_local.
    /// For field and index access, this returns an error as those require
    /// memory store operations rather than local assignment.
    fn get_or_allocate_local(
        &mut self,
        place: &HirPlace,
        data_type: &DataType,
    ) -> Result<u32, CompilerError> {
        match place {
            HirPlace::Var(name) => {
                // Check if we already have a local for this variable
                if let Some(&local_idx) = self.var_to_local.get(name) {
                    return Ok(local_idx);
                }

                // Allocate a new local for this variable
                let lir_type = data_type_to_lir_type(data_type);
                let local_idx = self.local_allocator.allocate(lir_type);
                self.var_to_local.insert(*name, local_idx);
                Ok(local_idx)
            }
            HirPlace::Field { base, field } => Err(CompilerError::lir_transformation(format!(
                "Cannot allocate local for field access: {}.{} - use memory store instead",
                self.place_to_string(base),
                field
            ))),
            HirPlace::Index { base, .. } => Err(CompilerError::lir_transformation(format!(
                "Cannot allocate local for index access: {}[...] - use memory store instead",
                self.place_to_string(base)
            ))),
        }
    }

    /// Lowers a borrowed assignment (non-mutable).
    ///
    /// This handles regular `=` assignment in Beanstalk, which creates a
    /// shared reference without ownership transfer.
    ///
    /// The lowering process:
    /// 1. Lower the value expression (pushes result onto stack)
    /// 2. Get or allocate a local for the target variable
    /// 3. Emit `LocalSet` to store the value
    /// 4. Emit `TagAsBorrowed` to mark the value as borrowed
    ///
    /// After this operation, the target variable holds a borrowed reference
    /// and must not drop the value.
    ///
    /// **Validates: Requirements 3.5**
    pub fn lower_borrowed_assign(
        &mut self,
        target: &HirPlace,
        value: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Step 1: Lower the value expression
        insts.extend(self.lower_expr(value)?);

        // Step 2: Get or allocate the target local
        let target_local = self.get_or_allocate_local(target, &value.data_type)?;

        // Step 3: Store the value in the local
        insts.push(LirInst::LocalSet(target_local));

        // Step 4: Tag the local as borrowed
        insts.push(self.emit_tag_as_borrowed(target_local));

        Ok(insts)
    }

    /// Lowers an assignment statement based on mutability.
    ///
    /// This is the main entry point for lowering `HirStmt::Assign` nodes.
    /// It dispatches to either `lower_mutable_assign` or `lower_borrowed_assign`
    /// based on the `is_mutable` flag.
    ///
    /// For field and index assignments, it delegates to the appropriate
    /// memory store functions.
    ///
    /// **Validates: Requirements 1.5, 3.1, 3.4, 3.5**
    pub fn lower_assign(
        &mut self,
        target: &HirPlace,
        value: &HirExpr,
        is_mutable: bool,
    ) -> Result<Vec<LirInst>, CompilerError> {
        match target {
            HirPlace::Var(_) => {
                // Variable assignment - use local set with ownership tagging
                if is_mutable {
                    self.lower_mutable_assign(target, value)
                } else {
                    self.lower_borrowed_assign(target, value)
                }
            }
            HirPlace::Field { base, field } => {
                // Field assignment - use memory store
                self.lower_field_assignment(base, *field, value)
            }
            HirPlace::Index { base, index } => {
                // Index assignment - use collection element store
                self.lower_collection_element_assignment(base, index, value)
            }
        }
    }

    // ========================================================================
    // Function Call Lowering (Task 8)
    // ========================================================================

    // ------------------------------------------------------------------------
    // Task 8.1: Regular Function Calls
    // ------------------------------------------------------------------------

    /// Lowers a regular function call to LIR instructions.
    ///
    /// The lowering process:
    /// 1. Lower each argument expression
    /// 2. Determine ownership transfer vs borrow for each argument
    /// 3. Emit ownership tagging instructions
    /// 4. Look up function index
    /// 5. Emit `Call` instruction
    ///
    /// For arguments that are at their last use, ownership is transferred
    /// (tagged as owned). For arguments that will be used again, they are
    /// borrowed (tagged as borrowed).
    ///
    /// **Validates: Requirements 1.4, 7.1, 7.3, 7.4**
    pub fn lower_function_call(
        &mut self,
        target: InternedString,
        args: &[HirExpr],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Step 1-3: Lower each argument with ownership tagging
        for arg in args {
            insts.extend(self.lower_argument_with_ownership(arg)?);
        }

        // Step 4: Look up function index
        let func_idx = self.get_function_index(target).ok_or_else(|| {
            CompilerError::lir_transformation(format!("Unknown function: {}", target))
        })?;

        // Step 5: Emit call instruction
        insts.push(LirInst::Call(func_idx));

        Ok(insts)
    }

    /// Lowers a function call expression (used when call result is needed).
    ///
    /// This is similar to `lower_function_call` but is called from `lower_expr`
    /// when the function call appears in an expression context.
    ///
    /// **Validates: Requirements 1.4, 7.1, 7.3, 7.4**
    pub fn lower_call_expr(
        &mut self,
        target: InternedString,
        args: &[HirExpr],
    ) -> Result<Vec<LirInst>, CompilerError> {
        // Function call expressions use the same lowering as statements
        self.lower_function_call(target, args)
    }

    /// Lowers a method call expression.
    ///
    /// Method calls are lowered similarly to function calls, but the receiver
    /// is passed as the first argument.
    ///
    /// **Validates: Requirements 1.4, 7.1, 7.3, 7.4**
    pub fn lower_method_call(
        &mut self,
        receiver: &HirExpr,
        method: InternedString,
        args: &[HirExpr],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower the receiver as the first argument with ownership tagging
        insts.extend(self.lower_argument_with_ownership(receiver)?);

        // Lower remaining arguments with ownership tagging
        for arg in args {
            insts.extend(self.lower_argument_with_ownership(arg)?);
        }

        // Look up method as a function (methods are lowered to regular functions)
        let func_idx = self.get_function_index(method).ok_or_else(|| {
            CompilerError::lir_transformation(format!("Unknown method: {}", method))
        })?;

        // Emit call instruction
        insts.push(LirInst::Call(func_idx));

        Ok(insts)
    }

    /// Lowers an argument expression with ownership tagging.
    ///
    /// This determines whether the argument should be passed as owned or borrowed
    /// based on last-use analysis, and emits the appropriate tagging instructions.
    ///
    /// For heap-allocated values (structs, collections, strings):
    /// - If this is the last use of the variable, tag as owned (ownership transfer)
    /// - Otherwise, tag as borrowed (no ownership transfer)
    ///
    /// For stack-allocated values (int, float, bool, char):
    /// - Values are copied, so no ownership tagging is needed
    ///
    /// **Validates: Requirements 7.1, 7.3, 7.4**
    fn lower_argument_with_ownership(&mut self, arg: &HirExpr) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Check if this argument is a heap-allocated type that needs ownership tagging
        let needs_ownership_tagging = self.is_heap_allocated_type(&arg.data_type);

        if needs_ownership_tagging {
            // For heap-allocated types, we need to determine ownership
            match &arg.kind {
                HirExprKind::Load(HirPlace::Var(var_name)) => {
                    // Variable load - check if this is the last use
                    let local_idx = self.var_to_local.get(var_name).ok_or_else(|| {
                        CompilerError::lir_transformation(format!(
                            "Undefined variable in function argument: {}",
                            var_name
                        ))
                    })?;

                    if self.is_last_use(*var_name) {
                        // Last use - transfer ownership
                        insts.push(LirInst::PrepareOwnedArg(*local_idx));
                    } else {
                        // Not last use - borrow
                        insts.push(LirInst::PrepareBorrowedArg(*local_idx));
                    }
                }
                HirExprKind::Move(HirPlace::Var(var_name)) => {
                    // Explicit move - always transfer ownership
                    let local_idx = self.var_to_local.get(var_name).ok_or_else(|| {
                        CompilerError::lir_transformation(format!(
                            "Undefined variable in move expression: {}",
                            var_name
                        ))
                    })?;
                    insts.push(LirInst::PrepareOwnedArg(*local_idx));
                }
                _ => {
                    // Complex expression - lower it and tag the result
                    insts.extend(self.lower_expr(arg)?);

                    // Store in a temporary local for tagging
                    let lir_type = data_type_to_lir_type(&arg.data_type);
                    let temp_local = self.local_allocator.allocate(lir_type);
                    insts.push(LirInst::LocalTee(temp_local));

                    // For complex expressions, we assume ownership is transferred
                    // (the expression creates a new value)
                    insts.push(LirInst::TagAsOwned(temp_local));
                    insts.push(LirInst::LocalGet(temp_local));

                    // Free the temporary local
                    self.local_allocator.free(temp_local);
                }
            }
        } else {
            // Stack-allocated types - just lower the expression (values are copied)
            insts.extend(self.lower_expr(arg)?);
        }

        Ok(insts)
    }

    /// Checks if a data type is heap-allocated and needs ownership tagging.
    ///
    /// Heap-allocated types include:
    /// - Strings
    /// - Structs
    /// - Collections
    /// - Templates
    ///
    /// Stack-allocated types (no ownership tagging needed):
    /// - Int, Float, Bool, Char
    fn is_heap_allocated_type(&self, data_type: &DataType) -> bool {
        match data_type {
            DataType::String => true,
            DataType::Struct(_, _) => true,
            DataType::Collection(_, _) => true,
            DataType::Template => true,
            DataType::Parameters(_) => true,
            DataType::Option(_) => true,
            DataType::Choices(_) => true,
            DataType::Reference(inner, _) => self.is_heap_allocated_type(inner),
            // Stack-allocated types
            DataType::Int
            | DataType::Float
            | DataType::Decimal
            | DataType::Bool
            | DataType::True
            | DataType::False
            | DataType::Char
            | DataType::None
            | DataType::Inferred
            | DataType::Range
            | DataType::Function(_, _)
            | DataType::CoerceToString => false,
        }
    }

    // ------------------------------------------------------------------------
    // Task 8.2: Host Function Calls
    // ------------------------------------------------------------------------

    /// Lowers a host function call to LIR instructions.
    ///
    /// Host functions are imported from the host environment (e.g., `io`).
    /// They use a separate index space from regular functions.
    ///
    /// The lowering process:
    /// 1. Lower each argument expression with ownership tagging
    /// 2. Look up or register the host function index
    /// 3. Emit call to import function
    ///
    /// **Validates: Requirements 7.5**
    pub fn lower_host_call(
        &mut self,
        target: InternedString,
        _module: InternedString,
        _import: InternedString,
        args: &[HirExpr],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Step 1: Lower each argument with ownership tagging
        for arg in args {
            insts.extend(self.lower_argument_with_ownership(arg)?);
        }

        // Step 2: Get or register the host function index
        // Host functions are registered on first use
        let host_func_idx = self.register_host_function(target);

        // Step 3: Emit call instruction
        // Note: Host function calls use the same Call instruction but with
        // indices in the import function space. The codegen stage will
        // distinguish between regular and import calls based on the index.
        // For now, we use a simple offset scheme: host functions start at
        // index 0x10000 to distinguish them from regular functions.
        let import_call_idx = 0x10000 + host_func_idx;
        insts.push(LirInst::Call(import_call_idx));

        Ok(insts)
    }

    // ------------------------------------------------------------------------
    // Task 8.3: Function Parameter Handling
    // ------------------------------------------------------------------------

    /// Lowers function parameters to WASM function parameters.
    ///
    /// This handles the function prologue where parameters are received
    /// and potentially-owned parameters have their ownership tags masked out.
    ///
    /// For each parameter:
    /// 1. Map the HIR parameter to a WASM function parameter
    /// 2. If the parameter is a heap-allocated type, emit `HandleOwnedParam`
    ///    to mask out the ownership tag and store the real pointer
    ///
    /// **Validates: Requirements 3.2, 5.5**
    pub fn lower_function_parameters(
        &mut self,
        params: &[(InternedString, DataType)],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        for (param_idx, (param_name, param_type)) in params.iter().enumerate() {
            let param_local = param_idx as u32;

            // Register the parameter in var_to_local
            // For heap-allocated types, we'll use a separate local for the untagged pointer
            if self.is_heap_allocated_type(param_type) {
                // Allocate a local for the untagged pointer
                let lir_type = data_type_to_lir_type(param_type);
                let real_ptr_local = self.local_allocator.allocate(lir_type);

                // Emit HandleOwnedParam to extract the real pointer
                insts.push(LirInst::HandleOwnedParam {
                    param_local,
                    real_ptr_local,
                });

                // Map the parameter name to the untagged pointer local
                self.var_to_local.insert(*param_name, real_ptr_local);
            } else {
                // Stack-allocated types - use the parameter directly
                self.var_to_local.insert(*param_name, param_local);
            }
        }

        Ok(insts)
    }

    /// Converts function signature parameters to LIR types.
    ///
    /// This is used when building the LirFunction to specify the
    /// parameter types in the WASM function signature.
    ///
    /// **Validates: Requirements 5.5**
    pub fn params_to_lir_types(&self, params: &[(InternedString, DataType)]) -> Vec<LirType> {
        params
            .iter()
            .map(|(_, data_type)| data_type_to_lir_type(data_type))
            .collect()
    }

    /// Converts function return types to LIR types.
    ///
    /// This is used when building the LirFunction to specify the
    /// return types in the WASM function signature.
    pub fn returns_to_lir_types(&self, returns: &[DataType]) -> Vec<LirType> {
        returns.iter().map(data_type_to_lir_type).collect()
    }

    // ========================================================================
    // Control Flow Lowering (Task 10)
    // ========================================================================

    // ------------------------------------------------------------------------
    // Task 10.1: If-Statement Lowering
    // ------------------------------------------------------------------------

    /// Lowers a HIR if-statement to LIR instructions.
    ///
    /// The lowering process:
    /// 1. Lower the condition expression (pushes boolean onto stack)
    /// 2. Lower the then block (recursive call to lower_block)
    /// 3. Lower the else block if present (recursive call to lower_block)
    /// 4. Emit `LirInst::If` with the then and else branches
    ///
    /// WASM if-else semantics:
    /// - The condition is consumed from the stack
    /// - If condition is non-zero, execute then branch
    /// - If condition is zero and else branch exists, execute else branch
    ///
    /// **Validates: Requirements 2.1**
    pub fn lower_if(
        &mut self,
        condition: &HirExpr,
        then_block: BlockId,
        else_block: Option<BlockId>,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Step 1: Lower the condition expression
        // This pushes a boolean (I32: 0 or 1) onto the stack
        insts.extend(self.lower_expr(condition)?);

        // Step 2: Lower the then block
        let then_insts = self.lower_block(then_block, blocks)?;

        // Step 3: Lower the else block if present
        let else_insts = if let Some(else_id) = else_block {
            Some(self.lower_block(else_id, blocks)?)
        } else {
            None
        };

        // Step 4: Emit LIR if instruction
        insts.push(LirInst::If {
            then_branch: then_insts,
            else_branch: else_insts,
        });

        Ok(insts)
    }

    // ------------------------------------------------------------------------
    // Task 10.2: Match Expression Lowering
    // ------------------------------------------------------------------------

    /// Lowers a HIR match expression to LIR instructions.
    ///
    /// Match expressions are lowered to a sequence of conditional branches:
    /// 1. Lower the scrutinee expression (value being matched)
    /// 2. For each arm:
    ///    a. Lower the pattern (comparison against scrutinee)
    ///    b. Lower the guard if present
    ///    c. Lower the arm body block
    /// 3. Handle the default case if present
    ///
    /// The lowering uses nested if-else structures to implement pattern matching:
    /// ```
    /// if pattern1_matches {
    ///     arm1_body
    /// } else if pattern2_matches {
    ///     arm2_body
    /// } else {
    ///     default_body
    /// }
    /// ```
    ///
    /// **Validates: Requirements 2.2**
    pub fn lower_match(
        &mut self,
        scrutinee: &HirExpr,
        arms: &[HirMatchArm],
        default_block: Option<BlockId>,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Step 1: Lower the scrutinee expression and store in a temporary
        insts.extend(self.lower_expr(scrutinee)?);

        // Store scrutinee in a temporary local for repeated comparisons
        let scrutinee_type = data_type_to_lir_type(&scrutinee.data_type);
        let scrutinee_local = self.local_allocator.allocate(scrutinee_type);
        insts.push(LirInst::LocalSet(scrutinee_local));

        // Step 2: Build nested if-else structure for arms
        let match_insts = self.lower_match_arms(scrutinee_local, scrutinee_type, arms, default_block, blocks)?;
        insts.extend(match_insts);

        // Free the scrutinee local
        self.local_allocator.free(scrutinee_local);

        Ok(insts)
    }

    /// Lowers match arms to nested if-else instructions.
    ///
    /// This recursively builds the if-else chain for pattern matching.
    fn lower_match_arms(
        &mut self,
        scrutinee_local: u32,
        scrutinee_type: LirType,
        arms: &[HirMatchArm],
        default_block: Option<BlockId>,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        if arms.is_empty() {
            // No more arms - lower the default block if present
            if let Some(default_id) = default_block {
                return self.lower_block(default_id, blocks);
            } else {
                // No default - this should be caught by exhaustiveness checking
                // For now, emit a nop
                return Ok(vec![LirInst::Nop]);
            }
        }

        let arm = &arms[0];
        let remaining_arms = &arms[1..];

        // Lower the pattern comparison
        let pattern_insts = self.lower_pattern_comparison(scrutinee_local, scrutinee_type, &arm.pattern)?;

        // Lower the guard if present
        let condition_insts = if let Some(guard) = &arm.guard {
            let mut guard_insts = pattern_insts;
            // Pattern must match AND guard must be true
            // Load pattern result, then evaluate guard, then AND them
            let pattern_result_local = self.local_allocator.allocate(LirType::I32);
            guard_insts.push(LirInst::LocalSet(pattern_result_local));
            guard_insts.extend(self.lower_expr(guard)?);
            guard_insts.push(LirInst::LocalGet(pattern_result_local));
            guard_insts.push(LirInst::I32Mul); // AND operation for booleans
            self.local_allocator.free(pattern_result_local);
            guard_insts
        } else {
            pattern_insts
        };

        // Lower the arm body
        let body_insts = self.lower_block(arm.body, blocks)?;

        // Recursively lower remaining arms as the else branch
        let else_insts = self.lower_match_arms(scrutinee_local, scrutinee_type, remaining_arms, default_block, blocks)?;

        // Build the if-else structure
        let mut insts = condition_insts;
        insts.push(LirInst::If {
            then_branch: body_insts,
            else_branch: if else_insts.is_empty() { None } else { Some(else_insts) },
        });

        Ok(insts)
    }

    /// Lowers a pattern comparison to LIR instructions.
    ///
    /// The result is a boolean (I32: 0 or 1) on the stack indicating
    /// whether the pattern matches the scrutinee.
    fn lower_pattern_comparison(
        &mut self,
        scrutinee_local: u32,
        scrutinee_type: LirType,
        pattern: &HirPattern,
    ) -> Result<Vec<LirInst>, CompilerError> {
        match pattern {
            HirPattern::Literal(lit_expr) => {
                // Compare scrutinee with literal value
                let mut insts = Vec::new();

                // Load scrutinee
                insts.push(LirInst::LocalGet(scrutinee_local));

                // Lower the literal expression
                insts.extend(self.lower_expr(lit_expr)?);

                // Emit equality comparison based on type
                let eq_inst = match scrutinee_type {
                    LirType::I32 => LirInst::I32Eq,
                    LirType::I64 => LirInst::I64Eq,
                    LirType::F64 => LirInst::F64Eq,
                    LirType::F32 => {
                        return Err(CompilerError::lir_transformation(
                            "F32 pattern matching not yet supported",
                        ))
                    }
                };
                insts.push(eq_inst);

                Ok(insts)
            }
            HirPattern::Range { start, end } => {
                // Check if scrutinee is within range [start, end]
                // scrutinee >= start AND scrutinee <= end
                let mut insts = Vec::new();

                // Check scrutinee >= start
                insts.push(LirInst::LocalGet(scrutinee_local));
                insts.extend(self.lower_expr(start)?);
                let ge_insts = self.emit_greater_or_equal(scrutinee_type)?;
                insts.extend(ge_insts);

                // Store result in temporary
                let ge_result_local = self.local_allocator.allocate(LirType::I32);
                insts.push(LirInst::LocalSet(ge_result_local));

                // Check scrutinee <= end
                insts.push(LirInst::LocalGet(scrutinee_local));
                insts.extend(self.lower_expr(end)?);
                let le_insts = self.emit_less_or_equal(scrutinee_type)?;
                insts.extend(le_insts);

                // AND the two results
                insts.push(LirInst::LocalGet(ge_result_local));
                insts.push(LirInst::I32Mul); // AND for booleans

                self.local_allocator.free(ge_result_local);

                Ok(insts)
            }
            HirPattern::Wildcard => {
                // Wildcard always matches
                Ok(vec![LirInst::I32Const(1)])
            }
        }
    }

    /// Emits instructions for greater-or-equal comparison.
    ///
    /// a >= b is equivalent to NOT(a < b)
    fn emit_greater_or_equal(&self, ty: LirType) -> Result<Vec<LirInst>, CompilerError> {
        // a >= b is equivalent to !(a < b)
        let lt_inst = match ty {
            LirType::I32 => LirInst::I32LtS,
            LirType::I64 => LirInst::I64LtS,
            LirType::F64 | LirType::F32 => {
                return Err(CompilerError::lir_transformation(
                    "Float comparison >= not yet supported",
                ))
            }
        };

        // Emit: lt, then negate (eqz)
        Ok(vec![
            lt_inst,
            LirInst::I32Const(0),
            LirInst::I32Eq, // NOT operation: x == 0
        ])
    }

    /// Emits instructions for less-or-equal comparison.
    ///
    /// a <= b is equivalent to NOT(a > b)
    fn emit_less_or_equal(&self, ty: LirType) -> Result<Vec<LirInst>, CompilerError> {
        // a <= b is equivalent to !(a > b)
        let gt_inst = match ty {
            LirType::I32 => LirInst::I32GtS,
            LirType::I64 => LirInst::I64GtS,
            LirType::F64 | LirType::F32 => {
                return Err(CompilerError::lir_transformation(
                    "Float comparison <= not yet supported",
                ))
            }
        };

        // Emit: gt, then negate (eqz)
        Ok(vec![
            gt_inst,
            LirInst::I32Const(0),
            LirInst::I32Eq, // NOT operation: x == 0
        ])
    }

    // ------------------------------------------------------------------------
    // Task 10.3: Loop Lowering
    // ------------------------------------------------------------------------

    /// Lowers a HIR loop to LIR instructions.
    ///
    /// The lowering process:
    /// 1. Push loop context onto the loop stack (for break/continue)
    /// 2. Lower iterator setup if present (for-in loops)
    /// 3. Lower the loop body block
    /// 4. Emit `LirInst::Loop` with body instructions
    /// 5. Pop loop context from the stack
    ///
    /// WASM loop semantics:
    /// - Loop instruction creates a label at the start of the loop
    /// - `br` to the loop label jumps to the start (continue)
    /// - `br` to an outer block exits the loop (break)
    ///
    /// For proper break handling, we wrap the loop in a block:
    /// ```
    /// block  ;; break target (depth 0 from inside loop body)
    ///   loop  ;; continue target (depth 0 from inside loop body)
    ///     body...
    ///     br 1  ;; continue (jump to loop start)
    ///   end
    /// end
    /// ```
    ///
    /// **Validates: Requirements 2.3**
    pub fn lower_loop(
        &mut self,
        label: BlockId,
        binding: Option<(InternedString, DataType)>,
        iterator: Option<&HirExpr>,
        body: BlockId,
        index_binding: Option<InternedString>,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        // Step 1: Push loop context for break/continue handling
        let loop_depth = self.loop_stack.len() as u32;
        self.loop_stack.push(LoopContext::new(label, loop_depth));

        let mut insts = Vec::new();

        // Step 2: Handle iterator setup if present (for-in loops)
        if let Some(iter_expr) = iterator {
            insts.extend(self.lower_iterator_setup(&binding, &index_binding, iter_expr)?);
        }

        // Step 3: Lower the loop body
        let body_insts = self.lower_block(body, blocks)?;

        // Step 4: Emit LIR loop instruction
        // We wrap the loop in a block for proper break handling
        // The block is the break target, the loop is the continue target
        insts.push(LirInst::Block {
            instructions: vec![LirInst::Loop {
                instructions: body_insts,
            }],
        });

        // Step 5: Pop loop context
        self.loop_stack.pop();

        Ok(insts)
    }

    /// Lowers iterator setup for for-in loops.
    ///
    /// This handles the initialization of loop variables for iteration:
    /// - For range iteration: initialize counter variable
    /// - For collection iteration: initialize index and element variables
    fn lower_iterator_setup(
        &mut self,
        binding: &Option<(InternedString, DataType)>,
        index_binding: &Option<InternedString>,
        iterator: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Allocate locals for loop variables
        if let Some((var_name, var_type)) = binding {
            let lir_type = data_type_to_lir_type(var_type);
            let local_idx = self.local_allocator.allocate(lir_type);
            self.var_to_local.insert(*var_name, local_idx);

            // Initialize the loop variable based on iterator type
            match &iterator.kind {
                HirExprKind::Range { start, .. } => {
                    // For range iteration, initialize to start value
                    insts.extend(self.lower_expr(start)?);
                    insts.push(LirInst::LocalSet(local_idx));
                }
                HirExprKind::Load(_) | HirExprKind::Collection(_) => {
                    // For collection iteration, we need to set up index tracking
                    // The element will be loaded in each iteration
                    // Initialize to a default value (will be set in loop body)
                    match lir_type {
                        LirType::I32 => insts.push(LirInst::I32Const(0)),
                        LirType::I64 => insts.push(LirInst::I64Const(0)),
                        LirType::F32 => insts.push(LirInst::F32Const(0.0)),
                        LirType::F64 => insts.push(LirInst::F64Const(0.0)),
                    }
                    insts.push(LirInst::LocalSet(local_idx));
                }
                _ => {
                    // For other iterator types, just initialize to default
                    match lir_type {
                        LirType::I32 => insts.push(LirInst::I32Const(0)),
                        LirType::I64 => insts.push(LirInst::I64Const(0)),
                        LirType::F32 => insts.push(LirInst::F32Const(0.0)),
                        LirType::F64 => insts.push(LirInst::F64Const(0.0)),
                    }
                    insts.push(LirInst::LocalSet(local_idx));
                }
            }
        }

        // Allocate index variable if present
        if let Some(idx_name) = index_binding {
            let idx_local = self.local_allocator.allocate(LirType::I64);
            self.var_to_local.insert(*idx_name, idx_local);
            // Initialize index to 0
            insts.push(LirInst::I64Const(0));
            insts.push(LirInst::LocalSet(idx_local));
        }

        Ok(insts)
    }

    // ------------------------------------------------------------------------
    // Task 10.4: Break and Continue Lowering
    // ------------------------------------------------------------------------

    /// Lowers a HIR break statement to LIR instructions.
    ///
    /// Break exits the target loop by branching to the outer block.
    /// The branch depth is calculated based on the loop nesting.
    ///
    /// In our loop structure:
    /// ```
    /// block  ;; depth 0 from inside loop (break target)
    ///   loop  ;; depth 0 from inside loop body
    ///     ...
    ///     br 1  ;; break: branch to outer block
    ///   end
    /// end
    /// ```
    ///
    /// **Validates: Requirements 2.4**
    pub fn lower_break(&self, target: BlockId) -> Result<Vec<LirInst>, CompilerError> {
        // Find the target loop in the loop stack
        let loop_ctx = self.find_loop_context(target)?;

        // Calculate branch depth
        // We need to branch to the outer block (break target)
        // Each loop adds 2 levels: the outer block and the loop itself
        // From inside the loop body, depth 1 reaches the outer block
        let current_depth = self.loop_stack.len() as u32;
        let target_depth = loop_ctx.depth;
        let nesting_diff = current_depth - target_depth - 1;

        // Branch depth: 1 (to exit loop) + 2 * nesting_diff (for nested loops)
        let branch_depth = 1 + nesting_diff * 2;

        Ok(vec![LirInst::Br(branch_depth)])
    }

    /// Lowers a HIR continue statement to LIR instructions.
    ///
    /// Continue jumps to the start of the target loop by branching to the loop label.
    ///
    /// In our loop structure:
    /// ```
    /// block  ;; depth 1 from inside loop body
    ///   loop  ;; depth 0 from inside loop body (continue target)
    ///     ...
    ///     br 0  ;; continue: branch to loop start
    ///   end
    /// end
    /// ```
    ///
    /// **Validates: Requirements 2.4**
    pub fn lower_continue(&self, target: BlockId) -> Result<Vec<LirInst>, CompilerError> {
        // Find the target loop in the loop stack
        let loop_ctx = self.find_loop_context(target)?;

        // Calculate branch depth
        // We need to branch to the loop label (continue target)
        // From inside the loop body, depth 0 reaches the loop start
        let current_depth = self.loop_stack.len() as u32;
        let target_depth = loop_ctx.depth;
        let nesting_diff = current_depth - target_depth - 1;

        // Branch depth: 0 (to loop start) + 2 * nesting_diff (for nested loops)
        let branch_depth = nesting_diff * 2;

        Ok(vec![LirInst::Br(branch_depth)])
    }

    /// Finds the loop context for a given target block ID.
    fn find_loop_context(&self, target: BlockId) -> Result<&LoopContext, CompilerError> {
        self.loop_stack
            .iter()
            .rev()
            .find(|ctx| ctx.label == target)
            .ok_or_else(|| {
                CompilerError::lir_transformation(format!(
                    "Break/continue target not found in loop stack: block {}",
                    target
                ))
            })
    }

    // ------------------------------------------------------------------------
    // Task 10.5: Return Lowering
    // ------------------------------------------------------------------------

    /// Lowers a HIR return statement to LIR instructions.
    ///
    /// The lowering process:
    /// 1. Lower each return value expression (pushes values onto stack)
    /// 2. Emit `LirInst::Return`
    ///
    /// WASM return semantics:
    /// - Return values must be on the stack in order
    /// - The return instruction pops the values and exits the function
    ///
    /// **Validates: Requirements 2.5, 7.2**
    pub fn lower_return(&mut self, values: &[HirExpr]) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Step 1: Lower each return value expression
        // Values are pushed onto the stack in order
        for value in values {
            insts.extend(self.lower_expr(value)?);
        }

        // Step 2: Emit return instruction
        insts.push(LirInst::Return);

        Ok(insts)
    }

    /// Lowers a HIR error return statement to LIR instructions.
    ///
    /// Error returns are similar to regular returns but return the error value.
    /// In Beanstalk's error handling model, this returns the error variant
    /// of a Result type.
    ///
    /// **Validates: Requirements 2.5, 7.2**
    pub fn lower_return_error(&mut self, error: &HirExpr) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower the error expression
        insts.extend(self.lower_expr(error)?);

        // Emit return instruction
        insts.push(LirInst::Return);

        Ok(insts)
    }

    /// Lowers a HIR panic statement to LIR instructions.
    ///
    /// Panic terminates execution with an optional message.
    /// This is lowered to an unreachable instruction in WASM.
    pub fn lower_panic(&mut self, message: Option<&HirExpr>) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // If there's a message, we could call a panic handler
        // For now, we just emit unreachable
        if let Some(msg) = message {
            // Lower the message expression (for potential panic handler)
            insts.extend(self.lower_expr(msg)?);
            // Drop the message since we're not using it yet
            insts.push(LirInst::Drop);
        }

        // Emit unreachable (trap)
        // Note: We don't have an Unreachable instruction in LirInst yet
        // For now, we'll use a call to a panic function (index 0xFFFFFFFF)
        insts.push(LirInst::Call(0xFFFFFFFF));

        Ok(insts)
    }

    // ========================================================================
    // Block Lowering (Task 13.2)
    // ========================================================================

    /// Lowers a HIR block to a sequence of LIR instructions.
    ///
    /// This iterates through all nodes in the block and lowers each one
    /// based on its kind (statement or terminator).
    ///
    /// **Validates: Requirements 1.1**
    pub fn lower_block(
        &mut self,
        block_id: BlockId,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        // Find the block by ID
        let block = blocks
            .iter()
            .find(|b| b.id == block_id)
            .ok_or_else(|| {
                CompilerError::lir_transformation(format!("Block not found: {}", block_id))
            })?;

        let mut insts = Vec::new();

        // Lower each node in the block
        for node in &block.nodes {
            let node_insts = self.lower_hir_node(node, blocks)?;
            insts.extend(node_insts);
        }

        Ok(insts)
    }

    /// Lowers a single HIR node to LIR instructions.
    ///
    /// Dispatches to the appropriate lowering function based on the node kind.
    fn lower_hir_node(
        &mut self,
        node: &HirNode,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        match &node.kind {
            HirKind::Stmt(stmt) => self.lower_stmt(stmt, blocks),
            HirKind::Terminator(term) => self.lower_terminator(term, blocks),
        }
    }

    /// Lowers a HIR statement to LIR instructions.
    fn lower_stmt(
        &mut self,
        stmt: &HirStmt,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        match stmt {
            HirStmt::Assign {
                target,
                value,
                is_mutable,
            } => self.lower_assign(target, value, *is_mutable),

            HirStmt::Call { target, args } => self.lower_function_call(*target, args),

            HirStmt::HostCall {
                target,
                module,
                import,
                args,
            } => self.lower_host_call(*target, *module, *import, args),

            HirStmt::PossibleDrop(place) => self.lower_possible_drop(place),

            HirStmt::ExprStmt(expr) => {
                let mut insts = self.lower_expr(expr)?;
                // Drop the result if the expression produces a value
                if !matches!(expr.data_type, DataType::None) {
                    insts.push(LirInst::Drop);
                }
                Ok(insts)
            }

            HirStmt::FunctionDef { .. } => {
                // Function definitions are handled at the module level
                Ok(vec![])
            }

            HirStmt::StructDef { .. } => {
                // Struct definitions are handled at the module level
                Ok(vec![])
            }

            HirStmt::RuntimeTemplateCall {
                template_fn,
                captures,
                ..
            } => {
                // Lower as a function call to the template function
                self.lower_function_call(*template_fn, captures)
            }

            HirStmt::TemplateFn { name, params, body } => {
                // Template functions are lowered like regular functions
                // For now, just lower the body
                let _ = (name, params); // Suppress unused warnings
                self.lower_block(*body, blocks)
            }
        }
    }

    /// Lowers a HIR terminator to LIR instructions.
    fn lower_terminator(
        &mut self,
        term: &HirTerminator,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        match term {
            HirTerminator::If {
                condition,
                then_block,
                else_block,
            } => self.lower_if(condition, *then_block, *else_block, blocks),

            HirTerminator::Match {
                scrutinee,
                arms,
                default_block,
            } => self.lower_match(scrutinee, arms, *default_block, blocks),

            HirTerminator::Loop {
                label,
                binding,
                iterator,
                body,
                index_binding,
            } => self.lower_loop(
                *label,
                binding.clone(),
                iterator.as_ref(),
                *body,
                *index_binding,
                blocks,
            ),

            HirTerminator::Break { target } => self.lower_break(*target),

            HirTerminator::Continue { target } => self.lower_continue(*target),

            HirTerminator::Return(values) => self.lower_return(values),

            HirTerminator::ReturnError(error) => self.lower_return_error(error),

            HirTerminator::Panic { message } => self.lower_panic(message.as_ref()),
        }
    }

    // ========================================================================
    // Function and Struct Definition Lowering (Task 12)
    // ========================================================================

    // ------------------------------------------------------------------------
    // Task 12.1: Function Definition Lowering
    // ------------------------------------------------------------------------

    /// Lowers a HIR function definition to a LirFunction.
    ///
    /// The lowering process:
    /// 1. Reset context for the new function
    /// 2. Map parameters to WASM function parameters
    /// 3. Lower the function body block
    /// 4. Collect local types from the allocator
    /// 5. Build the complete LirFunction structure
    ///
    /// **Validates: Requirements 1.1, 5.5**
    pub fn lower_function_def(
        &mut self,
        name: InternedString,
        signature: &FunctionSignature,
        body: BlockId,
        blocks: &[HirBlock],
        is_main: bool,
    ) -> Result<LirFunction, CompilerError> {
        // Step 1: Reset context for the new function
        self.reset_for_function(name);

        // Step 2: Map parameters to WASM function parameters
        // Extract parameter names and types from the signature
        let params: Vec<(InternedString, DataType)> = signature
            .parameters
            .iter()
            .map(|arg| (arg.id, arg.value.data_type.clone()))
            .collect();

        // Lower function parameters (handles ownership tagging for heap types)
        let prologue_insts = self.lower_function_parameters(&params)?;

        // Convert parameters to LIR types for the function signature
        let param_types = self.params_to_lir_types(&params);

        // Step 3: Lower the function body block
        let mut body_insts = prologue_insts;
        body_insts.extend(self.lower_block(body, blocks)?);

        // Step 4: Collect local types from the allocator
        // Note: The first N locals are the function parameters, so we need to
        // exclude them from the locals list (they're already in param_types)
        let all_local_types = self.local_allocator.get_local_types().to_vec();
        let num_params = param_types.len();
        let locals: Vec<LirType> = if all_local_types.len() > num_params {
            all_local_types[num_params..].to_vec()
        } else {
            Vec::new()
        };

        // Step 5: Build return types from the signature
        let return_types: Vec<LirType> = signature
            .returns
            .iter()
            .map(|arg| data_type_to_lir_type(&arg.value.data_type))
            .collect();

        // Step 6: Build the complete LirFunction structure
        Ok(LirFunction {
            name: name.to_string(),
            params: param_types,
            returns: return_types,
            locals,
            body: body_insts,
            is_main,
        })
    }

    // ------------------------------------------------------------------------
    // Task 12.2: Struct Definition Lowering
    // ------------------------------------------------------------------------

    /// Lowers a HIR struct definition to a LirStruct.
    ///
    /// The lowering process:
    /// 1. Compute field layouts with proper alignment and offsets
    /// 2. Store the layout in the struct layouts map for later use
    /// 3. Build the LirStruct structure
    ///
    /// **Validates: Requirements 1.1, 9.4**
    pub fn lower_struct_def(
        &mut self,
        name: InternedString,
        fields: &[Arg],
    ) -> Result<LirStruct, CompilerError> {
        // Step 1: Register the struct layout (computes field offsets)
        // This also stores the layout in the context for later use during
        // field access lowering
        self.register_struct_layout(name, fields);

        // Step 2: Get the computed layout
        let layout = self.get_struct_layout(name).ok_or_else(|| {
            CompilerError::lir_transformation(format!(
                "Failed to compute struct layout for: {}",
                name
            ))
        })?;

        // Step 3: Build the LirStruct structure
        let lir_fields: Vec<LirField> = layout
            .fields
            .iter()
            .map(|field| LirField {
                name: field.name,
                offset: field.offset,
                ty: field.ty,
            })
            .collect();

        Ok(LirStruct {
            name,
            fields: lir_fields,
            total_size: layout.total_size,
        })
    }
}


// ============================================================================
// Main Lowering Interface (Task 13.1)
// ============================================================================

use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::hir::nodes::HirModule;
use crate::compiler::lir::nodes::LirModule;

/// Lowers a validated HIR module to a LIR module.
///
/// This is the main entry point for the HIR to LIR lowering stage.
/// It transforms the high-level, language-shaped HIR into the low-level,
/// WASM-shaped LIR ready for code generation.
///
/// The lowering process:
/// 1. Initialize a new `LoweringContext`
/// 2. Lower all struct definitions first (needed for field offset calculations)
/// 3. Register all functions (needed for function index resolution)
/// 4. Lower all function definitions
/// 5. Build and return the final `LirModule`
///
/// # Arguments
/// * `hir_module` - The validated HIR module from the borrow checker
///
/// # Returns
/// * `Ok(LirModule)` - The lowered LIR module ready for WASM codegen
/// * `Err(CompilerMessages)` - Accumulated errors during lowering
///
/// **Validates: Requirements 8.1, 8.2**
pub fn lower_hir_to_lir(hir_module: HirModule) -> Result<LirModule, CompilerMessages> {
    let mut ctx = LoweringContext::new();
    
    // Step 1: Lower struct definitions first
    // This populates the struct_layouts map needed for field access lowering
    let mut lir_structs = Vec::new();
    for struct_node in &hir_module.structs {
        match &struct_node.kind {
            HirKind::Stmt(HirStmt::StructDef { name, fields }) => {
                match ctx.lower_struct_def(*name, fields) {
                    Ok(lir_struct) => lir_structs.push(lir_struct),
                    Err(e) => ctx.errors.push(e),
                }
            }
            _ => {
                ctx.errors.push(CompilerError::lir_transformation(
                    "Expected StructDef node in structs list",
                ));
            }
        }
    }
    
    // Step 2: Register all functions first (for function index resolution)
    // This allows function calls to resolve their target indices
    for func_node in &hir_module.functions {
        if let HirKind::Stmt(HirStmt::FunctionDef { name, .. }) = &func_node.kind {
            ctx.register_function(*name);
        }
    }
    
    // Step 3: Lower all function definitions
    let mut lir_functions = Vec::new();
    for func_node in &hir_module.functions {
        match &func_node.kind {
            HirKind::Stmt(HirStmt::FunctionDef { name, signature, body }) => {
                // Determine if this is the main function
                // The main function is typically named "main" or is the entry point
                let is_main = name.to_string() == "main" || *body == hir_module.entry_block;
                
                match ctx.lower_function_def(*name, signature, *body, &hir_module.blocks, is_main) {
                    Ok(lir_func) => lir_functions.push(lir_func),
                    Err(e) => ctx.errors.push(e),
                }
            }
            _ => {
                ctx.errors.push(CompilerError::lir_transformation(
                    "Expected FunctionDef node in functions list",
                ));
            }
        }
    }
    
    // Step 4: Check for accumulated errors
    if !ctx.errors.is_empty() {
        return Err(CompilerMessages {
            errors: ctx.errors,
            warnings: Vec::new(),
        });
    }
    
    // Step 5: Build and return the final LIR module
    Ok(LirModule {
        functions: lir_functions,
        structs: lir_structs,
    })
}

impl LoweringContext {
    /// Builds the final LIR module from the accumulated lowered components.
    ///
    /// This method is called after all struct and function definitions have
    /// been lowered. It collects the results and constructs the final module.
    ///
    /// Note: This method is provided for flexibility but the main lowering
    /// function `lower_hir_to_lir` handles module construction directly.
    ///
    /// **Validates: Requirements 8.2**
    pub fn build_lir_module(
        &self,
        functions: Vec<LirFunction>,
        structs: Vec<LirStruct>,
    ) -> LirModule {
        LirModule { functions, structs }
    }
    
    /// Adds an error to the error accumulation list.
    ///
    /// This allows lowering to continue after non-fatal errors,
    /// collecting all errors for reporting at the end.
    ///
    /// **Validates: Requirements 8.3, 10.1, 10.2, 10.3, 10.4**
    pub fn add_error(&mut self, error: CompilerError) {
        self.errors.push(error);
    }
    
    /// Returns true if any errors have been accumulated.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
    
    /// Takes ownership of accumulated errors, leaving the list empty.
    ///
    /// This is useful for returning errors at the end of lowering.
    pub fn take_errors(&mut self) -> Vec<CompilerError> {
        std::mem::take(&mut self.errors)
    }

    // ========================================================================
    // Error Reporting Helpers (Task 15.1)
    // ========================================================================

    /// Creates an error for an unsupported HIR node type.
    ///
    /// This is used when encountering HIR constructs that are not yet
    /// implemented in the lowering stage. These are typically compiler bugs
    /// where the LIR infrastructure is missing or incomplete.
    ///
    /// # Arguments
    /// * `node_description` - A description of the unsupported node type
    ///
    /// **Validates: Requirements 10.1, 11.8**
    pub fn unsupported_node_error(&self, node_description: &str) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "This HIR construct is not yet supported in LIR lowering",
        );

        CompilerError {
            msg: format!("Unsupported HIR node type: {}", node_description),
            location: ErrorLocation::default(),
            error_type: ErrorType::LirTransformation,
            metadata,
        }
    }

    /// Creates an error for an unsupported HIR node type with source location.
    ///
    /// This variant includes source location information for better error reporting.
    ///
    /// # Arguments
    /// * `node_description` - A description of the unsupported node type
    /// * `location` - The source location where the unsupported node was encountered
    ///
    /// **Validates: Requirements 10.1, 11.8**
    pub fn unsupported_node_error_with_location(
        &self,
        node_description: &str,
        location: &TextLocation,
    ) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "This HIR construct is not yet supported in LIR lowering",
        );

        CompilerError {
            msg: format!("Unsupported HIR node type: {}", node_description),
            location: location.to_error_location_without_table(),
            error_type: ErrorType::LirTransformation,
            metadata,
        }
    }

    /// Creates an error for a type mismatch during lowering.
    ///
    /// Type mismatches indicate that the types of operands or values don't match
    /// what was expected for an operation. This uses the Type error type for
    /// better categorization.
    ///
    /// # Arguments
    /// * `expected` - Description of the expected type
    /// * `found` - Description of the actual type found
    ///
    /// **Validates: Requirements 10.2, 11.8**
    pub fn type_mismatch_error(&self, expected: &str, found: &str) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Ensure operand types match the operation requirements",
        );

        CompilerError {
            msg: format!("Type mismatch: expected {}, found {}", expected, found),
            location: ErrorLocation::default(),
            error_type: ErrorType::Type,
            metadata,
        }
    }

    /// Creates an error for a type mismatch during lowering with source location.
    ///
    /// This variant includes source location information for better error reporting.
    ///
    /// # Arguments
    /// * `expected` - Description of the expected type
    /// * `found` - Description of the actual type found
    /// * `location` - The source location where the type mismatch was detected
    ///
    /// **Validates: Requirements 10.2, 11.8**
    pub fn type_mismatch_error_with_location(
        &self,
        expected: &str,
        found: &str,
        location: &TextLocation,
    ) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Ensure operand types match the operation requirements",
        );

        CompilerError {
            msg: format!("Type mismatch: expected {}, found {}", expected, found),
            location: location.to_error_location_without_table(),
            error_type: ErrorType::Type,
            metadata,
        }
    }

    /// Creates an error for control flow inconsistencies.
    ///
    /// Control flow errors indicate problems with branching, loops, or other
    /// control structures that cannot be properly lowered to LIR.
    ///
    /// # Arguments
    /// * `description` - A description of the control flow problem
    ///
    /// **Validates: Requirements 10.3, 11.8**
    pub fn control_flow_error(&self, description: &str) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Check the control flow structure for consistency",
        );

        CompilerError {
            msg: format!("Control flow error: {}", description),
            location: ErrorLocation::default(),
            error_type: ErrorType::LirTransformation,
            metadata,
        }
    }

    /// Creates an error for control flow inconsistencies with source location.
    ///
    /// This variant includes source location information for better error reporting.
    ///
    /// # Arguments
    /// * `description` - A description of the control flow problem
    /// * `location` - The source location where the control flow error was detected
    ///
    /// **Validates: Requirements 10.3, 11.8**
    pub fn control_flow_error_with_location(
        &self,
        description: &str,
        location: &TextLocation,
    ) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Check the control flow structure for consistency",
        );

        CompilerError {
            msg: format!("Control flow error: {}", description),
            location: location.to_error_location_without_table(),
            error_type: ErrorType::LirTransformation,
            metadata,
        }
    }

    /// Creates an error for invalid memory operations.
    ///
    /// Memory operation errors indicate problems with struct field access,
    /// collection element access, or other memory-related operations.
    ///
    /// # Arguments
    /// * `description` - A description of the invalid memory operation
    ///
    /// **Validates: Requirements 10.4, 11.8**
    pub fn memory_operation_error(&self, description: &str) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Check the memory operation for validity",
        );

        CompilerError {
            msg: format!("Invalid memory operation: {}", description),
            location: ErrorLocation::default(),
            error_type: ErrorType::LirTransformation,
            metadata,
        }
    }

    /// Creates an error for invalid memory operations with source location.
    ///
    /// This variant includes source location information for better error reporting.
    ///
    /// # Arguments
    /// * `description` - A description of the invalid memory operation
    /// * `location` - The source location where the memory operation error was detected
    ///
    /// **Validates: Requirements 10.4, 11.8**
    pub fn memory_operation_error_with_location(
        &self,
        description: &str,
        location: &TextLocation,
    ) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Check the memory operation for validity",
        );

        CompilerError {
            msg: format!("Invalid memory operation: {}", description),
            location: location.to_error_location_without_table(),
            error_type: ErrorType::LirTransformation,
            metadata,
        }
    }

    /// Creates an error for undefined variables.
    ///
    /// This is a convenience method for the common case of referencing
    /// a variable that hasn't been declared or is out of scope.
    ///
    /// # Arguments
    /// * `var_name` - The name of the undefined variable
    ///
    /// **Validates: Requirements 10.1, 11.8**
    pub fn undefined_variable_error(&self, var_name: &str) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Ensure the variable is declared before use",
        );

        CompilerError {
            msg: format!("Undefined variable: {}", var_name),
            location: ErrorLocation::default(),
            error_type: ErrorType::LirTransformation,
            metadata,
        }
    }

    /// Creates an error for undefined variables with source location.
    ///
    /// # Arguments
    /// * `var_name` - The name of the undefined variable
    /// * `location` - The source location where the undefined variable was referenced
    ///
    /// **Validates: Requirements 10.1, 11.8**
    pub fn undefined_variable_error_with_location(
        &self,
        var_name: &str,
        location: &TextLocation,
    ) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Ensure the variable is declared before use",
        );

        CompilerError {
            msg: format!("Undefined variable: {}", var_name),
            location: location.to_error_location_without_table(),
            error_type: ErrorType::LirTransformation,
            metadata,
        }
    }

    /// Creates an error for unknown struct types.
    ///
    /// This is used when a struct type is referenced but its layout
    /// has not been registered.
    ///
    /// # Arguments
    /// * `struct_name` - The name of the unknown struct type
    ///
    /// **Validates: Requirements 10.4, 11.8**
    pub fn unknown_struct_error(&self, struct_name: &str) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Ensure the struct type is defined before use",
        );

        CompilerError {
            msg: format!("Unknown struct type: {}", struct_name),
            location: ErrorLocation::default(),
            error_type: ErrorType::LirTransformation,
            metadata,
        }
    }

    /// Creates an error for unknown struct fields.
    ///
    /// This is used when a field is accessed on a struct but the field
    /// doesn't exist in the struct's layout.
    ///
    /// # Arguments
    /// * `struct_name` - The name of the struct type
    /// * `field_name` - The name of the unknown field
    ///
    /// **Validates: Requirements 10.4, 11.8**
    pub fn unknown_field_error(&self, struct_name: &str, field_name: &str) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Check that the field name is correct and exists in the struct definition",
        );

        CompilerError {
            msg: format!(
                "Unknown field '{}' in struct '{}'",
                field_name, struct_name
            ),
            location: ErrorLocation::default(),
            error_type: ErrorType::LirTransformation,
            metadata,
        }
    }

    /// Creates an error for unknown functions.
    ///
    /// This is used when a function is called but hasn't been registered.
    ///
    /// # Arguments
    /// * `func_name` - The name of the unknown function
    ///
    /// **Validates: Requirements 10.1, 11.8**
    pub fn unknown_function_error(&self, func_name: &str) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Ensure the function is defined before it is called",
        );

        CompilerError {
            msg: format!("Unknown function: {}", func_name),
            location: ErrorLocation::default(),
            error_type: ErrorType::LirTransformation,
            metadata,
        }
    }

    /// Creates an internal compiler error for unexpected states.
    ///
    /// This is used for situations that should never occur if the compiler
    /// is working correctly. These indicate bugs in the compiler itself.
    ///
    /// # Arguments
    /// * `description` - A description of the unexpected state
    ///
    /// **Validates: Requirements 10.5, 11.8**
    pub fn internal_error(&self, description: &str) -> CompilerError {
        let mut metadata = HashMap::new();
        metadata.insert(ErrorMetaDataKey::CompilationStage, "HIR to LIR Lowering");
        metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "This is a compiler bug - please report it",
        );

        CompilerError {
            msg: format!("COMPILER BUG: {}", description),
            location: ErrorLocation::default(),
            error_type: ErrorType::Compiler,
            metadata,
        }
    }
}

// ============================================================================
// LIR Pretty-Printing (Task 16.2)
// ============================================================================

/// Pretty-prints a LIR module for debugging purposes.
///
/// This function is called when the `show_lir` feature flag is enabled.
/// It outputs:
/// - Function signatures with parameters and return types
/// - Local variable declarations
/// - LIR instruction sequences with proper indentation
/// - Ownership operations are clearly formatted
///
/// **Validates: Requirements 8.5**
pub fn display_lir(module: &LirModule) -> String {
    let mut output = String::new();
    
    output.push_str("=== LIR Module ===\n\n");
    
    // Display structs
    if !module.structs.is_empty() {
        output.push_str("--- Structs ---\n");
        for lir_struct in &module.structs {
            output.push_str(&display_lir_struct(lir_struct));
            output.push('\n');
        }
        output.push('\n');
    }
    
    // Display functions
    output.push_str("--- Functions ---\n");
    for func in &module.functions {
        output.push_str(&display_lir_function(func));
        output.push('\n');
    }
    
    output
}

/// Pretty-prints a LIR struct definition.
fn display_lir_struct(lir_struct: &LirStruct) -> String {
    let mut output = String::new();
    
    output.push_str(&format!("struct {} (size: {} bytes):\n", lir_struct.name, lir_struct.total_size));
    
    for field in &lir_struct.fields {
        output.push_str(&format!(
            "  {} {} @ offset {}\n",
            display_lir_type(field.ty),
            field.name,
            field.offset
        ));
    }
    
    output
}

/// Pretty-prints a LIR function definition.
fn display_lir_function(func: &LirFunction) -> String {
    let mut output = String::new();
    
    // Function signature
    let params_str: Vec<String> = func.params.iter().map(|t| display_lir_type(*t)).collect();
    let returns_str: Vec<String> = func.returns.iter().map(|t| display_lir_type(*t)).collect();
    
    let main_marker = if func.is_main { " [MAIN]" } else { "" };
    
    output.push_str(&format!(
        "func {}({}) -> ({}){}\n",
        func.name,
        params_str.join(", "),
        returns_str.join(", "),
        main_marker
    ));
    
    // Local variables
    if !func.locals.is_empty() {
        output.push_str("  locals:\n");
        for (i, local_type) in func.locals.iter().enumerate() {
            output.push_str(&format!("    ${}: {}\n", i, display_lir_type(*local_type)));
        }
    }
    
    // Instructions
    output.push_str("  body:\n");
    for inst in &func.body {
        output.push_str(&display_lir_inst(inst, 2));
    }
    
    output
}

/// Pretty-prints a LIR type.
fn display_lir_type(ty: LirType) -> String {
    match ty {
        LirType::I32 => "i32".to_owned(),
        LirType::I64 => "i64".to_owned(),
        LirType::F32 => "f32".to_owned(),
        LirType::F64 => "f64".to_owned(),
    }
}

/// Pretty-prints a LIR instruction with the given indentation level.
fn display_lir_inst(inst: &LirInst, indent: usize) -> String {
    let indent_str = "  ".repeat(indent);
    let mut output = String::new();
    
    match inst {
        // Variable access
        LirInst::LocalGet(idx) => {
            output.push_str(&format!("{}local.get ${}\n", indent_str, idx));
        }
        LirInst::LocalSet(idx) => {
            output.push_str(&format!("{}local.set ${}\n", indent_str, idx));
        }
        LirInst::LocalTee(idx) => {
            output.push_str(&format!("{}local.tee ${}\n", indent_str, idx));
        }
        LirInst::GlobalGet(idx) => {
            output.push_str(&format!("{}global.get ${}\n", indent_str, idx));
        }
        LirInst::GlobalSet(idx) => {
            output.push_str(&format!("{}global.set ${}\n", indent_str, idx));
        }
        
        // Memory access
        LirInst::I32Load { offset, align } => {
            output.push_str(&format!("{}i32.load offset={} align={}\n", indent_str, offset, align));
        }
        LirInst::I32Store { offset, align } => {
            output.push_str(&format!("{}i32.store offset={} align={}\n", indent_str, offset, align));
        }
        LirInst::I64Load { offset, align } => {
            output.push_str(&format!("{}i64.load offset={} align={}\n", indent_str, offset, align));
        }
        LirInst::I64Store { offset, align } => {
            output.push_str(&format!("{}i64.store offset={} align={}\n", indent_str, offset, align));
        }
        LirInst::F32Load { offset, align } => {
            output.push_str(&format!("{}f32.load offset={} align={}\n", indent_str, offset, align));
        }
        LirInst::F32Store { offset, align } => {
            output.push_str(&format!("{}f32.store offset={} align={}\n", indent_str, offset, align));
        }
        LirInst::F64Load { offset, align } => {
            output.push_str(&format!("{}f64.load offset={} align={}\n", indent_str, offset, align));
        }
        LirInst::F64Store { offset, align } => {
            output.push_str(&format!("{}f64.store offset={} align={}\n", indent_str, offset, align));
        }
        
        // Constants
        LirInst::I32Const(val) => {
            output.push_str(&format!("{}i32.const {}\n", indent_str, val));
        }
        LirInst::I64Const(val) => {
            output.push_str(&format!("{}i64.const {}\n", indent_str, val));
        }
        LirInst::F32Const(val) => {
            output.push_str(&format!("{}f32.const {}\n", indent_str, val));
        }
        LirInst::F64Const(val) => {
            output.push_str(&format!("{}f64.const {}\n", indent_str, val));
        }
        
        // I32 Arithmetic & Logical
        LirInst::I32Add => output.push_str(&format!("{}i32.add\n", indent_str)),
        LirInst::I32Sub => output.push_str(&format!("{}i32.sub\n", indent_str)),
        LirInst::I32Mul => output.push_str(&format!("{}i32.mul\n", indent_str)),
        LirInst::I32DivS => output.push_str(&format!("{}i32.div_s\n", indent_str)),
        LirInst::I32Eq => output.push_str(&format!("{}i32.eq\n", indent_str)),
        LirInst::I32Ne => output.push_str(&format!("{}i32.ne\n", indent_str)),
        LirInst::I32LtS => output.push_str(&format!("{}i32.lt_s\n", indent_str)),
        LirInst::I32GtS => output.push_str(&format!("{}i32.gt_s\n", indent_str)),
        
        // I64 Arithmetic & Logical
        LirInst::I64Add => output.push_str(&format!("{}i64.add\n", indent_str)),
        LirInst::I64Sub => output.push_str(&format!("{}i64.sub\n", indent_str)),
        LirInst::I64Mul => output.push_str(&format!("{}i64.mul\n", indent_str)),
        LirInst::I64DivS => output.push_str(&format!("{}i64.div_s\n", indent_str)),
        LirInst::I64Eq => output.push_str(&format!("{}i64.eq\n", indent_str)),
        LirInst::I64Ne => output.push_str(&format!("{}i64.ne\n", indent_str)),
        LirInst::I64LtS => output.push_str(&format!("{}i64.lt_s\n", indent_str)),
        LirInst::I64GtS => output.push_str(&format!("{}i64.gt_s\n", indent_str)),
        
        // F64 Arithmetic & Logical
        LirInst::F64Add => output.push_str(&format!("{}f64.add\n", indent_str)),
        LirInst::F64Sub => output.push_str(&format!("{}f64.sub\n", indent_str)),
        LirInst::F64Mul => output.push_str(&format!("{}f64.mul\n", indent_str)),
        LirInst::F64Div => output.push_str(&format!("{}f64.div\n", indent_str)),
        LirInst::F64Eq => output.push_str(&format!("{}f64.eq\n", indent_str)),
        LirInst::F64Ne => output.push_str(&format!("{}f64.ne\n", indent_str)),
        
        // Control flow
        LirInst::Block { instructions } => {
            output.push_str(&format!("{}block\n", indent_str));
            for inner_inst in instructions {
                output.push_str(&display_lir_inst(inner_inst, indent + 1));
            }
            output.push_str(&format!("{}end\n", indent_str));
        }
        LirInst::Loop { instructions } => {
            output.push_str(&format!("{}loop\n", indent_str));
            for inner_inst in instructions {
                output.push_str(&display_lir_inst(inner_inst, indent + 1));
            }
            output.push_str(&format!("{}end\n", indent_str));
        }
        LirInst::If { then_branch, else_branch } => {
            output.push_str(&format!("{}if\n", indent_str));
            for inner_inst in then_branch {
                output.push_str(&display_lir_inst(inner_inst, indent + 1));
            }
            if let Some(else_insts) = else_branch {
                output.push_str(&format!("{}else\n", indent_str));
                for inner_inst in else_insts {
                    output.push_str(&display_lir_inst(inner_inst, indent + 1));
                }
            }
            output.push_str(&format!("{}end\n", indent_str));
        }
        LirInst::Br(depth) => {
            output.push_str(&format!("{}br {}\n", indent_str, depth));
        }
        LirInst::BrIf(depth) => {
            output.push_str(&format!("{}br_if {}\n", indent_str, depth));
        }
        LirInst::Return => {
            output.push_str(&format!("{}return\n", indent_str));
        }
        LirInst::Call(idx) => {
            output.push_str(&format!("{}call ${}\n", indent_str, idx));
        }
        
        // Stack management
        LirInst::Drop => output.push_str(&format!("{}drop\n", indent_str)),
        LirInst::Nop => output.push_str(&format!("{}nop\n", indent_str)),
        
        // Ownership operations (clearly formatted)
        LirInst::TagAsOwned(idx) => {
            output.push_str(&format!("{}[ownership] tag_as_owned ${}\n", indent_str, idx));
        }
        LirInst::TagAsBorrowed(idx) => {
            output.push_str(&format!("{}[ownership] tag_as_borrowed ${}\n", indent_str, idx));
        }
        LirInst::MaskPointer => {
            output.push_str(&format!("{}[ownership] mask_pointer\n", indent_str));
        }
        LirInst::TestOwnership => {
            output.push_str(&format!("{}[ownership] test_ownership\n", indent_str));
        }
        LirInst::PossibleDrop(idx) => {
            output.push_str(&format!("{}[ownership] possible_drop ${}\n", indent_str, idx));
        }
        LirInst::PrepareOwnedArg(idx) => {
            output.push_str(&format!("{}[ownership] prepare_owned_arg ${}\n", indent_str, idx));
        }
        LirInst::PrepareBorrowedArg(idx) => {
            output.push_str(&format!("{}[ownership] prepare_borrowed_arg ${}\n", indent_str, idx));
        }
        LirInst::HandleOwnedParam { param_local, real_ptr_local } => {
            output.push_str(&format!(
                "{}[ownership] handle_owned_param ${} -> ${}\n",
                indent_str, param_local, real_ptr_local
            ));
        }
    }
    
    output
}
