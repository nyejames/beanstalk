use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::wir::wir_nodes::Terminator;
use crate::compiler::wir::place::{Place, WasmType};
use crate::compiler::wir::wir_nodes::{Operand, Constant};
use wasm_encoder::Function;
use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test lowering of Terminator::Goto
    #[test]
    fn test_lower_goto_terminator() {
        let mut wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let terminator = Terminator::Goto { target: 1 };
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "Goto terminator lowering should succeed");
    }

    /// Test lowering of Terminator::If
    #[test]
    fn test_lower_if_terminator() {
        let mut wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let condition = Operand::Constant(Constant::I32(1));
        
        let terminator = Terminator::If {
            condition,
            then_block: 1,
            else_block: 2,
        };
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "If terminator lowering should succeed");
    }

    /// Test lowering of Terminator::Return
    #[test]
    fn test_lower_return_terminator() {
        let mut wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let terminator = Terminator::Return {
            values: vec![Operand::Constant(Constant::I32(42))],
        };
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "Return terminator lowering should succeed");
    }

    /// Test lowering of Terminator::Unreachable
    #[test]
    fn test_lower_unreachable_terminator() {
        let mut wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let terminator = Terminator::Unreachable;
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "Unreachable terminator lowering should succeed");
    }
}