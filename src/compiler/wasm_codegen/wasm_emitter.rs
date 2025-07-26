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

    pub const HEADER: [u8; 8] = [
        0x00, 0x61, 0x73, 0x6D, // Magic Bytes
        0x01, 0x00, 0x00, 0x00, // Wasm Version 1
    ];

    pub fn finish(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            // estimate sum of all sections and some LEB overhead
            8 // Header
                + self.type_section.len()
                + self.import_section.len()
                + self.function_section.len()
                + self.table_section.len()
                + self.memory_section.len()
                + self.global_section.len()
                + self.export_section.len()
                + self.start_section.len()
                + self.element_section.len()
                + self.code_section.len()
                + self.data_section.len()
                + 11 * 5, // rough LEB128 size for each non-empty section
        );

        // header
        out.extend_from_slice(&Self::HEADER);

        // helper to write a section if non-empty
        fn emit_section(out: &mut Vec<u8>, id: u8, content: &[u8]) {
            if content.is_empty() {
                return;
            }
            out.push(id);
            unsigned(out, content.len() as u64).expect("LEB128 failed");
            out.extend_from_slice(content);
        }

        emit_section(&mut out, 1, &self.type_section);
        emit_section(&mut out, 2, &self.import_section);
        emit_section(&mut out, 3, &self.function_section);
        emit_section(&mut out, 4, &self.table_section);
        emit_section(&mut out, 5, &self.memory_section);
        emit_section(&mut out, 6, &self.global_section);
        emit_section(&mut out, 7, &self.export_section);
        emit_section(&mut out, 8, &self.start_section);
        emit_section(&mut out, 9, &self.element_section);
        emit_section(&mut out, 10, &self.code_section);
        emit_section(&mut out, 11, &self.data_section);

        out
    }
}

// pub fn add_wasm_fn(
//     wasm_module: &mut WasmModule,
//     name: String,
//     args: &[Arg],
//     body: &[AstNode],
//     return_types: &[DataType],
// ) {
// }
