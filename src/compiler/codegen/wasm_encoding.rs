use wasm_encoder::*;

pub struct WasmModule {
    // Function acting as Global scope of Beanstalk module
    // Runs automatically when the module is loaded and can't have any args or returns
    start_section: Option<StartSection>,
    type_section: TypeSection,
    import_section: ImportSection,
    function_signature_section: FunctionSection,
    table_section: TableSection,
    memory_section: MemorySection,
    global_section: GlobalSection,
    export_section: ExportSection,
    element_section: ElementSection,
    code_section: CodeSection,
    data_section: DataSection,

    // Internal state for tracking
    pub function_count: u32,
    pub type_count: u32,
    global_count: u32,
    local_count: u32,
    string_constants: Vec<String>,
    string_constant_map: std::collections::HashMap<String, u32>,
}

impl Default for WasmModule {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmModule {
    pub fn new() -> Self {
        let start_section = StartSection { function_index: 0 };

        Self {
            start_section: Option::from(start_section),
            type_section: TypeSection::new(),
            import_section: ImportSection::new(),
            function_signature_section: FunctionSection::new(),
            table_section: TableSection::new(),
            memory_section: MemorySection::new(),
            global_section: GlobalSection::new(),
            export_section: ExportSection::new(),
            element_section: ElementSection::new(),
            code_section: CodeSection::new(),
            data_section: DataSection::new(),
            function_count: 0,
            type_count: 0,
            global_count: 0,
            local_count: 0,
            string_constants: Vec::new(),
            string_constant_map: std::collections::HashMap::new(),
        }
    }
    pub fn finish(self) -> Vec<u8> {
        let mut module = Module::new();

        // Encode each section in the correct order
        module
            .section(&self.type_section)
            .section(&self.import_section)
            .section(&self.function_signature_section)
            .section(&self.table_section)
            .section(&self.memory_section)
            .section(&self.global_section)
            .section(&self.export_section);

        if let Some(start_section) = self.start_section {
            module.section(&start_section);
        }

        module
            .section(&self.element_section)
            .section(&self.code_section)
            .section(&self.data_section);

        module.finish()
    }

}