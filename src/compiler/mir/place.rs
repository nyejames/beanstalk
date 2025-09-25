use crate::compiler::datatypes::DataType;
use std::collections::HashMap;

/// WASM-native Place abstraction optimized for WASM memory model
/// 
/// This Place system is designed to map directly to WASM memory locations
/// and instruction sequences, enabling efficient lowering to WASM bytecode.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Place {
    /// Direct WASM local variable (maps to local.get/local.set)
    Local {
        /// WASM local index (0-based)
        index: u32,
        /// Type information for WASM validation
        wasm_type: WasmType,
    },
    
    /// Direct WASM global variable (maps to global.get/global.set)
    Global {
        /// WASM global index (0-based)
        index: u32,
        /// Type information for WASM validation
        wasm_type: WasmType,
    },
    
    /// Linear memory location (maps to memory.load/memory.store)
    Memory {
        /// Base memory location
        base: MemoryBase,
        /// Byte offset from base
        offset: ByteOffset,
        /// Size and alignment for WASM memory operations
        size: TypeSize,
    },
    
    /// Projection into a complex type (field access, array indexing)
    Projection {
        /// Base place being projected from
        base: Box<Place>,
        /// Projection element (field, index, etc.)
        elem: ProjectionElem,
    },
}

/// WASM value types that correspond directly to WASM instruction operands
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WasmType {
    /// 32-bit integer (i32 in WASM)
    I32,
    /// 64-bit integer (i64 in WASM)
    I64,
    /// 32-bit float (f32 in WASM)
    F32,
    /// 64-bit float (f64 in WASM)
    F64,
    /// Reference type (externref in WASM)
    ExternRef,
    /// Function reference (funcref in WASM)
    FuncRef,
}

/// Base location for memory operations in WASM linear memory
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemoryBase {
    /// WASM linear memory (memory 0)
    LinearMemory,
    /// Stack-allocated temporary (maps to WASM locals)
    Stack,
    /// Heap allocation in linear memory
    Heap { 
        /// Allocation ID for tracking
        alloc_id: u32,
        /// Size of the allocation
        size: u32,
    },
}

/// Byte offset within memory
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ByteOffset(pub u32);

/// Type size information for WASM memory operations
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeSize {
    /// 1 byte (i32.load8_u/i32.store8)
    Byte,
    /// 2 bytes (i32.load16_u/i32.store16)
    Short,
    /// 4 bytes (i32.load/i32.store, f32.load/f32.store)
    Word,
    /// 8 bytes (i64.load/i64.store, f64.load/f64.store)
    DoubleWord,
    /// Custom size for complex types
    Custom { bytes: u32, alignment: u32 },
}

/// Projection elements for accessing parts of complex types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjectionElem {
    /// Field access in a struct/object
    Field {
        /// Field index (0-based)
        index: u32,
        /// Byte offset within the struct
        offset: FieldOffset,
        /// Size of the field
        size: FieldSize,
    },
    
    /// Array/collection index access
    Index {
        /// Index operand (can be another place)
        index: Box<Place>,
        /// Element size for offset calculation
        element_size: u32,
    },
    
    /// Slice/string length access
    Length,
    
    /// Slice/string data pointer access
    Data,
    
    /// Dereference operation (for pointers/references)
    Deref,
}

/// Field offset within a struct (byte-aligned)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldOffset(pub u32);

/// Field size information
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FieldSize {
    /// Fixed size field
    Fixed(u32),
    /// Variable size field (like strings, arrays)
    Variable,
    /// WASM value type
    WasmType(WasmType),
}

/// WASM stack operation tracking for place operations
#[derive(Debug, Clone, PartialEq)]
pub struct StackOperation {
    /// Operation type
    pub op_type: StackOpType,
    /// WASM type being operated on
    pub wasm_type: WasmType,
    /// Stack depth change (+1 for push, -1 for pop)
    pub stack_delta: i32,
}

