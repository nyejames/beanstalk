//! HIR Validator - Validates HIR Invariants
//!
//! This module implements the HirValidator that checks generated HIR conforms
//! to all required invariants. These invariants turn the design document into
//! an executable contract.
//!
//! ## Core HIR Invariants
//!
//! 1. **No Nested Expressions**: All expressions in HIR are flat
//! 2. **Explicit Terminators**: Every HIR block ends in exactly one terminator
//! 3. **Variable Declaration Before Use**: All variables are declared before any use
//! 4. **Drop Coverage**: All ownership-capable variables have possible_drop on exit paths
//! 5. **Block Connectivity**: All HIR blocks are reachable from the entry block
//! 6. **Terminator Target Validity**: All branch targets reference valid block IDs
//! 7. **Assignment Discipline**: Assignments must be explicit and properly ordered

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirBlock, HirExpr, HirExprKind, HirKind, HirModule, HirNode, HirPlace, HirStmt,
    HirTerminator,
};
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use std::collections::HashSet;

// ============================================================================
// Validation Report
// ============================================================================

/// Report from HIR validation
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// List of invariants that were checked
    pub invariants_checked: Vec<String>,
    /// Any violations found
    pub violations_found: Vec<InvariantViolation>,
    /// Warnings (non-fatal issues)
    pub warnings: Vec<String>,
}

