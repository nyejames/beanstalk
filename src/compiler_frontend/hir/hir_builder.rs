//! HIR Builder
//!
//! Responsible for lowering Typed AST → HIR.
//!
//! This stage:
//! - Linearizes control flow into blocks
//! - Allocates locals
//! - Constructs HIR expressions/statements
//! - Establishes an explicit region tree
//!
//! This stage does NOT:
//! - Insert possible_drop
//! - Perform borrow checking
//! - Perform ownership eligibility analysis
//!
//! Those occur in later compilation phases.

use crate::compiler_frontend::hir::{hir_datatypes::*, hir_nodes::*};

use crate::compiler_frontend::ast::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, TextLocation};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::string_interning::StringTable;
use crate::return_err_as_messages;

// -----------
// Entry Point
// -----------
pub fn lower_module(
    ast: Ast,
    string_table: &mut StringTable,
) -> Result<HirModule, CompilerMessages> {
    let mut ctx = HirBuilder::new(string_table);
    ctx.build_hir_module(ast)
}

// -------------------
// HIR Builder Context
// -------------------
//
// This struct is the main entry point for the HIR builder. It manages the state of the builder
// and provides the lowering logic for each AST node.
//
// The builder is stateful and re-entrant, so it's not safe to use concurrently.

pub struct HirBuilder<'a> {
    // === Result being built ===
    module: HirModule,

    // === For variable name resolution ===
    string_table: &'a mut StringTable,

    // === ID Counters ===
    next_block_id: u32,
    next_local_id: u32,
    next_region_id: u32,

    // === Current Function State ===
    current_function: Option<FunctionId>,
    current_block: Option<BlockId>,
    current_region: Option<RegionId>,

    // Parallel Metadata Arrays (Index-aligned with the arenas above)
    // This is for resolving statements back to their original source code locations
    pub statement_locations: Vec<TextLocation>,
}

impl<'a> HirBuilder<'a> {
    // -----------
    // Constructor
    // -----------
    pub fn new(string_table: &'a mut StringTable) -> HirBuilder<'a> {
        HirBuilder {
            module: HirModule::new(),

            string_table,

            next_block_id: 0,
            next_local_id: 0,
            next_region_id: 0,

            current_function: None,
            current_block: None,
            current_region: None,
            
            statement_locations: vec![],
        }
    }

    // ========================================================================
    // Main Build Method
    // ========================================================================
    /// Builds an HIR module from an AST.
    /// This is the main entry point for HIR generation.
    pub fn build_hir_module(mut self, ast: Ast) -> Result<HirModule, CompilerMessages> {
        // Process each AST node
        for node in &ast.nodes {
            match self.process_ast_node(node) {
                Ok(_) => {}
                Err(e) => {
                    return Err(CompilerMessages {
                        errors: vec![e],
                        warnings: self.module.warnings.clone(),
                    });
                }
            }
        }

        Ok(self.module)
    }

    /// Processes a single AST node and generates corresponding HIR
    fn process_ast_node(&mut self, node: &AstNode) -> Result<(), CompilerError> {
        match &node.kind {
            NodeKind::Function(name, signature, body) => {
                // TODO: Lower functions in another file
                todo!();
            }
            NodeKind::FunctionCall {
                name,
                args,
                returns,
                location,
            } => {
                // TODO: Lower function calls in another file
                todo!();
            }
            NodeKind::HostFunctionCall {
                host_function_id,
                args,
                returns,
                location,
            } => {
                // TODO: Lower host function calls in another file
                todo!();
            }
            NodeKind::Return(exprs) => {
                todo!();
            }
            NodeKind::StructDefinition(name, fields) => {
                let struct_node: HirStruct = todo!();
                self.module.structs.push(struct_node.clone());
            }

            // Variable declarations
            NodeKind::VariableDeclaration(arg) => {
                let struct_node: HirStruct = todo!();
                self.module.structs.push(struct_node.clone());
            }

            // Assignments
            NodeKind::Assignment { target, value } => {
                todo!();
            }

            // If statements
            NodeKind::If(condition, then_body, else_body) => {
                todo!();
            }

            // For loops
            NodeKind::ForLoop(binding, iterator, body) => {
                todo!();
            }

            // While loops
            NodeKind::WhileLoop(condition, body) => {
                todo!();
            }

            // Match expressions
            NodeKind::Match(scrutinee, arms, default) => {
                todo!();
            }

            // R-values (expressions as statements)
            NodeKind::Rvalue(expr) => {
                // May be depreciated in the future as this is currently just function calls
                // Should be checked and probably renamed to just "function_call" which will become a statement
                todo!();
            }

            // Empty nodes - no HIR generated
            NodeKind::Empty | NodeKind::Newline | NodeKind::Spaces(_) => Ok(()),

            // Warnings are passed through (no HIR generated)
            // Note: warnings are collected by the build system at each stage,
            // so there is no need to collect them from the AST here.
            NodeKind::Warning(w) => Ok(()),

            // Operators should be handled within expressions
            // Note: The AST structure may change in the future so operators are specifically only inside expressions
            // Which means this could be removed.
            NodeKind::Operator(_) => Ok(()),

            // Field access as a statement
            NodeKind::FieldAccess {
                base,
                field,
                data_type,
                ..
            } => {
                todo!();
            }

            // Other node kinds - return empty for now
            _ => {
                // For unsupported nodes, return empty
                // This allows gradual implementation
                todo!();
            }
        }
    }
}
