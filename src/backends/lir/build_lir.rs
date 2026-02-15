//! Main Lowering Entry Point
//!
//! This module contains the main `lower_hir_to_lir` function that transforms
//! a HIR module into a LIR module.

use crate::backends::lir::nodes::{LirFunction, LirModule, LirStruct};
use crate::compiler_frontend::compiler_messages::compiler_errors::{
    CompilerError, CompilerMessages,
};
use crate::compiler_frontend::hir::nodes::{HirBlock, HirKind, HirModule, HirStmt};

use super::context::LoweringContext;

/// Lowers a HIR module to a LIR module.
///
/// This is the main entry point for the HIR to LIR lowering pass.
/// It processes all function and struct definitions in the HIR module
/// and produces a complete LIR module ready for WASM codegen.
pub fn lower_hir_to_lir(hir_module: HirModule) -> Result<LirModule, CompilerMessages> {
    let mut ctx = LoweringContext::new();
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut errors = Vec::new();

    // First pass: register all functions and structs
    for block in &hir_module.blocks {
        for node in &block.nodes {
            if let HirKind::Stmt(stmt) = &node.kind {
                match stmt {
                    HirStmt::FunctionDef { name, .. } => {
                        ctx.register_function(*name);
                    }
                    HirStmt::StructDef { name, fields } => {
                        ctx.register_struct_layout(*name, fields);
                    }
                    _ => {}
                }
            }
        }
    }

    // Second pass: lower all definitions
    for block in &hir_module.blocks {
        for node in &block.nodes {
            if let HirKind::Stmt(stmt) = &node.kind {
                match stmt {
                    HirStmt::FunctionDef {
                        name,
                        signature,
                        body,
                    } => {
                        // Determine if this is the main function
                        let is_main = name.to_string() == "main";
                        match lower_function(
                            &mut ctx,
                            *name,
                            signature,
                            *body,
                            &hir_module.blocks,
                            is_main,
                        ) {
                            Ok(func) => functions.push(func),
                            Err(e) => errors.push(e),
                        }
                    }
                    HirStmt::StructDef { name, fields } => {
                        match ctx.lower_struct_def(*name, fields) {
                            Ok(s) => structs.push(s),
                            Err(e) => errors.push(e),
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Collect any errors from the context
    errors.extend(ctx.errors);

    if errors.is_empty() {
        Ok(LirModule { functions, structs })
    } else {
        Err(CompilerMessages {
            errors,
            warnings: Vec::new(),
        })
    }
}

/// Lowers a single function definition.
fn lower_function(
    ctx: &mut LoweringContext,
    name: crate::compiler_frontend::string_interning::InternedString,
    signature: &crate::compiler_frontend::ast::statements::functions::FunctionSignature,
    body: crate::compiler_frontend::hir::nodes::BlockId,
    blocks: &[HirBlock],
    is_main: bool,
) -> Result<LirFunction, CompilerError> {
    ctx.lower_function_def(name, signature, body, blocks, is_main)
}

/// Helper to build a LirModule from collected functions and structs.
pub fn build_lir_module(functions: Vec<LirFunction>, structs: Vec<LirStruct>) -> LirModule {
    LirModule { functions, structs }
}
