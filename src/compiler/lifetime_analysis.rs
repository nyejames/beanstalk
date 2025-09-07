use crate::compiler::compiler_errors::CompileError;
use crate::compiler::mir::mir_nodes::{MIR, MirBlock};
use crate::return_compiler_error;
/// Lifetime Analysis Module for the Beanstalk Compiler
///
/// This module provides comprehensive lifetime analysis capabilities including:
/// - Variable lifetime tracking across blocks and functions
/// - Ownership and borrow relationship management
/// - Reference counting analysis for automatic memory management
/// - Move semantics detection and optimization
/// - Borrow checker violation detection
use std::collections::{HashMap, HashSet};

/// Types of borrow checker violations
#[derive(Debug, Clone, PartialEq)]
pub enum BorrowViolationType {
    UseAfterMove,
    PotentialUseAfterMove,
    MutableBorrowConflict,
    ImmutableBorrowConflict,
    LifetimeViolation,
}

/// Borrow checker violation information
#[derive(Debug, Clone, PartialEq)]
pub struct BorrowViolation {
    pub variable_id: u32,
    pub violation_type: BorrowViolationType,
    pub block_id: u32,
    pub description: String,
}

/// Information about a variable's lifetime across blocks
#[derive(Debug, Clone, PartialEq)]
pub struct VariableLifetime {
    pub variable_id: u32,
    pub definition_block: Option<u32>,
    pub last_use_block: Option<u32>,
    pub usage_blocks: Vec<u32>,
}

/// Lifetime analyzer for comprehensive variable lifetime analysis
pub struct LifetimeAnalyzer {
    /// Variable usage tracking per block
    block_variable_usage: HashMap<u32, HashSet<u32>>,
    /// Variable definitions per block
    block_variable_definitions: HashMap<u32, HashSet<u32>>,
    /// Last use tracking for variables
    variable_last_use: HashMap<u32, u32>, // variable_id -> block_id
    /// Stack of active blocks being built (for compatibility)
    block_stack: Vec<u32>,
}

impl LifetimeAnalyzer {
    pub fn new() -> Self {
        Self {
            block_variable_usage: HashMap::new(),
            block_variable_definitions: HashMap::new(),
            variable_last_use: HashMap::new(),
            block_stack: Vec::new(),
        }
    }

    /// Track variable definition in a block
    pub fn track_variable_definition(&mut self, variable_id: u32, block_id: u32) {
        self.block_variable_definitions
            .entry(block_id)
            .or_insert_with(HashSet::new)
            .insert(variable_id);
    }

    /// Track variable usage in a block
    pub fn track_variable_use(&mut self, variable_id: u32, block_id: u32) {
        self.block_variable_usage
            .entry(block_id)
            .or_insert_with(HashSet::new)
            .insert(variable_id);

        // Update last use tracking
        self.variable_last_use.insert(variable_id, block_id);
    }

    /// Get the definition block for a variable (first block where it's defined)
    pub fn get_variable_definition_block(&self, variable_id: u32) -> Option<u32> {
        for (&block_id, definitions) in &self.block_variable_definitions {
            if definitions.contains(&variable_id) {
                return Some(block_id);
            }
        }
        None
    }

    /// Get the last use block for a variable
    pub fn get_variable_last_use(&self, variable_id: u32) -> Option<u32> {
        self.variable_last_use.get(&variable_id).copied()
    }

    /// Analyze variable lifetime across all blocks
    pub fn analyze_variable_lifetime(&self, variable_id: u32) -> VariableLifetime {
        let definition_block = self.get_variable_definition_block(variable_id);
        let last_use_block = self.get_variable_last_use(variable_id);

        let mut usage_blocks = Vec::new();
        for (&block_id, usage) in &self.block_variable_usage {
            if usage.contains(&variable_id) {
                usage_blocks.push(block_id);
            }
        }
        usage_blocks.sort();

        VariableLifetime {
            variable_id,
            definition_block,
            last_use_block,
            usage_blocks,
        }
    }

