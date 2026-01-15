//! Drop Point Inserter for HIR Builder
//!
//! This module implements the DropPointInserter component that handles insertion
//! of possible_drop operations at appropriate control flow boundaries during HIR generation.
//!
//! ## Key Design Principles
//!
//! - **Structural Analysis Only**: Drop insertion is purely structural, not analytical
//! - **Conservative Insertion**: Insert drops where ownership *could* exist, not where it *does*
//! - **No Deep Ownership Analysis**: The borrow checker is the authority for ownership
//! - **Control Flow Boundaries**: Drops are inserted at scope exits, returns, breaks, and merges
//!
//! ## Drop Insertion Strategy
//!
//! The inserter uses a conservative, structural approach:
//! 1. Identify variables that *could* be owned based on type and usage
//! 2. Insert possible_drop at every control flow boundary where the variable is in scope
//! 3. Let the runtime ownership flag determine if the drop actually executes
//!
//! This approach ensures memory safety without requiring complex static analysis.

use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::hir::build_hir::{DropInsertionType, HirBuilderContext};
use crate::compiler::hir::nodes::{BlockId, HirKind, HirNode, HirPlace, HirStmt};
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::InternedString;

/// The DropPointInserter component handles insertion of possible_drop operations
/// at appropriate control flow boundaries.
///
/// This component operates on borrowed HirBuilderContext rather than owning
/// independent state, ensuring a single authoritative HIR state per module.
///
/// ## Structural Drop Insertion
///
/// The inserter performs STRUCTURAL analysis only:
/// - Identifies variables that could potentially be owned based on type
/// - Inserts possible_drop at control flow boundaries
/// - Does NOT perform deep ownership analysis
/// - Does NOT determine actual ownership (that's the borrow checker's job)
#[derive(Debug, Default)]
pub struct DropPointInserter {
    // No independent context - operates on borrowed HirBuilderContext
}

impl DropPointInserter {
    /// Creates a new DropPointInserter
    pub fn new() -> Self {
        DropPointInserter {}
    }

    // =========================================================================
    // Drop Insertion Methods
    // =========================================================================

    /// Inserts possible_drop operations for variables exiting a scope.
    ///
    /// This is called when a scope exits normally (not via return/break/continue).
    /// It conservatively inserts drops for all variables that could be owned.
    pub fn insert_scope_exit_drops(
        &mut self,
        variables: &[InternedString],
        ctx: &mut HirBuilderContext,
    ) -> Vec<HirNode> {
        let mut drop_nodes = Vec::new();

        for &var in variables {
            // Only insert drops for ownership-capable variables
            if ctx.is_potentially_owned(&var) {
                // Get the location from the drop candidate or use a default
                let location = ctx
                    .get_drop_candidates_for_scope(ctx.current_scope_depth())
                    .iter()
                    .find(|c| c.variable == var)
                    .map(|c| c.location.clone())
                    .unwrap_or_default();

                let drop_node = self.create_possible_drop(HirPlace::Var(var), location, ctx);
                drop_nodes.push(drop_node);
            }
        }

        drop_nodes
    }

    /// Inserts possible_drop operations before a return statement.
    ///
    /// This ensures that all owned variables in the current function scope
    /// are properly dropped before returning.
    pub fn insert_return_drops(
        &mut self,
        owned_variables: &[InternedString],
        ctx: &mut HirBuilderContext,
    ) -> Vec<HirNode> {
        let mut drop_nodes = Vec::new();

        for &var in owned_variables {
            if ctx.is_potentially_owned(&var) {
                let location = ctx
                    .get_drop_candidates_for_scope(0) // Function scope is level 0
                    .iter()
                    .find(|c| c.variable == var)
                    .map(|c| c.location.clone())
                    .unwrap_or_default();

                let drop_node = self.create_possible_drop(HirPlace::Var(var), location, ctx);
                drop_nodes.push(drop_node);
            }
        }

        drop_nodes
    }

    /// Inserts possible_drop operations before a break statement.
    ///
    /// This handles drops for variables owned in scopes that are being exited
    /// by the break statement.
    pub fn insert_break_drops(
        &mut self,
        target_scope: usize,
        owned_variables: &[InternedString],
        ctx: &mut HirBuilderContext,
    ) -> Vec<HirNode> {
        let mut drop_nodes = Vec::new();
        let current_scope = ctx.current_scope_depth();

        // Drop variables from scopes between current and target
        for &var in owned_variables {
            if ctx.is_potentially_owned(&var) {
                // Check if this variable is in a scope being exited
                if let Some(var_scope) = ctx
                    .get_drop_candidates_for_scope(target_scope)
                    .iter()
                    .find(|c| c.variable == var)
                    .map(|c| c.scope_level)
                {
                    if var_scope > target_scope && var_scope <= current_scope {
                        let location = ctx
                            .get_drop_candidates_for_scope(var_scope)
                            .iter()
                            .find(|c| c.variable == var)
                            .map(|c| c.location.clone())
                            .unwrap_or_default();

                        let drop_node =
                            self.create_possible_drop(HirPlace::Var(var), location, ctx);
                        drop_nodes.push(drop_node);
                    }
                }
            }
        }

        drop_nodes
    }

    /// Inserts possible_drop operations before a continue statement.
    ///
    /// Similar to break drops, but for continue statements which exit to
    /// the loop header rather than after the loop.
    pub fn insert_continue_drops(
        &mut self,
        target_scope: usize,
        owned_variables: &[InternedString],
        ctx: &mut HirBuilderContext,
    ) -> Vec<HirNode> {
        // Continue drops are similar to break drops
        self.insert_break_drops(target_scope, owned_variables, ctx)
    }