/// Types of WASM stack operations
#[derive(Debug, Clone, PartialEq)]
pub enum StackOpType {
    /// Load value onto stack
    Load,
    /// Store value from stack
    Store,
    /// Duplicate top of stack
    Dup,
    /// Drop top of stack
    Drop,
    /// Arithmetic operation
    Arithmetic(ArithmeticOp),
    /// Comparison operation
    Comparison(ComparisonOp),
}

/// WASM arithmetic operations
#[derive(Debug, Clone, PartialEq)]
pub enum ArithmeticOp {
    Add, Sub, Mul, Div, Rem,
    And, Or, Xor, Shl, Shr,
    Neg, Abs, Sqrt, Min, Max,
}

/// WASM comparison operations
#[derive(Debug, Clone, PartialEq)]
pub enum ComparisonOp {
    Eq, Ne, Lt, Le, Gt, Ge,
}

impl Place {
    /// Create a new local place with WASM type information
    pub fn local(index: u32, data_type: &DataType) -> Self {
        let wasm_type = WasmType::from_data_type(data_type);
        Place::Local {
            index,
            wasm_type,
        }
    }
    
    /// Create a new global place with WASM type information
    pub fn global(index: u32, data_type: &DataType) -> Self {
        let wasm_type = WasmType::from_data_type(data_type);
        Place::Global {
            index,
            wasm_type,
        }
    }
    
    /// Create a memory place in linear memory
    pub fn memory(offset: u32, size: TypeSize) -> Self {
        Place::Memory {
            base: MemoryBase::LinearMemory,
            offset: ByteOffset(offset),
            size,
        }
    }
    
    /// Create a heap allocation place
    pub fn heap_alloc(alloc_id: u32, size: u32, offset: u32, type_size: TypeSize) -> Self {
        Place::Memory {
            base: MemoryBase::Heap { alloc_id, size },
            offset: ByteOffset(offset),
            size: type_size,
        }
    }
    
    /// Project a field from this place
    pub fn project_field(self, field_index: u32, field_offset: u32, field_size: FieldSize) -> Self {
        Place::Projection {
            base: Box::new(self),
            elem: ProjectionElem::Field {
                index: field_index,
                offset: FieldOffset(field_offset),
                size: field_size,
            },
        }
    }
    
    /// Project an array index from this place
    pub fn project_index(self, index_place: Place, element_size: u32) -> Self {
        Place::Projection {
            base: Box::new(self),
            elem: ProjectionElem::Index {
                index: Box::new(index_place),
                element_size,
            },
        }
    }
    
    /// Get the WASM type of this place
    pub fn wasm_type(&self) -> WasmType {
        match self {
            Place::Local { wasm_type, .. } => wasm_type.clone(),
            Place::Global { wasm_type, .. } => wasm_type.clone(),
            Place::Memory { size, .. } => size.to_wasm_type(),
            Place::Projection { elem, .. } => elem.wasm_type(),
        }
    }
    
    /// Check if this place maps directly to a WASM local
    pub fn is_wasm_local(&self) -> bool {
        matches!(self, Place::Local { .. })
    }
    
    /// Check if this place maps directly to a WASM global
    pub fn is_wasm_global(&self) -> bool {
        matches!(self, Place::Global { .. })
    }
    
    /// Check if this place requires linear memory access
    pub fn requires_memory_access(&self) -> bool {
        match self {
            Place::Memory { .. } => true,
            Place::Projection { base, .. } => base.requires_memory_access(),
            _ => false,
        }
    }
    
    /// Get the number of WASM instructions needed to load this place
    pub fn load_instruction_count(&self) -> u32 {
        match self {
            Place::Local { .. } => 1, // local.get
            Place::Global { .. } => 1, // global.get
            Place::Memory { .. } => 2, // i32.const offset + memory.load
            Place::Projection { base, elem } => {
                base.load_instruction_count() + elem.instruction_count()
            }
        }
    }
    
