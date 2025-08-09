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
    // 
    // /// Entry: compile a FunctionAst and add it to the module.
    // fn compile_function(&mut self, f: &FunctionAst) {
    //     // 1) Type section: function signature
    //     let mut types = TypeSection::new();
    //     let params: Vec<ValType> = f.params.iter().map(|(_, t)| *t).collect();
    //     let results: Vec<ValType> = f.ret.iter().cloned().collect();
    //     let type_idx = types.function(params.clone(), results.clone());
    //     self.module.section(&types);
    // 
    //     // 2) Function section: declare one function with that type
    //     let mut functions = FunctionSection::new();
    //     functions.function(type_idx);
    //     self.module.section(&functions);
    // 
    //     // 3) Prepare locals: params are locals 0..n-1.
    //     // We'll scan the body for let-bound locals for this small example.
    //     // In real compiler, run a pass to collect all local names and their types.
    //     let mut local_index = f.params.len() as u32;
    //     let mut locals_map: HashMap<String, u32> = HashMap::new();
    // 
    //     // Simple collector for 'let' nodes to create locals (demo only).
    //     fn collect_lets(expr: &Expr, out: &mut Vec<String>) {
    //         match expr {
    //             Expr::Let { name, value: _, body } => {
    //                 out.push(name.clone());
    //                 collect_lets(body, out);
    //             }
    //             Expr::If { cond, then_branch, else_branch } => {
    //                 collect_lets(cond, out);
    //                 collect_lets(then_branch, out);
    //                 collect_lets(else_branch, out);
    //             }
    //             Expr::Add(a, b) | Expr::Mul(a, b) => {
    //                 collect_lets(a, out);
    //                 collect_lets(b, out);
    //             }
    //             Expr::Call { args, .. } => {
    //                 for a in args { collect_lets(a, out); }
    //             }
    //             _ => {}
    //         }
    //     }
    // 
    //     let mut lets = Vec::new();
    //     collect_lets(&f.body, &mut lets);
    //     // assign indices to those lets
    //     for name in lets {
    //         locals_map.insert(name, local_index);
    //         local_index += 1;
    //     }
    // 
    //     // Because wasm_encoder requires locals declared up-front as (count, ValType),
    //     // make a locals vector: we'll assume every let is i32 in this demo.
    //     let num_locals = (local_index as usize).saturating_sub(f.params.len());
    //     let mut locals = vec![];
    //     if num_locals > 0 {
    //         locals.push((num_locals as u32, ValType::I32));
    //     }
    // 
    //     // 4) Build code body
    //     let mut code_section = CodeSection::new();
    //     let mut func = Function::new(locals);
    // 
    //     // Map parameter names to indices:
    //     for (i, (name, _)) in f.params.iter().enumerate() {
    //         locals_map.insert(name.clone(), i as u32);
    //     }
    // 
    //     // Emit the body expression into the function using stack machine
    //     self.emit_expr(&f.body, &mut func, &mut locals_map);
    // 
    //     // If function has a return, the expression should leave the value on the stack.
    //     func.instruction(&Instruction::End);
    //     code_section.function(&func);
    //     self.module.section(&code_section);
    // 
    //     // 5) Export the function for testing
    //     let mut exports = ExportSection::new();
    //     exports.export(&f.name, wasm_encoder::Export::Func(0));
    //     self.module.section(&exports);
    // }
    // 
    // /// Emit instructions that evaluate `expr` and leave its i32 result on the stack.
    // /// For simplicity this demo assumes values are i32.
    // fn emit_expr(&self, expr: &Expr, func: &mut Function, locals_map: &mut HashMap<String, u32>) {
    //     match expr {
    //         Expr::ConstI32(v) => {
    //             func.instruction(&Instruction::I32Const(*v));
    //         }
    //         Expr::Var(name) => {
    //             let idx = locals_map.get(name).expect("unknown variable");
    //             func.instruction(&Instruction::LocalGet(*idx));
    //         }
    //         Expr::Add(a, b) => {
    //             self.emit_expr(a, func, locals_map);
    //             self.emit_expr(b, func, locals_map);
    //             func.instruction(&Instruction::I32Add);
    //         }
    //         Expr::Mul(a, b) => {
    //             self.emit_expr(a, func, locals_map);
    //             self.emit_expr(b, func, locals_map);
    //             func.instruction(&Instruction::I32Mul);
    //         }
    //         Expr::Let { name, value, body } => {
    //             // allocate local must already exist in locals_map
    //             let idx = *locals_map.get(name).expect("let local missing");
    //             self.emit_expr(value, func, locals_map);
    //             func.instruction(&Instruction::LocalSet(idx));
    //             self.emit_expr(body, func, locals_map);
    //         }
    //         Expr::If { cond, then_branch, else_branch } => {
    //             // Condition: produce i32 (0 or non-zero).
    //             self.emit_expr(cond, func, locals_map);
    // 
    //             // Wasm If as expression returning i32:
    //             func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    //             // then:
    //             self.emit_expr(then_branch, func, locals_map);
    //             func.instruction(&Instruction::Else);
    //             // else:
    //             self.emit_expr(else_branch, func, locals_map);
    //             func.instruction(&Instruction::End);
    //         }
    //         Expr::Call { name: _, args } => {
    //             // For demo assume call index 0 (only one function exported); in a real compiler
    //             // you'd maintain a symbol/function->index map.
    //             for arg in args {
    //                 self.emit_expr(arg, func, locals_map);
    //             }
    //             func.instruction(&Instruction::Call(0)); // index 0 in this tiny example
    //         }
    //     }
    // }
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