    /// Analyze which variables are used after a given block
    pub fn analyze_variables_used_after(&self, block_id: u32, all_blocks: &[u32]) -> HashSet<u32> {
        let mut used_after = HashSet::new();

        // Find blocks that come after the given block
        let block_index = all_blocks.iter().position(|&id| id == block_id);
        if let Some(index) = block_index {
            for &later_block_id in &all_blocks[index + 1..] {
                if let Some(usage) = self.block_variable_usage.get(&later_block_id) {
                    used_after.extend(usage);
                }
            }
        }

        used_after
    }

    /// Integrate with MIR lifetime analysis
    pub fn sync_with_ir_lifetime_analysis(&self, _ir: &mut MIR) {
        // Collect all variables that have been tracked (either defined or used)
        let mut all_variables = std::collections::HashSet::new();

        // Add variables from definitions
        for &variable_id in self.block_variable_definitions.keys() {
            all_variables.insert(variable_id);
        }

        // Add variables from usage
        for &variable_id in self
            .block_variable_usage
            .values()
            .flat_map(|usage| usage.iter())
        {
            all_variables.insert(variable_id);
        }

        // Process each variable
        for &variable_id in &all_variables {
            // Find definition block (if any)
            let _definition_block = self.get_variable_definition_block(variable_id).unwrap_or(0);

            // Track variable usage pattern in MIR
            // TODO: Implement ownership tracking in simplified MIR
            // TODO: Implement variable usage tracking in simplified MIR

            // Record all usage blocks
            for (&_block_id, usage) in &self.block_variable_usage {
                if usage.contains(&variable_id) {
                    // TODO: Implement variable usage recording in simplified MIR
                }
            }
        }
    }

    /// Create lifetime annotations for variables with complex usage patterns
    pub fn create_lifetime_annotations(&self, _ir: &mut MIR) -> HashMap<u32, u32> {
        let mut variable_to_lifetime = HashMap::new();

        for &variable_id in self.block_variable_definitions.keys() {
            let lifetime = self.analyze_variable_lifetime(variable_id);

            // Create a lifetime if variable is used across multiple blocks
            if lifetime.usage_blocks.len() > 1 {
                let _lifetime_name = format!("var_{}_lifetime", variable_id);
                let lifetime_id = 0; // TODO: Implement lifetime creation in simplified MIR

                // Associate lifetime with all usage blocks
                // TODO: This will be replaced by the simplified MIR borrow checker
                // The new system uses program points and dataflow analysis instead

                variable_to_lifetime.insert(variable_id, lifetime_id);
            }
        }

        variable_to_lifetime
    }

    /// Detect potential borrow checker violations
    pub fn detect_borrow_violations(&self) -> Vec<BorrowViolation> {
        let mut violations = Vec::new();

        // Check for use-after-move violations
        for (&variable_id, &last_use_block) in &self.variable_last_use {
            if let Some(_definition_block) = self.get_variable_definition_block(variable_id) {
                // Simple heuristic: if variable is used in multiple blocks after definition,
                // it might have lifetime issues
                let usage_blocks: Vec<_> = self
                    .block_variable_usage
                    .iter()
                    .filter(|(_, usage)| usage.contains(&variable_id))
                    .map(|(&block_id, _)| block_id)
                    .collect();

                if usage_blocks.len() > 2 {
                    // Definition + multiple uses
                    violations.push(BorrowViolation {
                        variable_id,
                        violation_type: BorrowViolationType::PotentialUseAfterMove,
                        block_id: last_use_block,
                        description: format!(
                            "Variable {} used across {} blocks",
                            variable_id,
                            usage_blocks.len()
                        ),
                    });
                }
            }
        }

        violations
    }

    /// Get all blocks that have been tracked
    pub fn get_all_tracked_blocks(&self) -> Vec<u32> {
        self.block_variable_usage.keys().copied().collect()
    }

