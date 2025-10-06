//! Tests for WIR construction and transformation
//! 
//! This module tests WIR data structures and basic operations.

use crate::compiler::wir::wir_nodes::{WIR, WirFunction, Statement, Terminator, Rvalue, Operand, Constant};
use crate::compiler::wir::place::{Place, WasmType};
use std::collections::HashMap;

#[cfg(test)]
mod wir_construction_tests {
    use super::*;

    /// Test basic WIR creation
    #[test]
    fn test_wir_creation() {
        let wir = WIR::new();
        
        assert_eq!(wir.functions.len(), 0, "New WIR should have no functions");
        assert_eq!(wir.globals.len(), 0, "New WIR should have no globals");
        assert_eq!(wir.exports.len(), 0, "New WIR should have no exports");
    }

    /// Test WIR function creation
    #[test]
    fn test_wir_function_creation() {
        let function = WirFunction {
            id: 0,
            name: "test_func".to_string(),
            parameters: vec![],
            return_types: vec![WasmType::I32],
            blocks: vec![],
            locals: HashMap::new(),
            signature: crate::compiler::wir::wir_nodes::FunctionSignature {
                params: vec![],
                returns: vec![WasmType::I32],
            },
            events: HashMap::new(),
        };
        
        assert_eq!(function.id, 0, "Function ID should be set correctly");
        assert_eq!(function.name, "test_func", "Function name should be set correctly");
        assert_eq!(function.blocks.len(), 0, "New function should have no blocks");
    }

    /// Test basic WIR statements
    #[test]
    fn test_wir_statements() {
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