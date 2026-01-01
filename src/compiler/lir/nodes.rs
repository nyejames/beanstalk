//! LIR node definitions (scaffold)
//!
//! Defines the Low-Level IR structures used by the Wasm-adjacent lowering stage.

use crate::compiler::string_interning::InternedString;

/// A complete LIR module containing lowered functions and type information.
#[derive(Debug, Default, Clone)]
pub struct LirModule {
    pub functions: Vec<LirFunction>,
    pub structs: Vec<LirStruct>,
}

#[derive(Debug, Default, Clone)]
pub struct LirFunction {
    pub name: String,
    pub params: Vec<LirType>,
    pub returns: Vec<LirType>,
    pub locals: Vec<LirType>,
    pub body: Vec<LirInst>,
    pub is_main: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LirType {
    I32,
    I64,
    F32,
    F64,
}

#[derive(Debug, Clone)]
pub struct LirStruct {
    pub name: InternedString,
    pub fields: Vec<LirField>,
    pub total_size: u32,
}

#[derive(Debug, Clone)]
pub struct LirField {
    pub name: InternedString,
    pub offset: u32,
    pub ty: LirType,
}

/// Instruction set for Low-Level IR, mapping closely to Wasm bytecode.
#[derive(Debug, Clone)]
pub enum LirInst {
    // Variable access
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    GlobalGet(u32),
    GlobalSet(u32),

    // Memory access
    I32Load {
        offset: u32,
        align: u32,
    },
    I32Store {
        offset: u32,
        align: u32,
    },
    I64Load {
        offset: u32,
        align: u32,
    },
    I64Store {
        offset: u32,
        align: u32,
    },
    F32Load {
        offset: u32,
        align: u32,
    },
    F32Store {
        offset: u32,
        align: u32,
    },
    F64Load {
        offset: u32,
        align: u32,
    },
    F64Store {
        offset: u32,
        align: u32,
    },

    // Constants
    I32Const(i32),
    I64Const(i64),
    F32Const(f32),
    F64Const(f64),

    // Arithmetic & Logical
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32Eq,
    I32Ne,
    I32LtS,
    I32GtS,

    I64Add,
    I64Sub,
    I64Mul,
    I64DivS,
    I64Eq,
    I64Ne,
    I64LtS,
    I64GtS,

    F64Add,
    F64Sub,
    F64Mul,
    F64Div,
    F64Eq,
    F64Ne,

    // Control flow
    Block {
        instructions: Vec<LirInst>,
    },
    Loop {
        instructions: Vec<LirInst>,
    },
    If {
        then_branch: Vec<LirInst>,
        else_branch: Option<Vec<LirInst>>,
    },
    Br(u32),
    BrIf(u32),
    Return,
    Call(u32), // function index

    // Stack management
    Drop,
    Nop,

    // =========================================================================
    // Ownership Operations
    // These instructions implement Beanstalk's tagged pointer ownership system
    // =========================================================================
    /// Tag a local as owned (set ownership bit)
    /// Stack: [] -> []
    /// Local effect: local = local | 1
    TagAsOwned(u32),

    /// Tag a local as borrowed (clear ownership bit)
    /// Stack: [] -> []
    /// Local effect: local = local & ~1
    TagAsBorrowed(u32),

    /// Extract real pointer from tagged pointer (mask out ownership bit)
    /// Stack: [tagged_ptr] -> [real_ptr]
    MaskPointer,

    /// Test ownership bit, result on stack (1 = owned, 0 = borrowed)
    /// Stack: [tagged_ptr] -> [ownership_bit]
    TestOwnership,

    /// Conditional drop based on ownership flag
    /// Stack: [] -> []
    /// If local is owned, calls free function
    PossibleDrop(u32),

    /// Prepare argument as owned for function call
    /// Stack: [] -> [tagged_ptr]
    /// Loads local and sets ownership bit
    PrepareOwnedArg(u32),

    /// Prepare argument as borrowed for function call
    /// Stack: [] -> [tagged_ptr]
    /// Loads local and clears ownership bit
    PrepareBorrowedArg(u32),

    /// Handle potentially owned parameter in function prologue
    /// Extracts real pointer and stores in a separate local
    /// param_local: the parameter local index
    /// real_ptr_local: where to store the untagged pointer
    HandleOwnedParam {
        param_local: u32,
        real_ptr_local: u32,
    },
}

impl Default for LirInst {
    fn default() -> Self {
        LirInst::Nop
    }
}