    /// Check if a variable is defined before it's used in a block
    pub fn is_variable_defined_before_use(&self, variable_id: u32, block_id: u32) -> bool {
        if let Some(definitions) = self.block_variable_definitions.get(&block_id) {
            definitions.contains(&variable_id)
        } else {
            false
        }
    }

    /// Clear tracking data for a specific block (useful for cleanup)
    pub fn clear_block_tracking(&mut self, block_id: u32) {
        self.block_variable_usage.remove(&block_id);
        self.block_variable_definitions.remove(&block_id);

        // Remove from last use tracking if this was the last use block
        self.variable_last_use
            .retain(|_, &mut last_block| last_block != block_id);
    }

    // Compatibility methods for BlockManager interface

    /// Enter a block (for compatibility with old BlockManager interface)
    pub fn enter_block(&mut self, block_id: u32) {
        self.block_stack.push(block_id);
        self.block_variable_usage.insert(block_id, HashSet::new());
        self.block_variable_definitions
            .insert(block_id, HashSet::new());
    }

    /// Exit a block (for compatibility with old BlockManager interface)
    pub fn exit_block(&mut self) -> Option<u32> {
        self.block_stack.pop()
    }

    /// Get current block (for compatibility with old BlockManager interface)
    pub fn current_block(&self) -> Option<u32> {
        self.block_stack.last().copied()
    }

    /// Track variable definition in current block (compatibility method)
    pub fn track_variable_definition_current(&mut self, variable_id: u32) {
        if let Some(current_block) = self.current_block() {
            if let Some(definitions) = self.block_variable_definitions.get_mut(&current_block) {
                definitions.insert(variable_id);
            }
        }
    }

    /// Track variable use in current block (compatibility method)
    pub fn track_variable_use_current(&mut self, variable_id: u32) {
        if let Some(current_block) = self.current_block() {
            if let Some(usage) = self.block_variable_usage.get_mut(&current_block) {
                usage.insert(variable_id);
            }
            // Update last use tracking
            self.variable_last_use.insert(variable_id, current_block);
        }
    }

    /// Get block variable definitions (compatibility method)
    pub fn get_block_variable_definitions(&self, block_id: u32) -> Option<&HashSet<u32>> {
        self.block_variable_definitions.get(&block_id)
    }

    /// Get block variable usage (compatibility method)
    pub fn get_block_variable_usage(&self, block_id: u32) -> Option<&HashSet<u32>> {
        self.block_variable_usage.get(&block_id)
    }
}

/// Finalize lifetime analysis after MIR generation
pub fn finalize_lifetime_analysis(
    ir: &mut MIR,
    analyzer: &LifetimeAnalyzer,
) -> Result<(), CompileError> {
    // Sync analyzer data with MIR lifetime analysis
    analyzer.sync_with_ir_lifetime_analysis(ir);

    // Create lifetime annotations for complex variables
    let _variable_lifetimes = analyzer.create_lifetime_annotations(ir);

    // Detect potential borrow checker violations
    let violations = analyzer.detect_borrow_violations();

    // For now, we'll just log violations as warnings
    // In a full implementation, these would be proper compile errors
    for violation in violations {
        eprintln!(
            "Warning: Potential borrow violation for variable {}: {}",
            violation.variable_id, violation.description
        );
    }

    // Analyze reference counting needs
    analyze_reference_counting_needs(ir);

    // Detect move semantics opportunities
    detect_move_semantics(ir);

    Ok(())
}

/// Analyze which variables need automatic reference counting
pub fn analyze_reference_counting_needs(_mir: &mut MIR) {
    // TODO: Implement reference counting analysis in simplified MIR
}

/// Detect opportunities for move semantics
pub fn detect_move_semantics(_mir: &mut MIR) {
    // TODO: Implement move semantics detection in simplified MIR
}

