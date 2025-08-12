use std::collections::{HashMap, HashSet};
use wasm_encoder::ExportKind;

pub struct IR {
    functions: Vec<Function>,
    globals: HashSet<u32>,            // Static Memory (non constants)
    exports: HashMap<String, Export>, // The id of the global
    global_id: u32,                   // The next global id to use
    local_id: u32,                    // The next local id to use
}

impl IR {
    pub fn new() -> Self {
        Self {
            functions: vec![],
            globals: HashSet::new(),
            exports: HashMap::new(),
            global_id: 0,
            local_id: 0,
        }
    }
}

pub struct Export {
    id: u32,
    kind: ExportKind,
}
pub struct Function {
    parameters: HashSet<u32>,
    returns: HashSet<u32>,
    blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    UnconditionalJump(u32),
    Returns,
    ConditionalJump(u32, u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    id: u32,                            // For other branches to jump to this block
    variables_used: HashSet<u32>,       // Ids of locals that are needed by this block
    variables_used_after: HashSet<u32>, // Ids of locals that are used after this block (must stay alive)
    instructions: Vec<IRNode>,
    terminator: Terminator,
}

#[derive(Debug, Clone, PartialEq)]

pub enum IRNode {
    // Declarations
    // i32 used for pointers to make it easy to use in Wasm

    // id, value, global?
    SetInt(i32, i64, bool), // local.set or global.set (value)
    SetFloat(i32, f64, bool),
    SetBool(i32, i32, bool),

    // Immutable String
    // Pointer, String
    SetSlice(i32, Vec<u8>),

    // String
    // Pointer, Capacity, String
    SetString(i32, i32, Vec<u8>), // memory.grow (capacity) memory.set (pointer) ()

    // Only 64-bit types for now
    // Simple Variable References
    GetLocal(i32), // local.get -- Might need to be an id for a string lookup if throwing borrow checker errors from here
    GetGlobal(i32), // global.get

    // Constants
    IntConst(i64),   // i64.const
    FloatConst(f64), // f64.const
    BoolConst(i32),  // i32.const

    // Function Calls
    Call(i32),

    // Numeric Instructions

    // Floats
    // Arithmetics
    FloatAdd,     // f64.add
    FloatSub,     // f64.sub
    FloatMul,     // f64.mul
    FloatDiv,     // f64.div
    FloatNeg,     // f64.neg
    FloatSqrt,    // f64.sqrt (FLOAT ONLY)
    FloatMin,     // f64.min (FLOAT ONLY)
    FloatMax,     // f64.max (FLOAT ONLY)
    FloatNearest, // f64.nearest (FLOAT ONLY)

    // Comparisons
    FloatGreaterThan,        // f64.gt
    FloatLessThan,           // f64.lt
    FloatGreaterThanOrEqual, // f64.ge
    FloatLessThanOrEqual,    // f64.le
    FloatEquals,             // f64.eq
    FloatNotEquals,          // f64.ne

    // Bitwise
    FloatAnd, // f64.and
    FloatOr,  // f64.or

    // Integers
    // Arithmetics
    IntAdd, // i64.add
    IntSub, // i64.sub
    IntMul, // i64.mul
    IntDiv, // i64.div
    IntNeg, // i64.neg

    // Comparisons
    IntGreaterThan,        // i64.gt
    IntLessThan,           // i64.lt
    IntGreaterThanOrEqual, // i64.ge
    IntLessThanOrEqual,    // i64.le
    IntEquals,             // i64.eq
    IntNotEquals,          // i64.ne

    // Bitwise
    IntAnd, // i64.and
    IntOr,  // i64.or

    // Control Flow
    If(bool), // Whether the branch will run for a false or true condition (0 or 1)
}
