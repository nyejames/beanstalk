use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::mir::mir_nodes::{Terminator, WasmIfInfo, BrTableInfo, WasmLoopInfo, LoopType};
use crate::compiler::mir::place::{Place, WasmType};
use crate::compiler::mir::mir_nodes::{Operand, Constant};
use wasm_encoder::Function;
use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test lowering of Terminator::Goto
    #[test]
    fn test_lower_goto_terminator() {
        let wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let terminator = Terminator::Goto {
            target: 1,
            label_depth: 0,
        };
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "Goto terminator lowering should succeed");
    }

    /// Test lowering of Terminator::If
    #[test]
    fn test_lower_if_terminator() {
        let wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let condition = Operand::Constant(Constant::I32(1));
        let wasm_if_info = WasmIfInfo {
            has_else: true,
            result_type: None,
            nesting_level: 0,
        };
        
        let terminator = Terminator::If {
            condition,
            then_block: 1,
            else_block: 2,
            wasm_if_info,
        };
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "If terminator lowering should succeed");
    }

    /// Test lowering of Terminator::Return
    #[test]
    fn test_lower_return_terminator() {
        let wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let return_value = Operand::Constant(Constant::I32(42));
        let terminator = Terminator::Return {
            values: vec![return_value],
        };
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "Return terminator lowering should succeed");
    }

    /// Test lowering of Terminator::Unreachable
    #[test]
    fn test_lower_unreachable_terminator() {
        let wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let terminator = Terminator::Unreachable;
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "Unreachable terminator lowering should succeed");
    }

    /// Test lowering of Terminator::Switch
    #[test]
    fn test_lower_switch_terminator() {
        let wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let discriminant = Operand::Constant(Constant::I32(1));
        let br_table_info = BrTableInfo {
            target_count: 3,
            is_dense: true,
            min_target: 0,
            max_target: 2,
        };
        
        let terminator = Terminator::Switch {
            discriminant,
            targets: vec![0, 1, 2],
            default: 3,
            br_table_info,
        };
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "Switch terminator lowering should succeed");
    }

    /// Test lowering of legacy Terminator::UnconditionalJump
    #[test]
    fn test_lower_unconditional_jump_terminator() {
        let wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let terminator = Terminator::UnconditionalJump(1);
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "UnconditionalJump terminator lowering should succeed");
    }

    /// Test lowering of legacy Terminator::Returns
    #[test]
    fn test_lower_returns_terminator() {
        let wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let terminator = Terminator::Returns;
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "Returns terminator lowering should succeed");
    }

    /// Test lowering of Terminator::Loop
    #[test]
    fn test_lower_loop_terminator() {
        let wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let loop_info = WasmLoopInfo {
            loop_type: LoopType::While,
            has_breaks: false,
            has_continues: true,
            result_type: None,
        };
        
        let terminator = Terminator::Loop {
            target: 0,
            loop_header: 0,
            loop_info,
        };
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "Loop terminator lowering should succeed");
    }

    /// Test lowering of Terminator::Block
    #[test]
    fn test_lower_block_terminator() {
        let wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let terminator = Terminator::Block {
            inner_blocks: vec![1, 2],
            result_type: Some(WasmType::I32),
            exit_target: 3,
        };
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_ok(), "Block terminator lowering should succeed");
    }

    /// Test error case for ConditionalJump (should return error)
    #[test]
    fn test_conditional_jump_error() {
        let wasm_module = WasmModule::new();
        let mut function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        let terminator = Terminator::ConditionalJump(1, 2);
        
        let result = wasm_module.lower_terminator(&terminator, &mut function, &local_map);
        assert!(result.is_err(), "ConditionalJump should return an error");
    }

    /// Test BlockLabelManager functionality
    #[test]
    fn test_block_label_manager() {
        let wasm_module = WasmModule::new();
        let mut label_manager = wasm_module.create_block_label_manager();
        
        // Test initial state
        assert_eq!(label_manager.get_current_depth(), 0);
        
        // Test entering control frames
        use crate::compiler::codegen::wasm_encoding::ControlFrameType;
        let depth1 = label_manager.enter_control_frame(ControlFrameType::Block, Some(1));
        assert_eq!(depth1, 0);
        assert_eq!(label_manager.get_current_depth(), 1);
        
        let depth2 = label_manager.enter_control_frame(ControlFrameType::If, Some(2));
        assert_eq!(depth2, 1);
        assert_eq!(label_manager.get_current_depth(), 2);
        
        // Test label depth lookup
        assert_eq!(label_manager.get_label_depth(1), Some(0));
        assert_eq!(label_manager.get_label_depth(2), Some(1));
        assert_eq!(label_manager.get_label_depth(999), None);
        
        // Test branch depth calculation
        assert_eq!(label_manager.calculate_branch_depth(1), Some(2));
        assert_eq!(label_manager.calculate_branch_depth(2), Some(1));
        
        // Test exiting control frames
        let frame2 = label_manager.exit_control_frame();
        assert!(frame2.is_some());
        assert_eq!(label_manager.get_current_depth(), 1);
        
        let frame1 = label_manager.exit_control_frame();
        assert!(frame1.is_some());
        assert_eq!(label_manager.get_current_depth(), 0);
        
        let no_frame = label_manager.exit_control_frame();
        assert!(no_frame.is_none());
    }
}