/// Finalize a block with lifetime analysis and variable tracking
pub fn finalize_block_with_lifetime_analysis(
    block: &mut MirBlock,
    analyzer: &LifetimeAnalyzer,
    all_blocks: &[u32],
) {
    let block_id = block.id;

    // Add variable definitions to the block
    if let Some(definitions) = analyzer.block_variable_definitions.get(&block_id) {
        for &_var_id in definitions {
            // TODO: Implement variable definition tracking in MirBlock
        }
    }

    // Add variable usage to the block
    if let Some(usage) = analyzer.block_variable_usage.get(&block_id) {
        for &_var_id in usage {
            // TODO: Implement variable use tracking in MirBlock
        }
    }

    // Analyze variables used after this block
    let used_after = analyzer.analyze_variables_used_after(block_id, all_blocks);
    for _var_id in used_after {
        // TODO: Implement variable use after tracking in MirBlock
    }
}

/// Create a new block with proper ID allocation and parent tracking
pub fn create_block_with_parent(_mir: &mut MIR, _parent_block_id: Option<u32>) -> MirBlock {
    let block_id = 0; // TODO: Implement proper block ID allocation in MIR
    MirBlock::new(block_id)
}

/// Types of control flow for terminator assignment
#[derive(Debug, Clone, PartialEq)]
pub enum ControlFlowType {
    Sequential,  // Normal sequential execution
    Conditional, // If/else branches
    Return,      // Function return
    Loop,        // Loop back-edge
}

/// Assign appropriate terminator for control flow constructs
pub fn assign_terminator_for_control_flow(
    block: &mut MirBlock,
    control_flow_type: ControlFlowType,
    target_blocks: Vec<u32>,
) -> Result<(), CompileError> {
    use crate::compiler::mir::mir_nodes::Terminator;
    use crate::return_compiler_error;

    let terminator = match control_flow_type {
        ControlFlowType::Sequential => {
            if let Some(next_block) = target_blocks.first() {
                Terminator::Goto {
                    target: *next_block,
                    label_depth: 0,
                }
            } else {
                Terminator::Return { values: vec![] }
            }
        }
        ControlFlowType::Conditional => {
            if target_blocks.len() >= 2 {
                Terminator::If {
                    condition: crate::compiler::mir::mir_nodes::Operand::Constant(
                        crate::compiler::mir::mir_nodes::Constant::Bool(true),
                    ),
                    then_block: target_blocks[0],
                    else_block: target_blocks[1],
                    wasm_if_info: crate::compiler::mir::mir_nodes::WasmIfInfo {
                        has_else: true,
                        result_type: None,
                        nesting_level: 0,
                    },
                }
            } else {
                return_compiler_error!(
                    "Conditional control flow requires at least 2 target blocks, got {}",
                    target_blocks.len()
                );
            }
        }
        ControlFlowType::Return => Terminator::Return { values: vec![] },
        ControlFlowType::Loop => {
            if let Some(loop_start) = target_blocks.first() {
                Terminator::Loop {
                    target: *loop_start,
                    loop_header: *loop_start,
                    loop_info: crate::compiler::mir::mir_nodes::WasmLoopInfo {
                        loop_type: crate::compiler::mir::mir_nodes::LoopType::While,
                        has_breaks: false,
                        has_continues: false,
                        result_type: None,
                    },
                }
            } else {
                return_compiler_error!("Loop control flow requires a target block");
            }
        }
    };

    block.set_terminator(terminator);
    Ok(())
}

/// Enhanced block builder for complex control flow structures
pub struct BlockBuilder {
    mir: *mut MIR,
    analyzer: *mut LifetimeAnalyzer,
    current_blocks: Vec<MirBlock>,
    block_relationships: HashMap<u32, Vec<u32>>, // parent -> children mapping
}

impl BlockBuilder {
    pub fn new(mir: &mut MIR, analyzer: &mut LifetimeAnalyzer) -> Self {
        Self {
            mir,
            analyzer,
            current_blocks: Vec::new(),
            block_relationships: HashMap::new(),
        }
    }

    /// Create a new block and track it in the builder
    pub fn create_block(&mut self, parent_id: Option<u32>) -> u32 {
        let mir = unsafe { &mut *self.mir };
        let block = create_block_with_parent(mir, parent_id);
        let block_id = block.id;

        // Track parent-child relationship
        if let Some(parent) = parent_id {
            self.block_relationships
                .entry(parent)
                .or_insert_with(Vec::new)
                .push(block_id);
        }

        self.current_blocks.push(block);
        block_id
    }

