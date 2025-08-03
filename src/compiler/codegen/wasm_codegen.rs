use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind, Arg};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::tokens::TextLocation;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use leb128::write::{unsigned, signed};

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
    pub code_section: Vec<u8>,
    data_section: Vec<u8>,
    
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
            function_count: 0,
            type_count: 0,
            global_count: 0,
            local_count: 0,
            string_constants: Vec::new(),
            string_constant_map: std::collections::HashMap::new(),
        }
    }

    pub const HEADER: [u8; 8] = [
        0x00, 0x61, 0x73, 0x6D, // Magic Bytes
        0x01, 0x00, 0x00, 0x00, // Wasm Version 1
    ];

    // WASM opcodes
    pub const OP_I32_CONST: u8 = 0x41;
    const OP_I64_CONST: u8 = 0x42;
    const OP_F32_CONST: u8 = 0x43;
    const OP_F64_CONST: u8 = 0x44;
    const OP_I32_ADD: u8 = 0x6A;
    const OP_I32_SUB: u8 = 0x6B;
    const OP_I32_MUL: u8 = 0x6C;
    const OP_I32_DIV_S: u8 = 0x6D;
    const OP_I32_REM_S: u8 = 0x6F;
    const OP_I32_EQ: u8 = 0x46;
    const OP_I32_NE: u8 = 0x47;
    const OP_I32_LT_S: u8 = 0x48;
    const OP_I32_GT_S: u8 = 0x4A;
    const OP_I32_LE_S: u8 = 0x4C;
    const OP_I32_GE_S: u8 = 0x4E;
    const OP_LOCAL_GET: u8 = 0x20;
    const OP_LOCAL_SET: u8 = 0x21;
    const OP_LOCAL_TEE: u8 = 0x22;
    const OP_GLOBAL_GET: u8 = 0x23;
    const OP_GLOBAL_SET: u8 = 0x24;
    const OP_CALL: u8 = 0x10;
    const OP_RETURN: u8 = 0x0F;
    const OP_END: u8 = 0x0B;
    const OP_BLOCK: u8 = 0x02;
    const OP_LOOP: u8 = 0x03;
    const OP_IF: u8 = 0x04;
    const OP_ELSE: u8 = 0x05;
    const OP_BR: u8 = 0x0C;
    const OP_BR_IF: u8 = 0x0D;
    const OP_DROP: u8 = 0x1A;
    const OP_SELECT: u8 = 0x1B;

    // WASM value types
    const VALTYPE_I32: u8 = 0x7F;
    const VALTYPE_I64: u8 = 0x7E;
    const VALTYPE_F32: u8 = 0x7D;
    const VALTYPE_F64: u8 = 0x7C;

    /// Lower an AST block into WASM
    pub fn lower_ast_block(&mut self, ast_block: &AstBlock) -> Result<(), CompileError> {
        for node in &ast_block.ast {
            self.lower_ast_node(node)?;
        }
        Ok(())
    }

    /// Lower a single AST node into WASM
    pub fn lower_ast_node(&mut self, node: &AstNode) -> Result<(), CompileError> {
        match &node.kind {
            NodeKind::Declaration(name, expression, _) => {
                self.lower_declaration(name, expression)?;
            }
            NodeKind::FunctionCall(name, args, return_types, _) => {
                self.lower_function_call(name, args, return_types)?;
            }
            NodeKind::Return(expressions) => {
                self.lower_return(expressions)?;
            }
            NodeKind::If(condition, if_block) => {
                self.lower_if_statement(condition, if_block)?;
            }
            NodeKind::Else(else_block) => {
                self.lower_else_block(else_block)?;
            }
            NodeKind::ForLoop(item, collection, body) => {
                self.lower_for_loop(item, collection, body)?;
            }
            NodeKind::WhileLoop(condition, body) => {
                self.lower_while_loop(condition, body)?;
            }
            NodeKind::Print(expression) => {
                self.lower_print(expression)?;
            }
            NodeKind::Expression(expression) => {
                self.lower_expression(expression)?;
            }
            NodeKind::Reference(expression) => {
                self.lower_expression(expression)?;
            }
            NodeKind::Operator(operator) => {
                // Operators are handled in the runtime expression context
                // This is a simplified implementation - in a real scenario,
                // we'd need to handle the operator with its operands
                match operator {
                    Operator::Add => {
                        self.code_section.push(Self::OP_I32_ADD);
                    }
                    Operator::Subtract => {
                        self.code_section.push(Self::OP_I32_SUB);
                    }
                    Operator::Multiply => {
                        self.code_section.push(Self::OP_I32_MUL);
                    }
                    Operator::Divide => {
                        self.code_section.push(Self::OP_I32_DIV_S);
                    }
                    _ => {
                        return Err(CompileError {
                            msg: format!("Unsupported operator: {:?}", operator),
                            location: node.location.clone(),
                            error_type: ErrorType::Compiler,
                            file_path: std::path::PathBuf::new(),
                        });
                    }
                }
            }
            NodeKind::Comment(_) => {
                // Comments are ignored in WASM generation
            }
            NodeKind::Empty => {
                // Empty nodes are ignored
            }
            _ => {
                return Err(CompileError {
                    msg: format!("Unsupported AST node kind: {:?}", node.kind),
                    location: node.location.clone(),
                    error_type: ErrorType::Compiler,
                    file_path: std::path::PathBuf::new(),
                });
            }
        }
        Ok(())
    }

    /// Lower a variable declaration
    fn lower_declaration(&mut self, name: &str, expression: &Expression) -> Result<(), CompileError> {
        // First, evaluate the expression
        self.lower_expression(expression)?;
        
        // Store the result in a local variable
        let local_index = self.allocate_local(name);
        self.code_section.push(Self::OP_LOCAL_SET);
        unsigned(&mut self.code_section, local_index as u64).unwrap();
        
        Ok(())
    }

    /// Lower a function call
    fn lower_function_call(&mut self, name: &str, args: &[Expression], return_types: &[DataType]) -> Result<(), CompileError> {
        // Evaluate all arguments
        for arg in args {
            self.lower_expression(arg)?;
        }
        
        // Call the function
        self.code_section.push(Self::OP_CALL);
        // TODO: Look up function index by name
        let function_index = 0; // Placeholder
        unsigned(&mut self.code_section, function_index as u64).unwrap();
        
        Ok(())
    }

    /// Lower a return statement
    fn lower_return(&mut self, expressions: &[Expression]) -> Result<(), CompileError> {
        // Evaluate all return expressions
        for expr in expressions {
            self.lower_expression(expr)?;
        }
        
        self.code_section.push(Self::OP_RETURN);
        Ok(())
    }

    /// Lower an if statement
    fn lower_if_statement(&mut self, condition: &Expression, if_block: &AstBlock) -> Result<(), CompileError> {
        // Evaluate condition
        self.lower_expression(condition)?;
        
        // Add if opcode
        self.code_section.push(Self::OP_IF);
        self.code_section.push(Self::VALTYPE_I32); // Block type
        
        // Lower the if block
        self.lower_ast_block(if_block)?;
        
        self.code_section.push(Self::OP_END);
        Ok(())
    }

    /// Lower an else block
    fn lower_else_block(&mut self, else_block: &AstBlock) -> Result<(), CompileError> {
        self.code_section.push(Self::OP_ELSE);
        self.lower_ast_block(else_block)?;
        self.code_section.push(Self::OP_END);
        Ok(())
    }

    /// Lower a for loop
    fn lower_for_loop(&mut self, item: &Arg, collection: &Expression, body: &AstBlock) -> Result<(), CompileError> {
        // TODO: Implement for loop lowering
        // This is complex and depends on the collection type
        Err(CompileError {
            msg: "For loops not yet implemented in WASM codegen".to_string(),
            location: item.value.location.clone(),
            error_type: ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
        })
    }

    /// Lower a while loop
    fn lower_while_loop(&mut self, condition: &Expression, body: &AstBlock) -> Result<(), CompileError> {
        // Add loop opcode
        self.code_section.push(Self::OP_LOOP);
        self.code_section.push(Self::VALTYPE_I32); // Block type
        
        // Evaluate condition
        self.lower_expression(condition)?;
        
        // Branch if condition is false
        self.code_section.push(Self::OP_BR_IF);
        unsigned(&mut self.code_section, 1).unwrap(); // Branch to end of loop
        
        // Lower the loop body
        self.lower_ast_block(body)?;
        
        // Branch back to start of loop
        self.code_section.push(Self::OP_BR);
        unsigned(&mut self.code_section, 0).unwrap();
        
        self.code_section.push(Self::OP_END);
        Ok(())
    }

    /// Lower a print statement
    fn lower_print(&mut self, expression: &Expression) -> Result<(), CompileError> {
        // Evaluate the expression to print
        self.lower_expression(expression)?;
        
        // TODO: Call a print function (would need to be imported or defined)
        // For now, just drop the value
        self.code_section.push(Self::OP_DROP);
        
        Ok(())
    }

    /// Lower an expression
    pub fn lower_expression(&mut self, expression: &Expression) -> Result<(), CompileError> {
        match &expression.kind {
            ExpressionKind::Int(value) => {
                self.lower_int_literal(*value)?;
            }
            ExpressionKind::Float(value) => {
                self.lower_float_literal(*value)?;
            }
            ExpressionKind::String(value) => {
                self.lower_string_literal(value)?;
            }
            ExpressionKind::Bool(value) => {
                self.lower_bool_literal(*value)?;
            }
            ExpressionKind::Function(args, body, return_types) => {
                self.lower_function_expression(args, body, return_types)?;
            }
            ExpressionKind::Runtime(nodes) => {
                self.lower_runtime_expression(nodes)?;
            }
            ExpressionKind::Collection(items) => {
                self.lower_collection_expression(items)?;
            }
            ExpressionKind::Struct(args) => {
                self.lower_struct_expression(args)?;
            }
            ExpressionKind::Template(_, _, _) => {
                // Templates are handled separately in the template system
                return Err(CompileError {
                    msg: "Templates not supported in WASM expressions".to_string(),
                    location: expression.location.clone(),
                    error_type: ErrorType::Compiler,
                    file_path: std::path::PathBuf::new(),
                });
            }
            ExpressionKind::None => {
                // Push a default value based on the expected type
                self.lower_none_expression(&expression.data_type)?;
            }
        }
        Ok(())
    }

    /// Lower a binary operator expression
    fn lower_binary_operator(&mut self, lhs: &Expression, rhs: &Expression, operator: &Operator) -> Result<(), CompileError> {
        // Evaluate left operand
        self.lower_expression(lhs)?;
        
        // Evaluate right operand
        self.lower_expression(rhs)?;
        
        // Apply operator
        match operator {
            Operator::Add => {
                self.code_section.push(Self::OP_I32_ADD);
            }
            Operator::Subtract => {
                self.code_section.push(Self::OP_I32_SUB);
            }
            Operator::Multiply => {
                self.code_section.push(Self::OP_I32_MUL);
            }
            Operator::Divide => {
                self.code_section.push(Self::OP_I32_DIV_S);
            }
            Operator::Modulus => {
                self.code_section.push(Self::OP_I32_REM_S);
            }
            Operator::Equality => {
                self.code_section.push(Self::OP_I32_EQ);
            }
            Operator::NotEqual => {
                self.code_section.push(Self::OP_I32_NE);
            }
            Operator::GreaterThan => {
                self.code_section.push(Self::OP_I32_GT_S);
            }
            Operator::GreaterThanOrEqual => {
                self.code_section.push(Self::OP_I32_GE_S);
            }
            Operator::LessThan => {
                self.code_section.push(Self::OP_I32_LT_S);
            }
            Operator::LessThanOrEqual => {
                self.code_section.push(Self::OP_I32_LE_S);
            }
            Operator::And => {
                // Logical AND: both operands must be non-zero
                // This is a simplified implementation
                self.code_section.push(Self::OP_I32_ADD);
                self.code_section.push(Self::OP_I32_CONST);
                unsigned(&mut self.code_section, 2).unwrap();
                self.code_section.push(Self::OP_I32_EQ);
            }
            Operator::Or => {
                // Logical OR: at least one operand must be non-zero
                // This is a simplified implementation
                self.code_section.push(Self::OP_I32_ADD);
                self.code_section.push(Self::OP_I32_CONST);
                unsigned(&mut self.code_section, 0).unwrap();
                self.code_section.push(Self::OP_I32_GT_S);
            }
            _ => {
                return Err(CompileError {
                    msg: format!("Unsupported operator: {:?}", operator),
                    location: lhs.location.clone(),
                    error_type: ErrorType::Compiler,
                    file_path: std::path::PathBuf::new(),
                });
            }
        }
        
        Ok(())
    }

    /// Lower an integer literal
    fn lower_int_literal(&mut self, value: i32) -> Result<(), CompileError> {
        self.code_section.push(Self::OP_I32_CONST);
        signed(&mut self.code_section, value as i64).unwrap();
        Ok(())
    }

    /// Lower a float literal
    fn lower_float_literal(&mut self, value: f64) -> Result<(), CompileError> {
        self.code_section.push(Self::OP_F64_CONST);
        // Convert f64 to bytes
        let bytes = value.to_le_bytes();
        self.code_section.extend_from_slice(&bytes);
        Ok(())
    }

    /// Lower a string literal
    fn lower_string_literal(&mut self, value: &str) -> Result<(), CompileError> {
        // Store string in data section and return its address
        let string_index = self.add_string_constant(value);
        
        // TODO: Load string address onto stack
        // This would require memory operations and string handling
        // For now, just push a placeholder
        self.code_section.push(Self::OP_I32_CONST);
        unsigned(&mut self.code_section, string_index as u64).unwrap();
        
        Ok(())
    }

    /// Lower a boolean literal
    fn lower_bool_literal(&mut self, value: bool) -> Result<(), CompileError> {
        self.code_section.push(Self::OP_I32_CONST);
        unsigned(&mut self.code_section, if value { 1 } else { 0 }).unwrap();
        Ok(())
    }

    /// Lower a function expression
    fn lower_function_expression(&mut self, args: &[Arg], body: &[AstNode], return_types: &[DataType]) -> Result<(), CompileError> {
        // Add function type to type section
        let type_index = self.add_function_type(args, return_types);
        
        // Add function to function section
        self.function_section.push(type_index as u8);
        self.function_count += 1;
        
        // Generate function body
        let mut function_body = Vec::new();
        
        // Add locals count
        unsigned(&mut function_body, 0).unwrap(); // No additional locals for now
        
        // Lower function body
        for node in body {
            self.lower_ast_node(node)?;
        }
        
        // Add end opcode
        function_body.push(Self::OP_END);
        
        // Add function body to code section
        unsigned(&mut self.code_section, function_body.len() as u64).unwrap();
        self.code_section.extend_from_slice(&function_body);
        
        Ok(())
    }

    /// Lower a runtime expression
    fn lower_runtime_expression(&mut self, nodes: &[AstNode]) -> Result<(), CompileError> {
        for node in nodes {
            self.lower_ast_node(node)?;
        }
        Ok(())
    }

    /// Lower a collection expression
    fn lower_collection_expression(&mut self, items: &[Expression]) -> Result<(), CompileError> {
        // TODO: Implement collection lowering
        // This depends on the collection type and how it's stored in memory
        Err(CompileError {
            msg: "Collections not yet implemented in WASM codegen".to_string(),
            location: items.first().map(|e| e.location.clone()).unwrap_or_default(),
            error_type: ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
        })
    }

    /// Lower a struct expression
    fn lower_struct_expression(&mut self, args: &[Arg]) -> Result<(), CompileError> {
        // TODO: Implement struct lowering
        // This depends on struct layout and memory representation
        Err(CompileError {
            msg: "Structs not yet implemented in WASM codegen".to_string(),
            location: args.first().map(|a| a.value.location.clone()).unwrap_or_default(),
            error_type: ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
        })
    }

    /// Lower a none expression
    fn lower_none_expression(&mut self, data_type: &DataType) -> Result<(), CompileError> {
        match data_type {
            DataType::Int(_) | DataType::Bool(_) => {
                self.code_section.push(Self::OP_I32_CONST);
                unsigned(&mut self.code_section, 0).unwrap();
            }
            DataType::Float(_) => {
                self.code_section.push(Self::OP_F64_CONST);
                let bytes = 0.0_f64.to_le_bytes();
                self.code_section.extend_from_slice(&bytes);
            }
            _ => {
                return Err(CompileError {
                    msg: format!("Cannot create default value for type: {:?}", data_type),
                    location: Default::default(),
                    error_type: ErrorType::Compiler,
                    file_path: std::path::PathBuf::new(),
                });
            }
        }
        Ok(())
    }

    /// Add a function type to the type section
    fn add_function_type(&mut self, args: &[Arg], return_types: &[DataType]) -> u32 {
        let type_index = self.type_count;
        
        // Function type indicator
        self.type_section.push(0x60);
        
        // Parameter types
        unsigned(&mut self.type_section, args.len() as u64).unwrap();
        for arg in args {
            let valtype = self.data_type_to_valtype(&arg.value.data_type);
            self.type_section.push(valtype);
        }
        
        // Return types
        unsigned(&mut self.type_section, return_types.len() as u64).unwrap();
        for return_type in return_types {
            let valtype = self.data_type_to_valtype(return_type);
            self.type_section.push(valtype);
        }
        
        self.type_count += 1;
        type_index
    }

    /// Convert Beanstalk DataType to WASM value type
    fn data_type_to_valtype(&self, data_type: &DataType) -> u8 {
        match data_type {
            DataType::Int(_) => Self::VALTYPE_I32,
            DataType::Float(_) => Self::VALTYPE_F64,
            DataType::Bool(_) => Self::VALTYPE_I32, // Booleans as i32
            DataType::String(_) => Self::VALTYPE_I32, // String pointers as i32
            _ => Self::VALTYPE_I32, // Default to i32
        }
    }

    /// Allocate a local variable
    fn allocate_local(&mut self, name: &str) -> u32 {
        let index = self.local_count;
        self.local_count += 1;
        index
    }

    /// Add a string constant to the data section
    fn add_string_constant(&mut self, value: &str) -> u32 {
        if let Some(&index) = self.string_constant_map.get(value) {
            return index;
        }
        
        let index = self.string_constants.len() as u32;
        self.string_constants.push(value.to_string());
        self.string_constant_map.insert(value.to_string(), index);
        
        // Add to data section
        // TODO: Implement proper string storage in data section
        
        index
    }

    /// Example function to demonstrate WASM code generation from AST
    pub fn generate_simple_function() -> Result<Vec<u8>, CompileError> {
        let mut wasm_module = WasmModule::new();
        
        // Create a simple AST block with a function that adds two numbers
        let ast_block = AstBlock {
            scope: std::path::PathBuf::from("example"),
            ast: vec![
                // Function: add |a Int, b Int| -> Int: return a + b;
                AstNode {
                    kind: NodeKind::Declaration(
                        "add".to_string(),
                        Expression::function(
                            1, // owner_id
                            vec![
                                Arg {
                                    name: "a".to_string(),
                                    value: Expression::int(0, TextLocation::default(), 1),
                                },
                                Arg {
                                    name: "b".to_string(),
                                    value: Expression::int(0, TextLocation::default(), 1),
                                },
                            ],
                            AstBlock {
                                scope: std::path::PathBuf::from("example"),
                                ast: vec![
                                    AstNode {
                                        kind: NodeKind::Return(vec![
                                            Expression::runtime(
                                                vec![
                                                    AstNode {
                                                        kind: NodeKind::Reference(
                                                            Expression::int(0, TextLocation::default(), 1),
                                                        ),
                                                        location: TextLocation::default(),
                                                        scope: std::path::PathBuf::from("example"),
                                                    },
                                                    AstNode {
                                                        kind: NodeKind::Operator(Operator::Add),
                                                        location: TextLocation::default(),
                                                        scope: std::path::PathBuf::from("example"),
                                                    },
                                                    AstNode {
                                                        kind: NodeKind::Reference(
                                                            Expression::int(0, TextLocation::default(), 1),
                                                        ),
                                                        location: TextLocation::default(),
                                                        scope: std::path::PathBuf::from("example"),
                                                    },
                                                ],
                                                DataType::Int(crate::compiler::datatypes::Ownership::default()),
                                                TextLocation::default(),
                                                1,
                                            ),
                                        ]),
                                        location: TextLocation::default(),
                                        scope: std::path::PathBuf::from("example"),
                                    },
                                ],
                                is_entry_point: false,
                            },
                            vec![DataType::Int(crate::compiler::datatypes::Ownership::default())],
                            TextLocation::default(),
                        ),
                        crate::compiler::parsers::tokens::VarVisibility::Public,
                    ),
                    location: TextLocation::default(),
                    scope: std::path::PathBuf::from("example"),
                },
            ],
            is_entry_point: false,
        };
        
        // Lower the AST block to WASM
        wasm_module.lower_ast_block(&ast_block)?;
        
        // Add an export for the function
        wasm_module.add_export("add", 0, 0); // Export function at index 0
        
        // Generate the final WASM binary
        wasm_module.finish()
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
}

// pub fn add_wasm_fn(
//     wasm_module: &mut WasmModule,
//     name: String,
//     args: &[Arg],
//     body: &[AstNode],
//     return_types: &[DataType],
// ) {
// }