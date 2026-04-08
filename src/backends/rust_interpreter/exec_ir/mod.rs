//! Interpreter executable IR.
//!
//! WHAT: defines the runtime-oriented IR executed by the Rust interpreter.
//! WHY: the interpreter should lower from HIR into a semantic execution format, not reuse Wasm-shaped LIR.

macro_rules! define_exec_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub(crate) struct $name(pub u32);
    };
}

define_exec_id!(ExecModuleId);
define_exec_id!(ExecFunctionId);
define_exec_id!(ExecBlockId);
define_exec_id!(ExecLocalId);
define_exec_id!(ExecConstId);
define_exec_id!(ExecTypeId);

#[derive(Debug, Clone)]
pub(crate) struct ExecProgram {
    pub module: ExecModule,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecModule {
    pub id: ExecModuleId,
    pub functions: Vec<ExecFunction>,
    pub constants: Vec<ExecConst>,
    pub entry_function: Option<ExecFunctionId>,
}

impl ExecModule {
    pub(crate) fn new() -> Self {
        Self {
            id: ExecModuleId(0),
            functions: Vec::new(),
            constants: Vec::new(),
            entry_function: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExecFunction {
    pub id: ExecFunctionId,
    pub debug_name: String,
    pub entry_block: ExecBlockId,
    pub parameter_slots: Vec<ExecLocalId>,
    pub locals: Vec<ExecLocal>,
    pub blocks: Vec<ExecBlock>,
    pub result_type: ExecStorageType,
    pub flags: ExecFunctionFlags,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ExecFunctionFlags {
    pub is_start: bool,
    pub is_ctfe_allowed: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecBlock {
    pub id: ExecBlockId,
    pub instructions: Vec<ExecInstruction>,
    pub terminator: ExecTerminator,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecLocal {
    pub id: ExecLocalId,
    pub debug_name: Option<String>,
    pub storage_type: ExecStorageType,
    pub role: ExecLocalRole,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecConst {
    pub id: ExecConstId,
    pub value: ExecConstValue,
}

#[derive(Debug, Clone)]
pub(crate) enum ExecConstValue {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Char(char),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecLocalRole {
    Param,
    UserLocal,
    Temp,
    InternalScratch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecStorageType {
    /// Placeholder used until type lowering from HIR is implemented.
    Unknown,
    Unit,
    Bool,
    Int,
    Float,
    Char,
    HeapHandle,
    FunctionRef,
}

#[derive(Debug, Clone)]
pub(crate) enum ExecInstruction {
    LoadConst {
        target: ExecLocalId,
        const_id: ExecConstId,
    },
    ReadLocal {
        target: ExecLocalId,
        source: ExecLocalId,
    },
    CopyLocal {
        target: ExecLocalId,
        source: ExecLocalId,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum ExecTerminator {
    Return {
        value: Option<ExecLocalId>,
    },
    Jump {
        target: ExecBlockId,
    },
    BranchBool {
        condition: ExecLocalId,
        then_block: ExecBlockId,
        else_block: ExecBlockId,
    },
    PendingLowering {
        description: String,
    },
    UnreachableTrap,
}
