// codegen.rs

use cranelift::codegen::ir::{AbiParam, Function, Signature, types, UserFuncName};
use cranelift::codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift::codegen::ir::InstBuilder;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::datatypes::DataType;
use std::collections::HashMap;

pub struct CodeGenContext {
    /// Reusable scratch space for building each function
    builder_context: FunctionBuilderContext,
    /// Map of variable names to Cranelift variables
    variables: HashMap<String, Variable>,
    /// Function parameter mapping
    parameters: HashMap<String, Variable>,
    /// Temporary counter for generating unique temporary values
    temp_counter: usize,
}

impl CodeGenContext {
    pub fn new() -> Self {
        CodeGenContext {
            builder_context: FunctionBuilderContext::new(),
            variables: HashMap::new(),
            parameters: HashMap::new(),
            temp_counter: 0,
        }
    }

    /// Lowers a Beanstalk AST function into a Cranelift IR `Function`
    pub fn lower_function(&mut self, function_expression: &Expression) -> Result<Function, String> {
        // Extract function information from the expression
        let (args, _body, return_types) = match &function_expression.kind {
            ExpressionKind::Function(args, body, return_types) => {
                (args.clone(), body.clone(), return_types.clone())
            }
            _ => return Err("Expression is not a function".to_string()),
        };

        // Build the function signature
        let mut signature = Signature::new(CallConv::Fast);
        
        // Pre-compute parameter types to avoid borrow checker issues
        let param_types: Vec<types::Type> = args
            .iter()
            .map(|arg| self.lower_data_type(&arg.value.data_type))
            .collect::<Result<Vec<_>, String>>()?;
        
        // Add parameters to signature
        for param_type in &param_types {
            signature.params.push(AbiParam::new(*param_type));
        }
        
        // Add return types to signature
        for return_type in &return_types {
            let return_clif_type = self.lower_data_type(return_type)?;
            signature.returns.push(AbiParam::new(return_clif_type));
        }

        // Create function with a generated name
        let function_name = UserFuncName::user(0, 0); // TODO: Generate proper function names
        let mut function = Function::with_name_signature(function_name, signature);

        // Create function builder
        let mut builder = FunctionBuilder::new(&mut function, &mut self.builder_context);
        let entry_block = builder.create_block();

        // Set up entry block
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        // Map function parameters to Cranelift variables
        self.parameters.clear();
        for (index, arg) in args.iter().enumerate() {
            let param_value = builder.block_params(entry_block)[index];
            let variable = Variable::from_u32(index as u32);
            let param_type = param_types[index];
            
            builder.declare_var(param_type);
            builder.def_var(variable, param_value);
            self.parameters.insert(arg.name.clone(), variable);
        }

        // Lower the function body - simplified for now to avoid borrow checker issues
        // TODO: Implement proper AST lowering once borrow checker issues are resolved

        // Add implicit return if no explicit return exists
        if return_types.is_empty() {
            // TODO: Add proper return instruction using builder.ins().return_(&[])
        }

        builder.finalize();
        Ok(function)
    }

    /// Lowers a block of AST nodes to Cranelift IR
    fn lower_ast_block(
        &self,
        builder: &mut FunctionBuilder,
        ast_nodes: &[AstNode],
    ) -> Result<(), String> {
        for node in ast_nodes {
            self.lower_ast_node(builder, node)?;
        }
        Ok(())
    }

    /// Lowers a single AST node to Cranelift IR
    fn lower_ast_node(
        &self,
        builder: &mut FunctionBuilder,
        node: &AstNode,
    ) -> Result<(), String> {
        match &node.kind {
            NodeKind::Return(expressions) => {
                let return_values: Vec<cranelift::codegen::ir::Value> = expressions
                    .iter()
                    .map(|expr| self.lower_expression(builder, expr))
                    .collect::<Result<Vec<_>, String>>()?;
                builder.ins().return_(&return_values);
            }

            NodeKind::Declaration(_variable_name, expression, _visibility) => {
                let value = self.lower_expression(builder, expression)?;
                let variable = Variable::from_u32(0); // TODO: Generate proper variable ID
                let value_type = self.lower_data_type(&expression.data_type)?;
                
                builder.declare_var(value_type);
                builder.def_var(variable, value);
                // TODO: Track variables properly once borrow checker issues are resolved
            }

            NodeKind::If(condition, if_block) => {
                let condition_value = self.lower_expression(builder, condition)?;
                let true_block = builder.create_block();
                let false_block = builder.create_block();
                let merge_block = builder.create_block();

                // Branch based on condition
                builder.ins().brif(condition_value, true_block, &[], false_block, &[]);

                // Lower true branch
                builder.switch_to_block(true_block);
                self.lower_ast_block(builder, &if_block.ast)?;
                builder.ins().jump(merge_block, &[]);

                // Lower false branch (empty for now, could be extended for else)
                builder.switch_to_block(false_block);
                builder.ins().jump(merge_block, &[]);

                // Continue with merge block
                builder.switch_to_block(merge_block);
                builder.seal_block(true_block);
                builder.seal_block(false_block);
                builder.seal_block(merge_block);
            }

            NodeKind::ForLoop(_item_arg, _collection_expression, _loop_body) => {
                // TODO: Implement for loop lowering
                // This requires iterator support and loop control flow
                return Err("For loops not yet implemented in IR lowering".to_string());
            }

            NodeKind::WhileLoop(condition, loop_body) => {
                let loop_header = builder.create_block();
                let loop_body_block = builder.create_block();
                let loop_exit = builder.create_block();

                // Jump to loop header
                builder.ins().jump(loop_header, &[]);

                // Loop header: check condition
                builder.switch_to_block(loop_header);
                let condition_value = self.lower_expression(builder, condition)?;
                builder.ins().brif(condition_value, loop_body_block, &[], loop_exit, &[]);

                // Loop body
                builder.switch_to_block(loop_body_block);
                self.lower_ast_block(builder, &loop_body.ast)?;
                builder.ins().jump(loop_header, &[]);

                // Loop exit
                builder.switch_to_block(loop_exit);
                builder.seal_block(loop_header);
                builder.seal_block(loop_body_block);
                builder.seal_block(loop_exit);
            }

            NodeKind::FunctionCall(_function_name, _arguments, _return_types, _location) => {
                // TODO: Implement function call lowering
                // This requires function lookup and call instruction generation
                return Err("Function calls not yet implemented in IR lowering".to_string());
            }

            NodeKind::Print(expression) => {
                // TODO: Implement print statement lowering
                // This might involve calling a runtime print function
                let _value = self.lower_expression(builder, expression)?;
                // For now, just evaluate the expression but don't print
            }

            NodeKind::Expression(expression) => {
                // Evaluate expression but don't store result
                let _value = self.lower_expression(builder, expression)?;
            }

            NodeKind::Reference(expression) => {
                // Handle variable references - just lower the expression
                let _value = self.lower_expression(builder, expression)?;
            }

            NodeKind::Operator(_operator) => {
                // Handle standalone operators (should be part of expressions)
                return Err("Standalone operators not supported in IR lowering".to_string());
            }

            NodeKind::Comment(_) => {
                // Comments are ignored during code generation
            }

            NodeKind::Empty => {
                // Empty nodes are ignored
            }

            _ => {
                return Err(format!(
                    "AST node kind {:?} not yet implemented in IR lowering",
                    node.kind
                ));
            }
        }
        Ok(())
    }

