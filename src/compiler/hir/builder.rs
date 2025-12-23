//! HIR builder
//!
//! Converts AST into a structured HIR representation suitable for borrow checking.
//!
//! Key responsibilities:
//! - Linearize expressions into statements operating on places
//! - Eliminate nested expressions by introducing temporary locals
//! - Convert borrow intent (not ownership outcome)
//! - Preserve structured control flow for CFG analysis
//! - Maintain a place-based memory model

use crate::compiler::compiler_messages::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::{HirExpr, HirExprKind, HirKind, HirMatchArm, HirNode, HirNodeId};
use crate::compiler::hir::place::Place;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::statements::branching::MatchArm;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::{InternedString, StringTable};
use crate::return_compiler_error;
use std::collections::HashMap;

/// Build a HIR module from the AST
///
/// The HIR builder performs linearization of expressions and creates
/// a place-based representation suitable for borrow checking analysis.
pub struct HirBuilder<'a> {
    /// Current module scope for name resolution
    pub(crate) current_scope: InternedPath,

    /// Sequential ID generator for HIR nodes (used by the borrow checker for CFG)
    next_node_id: usize,

    /// Sequential ID generator for borrow IDs (for direct O(1) candidate move refinement)
    next_borrow_id: usize,

    /// Track local variable bindings and their types
    pub(crate) local_bindings: HashMap<InternedString, DataType>,

    /// Counter for generating unique temporary variable names
    pub(crate) temp_counter: usize,

    /// Counter for generating unique runtime template function names
    #[allow(dead_code)]
    template_counter: usize,

    /// Accumulated errors and warnings during lowering
    messages: CompilerMessages,

    /// String interning table
    pub(crate) string_table: &'a mut StringTable,
}

impl<'a> HirBuilder<'a> {
    pub fn new(scope: InternedPath, string_table: &'a mut StringTable) -> Self {
        Self {
            current_scope: scope,
            next_node_id: 0,
            next_borrow_id: 0,
            local_bindings: HashMap::new(),
            temp_counter: 0,
            template_counter: 0,
            messages: CompilerMessages::new(),
            string_table,
        }
    }

    pub(crate) fn next_id(&mut self) -> HirNodeId {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }

    /// Generate a new unique borrow ID for direct O(1) candidate move refinement
    pub(crate) fn next_borrow_id(&mut self) -> crate::compiler::borrow_checker::types::BorrowId {
        let id = self.next_borrow_id;
        self.next_borrow_id += 1;
        id
    }

    /// Generate a unique temporary variable name
    pub(crate) fn next_temp(&mut self) -> InternedString {
        let name = format!("_temp_{}", self.temp_counter);
        self.temp_counter += 1;
        self.string_table.intern(&name)
    }

    /// Main entry point: lower the entire AST to HIR
    pub fn lower_ast(
        ast: Vec<AstNode>,
        scope: InternedPath,
        string_table: &'a mut StringTable,
    ) -> Result<Vec<HirNode>, CompilerMessages> {
        let mut builder = Self::new(scope, string_table);

        let mut hir_nodes = Vec::new();
        for node in ast {
            match builder.lower_node(node) {
                Ok(mut node_hir) => hir_nodes.append(&mut node_hir),
                Err(e) => builder.messages.errors.push(e),
            }
        }

        if !builder.messages.errors.is_empty() {
            return Err(CompilerMessages {
                errors: builder.messages.errors,
                warnings: builder.messages.warnings,
            });
        }

        Ok(hir_nodes)
    }

    /// Helper: create a literal expression assignment to a temporary place
    pub(crate) fn create_literal_assignment(
        &mut self,
        expr_kind: HirExprKind,
        data_type: DataType,
        location: TextLocation,
    ) -> (Vec<HirNode>, Place) {
        let temp = self.next_temp();
        let temp_place = Place::local(temp);
        let literal_expr = HirExpr {
            kind: expr_kind,
            data_type,
            location: location.clone(),
        };
        let assign_node = self.create_assign_node_with_expr(
            temp_place.clone(),
            literal_expr,
            location,
            self.current_scope.clone(),
        );
        (vec![assign_node], temp_place)
    }

    /// Helper: lower a block of nodes
    pub(crate) fn lower_block(
        &mut self,
        nodes: Vec<AstNode>,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut hir_nodes = Vec::new();
        for node in nodes {
            let mut node_hir = self.lower_node(node)?;
            hir_nodes.append(&mut node_hir);
        }
        Ok(hir_nodes)
    }

    /// Create an assignment node from place to place
    pub(crate) fn create_assign_node(
        &mut self,
        target: Place,
        source: Place,
        location: TextLocation,
        scope: InternedPath,
    ) -> HirNode {
        let load_expr = HirExpr {
            kind: HirExprKind::Load(source),
            data_type: DataType::Inferred, // Type will be inferred
            location: location.clone(),
        };

        HirNode {
            kind: HirKind::Assign {
                place: target,
                value: load_expr,
            },
            location,
            scope,
            id: self.next_id(),
        }
    }

    /// Create an assignment node from an expression
    pub(crate) fn create_assign_node_with_expr(
        &mut self,
        target: Place,
        expr: HirExpr,
        location: TextLocation,
        scope: InternedPath,
    ) -> HirNode {
        HirNode {
            kind: HirKind::Assign {
                place: target,
                value: expr,
            },
            location,
            scope,
            id: self.next_id(),
        }
    }

    /// Helper: lower match arm
    pub(crate) fn lower_match_arm(&mut self, arm: MatchArm) -> Result<HirMatchArm, CompilerError> {
        // Lower the condition expression to create a pattern
        let pattern = self.lower_expr_to_pattern(arm.condition)?;

        // Lower the body recursively
        let body = self.lower_block(arm.body)?;

        Ok(HirMatchArm {
            pattern,
            guard: None, // Guards not yet supported in AST - will be added when parser supports them
            body,
        })
    }

    /// Helper: convert AST node to Place (for assignment targets)
    pub(crate) fn lower_ast_node_to_place(
        &mut self,
        node: AstNode,
    ) -> Result<Place, CompilerError> {
        match node.kind {
            NodeKind::Rvalue(expr) => Ok(self.lower_expr_to_place(expr)?.1),

            NodeKind::FieldAccess { base, field, .. } => {
                let base_place = self.lower_ast_node_to_place(*base)?;
                Ok(base_place.field(field))
            }

            // TODO: Add support for index access when it's implemented in the AST
            // For now, indexing might be handled through method calls like .get() and .set()
            _ => {
                return_compiler_error!(
                    "Invalid assignment target: {:?}",
                    node.kind; {
                        CompilationStage => "HIR Generation",
                        PrimarySuggestion => "Only variables and fields can be assigned to"
                    }
                )
            }
        }
    }
}