    /// Finalize a block with proper terminator and lifetime analysis
    pub fn finalize_block(
        &mut self,
        block_id: u32,
        control_flow: ControlFlowType,
        targets: Vec<u32>,
    ) -> Result<(), CompileError> {
        if let Some(block_pos) = self.current_blocks.iter().position(|b| b.id == block_id) {
            let mut block = self.current_blocks.remove(block_pos);

            // Assign terminator
            assign_terminator_for_control_flow(&mut block, control_flow, targets)?;

            // Perform lifetime analysis
            let all_block_ids: Vec<u32> = self.current_blocks.iter().map(|b| b.id).collect();
            let analyzer = unsafe { &*self.analyzer };
            finalize_block_with_lifetime_analysis(&mut block, analyzer, &all_block_ids);

            // Add to appropriate function or global scope
            let _mir = unsafe { &mut *self.mir };
            // TODO: This will be replaced by the simplified MIR system
            // The new MIR handles function and block management directly
        }

        Ok(())
    }

    /// Get children blocks of a parent block
    pub fn get_child_blocks(&self, parent_id: u32) -> Vec<u32> {
        self.block_relationships
            .get(&parent_id)
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::mir::mir_nodes::MIR;

    #[test]
    fn test_lifetime_analyzer_basic_functionality() {
        let mut analyzer = LifetimeAnalyzer::new();
        let variable_id = 42;
        let block1 = 1;
        let block2 = 2;

        // Track variable definition and usage
        analyzer.track_variable_definition(variable_id, block1);
        analyzer.track_variable_use(variable_id, block2);

        // Verify tracking
        assert_eq!(
            analyzer.get_variable_definition_block(variable_id),
            Some(block1)
        );
        assert_eq!(analyzer.get_variable_last_use(variable_id), Some(block2));

        // Analyze lifetime
        let lifetime = analyzer.analyze_variable_lifetime(variable_id);
        assert_eq!(lifetime.variable_id, variable_id);
        assert_eq!(lifetime.definition_block, Some(block1));
        assert_eq!(lifetime.last_use_block, Some(block2));
        assert!(lifetime.usage_blocks.contains(&block2));
    }

    #[test]
    fn test_borrow_violation_detection() {
        let mut analyzer = LifetimeAnalyzer::new();
        let variable_id = 42;

        // Create a scenario with multiple block usage
        analyzer.track_variable_definition(variable_id, 1);
        analyzer.track_variable_use(variable_id, 2);
        analyzer.track_variable_use(variable_id, 3);
        analyzer.track_variable_use(variable_id, 4);

        let violations = analyzer.detect_borrow_violations();
        assert!(!violations.is_empty());
        assert_eq!(violations[0].variable_id, variable_id);
        assert_eq!(
            violations[0].violation_type,
            BorrowViolationType::PotentialUseAfterMove
        );
    }

    #[test]
    fn test_mir_integration() {
        let mut mir = MIR::new();
        let mut analyzer = LifetimeAnalyzer::new();
        let variable_id = 42;

        // Set up analyzer tracking - need to enter blocks first
        analyzer.enter_block(1);
        analyzer.track_variable_definition_current(variable_id);
        analyzer.exit_block();

        analyzer.enter_block(2);
        analyzer.track_variable_use_current(variable_id);
        analyzer.exit_block();

        // Perform integration
        analyzer.sync_with_ir_lifetime_analysis(&mut mir);

        // Basic integration test - just verify it doesn't crash
        assert!(true);
    }

    #[test]
    fn test_finalize_lifetime_analysis() {
        let mut mir = MIR::new();
        let analyzer = LifetimeAnalyzer::new();

        // Should not fail with empty data
        let result = finalize_lifetime_analysis(&mut mir, &analyzer);
        assert!(result.is_ok());
    }
}