    /// Get the number of WASM instructions needed to store to this place
    pub fn store_instruction_count(&self) -> u32 {
        match self {
            Place::Local { .. } => 1, // local.set
            Place::Global { .. } => 1, // global.set
            Place::Memory { .. } => 2, // i32.const offset + memory.store
            Place::Projection { base, elem } => {
                base.load_instruction_count() + elem.instruction_count() + 1 // +1 for final store
            }
        }
    }
    
    /// Generate WASM stack operations for loading this place
    pub fn generate_load_operations(&self) -> Vec<StackOperation> {
        match self {
            Place::Local { wasm_type, .. } => vec![
                StackOperation {
                    op_type: StackOpType::Load,
                    wasm_type: wasm_type.clone(),
                    stack_delta: 1,
                }
            ],
            Place::Global { wasm_type, .. } => vec![
                StackOperation {
                    op_type: StackOpType::Load,
                    wasm_type: wasm_type.clone(),
                    stack_delta: 1,
                }
            ],
            Place::Memory { size, .. } => vec![
                StackOperation {
                    op_type: StackOpType::Load,
                    wasm_type: WasmType::I32, // Address
                    stack_delta: 1,
                },
                StackOperation {
                    op_type: StackOpType::Load,
                    wasm_type: size.to_wasm_type(),
                    stack_delta: 0, // Replace address with value
                }
            ],
            Place::Projection { base, elem } => {
                let mut ops = base.generate_load_operations();
                ops.extend(elem.generate_operations());
                ops
            }
        }
    }
    
    /// Generate WASM stack operations for storing to this place
    pub fn generate_store_operations(&self) -> Vec<StackOperation> {
        match self {
            Place::Local { wasm_type, .. } => vec![
                StackOperation {
                    op_type: StackOpType::Store,
                    wasm_type: wasm_type.clone(),
                    stack_delta: -1,
                }
            ],
            Place::Global { wasm_type, .. } => vec![
                StackOperation {
                    op_type: StackOpType::Store,
                    wasm_type: wasm_type.clone(),
                    stack_delta: -1,
                }
            ],
            Place::Memory { size, .. } => vec![
                StackOperation {
                    op_type: StackOpType::Load,
                    wasm_type: WasmType::I32, // Address
                    stack_delta: 1,
                },
                StackOperation {
                    op_type: StackOpType::Store,
                    wasm_type: size.to_wasm_type(),
                    stack_delta: -2, // Consume address and value
                }
            ],
            Place::Projection { base, elem } => {
                let mut ops = base.generate_load_operations(); // Get base address
                ops.extend(elem.generate_operations()); // Calculate final address
                ops.push(StackOperation {
                    op_type: StackOpType::Store,
                    wasm_type: elem.wasm_type(),
                    stack_delta: -2, // Consume address and value
                });
                ops
            }
        }
    }

    /// Get the memory base of this place (for overlap detection)
    pub fn memory_base(&self) -> Option<&MemoryBase> {
        match self {
            Place::Memory { base, .. } => Some(base),
            Place::Projection { base, .. } => base.memory_base(),
            _ => None,
        }
    }

    /// Get the memory offset of this place (for overlap detection)
    pub fn memory_offset(&self) -> Option<u32> {
        match self {
            Place::Memory { offset, .. } => Some(offset.0),
            Place::Projection { base, elem } => {
                let base_offset = base.memory_offset()?;
                match elem {
                    ProjectionElem::Field { offset, .. } => Some(base_offset + offset.0),
                    ProjectionElem::Index { .. } => None, // Dynamic offset
                    _ => Some(base_offset),
                }
            }
            _ => None,
        }
    }

    /// Get the memory size of this place (for overlap detection)
    pub fn memory_size(&self) -> Option<u32> {
        match self {
            Place::Memory { size, .. } => Some(size.byte_size()),
            Place::Projection { elem, .. } => match elem {
                ProjectionElem::Field { size, .. } => match size {
                    FieldSize::Fixed(bytes) => Some(*bytes),
                    FieldSize::WasmType(wasm_type) => Some(wasm_type.byte_size()),
                    FieldSize::Variable => None,
                },
                _ => Some(4), // Default size for projections
            },
            _ => None,
        }
    }

