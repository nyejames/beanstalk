//! LIR Pretty-Printing
//!
//! This module provides functions for displaying LIR structures in a
//! human-readable format for debugging and inspection.

use crate::compiler::lir::nodes::{LirFunction, LirInst, LirModule, LirStruct, LirType};

/// Pretty-prints a complete LIR module.
pub fn display_lir(module: &LirModule) -> String {
    let mut output = String::new();

    output.push_str("=== LIR Module ===\n\n");

    // Display structs
    if !module.structs.is_empty() {
        output.push_str("--- Structs ---\n");
        for s in &module.structs {
            output.push_str(&display_lir_struct(s));
            output.push('\n');
        }
        output.push('\n');
    }

    // Display functions
    if !module.functions.is_empty() {
        output.push_str("--- Functions ---\n");
        for func in &module.functions {
            output.push_str(&display_lir_function(func));
            output.push('\n');
        }
    }

    output
}

/// Pretty-prints a LIR struct definition.
pub fn display_lir_struct(lir_struct: &LirStruct) -> String {
    let mut output = String::new();

    output.push_str(&format!("struct {} (size: {} bytes) {{\n", lir_struct.name, lir_struct.total_size));

    for field in &lir_struct.fields {
        output.push_str(&format!(
            "  {}: {} @ offset {}\n",
            field.name,
            display_lir_type(field.ty),
            field.offset
        ));
    }

    output.push_str("}\n");
    output
}

/// Pretty-prints a LIR function.
pub fn display_lir_function(func: &LirFunction) -> String {
    let mut output = String::new();

    // Function signature
    let params_str: Vec<String> = func.params.iter().map(|t| display_lir_type(*t)).collect();
    let returns_str: Vec<String> = func.returns.iter().map(|t| display_lir_type(*t)).collect();

    output.push_str(&format!(
        "fn {}({}) -> ({})",
        func.name,
        params_str.join(", "),
        returns_str.join(", ")
    ));

    if func.is_main {
        output.push_str(" [main]");
    }

    output.push_str(" {\n");

    // Locals
    if !func.locals.is_empty() {
        output.push_str("  locals: ");
        let locals_str: Vec<String> = func.locals.iter().map(|t| display_lir_type(*t)).collect();
        output.push_str(&locals_str.join(", "));
        output.push('\n');
    }

    // Body
    output.push_str("  body:\n");
    for inst in &func.body {
        output.push_str(&display_lir_inst(inst, 2));
    }

    output.push_str("}\n");
    output
}

/// Pretty-prints a LIR type.
pub fn display_lir_type(ty: LirType) -> String {
    match ty {
        LirType::I32 => "i32".to_owned(),
        LirType::I64 => "i64".to_owned(),
        LirType::F32 => "f32".to_owned(),
        LirType::F64 => "f64".to_owned(),
    }
}

