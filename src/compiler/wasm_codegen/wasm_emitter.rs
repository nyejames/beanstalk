use crate::compiler::compiler_errors::CompileError;
use leb128::write::unsigned;

pub struct WasmModule {
    type_section: Vec<u8>,
    import_section: Vec<u8>,
    function_section: Vec<u8>,
    table_section: Vec<u8>,
    memory_section: Vec<u8>,
    global_section: Vec<u8>,
    export_section: Vec<u8>,
    start_section: Vec<u8>,
    element_section: Vec<u8>,
    code_section: Vec<u8>,
    data_section: Vec<u8>,
}

impl Default for WasmModule {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmModule {
    pub fn new() -> Self {
        Self {
            type_section: Vec::new(),
            import_section: Vec::new(),
            function_section: Vec::new(),
            table_section: Vec::new(),
            memory_section: Vec::new(),
            global_section: Vec::new(),
            export_section: Vec::new(),
            start_section: Vec::new(),
            element_section: Vec::new(),
            code_section: Vec::new(),
            data_section: Vec::new(),
        }
    }

    /// Add an export to the module
    ///
    /// * `name` - The name of the export
    /// * `index` - The index of the item being exported
    /// * `kind` - The kind of export (0 = function, 1 = global, 2 = memory, 3 = table)
    pub fn add_export(&mut self, name: &str, index: usize, kind: u8) {
        // Export name length
        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len();

        // Add name length in LEB128 format
        unsigned(&mut self.export_section, name_len as u64).unwrap();

        // Add name bytes
        self.export_section.extend_from_slice(name_bytes);

        // Add export kind
        self.export_section.push(kind);

        // Add export index in LEB128 format
        unsigned(&mut self.export_section, index as u64).unwrap();
    }

    /// Finalize the module and return the binary
    pub fn finish(self) -> Result<Vec<u8>, CompileError> {
        // Here we would combine all sections into a proper WASM binary
        // This is a simplified version

        let mut result = Vec::new();

        // WASM magic number
        result.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D]);

        // WASM version
        result.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);

        // Add sections with their section IDs
        // 1 = Type, 2 = Import, 3 = Function, 4 = Table, 5 = Memory, 6 = Global,
        // 7 = Export, 8 = Start, 9 = Element, 10 = Code, 11 = Data

        if !self.type_section.is_empty() {
            result.push(1); // Type section ID
            unsigned(&mut result, self.type_section.len() as u64).unwrap();
            result.extend_from_slice(&self.type_section);
        }

        if !self.import_section.is_empty() {
            result.push(2); // Import section ID
            unsigned(&mut result, self.import_section.len() as u64).unwrap();
            result.extend_from_slice(&self.import_section);
        }

        if !self.function_section.is_empty() {
            result.push(3); // Function section ID
            unsigned(&mut result, self.function_section.len() as u64).unwrap();
            result.extend_from_slice(&self.function_section);
        }

        if !self.table_section.is_empty() {
            result.push(4); // Table section ID
            unsigned(&mut result, self.table_section.len() as u64).unwrap();
            result.extend_from_slice(&self.table_section);
        }

        if !self.memory_section.is_empty() {
            result.push(5); // Memory section ID
            unsigned(&mut result, self.memory_section.len() as u64).unwrap();
            result.extend_from_slice(&self.memory_section);
        }

        if !self.global_section.is_empty() {
            result.push(6); // Global section ID
            unsigned(&mut result, self.global_section.len() as u64).unwrap();
            result.extend_from_slice(&self.global_section);
        }

        if !self.export_section.is_empty() {
            result.push(7); // Export section ID
            unsigned(&mut result, self.export_section.len() as u64).unwrap();
            result.extend_from_slice(&self.export_section);
        }

        if !self.start_section.is_empty() {
            result.push(8); // Start section ID
            unsigned(&mut result, self.start_section.len() as u64).unwrap();
            result.extend_from_slice(&self.start_section);
        }

        if !self.element_section.is_empty() {
            result.push(9); // Element section ID
            unsigned(&mut result, self.element_section.len() as u64).unwrap();
            result.extend_from_slice(&self.element_section);
        }

        if !self.code_section.is_empty() {
            result.push(10); // Code section ID
            unsigned(&mut result, self.code_section.len() as u64).unwrap();
            result.extend_from_slice(&self.code_section);
        }

        if !self.data_section.is_empty() {
            result.push(11); // Data section ID
            unsigned(&mut result, self.data_section.len() as u64).unwrap();
            result.extend_from_slice(&self.data_section);
        }

        Ok(result)
    }

    pub const HEADER: [u8; 8] = [
        0x00, 0x61, 0x73, 0x6D, // Magic Bytes
        0x01, 0x00, 0x00, 0x00, // Wasm Version 1
    ];
}

// pub fn add_wasm_fn(
//     wasm_module: &mut WasmModule,
//     name: String,
//     args: &[Arg],
//     body: &[AstNode],
//     return_types: &[DataType],
// ) {
// }