    /// Check if this place represents the same memory location as another
    pub fn same_memory_location(&self, other: &Place) -> bool {
        match (self, other) {
            (Place::Local { index: i1, .. }, Place::Local { index: i2, .. }) => i1 == i2,
            (Place::Global { index: i1, .. }, Place::Global { index: i2, .. }) => i1 == i2,
            (Place::Memory { base: b1, offset: o1, .. }, Place::Memory { base: b2, offset: o2, .. }) => {
                b1 == b2 && o1 == o2
            }
            _ => false,
        }
    }

    /// Get the root place (without projections)
    pub fn root_place(&self) -> &Place {
        match self {
            Place::Projection { base, .. } => base.root_place(),
            _ => self,
        }
    }

    /// Check if this place is a projection of another place
    pub fn is_projection_of(&self, other: &Place) -> bool {
        match self {
            Place::Projection { base, .. } => {
                base.as_ref() == other || base.is_projection_of(other)
            }
            _ => false,
        }
    }
}

impl WasmType {
    /// Convert from Beanstalk DataType to WASM type
    pub fn from_data_type(data_type: &DataType) -> Self {
        match data_type {
            DataType::Int(_) => WasmType::I64,
            DataType::Float(_) => WasmType::F64,
            DataType::Bool(_) => WasmType::I32,
            DataType::String(_) | DataType::Collection(_, _) => WasmType::I32, // Pointer to linear memory
            DataType::Function(_, _) => WasmType::FuncRef,
            DataType::Inferred(_) => WasmType::I32, // Default to i32 for unresolved types
            DataType::None => WasmType::I32,
            // Handle all other DataType variants
            _ => WasmType::I32, // Default to i32 for other types
        }
    }
    
    /// Get the byte size of this WASM type
    pub fn byte_size(&self) -> u32 {
        match self {
            WasmType::I32 | WasmType::F32 => 4,
            WasmType::I64 | WasmType::F64 => 8,
            WasmType::ExternRef | WasmType::FuncRef => 4, // Pointer size
        }
    }
    
    /// Check if this type can be stored in WASM locals
    pub fn is_local_compatible(&self) -> bool {
        true // All WASM types can be stored in locals
    }
    
    /// Check if this type requires linear memory storage
    pub fn requires_memory(&self) -> bool {
        matches!(self, WasmType::ExternRef) // Complex types need memory
    }
}

impl TypeSize {
    /// Convert TypeSize to corresponding WASM type
    pub fn to_wasm_type(&self) -> WasmType {
        match self {
            TypeSize::Byte | TypeSize::Short | TypeSize::Word => WasmType::I32,
            TypeSize::DoubleWord => WasmType::I64,
            TypeSize::Custom { bytes, .. } => {
                if *bytes <= 4 {
                    WasmType::I32
                } else {
                    WasmType::I64
                }
            }
        }
    }
    
    /// Get byte size
    pub fn byte_size(&self) -> u32 {
        match self {
            TypeSize::Byte => 1,
            TypeSize::Short => 2,
            TypeSize::Word => 4,
            TypeSize::DoubleWord => 8,
            TypeSize::Custom { bytes, .. } => *bytes,
        }
    }
    
    /// Get alignment requirement
    pub fn alignment(&self) -> u32 {
        match self {
            TypeSize::Byte => 1,
            TypeSize::Short => 2,
            TypeSize::Word => 4,
            TypeSize::DoubleWord => 8,
            TypeSize::Custom { alignment, .. } => *alignment,
        }
    }
}

impl ProjectionElem {
    /// Get the WASM type of the projected element
    pub fn wasm_type(&self) -> WasmType {
        match self {
            ProjectionElem::Field { size, .. } => match size {
                FieldSize::WasmType(wasm_type) => wasm_type.clone(),
                FieldSize::Fixed(bytes) => {
                    if *bytes <= 4 { WasmType::I32 } else { WasmType::I64 }
                }
                FieldSize::Variable => WasmType::I32, // Pointer
            },
            ProjectionElem::Index { .. } => WasmType::I32, // Array elements are typically i32-sized
            ProjectionElem::Length => WasmType::I32,
            ProjectionElem::Data => WasmType::I32, // Pointer
            ProjectionElem::Deref => WasmType::I32, // Dereferenced value
        }
    }
    