    /// Lowers a Beanstalk expression to a Cranelift IR value
    fn lower_expression(
        &self,
        builder: &mut FunctionBuilder,
        expression: &Expression,
    ) -> Result<cranelift::codegen::ir::Value, String> {
        match &expression.kind {
            ExpressionKind::Int(value) => {
                Ok(builder.ins().iconst(types::I32, *value as i64))
            }

            ExpressionKind::Float(value) => {
                Ok(builder.ins().f64const(*value))
            }

            ExpressionKind::Bool(value) => {
                Ok(builder.ins().iconst(types::I8, if *value { 1 } else { 0 }))
            }

            ExpressionKind::String(_value) => {
                // TODO: Implement proper string literal lowering with string constants
                // For now, return a placeholder value
                Ok(builder.ins().iconst(types::I64, 0))
            }

            ExpressionKind::None => {
                Ok(builder.ins().iconst(types::I32, 0))
            }

            ExpressionKind::Runtime(_ast_nodes) => {
                // TODO: Implement runtime expression lowering
                // This requires lowering the AST nodes within the expression
                return Err("Runtime expressions not yet implemented in IR lowering".to_string());
            }

            ExpressionKind::Collection(_items) => {
                // TODO: Implement collection literal lowering
                return Err("Collection literals not yet implemented in IR lowering".to_string());
            }

            ExpressionKind::Struct(_args) => {
                // TODO: Implement struct literal lowering
                return Err("Struct literals not yet implemented in IR lowering".to_string());
            }

            ExpressionKind::Function(_, _, _) => {
                // TODO: Implement function literal lowering
                return Err("Function literals not yet implemented in IR lowering".to_string());
            }

            ExpressionKind::Template(_, _, _) => {
                // TODO: Implement template literal lowering
                return Err("Template literals not yet implemented in IR lowering".to_string());
            }
        }
    }

    /// Converts a Beanstalk data type to a Cranelift IR type
    fn lower_data_type(&self, data_type: &DataType) -> Result<types::Type, String> {
        match data_type {
            DataType::Int(_) => Ok(types::I32),
            DataType::Float(_) => Ok(types::F64),
            DataType::Bool(_) => Ok(types::I8), // Use I8 for boolean values
            DataType::String(_) => Ok(types::I64), // TODO: Proper string representation
            DataType::None => Ok(types::I32), // Unit type represented as i32 zero
            DataType::Inferred(_) => Ok(types::I32), // Default to i32 for inferred types
            DataType::Collection(_, _) => Ok(types::I64), // TODO: Proper collection representation
            DataType::Struct(_, _) => Ok(types::I64), // TODO: Proper struct representation
            DataType::Function(_, _) => Ok(types::I64), // TODO: Proper function representation
            DataType::Template(_) => Ok(types::I64), // TODO: Proper template representation
            DataType::Args(_) => Ok(types::I64), // TODO: Proper args representation
            DataType::Choices(_) => Ok(types::I64), // TODO: Proper union representation
            DataType::Option(_) => Ok(types::I64), // TODO: Proper option representation
            DataType::Range => Ok(types::I64), // TODO: Proper range representation
            DataType::True => Ok(types::I8),
            DataType::False => Ok(types::I8),
            DataType::Decimal(_) => Ok(types::F64), // Use f64 for decimal
            DataType::CoerceToString(_) => Ok(types::I64), // TODO: Proper string coercion
        }
    }

    /// Gets a Cranelift variable for a given variable name
    fn get_variable(&self, name: &str) -> Option<Variable> {
        self.variables.get(name).copied().or_else(|| {
            self.parameters.get(name).copied()
        })
    }
}


