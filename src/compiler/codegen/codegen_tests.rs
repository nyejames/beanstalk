use crate::compiler::codegen::wasm_codegen::WasmModule;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::tokens::TextLocation;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_module_creation() {
        let wasm_module = WasmModule::new();
        assert_eq!(wasm_module.function_count, 0);
        assert_eq!(wasm_module.type_count, 0);
    }

    #[test]
    fn test_simple_function_generation() {
        let result = WasmModule::generate_simple_function();
        if let Err(e) = &result {
            println!("Error: {e:?}");
        }
        assert!(result.is_ok());

        let wasm_bytes = result.unwrap();
        // Check that we have the WASM magic number
        assert_eq!(&wasm_bytes[0..4], &[0x00, 0x61, 0x73, 0x6D]);
        // Check that we have the WASM version
        assert_eq!(&wasm_bytes[4..8], &[0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_int_literal_lowering() {
        let mut wasm_module = WasmModule::new();
        let expression = Expression::int(42, TextLocation::default(), 1);

        let result = wasm_module.lower_expression(&expression);
        assert!(result.is_ok());

        // Check that we have the i32.const opcode
        assert_eq!(wasm_module.code_section[0], WasmModule::OP_I32_CONST);
    }

    #[test]
    fn test_bool_literal_lowering() {
        let mut wasm_module = WasmModule::new();
        let expression = Expression::bool(true, TextLocation::default(), 1);

        let result = wasm_module.lower_expression(&expression);
        assert!(result.is_ok());

        // Check that we have the i32.const opcode
        assert_eq!(wasm_module.code_section[0], WasmModule::OP_I32_CONST);
    }
}