    /// Get the number of WASM instructions needed for this projection
    pub fn instruction_count(&self) -> u32 {
        match self {
            ProjectionElem::Field { .. } => 2, // i32.const offset + i32.add
            ProjectionElem::Index { index, .. } => {
                index.load_instruction_count() + 2 // load index + multiply + add (element_size is immediate)
            }
            ProjectionElem::Length | ProjectionElem::Data => 1, // offset calculation
            ProjectionElem::Deref => 1, // memory.load
        }
    }
    
    /// Generate WASM stack operations for this projection
    pub fn generate_operations(&self) -> Vec<StackOperation> {
        match self {
            ProjectionElem::Field { .. } => vec![
                StackOperation {
                    op_type: StackOpType::Load,
                    wasm_type: WasmType::I32, // Offset constant
                    stack_delta: 1,
                },
                StackOperation {
                    op_type: StackOpType::Arithmetic(ArithmeticOp::Add),
                    wasm_type: WasmType::I32,
                    stack_delta: -1, // Combine base + offset
                }
            ],
            ProjectionElem::Index { index, element_size: _ } => {
                let mut ops = index.generate_load_operations();
                ops.push(StackOperation {
                    op_type: StackOpType::Load,
                    wasm_type: WasmType::I32, // Element size constant
                    stack_delta: 1,
                });
                ops.push(StackOperation {
                    op_type: StackOpType::Arithmetic(ArithmeticOp::Mul),
                    wasm_type: WasmType::I32,
                    stack_delta: -1, // index * element_size
                });
                ops.push(StackOperation {
                    op_type: StackOpType::Arithmetic(ArithmeticOp::Add),
                    wasm_type: WasmType::I32,
                    stack_delta: -1, // base + (index * element_size)
                });
                ops
            }
            ProjectionElem::Length | ProjectionElem::Data => vec![
                StackOperation {
                    op_type: StackOpType::Load,
                    wasm_type: WasmType::I32, // Offset for length/data field
                    stack_delta: 1,
                },
                StackOperation {
                    op_type: StackOpType::Arithmetic(ArithmeticOp::Add),
                    wasm_type: WasmType::I32,
                    stack_delta: -1,
                }
            ],
            ProjectionElem::Deref => vec![
                StackOperation {
                    op_type: StackOpType::Load,
                    wasm_type: WasmType::I32, // Load from memory
                    stack_delta: 0, // Replace address with value
                }
            ],
        }
    }
}

/// Place manager for tracking WASM memory layout and place allocation
#[derive(Debug)]
pub struct PlaceManager {
    /// Next local index to allocate
    next_local_index: u32,
    /// Next global index to allocate
    next_global_index: u32,
    /// Linear memory layout tracker
    memory_layout: MemoryLayout,
    /// Local variable type mapping
    local_types: HashMap<u32, WasmType>,
    /// Global variable type mapping
    global_types: HashMap<u32, WasmType>,
    /// Heap allocation tracker
    heap_allocations: HashMap<u32, HeapAllocation>,
    /// Next allocation ID
    next_alloc_id: u32,
}

/// Memory layout manager for WASM linear memory
#[derive(Debug)]
pub struct MemoryLayout {
    /// Next available byte offset in linear memory
    next_offset: u32,
    /// Allocated regions
    regions: Vec<MemoryRegion>,
    /// Alignment requirements
    alignment: u32,
}

/// Memory region in WASM linear memory
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// Start offset
    pub start: u32,
    /// Size in bytes
    pub size: u32,
}



/// Heap allocation tracking
#[derive(Debug, Clone)]
pub struct HeapAllocation {
    /// Allocation ID
    pub id: u32,
    /// Size in bytes
    pub size: u32,
    /// Offset in linear memory
    pub offset: u32,
    /// Type information
    pub data_type: DataType,
}