/// Pretty-prints a LIR instruction with indentation.
pub fn display_lir_inst(inst: &LirInst, indent: usize) -> String {
    let indent_str = "  ".repeat(indent);
    let mut output = String::new();

    match inst {
        // Constants
        LirInst::I32Const(val) => output.push_str(&format!("{}i32.const {}\n", indent_str, val)),
        LirInst::I64Const(val) => output.push_str(&format!("{}i64.const {}\n", indent_str, val)),
        LirInst::F32Const(val) => output.push_str(&format!("{}f32.const {}\n", indent_str, val)),
        LirInst::F64Const(val) => output.push_str(&format!("{}f64.const {}\n", indent_str, val)),

        // Local operations
        LirInst::LocalGet(idx) => output.push_str(&format!("{}local.get {}\n", indent_str, idx)),
        LirInst::LocalSet(idx) => output.push_str(&format!("{}local.set {}\n", indent_str, idx)),
        LirInst::LocalTee(idx) => output.push_str(&format!("{}local.tee {}\n", indent_str, idx)),

        // Global operations
        LirInst::GlobalGet(idx) => output.push_str(&format!("{}global.get {}\n", indent_str, idx)),
        LirInst::GlobalSet(idx) => output.push_str(&format!("{}global.set {}\n", indent_str, idx)),

        // I32 operations
        LirInst::I32Add => output.push_str(&format!("{}i32.add\n", indent_str)),
        LirInst::I32Sub => output.push_str(&format!("{}i32.sub\n", indent_str)),
        LirInst::I32Mul => output.push_str(&format!("{}i32.mul\n", indent_str)),
        LirInst::I32DivS => output.push_str(&format!("{}i32.div_s\n", indent_str)),
        LirInst::I32Eq => output.push_str(&format!("{}i32.eq\n", indent_str)),
        LirInst::I32Ne => output.push_str(&format!("{}i32.ne\n", indent_str)),
        LirInst::I32LtS => output.push_str(&format!("{}i32.lt_s\n", indent_str)),
        LirInst::I32GtS => output.push_str(&format!("{}i32.gt_s\n", indent_str)),

        // I64 operations
        LirInst::I64Add => output.push_str(&format!("{}i64.add\n", indent_str)),
        LirInst::I64Sub => output.push_str(&format!("{}i64.sub\n", indent_str)),
        LirInst::I64Mul => output.push_str(&format!("{}i64.mul\n", indent_str)),
        LirInst::I64DivS => output.push_str(&format!("{}i64.div_s\n", indent_str)),
        LirInst::I64Eq => output.push_str(&format!("{}i64.eq\n", indent_str)),
        LirInst::I64Ne => output.push_str(&format!("{}i64.ne\n", indent_str)),
        LirInst::I64LtS => output.push_str(&format!("{}i64.lt_s\n", indent_str)),
        LirInst::I64GtS => output.push_str(&format!("{}i64.gt_s\n", indent_str)),

        // F64 operations
        LirInst::F64Add => output.push_str(&format!("{}f64.add\n", indent_str)),
        LirInst::F64Sub => output.push_str(&format!("{}f64.sub\n", indent_str)),
        LirInst::F64Mul => output.push_str(&format!("{}f64.mul\n", indent_str)),
        LirInst::F64Div => output.push_str(&format!("{}f64.div\n", indent_str)),
        LirInst::F64Eq => output.push_str(&format!("{}f64.eq\n", indent_str)),
        LirInst::F64Ne => output.push_str(&format!("{}f64.ne\n", indent_str)),

        // Memory operations
        LirInst::I32Load { offset, align } => {
            output.push_str(&format!("{}i32.load offset={} align={}\n", indent_str, offset, align))
        }
        LirInst::I64Load { offset, align } => {
            output.push_str(&format!("{}i64.load offset={} align={}\n", indent_str, offset, align))
        }
        LirInst::F32Load { offset, align } => {
            output.push_str(&format!("{}f32.load offset={} align={}\n", indent_str, offset, align))
        }
        LirInst::F64Load { offset, align } => {
            output.push_str(&format!("{}f64.load offset={} align={}\n", indent_str, offset, align))
        }
        LirInst::I32Store { offset, align } => {
            output.push_str(&format!("{}i32.store offset={} align={}\n", indent_str, offset, align))
        }
        LirInst::I64Store { offset, align } => {
            output.push_str(&format!("{}i64.store offset={} align={}\n", indent_str, offset, align))
        }
        LirInst::F32Store { offset, align } => {
            output.push_str(&format!("{}f32.store offset={} align={}\n", indent_str, offset, align))
        }
        LirInst::F64Store { offset, align } => {
            output.push_str(&format!("{}f64.store offset={} align={}\n", indent_str, offset, align))
        }

        // Control flow
        LirInst::Call(idx) => output.push_str(&format!("{}call {}\n", indent_str, idx)),
        LirInst::Return => output.push_str(&format!("{}return\n", indent_str)),
        LirInst::Br(depth) => output.push_str(&format!("{}br {}\n", indent_str, depth)),
        LirInst::BrIf(depth) => output.push_str(&format!("{}br_if {}\n", indent_str, depth)),
        LirInst::Drop => output.push_str(&format!("{}drop\n", indent_str)),
        LirInst::Nop => output.push_str(&format!("{}nop\n", indent_str)),

        LirInst::Block { instructions } => {
            output.push_str(&format!("{}block\n", indent_str));
            for inner in instructions {
                output.push_str(&display_lir_inst(inner, indent + 1));
            }
            output.push_str(&format!("{}end\n", indent_str));
        }

        LirInst::Loop { instructions } => {
            output.push_str(&format!("{}loop\n", indent_str));
            for inner in instructions {
                output.push_str(&display_lir_inst(inner, indent + 1));
            }
            output.push_str(&format!("{}end\n", indent_str));
        }

        LirInst::If {
            then_branch,
            else_branch,
        } => {
            output.push_str(&format!("{}if\n", indent_str));
            for inner in then_branch {
                output.push_str(&display_lir_inst(inner, indent + 1));
            }
            if let Some(else_insts) = else_branch {
                output.push_str(&format!("{}else\n", indent_str));
                for inner in else_insts {
                    output.push_str(&display_lir_inst(inner, indent + 1));
                }
            }
            output.push_str(&format!("{}end\n", indent_str));
        }

        // Ownership operations
        LirInst::TagAsOwned(idx) => {
            output.push_str(&format!("{}tag_as_owned {}\n", indent_str, idx))
        }
        LirInst::TagAsBorrowed(idx) => {
            output.push_str(&format!("{}tag_as_borrowed {}\n", indent_str, idx))
        }
        LirInst::MaskPointer => output.push_str(&format!("{}mask_pointer\n", indent_str)),
        LirInst::TestOwnership => output.push_str(&format!("{}test_ownership\n", indent_str)),
        LirInst::PossibleDrop(idx) => {
            output.push_str(&format!("{}possible_drop {}\n", indent_str, idx))
        }
        LirInst::PrepareOwnedArg(idx) => {
            output.push_str(&format!("{}prepare_owned_arg {}\n", indent_str, idx))
        }
        LirInst::PrepareBorrowedArg(idx) => {
            output.push_str(&format!("{}prepare_borrowed_arg {}\n", indent_str, idx))
        }
        LirInst::HandleOwnedParam {
            param_local,
            real_ptr_local,
        } => {
            output.push_str(&format!(
                "{}handle_owned_param {} -> {}\n",
                indent_str, param_local, real_ptr_local
            ))
        }
    }

    output
}