    /// Inserts possible_drop operations at control flow merge points.
    ///
    /// This is the most conservative form of drop insertion. At merge points,
    /// we insert drops for any variable that *might* be owned on *any* incoming path.
    pub fn insert_merge_drops(
        &mut self,
        potentially_owned: &[InternedString],
        ctx: &mut HirBuilderContext,
    ) -> Vec<HirNode> {
        let mut drop_nodes = Vec::new();

        for &var in potentially_owned {
            if ctx.is_potentially_owned(&var) {
                let location = ctx
                    .get_drop_candidates_for_scope(ctx.current_scope_depth())
                    .iter()
                    .find(|c| c.variable == var)
                    .map(|c| c.location.clone())
                    .unwrap_or_default();

                let drop_node = self.create_possible_drop(HirPlace::Var(var), location, ctx);
                drop_nodes.push(drop_node);
            }
        }

        drop_nodes
    }

    /// Creates a possible_drop HIR node for a given place.
    ///
    /// The possible_drop operation is conditional - it only executes if the
    /// value is actually owned at runtime (determined by the ownership flag).
    pub fn create_possible_drop(
        &mut self,
        place: HirPlace,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Stmt(HirStmt::PossibleDrop(place)),
            location,
            id: node_id,
        }
    }

    // =========================================================================
    // Ownership Capability Queries (Conservative)
    // =========================================================================

    /// Checks if a place is ownership capable.
    ///
    /// CONSERVATIVE: This only answers local, structural questions about
    /// ownership capability. It does NOT perform deep ownership analysis.
    ///
    /// Returns true if the place *could* potentially be owned based on:
    /// - The type of the base variable
    /// - The structure of the place (field access, indexing)
    ///
    /// This is a heuristic - the borrow checker makes final decisions.
    pub fn is_ownership_capable(&self, place: &HirPlace, ctx: &HirBuilderContext) -> bool {
        match place {
            HirPlace::Var(name) => {
                // Check if the variable is marked as potentially owned
                ctx.is_potentially_owned(name)
            }
            HirPlace::Field { base, .. } => {
                // Field access inherits ownership capability from base
                self.is_ownership_capable(base, ctx)
            }
            HirPlace::Index { base, .. } => {
                // Index access inherits ownership capability from base
                self.is_ownership_capable(base, ctx)
            }
        }
    }

    /// Tags an expression with potential ownership consumption.
    ///
    /// This marks expressions where ownership *could* be consumed, but doesn't
    /// determine if it actually is. The borrow checker makes that determination.
    ///
    /// This is used to mark function call arguments, assignments, and other
    /// operations where ownership transfer might occur.
    pub fn tag_potential_ownership_consumption(
        &mut self,
        var: InternedString,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) {
        // Mark the variable as potentially consumed
        ctx.mark_potentially_consumed(var);
        ctx.record_potential_last_use(var, location);
    }

    // =========================================================================
    // Drop Insertion Helpers
    // =========================================================================

    /// Gets all variables that need drops at the current scope level.
    ///
    /// This returns variables that:
    /// 1. Are potentially owned
    /// 2. Have not been consumed yet
    /// 3. Are in the current or parent scopes
    pub fn get_drop_candidates_for_current_scope(
        &self,
        ctx: &HirBuilderContext,
    ) -> Vec<InternedString> {
        let current_scope = ctx.current_scope_depth();
        ctx.get_drop_candidates_for_scope(current_scope)
            .iter()
            .filter(|c| {
                ctx.is_potentially_owned(&c.variable)
                    && !ctx
                        .metadata()
                        .ownership_hints
                        .is_potentially_consumed(&c.variable)
            })
            .map(|c| c.variable)
            .collect()
    }

    /// Inserts drops for a specific insertion type.
    ///
    /// This is a convenience method that dispatches to the appropriate
    /// drop insertion method based on the insertion type.
    pub fn insert_drops_for_type(
        &mut self,
        insertion_type: DropInsertionType,
        variables: &[InternedString],
        ctx: &mut HirBuilderContext,
    ) -> Vec<HirNode> {
        match insertion_type {
            DropInsertionType::ScopeExit => self.insert_scope_exit_drops(variables, ctx),
            DropInsertionType::Return => self.insert_return_drops(variables, ctx),
            DropInsertionType::Break { target } => {
                // Find the scope level for the target block
                let target_scope = self.find_scope_for_block(target, ctx);
                self.insert_break_drops(target_scope, variables, ctx)
            }
            DropInsertionType::Continue { target } => {
                // Find the scope level for the target block
                let target_scope = self.find_scope_for_block(target, ctx);
                self.insert_continue_drops(target_scope, variables, ctx)
            }
            DropInsertionType::Merge => self.insert_merge_drops(variables, ctx),
        }
    }

    /// Finds the scope level for a given block ID.
    ///
    /// This is used to determine which scopes are being exited by a break/continue.
    fn find_scope_for_block(&self, _block_id: BlockId, ctx: &HirBuilderContext) -> usize {
        // For now, return the current scope depth
        // A more sophisticated implementation would track block-to-scope mappings
        ctx.current_scope_depth()
    }

    // =========================================================================
    // Validation Helpers
    // =========================================================================

    /// Validates that drop points have been inserted correctly.
    ///
    /// This checks that:
    /// 1. All ownership-capable variables have drops on exit paths
    /// 2. Drops are inserted at appropriate control flow boundaries
    /// 3. No redundant drops are inserted
    pub fn validate_drop_insertion(
        &self,
        _ctx: &HirBuilderContext,
    ) -> Result<(), CompilerError> {
        // Placeholder for validation logic
        // Full implementation would check drop coverage across control flow
        Ok(())
    }
}