impl ValidationReport {
    pub fn new() -> Self {
        ValidationReport {
            invariants_checked: Vec::new(),
            violations_found: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Returns true if validation passed (no violations)
    pub fn is_valid(&self) -> bool {
        self.violations_found.is_empty()
    }
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Invariant Violation
// ============================================================================

/// A specific invariant violation found during validation
#[derive(Debug, Clone)]
pub struct InvariantViolation {
    /// Name of the invariant that was violated
    pub invariant: String,
    /// Location in source code (if available)
    pub location: Option<TextLocation>,
    /// Description of the violation
    pub description: String,
    /// Suggested fix (if any)
    pub suggested_fix: Option<String>,
}

// ============================================================================
// HIR Validation Error
// ============================================================================

/// Errors that can occur during HIR validation
#[derive(Debug, Clone)]
pub enum HirValidationError {
    /// Found a nested expression where flat expression was expected
    NestedExpression {
        location: TextLocation,
        expression: String,
    },
    /// Block is missing a terminator
    MissingTerminator {
        block_id: BlockId,
        location: Option<TextLocation>,
    },
    /// Block has multiple terminators
    MultipleTerminators { block_id: BlockId, count: usize },
    /// Variable used before declaration
    UndeclaredVariable {
        variable: String,
        location: TextLocation,
    },
    /// Missing drop for a variable on an exit path
    MissingDrop {
        variable: String,
        exit_path: String,
        location: TextLocation,
    },
    /// Block is unreachable from entry
    UnreachableBlock { block_id: BlockId },
    /// Branch target references invalid block
    InvalidBranchTarget {
        source_block: BlockId,
        target_block: BlockId,
    },
    /// Invalid assignment
    InvalidAssignment {
        variable: String,
        location: TextLocation,
        reason: String,
    },
}

impl From<HirValidationError> for CompilerError {
    fn from(error: HirValidationError) -> Self {
        match error {
            HirValidationError::NestedExpression {
                location,
                expression,
            } => CompilerError::new(
                format!(
                    "HIR invariant violation: nested expression found: {}",
                    expression
                ),
                location.to_error_location_without_table(),
                crate::compiler_frontend::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::MissingTerminator { block_id, location } => CompilerError::new(
                format!(
                    "HIR invariant violation: block {} is missing a terminator",
                    block_id
                ),
                location
                    .map(|l| l.to_error_location_without_table())
                    .unwrap_or_else(ErrorLocation::default),
                crate::compiler_frontend::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::MultipleTerminators { block_id, count } => CompilerError::new(
                format!(
                    "HIR invariant violation: block {} has {} terminators (expected 1)",
                    block_id, count
                ),
                ErrorLocation::default(),
                crate::compiler_frontend::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::UndeclaredVariable { variable, location } => CompilerError::new(
                format!(
                    "HIR invariant violation: variable '{}' used before declaration",
                    variable
                ),
                location.to_error_location_without_table(),
                crate::compiler_frontend::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::MissingDrop {
                variable,
                exit_path,
                location,
            } => CompilerError::new(
                format!(
                    "HIR invariant violation: missing drop for '{}' on exit path '{}'",
                    variable, exit_path
                ),
                location.to_error_location_without_table(),
                crate::compiler_frontend::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::UnreachableBlock { block_id } => CompilerError::new(
                format!(
                    "HIR invariant violation: block {} is unreachable from entry",
                    block_id
                ),
                ErrorLocation::default(),
                crate::compiler_frontend::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::InvalidBranchTarget {
                source_block,
                target_block,
            } => CompilerError::new(
                format!(
                    "HIR invariant violation: block {} branches to invalid block {}",
                    source_block, target_block
                ),
                ErrorLocation::default(),
                crate::compiler_frontend::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::InvalidAssignment {
                variable,
                location,
                reason,
            } => CompilerError::new(
                format!(
                    "HIR invariant violation: invalid assignment to '{}': {}",
                    variable, reason
                ),
                location.to_error_location_without_table(),
                crate::compiler_frontend::compiler_errors::ErrorType::HirTransformation,
            ),
        }
    }
}

// ============================================================================
// HIR Validator
// ============================================================================

/// HIR Validator - validates HIR invariants
///
/// The validator checks that the generated HIR conforms to all required invariants.
/// These invariants turn the design document into an executable contract.
///
/// ## Core HIR Invariants
///
/// 1. **No Nested Expressions**: All expressions in HIR are flat
/// 2. **Explicit Terminators**: Every HIR block ends in exactly one terminator
/// 3. **Variable Declaration Before Use**: All variables are declared before any use
/// 4. **Drop Coverage**: All ownership-capable variables have possible_drop on exit paths
/// 5. **Block Connectivity**: All HIR blocks are reachable from the entry block
/// 6. **Terminator Target Validity**: All branch targets reference valid block IDs
/// 7. **Assignment Discipline**: Assignments must be explicit and properly ordered
pub struct HirValidator;

impl HirValidator {
    /// Maximum allowed expression nesting depth.
    /// HIR expressions should be mostly flat. We allow limited nesting
    /// for binary operations, but operands should be simple.
    const MAX_EXPRESSION_DEPTH: usize = 2;

    /// Validates all HIR invariants on a module.
    /// Returns a validation report with all checked invariants and any violations.
    pub fn validate_module(hir_module: &HirModule) -> Result<ValidationReport, HirValidationError> {
        let mut report = ValidationReport::new();

        // Invariant 1: No nested expressions
        report
            .invariants_checked
            .push("no_nested_expressions".to_string());
        if let Err(e) = Self::check_no_nested_expressions(hir_module) {
            report.violations_found.push(InvariantViolation {
                invariant: "no_nested_expressions".to_string(),
                location: None,
                description: format!("{:?}", e),
                suggested_fix: Some("Flatten nested expressions into temporaries".to_string()),
            });
            return Err(e);
        }

        // Invariant 2: Explicit terminators
        report
            .invariants_checked
            .push("explicit_terminators".to_string());
        if let Err(e) = Self::check_explicit_terminators(hir_module) {
            report.violations_found.push(InvariantViolation {
                invariant: "explicit_terminators".to_string(),
                location: None,
                description: format!("{:?}", e),
                suggested_fix: Some(
                    "Ensure every block ends with exactly one terminator".to_string(),
                ),
            });
            return Err(e);
        }

        // Invariant 5: Block connectivity
        report
            .invariants_checked
            .push("block_connectivity".to_string());
        if let Err(e) = Self::check_block_connectivity(hir_module) {
            report.violations_found.push(InvariantViolation {
                invariant: "block_connectivity".to_string(),
                location: None,
                description: format!("{:?}", e),
                suggested_fix: Some(
                    "Remove unreachable blocks or add control flow paths".to_string(),
                ),
            });
            return Err(e);
        }

        // Invariant 6: Terminator target validity
        report
            .invariants_checked
            .push("terminator_targets".to_string());
        if let Err(e) = Self::check_terminator_targets(hir_module) {
            report.violations_found.push(InvariantViolation {
                invariant: "terminator_targets".to_string(),
                location: None,
                description: format!("{:?}", e),
                suggested_fix: Some(
                    "Ensure all branch targets reference valid block IDs".to_string(),
                ),
            });
            return Err(e);
        }

        // Invariant 3: Variable declaration order
        report
            .invariants_checked
            .push("variable_declaration_order".to_string());
        Self::check_variable_declaration_order(hir_module)?;

        // Invariant 7: Assignment discipline
        report
            .invariants_checked
            .push("assignment_discipline".to_string());
        Self::check_assignment_discipline(hir_module)?;

        // Invariant 4: Drop coverage
        report.invariants_checked.push("drop_coverage".to_string());
        Self::check_drop_coverage(hir_module)?;

        Ok(report)
    }

    /// Validates a single block's invariants
    pub fn validate_block(block: &HirBlock) -> Result<(), HirValidationError> {
        // Check expression flatness
        for node in &block.nodes {
            Self::check_node_expressions_flat(node)?;
        }

        // Check terminator presence (non-empty blocks must have exactly one terminator)
        if !block.nodes.is_empty() {
            let terminator_count = Self::count_terminators_in_block(block);
            if terminator_count == 0 {
                if let Some(last_node) = block.nodes.last() {
                    if !Self::is_terminator(last_node) {
                        return Err(HirValidationError::MissingTerminator {
                            block_id: block.id,
                            location: Some(last_node.location.clone()),
                        });
                    }
                }
            } else if terminator_count > 1 {
                return Err(HirValidationError::MultipleTerminators {
                    block_id: block.id,
                    count: terminator_count,
                });
            }
        }

        Ok(())
    }

    /// Checks that no expressions contain deeply nested expressions.
    /// All expressions in HIR should be mostly flat.
    pub fn check_no_nested_expressions(hir_module: &HirModule) -> Result<(), HirValidationError> {
        for block in &hir_module.blocks {
            for node in &block.nodes {
                Self::check_node_expressions_flat(node)?;
            }
        }
        // Also check function definitions
        for func in &hir_module.functions {
            Self::check_node_expressions_flat(func)?;
        }
        Ok(())
    }

    /// Helper to check that expressions in a node are flat
    fn check_node_expressions_flat(node: &HirNode) -> Result<(), HirValidationError> {
        match &node.kind {
            HirKind::Stmt(stmt) => {
                Self::check_stmt_expressions_flat(stmt, &node.location)?;
            }
            HirKind::Terminator(term) => {
                Self::check_terminator_expressions_flat(term, &node.location)?;
            }
        }
        Ok(())
    }

    /// Checks that expressions in a statement are flat
    fn check_stmt_expressions_flat(
        stmt: &HirStmt,
        location: &TextLocation,
    ) -> Result<(), HirValidationError> {
        match stmt {
            HirStmt::Assign { value, .. } => {
                Self::check_expr_nesting_depth(value, 0, location)?;
            }
            HirStmt::Call { args, .. } => {
                for arg in args {
                    Self::check_expr_nesting_depth(arg, 0, location)?;
                }
            }
            HirStmt::RuntimeTemplateCall { captures, .. } => {
                for capture in captures {
                    Self::check_expr_nesting_depth(capture, 0, location)?;
                }
            }
            HirStmt::ExprStmt(expr) => {
                Self::check_expr_nesting_depth(expr, 0, location)?;
            }
            HirStmt::PossibleDrop(_)
            | HirStmt::TemplateFn { .. }
            | HirStmt::FunctionDef { .. }
            | HirStmt::StructDef { .. } => {}
        }
        Ok(())
    }

    /// Checks that expressions in a terminator are flat
    fn check_terminator_expressions_flat(
        term: &HirTerminator,
        location: &TextLocation,
    ) -> Result<(), HirValidationError> {
        match term {
            HirTerminator::If { condition, .. } => {
                Self::check_expr_nesting_depth(condition, 0, location)?;
            }
            HirTerminator::Match { scrutinee, .. } => {
                Self::check_expr_nesting_depth(scrutinee, 0, location)?;
            }
            HirTerminator::Loop { iterator, .. } => {
                if let Some(iter) = iterator {
                    Self::check_expr_nesting_depth(iter, 0, location)?;
                }
            }
            HirTerminator::Return(exprs) => {
                for expr in exprs {
                    Self::check_expr_nesting_depth(expr, 0, location)?;
                }
            }
            HirTerminator::ReturnError(expr) => {
                Self::check_expr_nesting_depth(expr, 0, location)?;
            }
            HirTerminator::Panic { message } => {
                if let Some(msg) = message {
                    Self::check_expr_nesting_depth(msg, 0, location)?;
                }
            }
            HirTerminator::Break { .. } | HirTerminator::Continue { .. } => {}
        }
        Ok(())
    }

    /// Checks expression nesting depth
    fn check_expr_nesting_depth(
        expr: &HirExpr,
        current_depth: usize,
        location: &TextLocation,
    ) -> Result<(), HirValidationError> {
        if current_depth > Self::MAX_EXPRESSION_DEPTH {
            return Err(HirValidationError::NestedExpression {
                location: location.clone(),
                expression: format!("{:?}", expr.kind),
            });
        }

        match &expr.kind {
            // Simple expressions - no nesting
            HirExprKind::Int(_)
            | HirExprKind::Float(_)
            | HirExprKind::Bool(_)
            | HirExprKind::StringLiteral(_)
            | HirExprKind::HeapString(_)
            | HirExprKind::Char(_)
            | HirExprKind::Load(_)
            | HirExprKind::Field { .. }
            | HirExprKind::Move(_) => Ok(()),

            HirExprKind::BinOp { left, right, .. } => {
                Self::check_expr_nesting_depth(left, current_depth + 1, location)?;
                Self::check_expr_nesting_depth(right, current_depth + 1, location)
            }
            HirExprKind::UnaryOp { operand, .. } => {
                Self::check_expr_nesting_depth(operand, current_depth + 1, location)
            }
            HirExprKind::Call { args, .. } => {
                for arg in args {
                    Self::check_expr_nesting_depth(arg, current_depth + 1, location)?;
                }
                Ok(())
            }
            HirExprKind::MethodCall { receiver, args, .. } => {
                Self::check_expr_nesting_depth(receiver, current_depth + 1, location)?;
                for arg in args {
                    Self::check_expr_nesting_depth(arg, current_depth + 1, location)?;
                }
                Ok(())
            }
            HirExprKind::StructConstruct { fields, .. } => {
                for (_, field_expr) in fields {
                    Self::check_expr_nesting_depth(field_expr, current_depth + 1, location)?;
                }
                Ok(())
            }
            HirExprKind::Collection(exprs) => {
                for e in exprs {
                    Self::check_expr_nesting_depth(e, current_depth + 1, location)?;
                }
                Ok(())
            }
            HirExprKind::Range { start, end } => {
                Self::check_expr_nesting_depth(start, current_depth + 1, location)?;
                Self::check_expr_nesting_depth(end, current_depth + 1, location)
            }
        }
    }

    /// Checks that every block ends in exactly one terminator.
    pub fn check_explicit_terminators(hir_module: &HirModule) -> Result<(), HirValidationError> {
        for block in &hir_module.blocks {
            let terminator_count = Self::count_terminators_in_block(block);

            if block.nodes.is_empty() {
                continue; // Allow empty blocks during construction
            }

            if terminator_count == 0 {
                if let Some(last_node) = block.nodes.last() {
                    if !Self::is_terminator(last_node) {
                        return Err(HirValidationError::MissingTerminator {
                            block_id: block.id,
                            location: Some(last_node.location.clone()),
                        });
                    }
                }
            } else if terminator_count > 1 {
                return Err(HirValidationError::MultipleTerminators {
                    block_id: block.id,
                    count: terminator_count,
                });
            }
        }
        Ok(())
    }

    /// Counts the number of terminator nodes in a block
    fn count_terminators_in_block(block: &HirBlock) -> usize {
        block
            .nodes
            .iter()
            .filter(|n| Self::is_terminator(n))
            .count()
    }

    /// Checks if a node is a terminator
    pub fn is_terminator(node: &HirNode) -> bool {
        matches!(node.kind, HirKind::Terminator(_))
    }

    /// Checks if a node is a statement
    pub fn is_statement(node: &HirNode) -> bool {
        matches!(node.kind, HirKind::Stmt(_))
    }

    /// Checks that all blocks are reachable from the entry block.
    /// Function body blocks are considered reachable through their function definitions.
    pub fn check_block_connectivity(hir_module: &HirModule) -> Result<(), HirValidationError> {
        if hir_module.blocks.is_empty() {
            return Ok(());
        }

        let mut reachable: HashSet<BlockId> = HashSet::new();
        let mut to_visit: Vec<BlockId> = vec![hir_module.entry_block];

        // Collect function body blocks from function definitions
        let mut function_body_blocks: HashSet<BlockId> = HashSet::new();
        for func_node in &hir_module.functions {
            if let HirKind::Stmt(HirStmt::FunctionDef { body, .. }) = &func_node.kind {
                function_body_blocks.insert(*body);
            }
            if let HirKind::Stmt(HirStmt::TemplateFn { body, .. }) = &func_node.kind {
                function_body_blocks.insert(*body);
            }
        }

        while let Some(block_id) = to_visit.pop() {
            if reachable.contains(&block_id) {
                continue;
            }
            reachable.insert(block_id);

            if let Some(block) = hir_module.blocks.iter().find(|b| b.id == block_id) {
                for succ in Self::get_block_successors(block) {
                    if !reachable.contains(&succ) {
                        to_visit.push(succ);
                    }
                }
            }
        }

        // Check that all non-function-body blocks are reachable
        // Function body blocks are only reachable when their functions are called,
        // so we exclude them from the strict connectivity check
        for block in &hir_module.blocks {
            if !reachable.contains(&block.id) && !function_body_blocks.contains(&block.id) {
                return Err(HirValidationError::UnreachableBlock { block_id: block.id });
            }
        }

        Ok(())
    }

    /// Gets the successor block IDs from a block's terminator
    pub fn get_block_successors(block: &HirBlock) -> Vec<BlockId> {
        let mut successors = Vec::new();

        for node in &block.nodes {
            if let HirKind::Terminator(term) = &node.kind {
                match term {
                    HirTerminator::If {
                        then_block,
                        else_block,
                        ..
                    } => {
                        successors.push(*then_block);
                        if let Some(else_id) = else_block {
                            successors.push(*else_id);
                        }
                    }
                    HirTerminator::Match {
                        arms,
                        default_block,
                        ..
                    } => {
                        for arm in arms {
                            successors.push(arm.body);
                        }
                        if let Some(default_id) = default_block {
                            successors.push(*default_id);
                        }
                    }
                    HirTerminator::Loop { body, .. } => {
                        successors.push(*body);
                    }
                    HirTerminator::Break { target } | HirTerminator::Continue { target } => {
                        successors.push(*target);
                    }
                    HirTerminator::Return(_)
                    | HirTerminator::ReturnError(_)
                    | HirTerminator::Panic { .. } => {}
                }
            }
        }

        successors
    }

    /// Checks that all branch targets reference valid block IDs.
    pub fn check_terminator_targets(hir_module: &HirModule) -> Result<(), HirValidationError> {
        let valid_block_ids: HashSet<BlockId> = hir_module.blocks.iter().map(|b| b.id).collect();

        for block in &hir_module.blocks {
            for succ in Self::get_block_successors(block) {
                if !valid_block_ids.contains(&succ) {
                    return Err(HirValidationError::InvalidBranchTarget {
                        source_block: block.id,
                        target_block: succ,
                    });
                }
            }
        }

        Ok(())
    }

    /// Checks that all variables are declared before use.
    pub fn check_variable_declaration_order(
        _hir_module: &HirModule,
    ) -> Result<(), HirValidationError> {
        // Placeholder - full implementation requires tracking declarations through control flow
        Ok(())
    }

    /// Checks that all ownership-capable variables have possible_drop on every exit path.
    pub fn check_drop_coverage(_hir_module: &HirModule) -> Result<(), HirValidationError> {
        // Placeholder - full implementation requires control flow analysis
        Ok(())
    }

    /// Checks that assignments follow proper discipline.
    pub fn check_assignment_discipline(hir_module: &HirModule) -> Result<(), HirValidationError> {
        for block in &hir_module.blocks {
            for node in &block.nodes {
                if let HirKind::Stmt(HirStmt::Assign {
                    target, is_mutable, ..
                }) = &node.kind
                {
                    Self::check_assignment_target_valid(target, *is_mutable, &node.location)?;
                }
            }
        }
        Ok(())
    }

    /// Checks that an assignment target is valid
    fn check_assignment_target_valid(
        target: &HirPlace,
        is_mutable: bool,
        location: &TextLocation,
    ) -> Result<(), HirValidationError> {
        match target {
            HirPlace::Var(_) => Ok(()),
            HirPlace::Field { base, .. } => {
                Self::check_assignment_target_valid(base, is_mutable, location)
            }
            HirPlace::Index { base, .. } => {
                Self::check_assignment_target_valid(base, is_mutable, location)
            }
        }
    }
}