impl PlaceManager {
    /// Create a new place manager
    pub fn new() -> Self {
        Self {
            next_local_index: 0,
            next_global_index: 0,
            memory_layout: MemoryLayout::new(),
            local_types: HashMap::new(),
            global_types: HashMap::new(),
            heap_allocations: HashMap::new(),
            next_alloc_id: 0,
        }
    }
    
    /// Allocate a new local place
    pub fn allocate_local(&mut self, data_type: &DataType) -> Place {
        let index = self.next_local_index;
        self.next_local_index += 1;
        
        let wasm_type = WasmType::from_data_type(data_type);
        self.local_types.insert(index, wasm_type.clone());
        
        Place::Local { index, wasm_type }
    }
    
    /// Allocate a new global place
    pub fn allocate_global(&mut self, data_type: &DataType) -> Place {
        let index = self.next_global_index;
        self.next_global_index += 1;
        
        let wasm_type = WasmType::from_data_type(data_type);
        self.global_types.insert(index, wasm_type.clone());
        
        Place::Global { index, wasm_type }
    }
    
    /// Allocate memory in linear memory
    pub fn allocate_memory(&mut self, size: u32, alignment: u32) -> Place {
        let offset = self.memory_layout.allocate(size, alignment);
        let type_size = if size <= 4 { TypeSize::Word } else { TypeSize::DoubleWord };
        
        Place::memory(offset, type_size)
    }
    
    /// Allocate heap memory for complex types
    pub fn allocate_heap(&mut self, data_type: &DataType, size: u32) -> Place {
        let alloc_id = self.next_alloc_id;
        self.next_alloc_id += 1;
        
        let offset = self.memory_layout.allocate(
            size, 
            8 // 8-byte alignment for complex types
        );
        
        let allocation = HeapAllocation {
            id: alloc_id,
            size,
            offset,
            data_type: data_type.clone(),
        };
        
        self.heap_allocations.insert(alloc_id, allocation);
        
        let type_size = TypeSize::Custom { bytes: size, alignment: 8 };
        Place::heap_alloc(alloc_id, size, offset, type_size)
    }
    
    /// Get local variable types for WASM function signature
    pub fn get_local_types(&self) -> Vec<WasmType> {
        (0..self.next_local_index)
            .map(|i| self.local_types.get(&i).cloned().unwrap_or(WasmType::I32))
            .collect()
    }
    
    /// Get global variable types for WASM module
    pub fn get_global_types(&self) -> Vec<WasmType> {
        (0..self.next_global_index)
            .map(|i| self.global_types.get(&i).cloned().unwrap_or(WasmType::I32))
            .collect()
    }
    
    /// Get memory layout information
    pub fn get_memory_layout(&self) -> &MemoryLayout {
        &self.memory_layout
    }
    
    /// Get heap allocation information
    pub fn get_heap_allocation(&self, alloc_id: u32) -> Option<&HeapAllocation> {
        self.heap_allocations.get(&alloc_id)
    }
}

impl MemoryLayout {
    /// Create a new memory layout
    pub fn new() -> Self {
        Self {
            next_offset: 0,
            regions: Vec::new(),
            alignment: 8, // Default 8-byte alignment
        }
    }
    
    /// Allocate a region in linear memory
    pub fn allocate(&mut self, size: u32, alignment: u32) -> u32 {
        // Align the offset
        let aligned_offset = align_up(self.next_offset, alignment);
        
        let region = MemoryRegion {
            start: aligned_offset,
            size,
        };
        
        self.regions.push(region);
        self.next_offset = aligned_offset + size;
        
        aligned_offset
    }
    
    /// Get total memory usage
    pub fn total_size(&self) -> u32 {
        self.next_offset
    }
    
    /// Get all allocated regions
    pub fn get_regions(&self) -> &Vec<MemoryRegion> {
        &self.regions
    }
}

/// Align a value up to the next multiple of alignment
fn align_up(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) & !(alignment - 1)
}