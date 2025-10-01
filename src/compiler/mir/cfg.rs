use crate::compiler::mir::mir_nodes::{MirFunction, ProgramPoint, Terminator};
use std::collections::HashMap;

/// Simplified Control Flow Graph for MIR functions
/// 
/// This is a basic CFG implementation focused on correctness over optimization.
/// Complex CFG features can be added later if needed.
#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    /// Successors for each program point
    successors: HashMap<ProgramPoint, Vec<ProgramPoint>>,
    /// Predecessors for each program point
    predecessors: HashMap<ProgramPoint, Vec<ProgramPoint>>,
    /// Total number of program points
    program_point_count: usize,
    /// Whether this CFG represents linear control flow
    is_linear: bool,
}

impl ControlFlowGraph {
    /// Create a new empty CFG
    pub fn new(program_point_count: usize) -> Self {
        Self {
            successors: HashMap::new(),
            predecessors: HashMap::new(),
            program_point_count,
            is_linear: true,
        }
    }

    /// Build CFG from a MIR function
    pub fn build_from_function(function: &MirFunction) -> Result<Self, String> {
        let program_points = function.get_program_points_in_order();
        let program_point_count = program_points.len();
        
        let mut cfg = Self::new(program_point_count);
        
        // For now, build a simple linear CFG
        cfg.build_linear_cfg(&program_points)?;
        
        Ok(cfg)
    }

    /// Build linear CFG (simple sequential flow)
    fn build_linear_cfg(&mut self, program_points: &[ProgramPoint]) -> Result<(), String> {
        for (i, &current_point) in program_points.iter().enumerate() {
            let mut successors = Vec::new();
            let mut predecessors = Vec::new();
            
            // Add predecessor
            if i > 0 {
                predecessors.push(program_points[i - 1]);
            }
            
            // Add successor
            if i < program_points.len() - 1 {
                successors.push(program_points[i + 1]);
            }
            
            self.successors.insert(current_point, successors);
            self.predecessors.insert(current_point, predecessors);
        }
        
        Ok(())
    }

    /// Check if a function has linear control flow
    fn is_function_linear(&self, function: &MirFunction) -> bool {
        // Check if any block has non-linear terminators
        for block in &function.blocks {
            match &block.terminator {
                Terminator::Goto { .. } => return false, // Jump indicates non-linear
                Terminator::If { .. } => return false,   // Conditional branch indicates non-linear
                Terminator::Return { .. } | Terminator::Unreachable => continue, // These are fine
            }
        }
        
        // If we have more than one block, it's not linear
        function.blocks.len() <= 1
    }

    /// Get successors for a program point
    pub fn get_successors(&self, point: &ProgramPoint) -> &[ProgramPoint] {
        self.successors.get(point).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get predecessors for a program point
    pub fn get_predecessors(&self, point: &ProgramPoint) -> &[ProgramPoint] {
        self.predecessors.get(point).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Iterate over all program points
    pub fn iter_program_points(&self) -> impl Iterator<Item = ProgramPoint> + '_ {
        self.successors.keys().copied()
    }

    /// Check if CFG is linear
    pub fn is_linear(&self) -> bool {
        self.is_linear
    }
}