//! Tests for MIR construction and transformation
//! 
//! This module tests MIR data structures and basic operations.

use crate::compiler::mir::mir_nodes::{MIR, MirFunction, Statement, Terminator, Rvalue, Operand, Constant};
use crate::compiler::mir::place::{Place, WasmType};
use std::collections::HashMap;

#[cfg(test)]
mod mir_construction_tests {
    use super::*;

    /// Test basic MIR creation
    #[test]
    fn test_mir_creation() {
        let mir = MIR::new();
        
        assert_eq!(mir.functions.len(), 0, "New MIR should have no functions");
        assert_eq!(mir.globals.len(), 0, "New MIR should have no globals");
        assert_eq!(mir.exports.len(), 0, "New MIR should have no exports");
    }

    /// Test MIR function creation
    #[test]
    fn test_mir_function_creation() {
        let function = MirFunction {
            id: 0,
            name: "test_func".to_string(),
            parameters: vec![],
            return_types: vec![WasmType::I32],
            blocks: vec![],
            locals: HashMap::new(),
            signature: crate::compiler::mir::mir_nodes::FunctionSignature {
                params: vec![],
                returns: vec![WasmType::I32],
            },
            events: HashMap::new(),
        };
        
        assert_eq!(function.id, 0, "Function ID should be set correctly");
        assert_eq!(function.name, "test_func", "Function name should be set correctly");
        assert_eq!(function.blocks.len(), 0, "New function should have no blocks");
    }

    /// Test basic MIR statements
    #[test]
    fn test_mir_statements() {
        let place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let constant = Constant::I32(42);
        
        let assign_stmt = Statement::Assign {
            place: place.clone(),
            rvalue: Rvalue::Use(Operand::Constant(constant)),
        };
        
        match assign_stmt {
            Statement::Assign { place: p, rvalue: _ } => {
                assert_eq!(p, place, "Statement place should match");
            }
            _ => panic!("Expected assign statement"),
        }
    }
}