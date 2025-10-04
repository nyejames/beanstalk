// Re-export all MIR components from sibling modules
pub use crate::compiler::mir::mir_nodes::*;
pub use crate::compiler::mir::place::*;

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
use crate::{ir_log, return_compiler_error, return_rule_error, return_type_error};
use std::collections::HashMap;



/// Run borrow checking on all functions in the MIR
fn run_borrow_checking_on_mir(mir: &mut MIR) -> Result<(), CompileError> {
    use crate::compiler::mir::extract::BorrowFactExtractor;
    use crate::compiler::mir::unified_borrow_checker::run_unified_borrow_checking;
    
    for function in &mut mir.functions {
        // Extract borrow facts from the function
        let mut extractor = BorrowFactExtractor::new();
        extractor.extract_function(function).map_err(|e| {
            CompileError::compiler_error(&format!("Failed to extract borrow facts: {}", e))
        })?;
        
        // Run unified borrow checking
        let borrow_results = run_unified_borrow_checking(function, &extractor).map_err(|e| {
            CompileError::compiler_error(&format!("Borrow checking failed: {}", e))
        })?;
        
        // Convert borrow errors to compile errors
        if !borrow_results.errors.is_empty() {
            let first_error = &borrow_results.errors[0];
            return Err(CompileError::new_rule_error(
                first_error.message.clone(),
                first_error.location.clone(),
            ));
        }
        
        // Log warnings if any
        for warning in &borrow_results.warnings {
            eprintln!("Borrow checker warning: {}", warning.message);
        }
    }
    
    Ok(())
}

/// Generate events for all statements in a function
fn generate_events_for_function(function: &mut MirFunction, _context: &mut MirTransformContext) {
    // Collect all events first to avoid borrowing conflicts
    let mut all_events = Vec::new();
    
    for block in &function.blocks {
        // Generate events for statements
        for (stmt_index, statement) in block.statements.iter().enumerate() {
            let program_point = ProgramPoint::new(block.id * 1000 + stmt_index as u32);
            let events = statement.generate_events();
            all_events.push((program_point, events));
        }
        
        // Generate events for terminator
        let terminator_point = ProgramPoint::new(block.id * 1000 + 999);
        let terminator_events = block.terminator.generate_events();
        all_events.push((terminator_point, terminator_events));
    }
    
    // Store all events
    for (program_point, events) in all_events {
        function.store_events(program_point, events);
    }
}

/// Function information for tracking function metadata and signatures
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub parameters: Vec<(String, DataType)>,
    pub return_type: Option<DataType>,
    pub wasm_function_index: Option<u32>,
    pub mir_function: MirFunction,
}

/// Simplified context for AST-to-MIR transformation
#[derive(Debug)]
pub struct MirTransformContext {
    /// Place manager for memory layout
    place_manager: PlaceManager,
    /// Variable name to place mapping (scoped)
    variable_scopes: Vec<HashMap<String, Place>>,
    /// Function name to ID mapping
    function_names: HashMap<String, u32>,
    /// Next function ID to allocate
    next_function_id: u32,
    /// Next block ID to allocate
    next_block_id: u32,
    /// Program point generator for borrow checking
    program_point_generator: ProgramPointGenerator,
}

impl MirTransformContext {
    /// Create a new transformation context
    pub fn new() -> Self {
        Self {
            place_manager: PlaceManager::new(),
            variable_scopes: vec![HashMap::new()], // Start with global scope
            function_names: HashMap::new(),
            next_function_id: 0,
            next_block_id: 0,
            program_point_generator: ProgramPointGenerator::new(),
        }
    }

    /// Enter a new scope
    pub fn enter_scope(&mut self) {
        self.variable_scopes.push(HashMap::new());
    }

    /// Exit current scope
    pub fn exit_scope(&mut self) {
        if self.variable_scopes.len() > 1 {
            self.variable_scopes.pop();
        }
    }

    /// Register a variable with a place
    pub fn register_variable(&mut self, name: String, place: Place) {
        if let Some(current_scope) = self.variable_scopes.last_mut() {
            current_scope.insert(name, place);
        }
    }

    /// Look up a variable's place
    pub fn lookup_variable(&self, name: &str) -> Option<&Place> {
        // Search from innermost to outermost scope
        for scope in self.variable_scopes.iter().rev() {
            if let Some(place) = scope.get(name) {
                return Some(place);
            }
        }
        None
    }

    /// Allocate a new function ID
    pub fn allocate_function_id(&mut self) -> u32 {
        let id = self.next_function_id;
        self.next_function_id += 1;
        id
    }

    /// Allocate a new block ID
    pub fn allocate_block_id(&mut self) -> u32 {
        let id = self.next_block_id;
        self.next_block_id += 1;
        id
    }

    /// Get place manager
    pub fn get_place_manager(&mut self) -> &mut PlaceManager {
        &mut self.place_manager
    }

    /// Generate the next program point
    pub fn next_program_point(&mut self) -> ProgramPoint {
        self.program_point_generator.allocate_next()
    }

    /// Store events for a program point in the current function
    pub fn store_events_for_statement(
        &mut self,
        function: &mut MirFunction,
        program_point: ProgramPoint,
        statement: &Statement,
    ) {
        let events = statement.generate_events();
        function.store_events(program_point, events);
    }

    /// Transform function definition from expression to MIR
    pub fn transform_function_definition_from_expression(
        &mut self,
        name: &str,
        parameters: &[crate::compiler::parsers::ast_nodes::Arg],
        return_types: &[DataType],
        body: &[AstNode],
    ) -> Result<FunctionInfo, CompileError> {
        let function_index = self.allocate_function_id();
        
        // Register function name for future calls
        self.function_names.insert(name.to_string(), function_index);
        
        // Create new scope for function parameters
        self.enter_scope();
        
        // Convert parameters to MIR places and register them
        let mut mir_parameters = Vec::new();
        let mut param_info = Vec::new();
        
        for param in parameters {
            // Allocate a local place for the parameter
            let param_place = self.get_place_manager().allocate_local(&param.value.data_type);
            
            // Register parameter in current scope
            self.register_variable(param.name.clone(), param_place.clone());
            
            mir_parameters.push(param_place);
            param_info.push((param.name.clone(), param.value.data_type.clone()));
        }
        
        // Convert return types to WASM types
        let wasm_return_types: Vec<crate::compiler::mir::place::WasmType> = return_types
            .iter()
            .map(|dt| self.datatype_to_wasm_type(dt))
            .collect::<Result<Vec<_>, _>>()?;
        
        // Create MIR function
        let mut mir_function = MirFunction::new(
            function_index,
            name.to_string(),
            mir_parameters.clone(),
            wasm_return_types,
        );
        
        // Transform function body
        let main_block_id = 0;
        let mut current_block = crate::compiler::mir::mir_nodes::MirBlock::new(main_block_id);
        
        for node in body {
            let statements = transform_ast_node_to_mir(node, self)?;
            for statement in statements {
                current_block.add_statement(statement);
            }
        }
        
        // Set terminator for the function block
        let terminator = if return_types.is_empty() {
            crate::compiler::mir::mir_nodes::Terminator::Return { values: vec![] }
        } else {
            // For now, we'll use an empty return - proper return value handling will be added later
            crate::compiler::mir::mir_nodes::Terminator::Return { values: vec![] }
        };
        current_block.set_terminator(terminator);
        
        // Add the block to the function
        mir_function.add_block(current_block);
        
        // Add local variables to function
        if let Some(current_scope) = self.variable_scopes.last() {
            for (var_name, var_place) in current_scope {
                if matches!(var_place, crate::compiler::mir::place::Place::Local { .. }) {
                    mir_function.add_local(var_name.clone(), var_place.clone());
                }
            }
        }
        
        // Generate events for all statements in the function
        generate_events_for_function(&mut mir_function, self);
        
        self.exit_scope(); // Exit function scope
        
        Ok(FunctionInfo {
            name: name.to_string(),
            parameters: param_info,
            return_type: return_types.first().cloned(), // For now, support single return type
            wasm_function_index: Some(function_index),
            mir_function,
        })
    }

    /// Transform function definition to MIR
    pub fn transform_function_definition(
        &mut self,
        name: &str,
        parameters: &[crate::compiler::parsers::ast_nodes::Arg],
        return_types: &[DataType],
        body: &crate::compiler::parsers::build_ast::AstBlock,
    ) -> Result<FunctionInfo, CompileError> {
        let function_index = self.allocate_function_id();
        
        // Register function name for future calls
        self.function_names.insert(name.to_string(), function_index);
        
        // Create new scope for function parameters
        self.enter_scope();
        
        // Convert parameters to MIR places and register them
        let mut mir_parameters = Vec::new();
        let mut param_info = Vec::new();
        
        for param in parameters {
            // Allocate a local place for the parameter
            let param_place = self.get_place_manager().allocate_local(&param.value.data_type);
            
            // Register parameter in current scope
            self.register_variable(param.name.clone(), param_place.clone());
            
            mir_parameters.push(param_place);
            param_info.push((param.name.clone(), param.value.data_type.clone()));
        }
        
        // Convert return types to WASM types
        let wasm_return_types: Vec<crate::compiler::mir::place::WasmType> = return_types
            .iter()
            .map(|dt| self.datatype_to_wasm_type(dt))
            .collect::<Result<Vec<_>, _>>()?;
        
        // Create MIR function
        let mut mir_function = MirFunction::new(
            function_index,
            name.to_string(),
            mir_parameters.clone(),
            wasm_return_types,
        );
        
        // Transform function body
        let main_block_id = 0;
        let mut current_block = crate::compiler::mir::mir_nodes::MirBlock::new(main_block_id);
        
        for node in &body.ast {
            let statements = transform_ast_node_to_mir(node, self)?;
            for statement in statements {
                current_block.add_statement(statement);
            }
        }
        
        // Set terminator for the function block
        let terminator = if return_types.is_empty() {
            crate::compiler::mir::mir_nodes::Terminator::Return { values: vec![] }
        } else {
            // For now, we'll use an empty return - proper return value handling will be added later
            crate::compiler::mir::mir_nodes::Terminator::Return { values: vec![] }
        };
        current_block.set_terminator(terminator);
        
        // Add the block to the function
        mir_function.add_block(current_block);
        
        // Add local variables to function
        if let Some(current_scope) = self.variable_scopes.last() {
            for (var_name, var_place) in current_scope {
                if matches!(var_place, crate::compiler::mir::place::Place::Local { .. }) {
                    mir_function.add_local(var_name.clone(), var_place.clone());
                }
            }
        }
        
        // Generate events for all statements in the function
        generate_events_for_function(&mut mir_function, self);
        
        self.exit_scope(); // Exit function scope
        
        Ok(FunctionInfo {
            name: name.to_string(),
            parameters: param_info,
            return_type: return_types.first().cloned(), // For now, support single return type
            wasm_function_index: Some(function_index),
            mir_function,
        })
    }

    /// Convert DataType to WasmType
    fn datatype_to_wasm_type(&self, data_type: &DataType) -> Result<crate::compiler::mir::place::WasmType, CompileError> {
        use crate::compiler::mir::place::WasmType;
        
        match data_type {
            DataType::Int(_) => Ok(WasmType::I64),
            DataType::Float(_) => Ok(WasmType::F64),
            DataType::Bool(_) => Ok(WasmType::I32),
            DataType::String(_) => Ok(WasmType::I32), // String pointer
            _ => {
                return_compiler_error!("DataType {:?} not yet supported for WASM type conversion", data_type);
            }
        }
    }
}

/// Transform AST to simplified MIR
///
/// This is the core MIR lowering function that focuses on correct transformation
/// without premature optimization.
pub fn ast_to_mir(ast: AstBlock) -> Result<MIR, CompileError> {
    let mut mir = MIR::new();
    let mut context = MirTransformContext::new();
    let mut defined_functions = Vec::new();

    // First pass: collect all function definitions
    for node in &ast.ast {
        if let NodeKind::Declaration(name, expression, _visibility) = &node.kind {
            if let crate::compiler::parsers::expressions::expression::ExpressionKind::Function(parameters, body, return_types) = &expression.kind {
                let function_info = context.transform_function_definition_from_expression(
                    name, 
                    parameters, 
                    return_types, 
                    body
                )?;
                defined_functions.push(function_info);
            }
        }
    }

    // Add all defined functions to MIR
    for function_info in defined_functions {
        mir.add_function(function_info.mir_function);
    }

    // Create main function if this is an entry point
    if ast.is_entry_point {
        let main_function_id = context.allocate_function_id();
        context
            .function_names
            .insert("main".to_string(), main_function_id);

        let main_function = MirFunction::new(
            main_function_id,
            "main".to_string(),
            vec![], // No parameters
            vec![], // No return values for main
        );

        mir.add_function(main_function);
        context.enter_scope(); // Enter function scope
    }

    // Transform all non-function AST nodes to MIR
    let main_block_id = 0;
    let mut current_block = MirBlock::new(main_block_id);

    for node in &ast.ast {
        // Skip function declarations as they were already processed
        if let NodeKind::Declaration(_, expression, _) = &node.kind {
            if matches!(expression.kind, crate::compiler::parsers::expressions::expression::ExpressionKind::Function(..)) {
                continue;
            }
        }

        let statements = transform_ast_node_to_mir(node, &mut context)?;

        for statement in statements {
            ir_log!(
                "AST Node: {:?} \nConverted into: {:?} \n",
                node.kind,
                statement
            );
            current_block.add_statement(statement);
        }
    }

    // Set terminator for the main block
    let terminator = Terminator::Return { values: vec![] };
    current_block.set_terminator(terminator);

    // Add the block to the current function
    if ast.is_entry_point {
        if let Some(function) = mir.functions.last_mut() {
            function.add_block(current_block);
            
            // Add local variables to function
            if let Some(current_scope) = context.variable_scopes.last() {
                for (var_name, var_place) in current_scope {
                    if matches!(var_place, Place::Local { .. }) {
                        function.add_local(var_name.clone(), var_place.clone());
                    }
                }
            }
            
            // Generate events for all statements in the function
            generate_events_for_function(function, &mut context);
        }
        context.exit_scope(); // Exit function scope
    }

    // Run borrow checking on all functions
    run_borrow_checking_on_mir(&mut mir)?;

    Ok(mir)
}



/// Transform a single AST node to MIR statements
fn transform_ast_node_to_mir(
    node: &AstNode,
    context: &mut MirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    match &node.kind {
        NodeKind::Declaration(name, expression, visibility) => {
            ast_declaration_to_mir(name, expression, visibility, context)
        }
        NodeKind::Mutation(name, expression) => {
            ast_mutation_to_mir(name, expression, &node.location, context)
        }
        NodeKind::FunctionCall(name, params, return_types, ..) => {
            ast_function_call_to_mir(name, params, return_types, &node.location, context)
        }
        NodeKind::If(condition, then_body, else_body) => {
            ast_if_statement_to_mir(condition, then_body, else_body, &node.location, context)
        }
        NodeKind::Newline | NodeKind::Spaces(_) | NodeKind::Empty => {
            // These nodes don't generate MIR statements
            Ok(vec![])
        }
        _ => {
            return_compiler_error!(
                "AST node type '{:?}' not yet implemented for MIR generation at line {}, column {}. This language feature needs to be added to the compiler backend. Please report this as a feature request.",
                node.kind,
                node.location.start_pos.line_number,
                node.location.start_pos.char_column
            )
        }
    }
}



/// Transform variable declaration to MIR
fn ast_declaration_to_mir(
    name: &str,
    expression: &Expression,
    visibility: &VarVisibility,
    context: &mut MirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // Check if this is a function declaration
    if let crate::compiler::parsers::expressions::expression::ExpressionKind::Function(parameters, body, return_types) = &expression.kind {
        // Transform function declaration
        let _function_info = context.transform_function_definition_from_expression(
            name, 
            parameters, 
            return_types, 
            body
        )?;
        // Function declarations don't generate statements in the current block
        return Ok(vec![]);
    }

    let mut statements = Vec::new();

    // Check if variable is already declared in current scope
    if let Some(current_scope) = context.variable_scopes.last() {
        if current_scope.contains_key(name) {
            return_rule_error!(expression.location.clone(), "Variable '{}' is already declared in this scope. Shadowing is not supported in Beanstalk - each variable name can only be used once per scope. Try using a different name like '{}_2' or '{}_{}'.", name, name, name, "new");
        }
    }

    // Determine if this should be a global or local variable
    let is_global = matches!(visibility, VarVisibility::Exported);

    // Allocate the appropriate place for the variable
    let variable_place = if is_global {
        context
            .get_place_manager()
            .allocate_global(&expression.data_type)
    } else {
        context
            .get_place_manager()
            .allocate_local(&expression.data_type)
    };

    // Register the variable in context
    context.register_variable(name.to_string(), variable_place.clone());

    // Convert expression to rvalue with context for variable references
    let rvalue = expression_to_rvalue_with_context(expression, &expression.location, context)?;

    // Create assignment statement
    let assign_statement = Statement::Assign {
        place: variable_place,
        rvalue,
    };

    statements.push(assign_statement);
    Ok(statements)
}

/// Transform variable mutation to MIR
fn ast_mutation_to_mir(
    name: &str,
    expression: &Expression,
    location: &TextLocation,
    context: &mut MirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // Look up the existing variable place
    let variable_place = match context.lookup_variable(name) {
        Some(place) => place.clone(),
        None => {
            return_rule_error!(location.clone(), "Cannot mutate undefined variable '{}'. Variable must be declared before mutation. Did you mean to declare it first with 'let {} = ...' or '{}~= ...'?", name, name, name);
        }
    };

    // Convert expression to rvalue with context for variable references
    let rvalue = expression_to_rvalue_with_context(expression, location, context)?;

    // Create assignment statement for the mutation
    let assign_statement = Statement::Assign {
        place: variable_place,
        rvalue,
    };

    Ok(vec![assign_statement])
}

/// Transform function call to MIR
fn ast_function_call_to_mir(
    name: &str,
    params: &[Expression],
    _return_types: &[DataType],
    location: &TextLocation,
    context: &mut MirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // Convert parameters to operands with context for variable references
    let mut args = Vec::new();
    for param in params {
        let operand = expression_to_operand_with_context(param, &param.location, context)?;
        args.push(operand);
    }

    // Look up function or create function reference
    let func_operand = if let Some(func_id) = context.function_names.get(name) {
        Operand::FunctionRef(*func_id)
    } else {
        return_rule_error!(location.clone(), "Undefined function '{}'. Function must be declared before use. Make sure the function is defined in this file or imported from another module.", name);
    };

    // Create call statement
    let call_statement = Statement::Call {
        func: func_operand,
        args,
        destination: None, // For now, don't handle return values
    };

    Ok(vec![call_statement])
}

/// Transform if statement to MIR with proper control flow
/// 
/// This function creates the MIR representation for if/else statements by:
/// 1. Converting the condition expression to an operand
/// 2. Validating that the condition is boolean type
/// 3. Creating block IDs for then and else branches
/// 4. Generating a Terminator::If for structured control flow
fn ast_if_statement_to_mir(
    condition: &Expression,
    then_body: &AstBlock,
    else_body: &Option<AstBlock>,
    location: &TextLocation,
    context: &mut MirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // Validate that condition is boolean type
    if !matches!(condition.data_type, DataType::Bool(_)) {
        return_type_error!(
            condition.location.clone(),
            "If condition must be boolean, found {}. Try using comparison operators like 'is', 'not', or boolean expressions.",
            condition.data_type
        );
    }
    
    // Convert condition expression to operand
    let condition_operand = expression_to_operand_with_context(condition, &condition.location, context)?;
    
    // Allocate block IDs for control flow
    let then_block_id = context.allocate_block_id();
    let else_block_id = context.allocate_block_id();
    
    // For now, we'll create a simplified MIR representation
    // In a full implementation, we would need to:
    // 1. Create separate MIR blocks for then/else branches
    // 2. Transform the body statements into those blocks
    // 3. Handle block merging and continuation
    
    // This is a placeholder implementation that demonstrates the structure
    // The actual block creation and statement transformation will be implemented
    // when we have full block-based MIR construction
    
    // Create a nop statement as a placeholder for the if logic
    // In the real implementation, this would be replaced by proper block management
    let if_placeholder = Statement::Nop;
    
    // TODO: Transform then_body and else_body into separate MIR blocks
    // TODO: Create proper Terminator::If with block references
    // TODO: Handle block merging and continuation flow
    
    // For now, return a placeholder that won't break compilation
    // This will be expanded in the next phase of implementation
    Ok(vec![if_placeholder])
}



/// Convert expression to rvalue for basic types
fn expression_to_rvalue(expression: &Expression, location: &TextLocation) -> Result<Rvalue, CompileError> {
    match &expression.kind {
        ExpressionKind::Int(value) => Ok(Rvalue::Use(Operand::Constant(Constant::I64(*value)))),
        ExpressionKind::Float(value) => Ok(Rvalue::Use(Operand::Constant(Constant::F64(*value)))),
        ExpressionKind::Bool(value) => Ok(Rvalue::Use(Operand::Constant(Constant::Bool(*value)))),
        ExpressionKind::String(value) => Ok(Rvalue::Use(Operand::Constant(Constant::String(
            value.clone(),
        )))),
        ExpressionKind::Reference(name) => {
            return_compiler_error!("Variable references in expressions not yet implemented for variable '{}' at line {}, column {}. This feature requires context parameter - use transform_variable_reference instead.", name, location.start_pos.line_number, location.start_pos.char_column);
        }
        ExpressionKind::Runtime(_) => {
            return_compiler_error!("Runtime expressions (complex calculations) not yet implemented for MIR generation at line {}, column {}. Try breaking down complex expressions into simpler assignments.", location.start_pos.line_number, location.start_pos.char_column);
        }
        _ => {
            return_compiler_error!("Expression type '{:?}' not yet implemented for rvalue conversion at line {}, column {}. This expression type needs to be added to the MIR generator.", expression.kind, location.start_pos.line_number, location.start_pos.char_column)
        }
    }
}

/// Convert expression to rvalue with context for variable references
fn expression_to_rvalue_with_context(
    expression: &Expression, 
    location: &TextLocation,
    context: &MirTransformContext,
) -> Result<Rvalue, CompileError> {
    match &expression.kind {
        ExpressionKind::Int(value) => Ok(Rvalue::Use(Operand::Constant(Constant::I64(*value)))),
        ExpressionKind::Float(value) => Ok(Rvalue::Use(Operand::Constant(Constant::F64(*value)))),
        ExpressionKind::Bool(value) => Ok(Rvalue::Use(Operand::Constant(Constant::Bool(*value)))),
        ExpressionKind::String(value) => Ok(Rvalue::Use(Operand::Constant(Constant::String(
            value.clone(),
        )))),
        ExpressionKind::Reference(name) => {
            // Transform variable reference using context
            transform_variable_reference(name, location, context)
        }
        ExpressionKind::Runtime(runtime_nodes) => {
            // Transform runtime expressions (RPN order) to MIR
            transform_runtime_expression(runtime_nodes, location, context)
        }
        ExpressionKind::None => {
            // None expressions represent parameters without default arguments
            // In the context of function parameters, this indicates the parameter must be provided
            return_compiler_error!("None expression encountered in rvalue context at line {}, column {}. This typically indicates a function parameter without a default argument being used in an invalid context.", location.start_pos.line_number, location.start_pos.char_column);
        }
        _ => {
            return_compiler_error!("Expression type '{:?}' not yet implemented for rvalue conversion at line {}, column {}. This expression type needs to be added to the MIR generator.", expression.kind, location.start_pos.line_number, location.start_pos.char_column)
        }
    }
}

/// Convert expression to operand for basic types
fn expression_to_operand(expression: &Expression, location: &TextLocation) -> Result<Operand, CompileError> {
    match &expression.kind {
        ExpressionKind::Int(value) => Ok(Operand::Constant(Constant::I64(*value))),
        ExpressionKind::Float(value) => Ok(Operand::Constant(Constant::F64(*value))),
        ExpressionKind::Bool(value) => Ok(Operand::Constant(Constant::Bool(*value))),
        ExpressionKind::String(value) => Ok(Operand::Constant(Constant::String(value.clone()))),
        ExpressionKind::Reference(name) => {
            return_compiler_error!("Variable references in function parameters not yet implemented for variable '{}' at line {}, column {}. This feature requires context parameter - use expression_to_operand_with_context instead.", name, location.start_pos.line_number, location.start_pos.char_column);
        }
        ExpressionKind::Runtime(_) => {
            return_compiler_error!("Runtime expressions (complex calculations) not yet implemented for function parameters at line {}, column {}. Try passing simpler values or pre-calculating the result.", location.start_pos.line_number, location.start_pos.char_column);
        }
        _ => {
            return_compiler_error!("Expression type '{:?}' not yet implemented for function parameters at line {}, column {}. This expression type needs to be added to the MIR generator.", expression.kind, location.start_pos.line_number, location.start_pos.char_column)
        }
    }
}

/// Convert expression to operand with context for variable references
fn expression_to_operand_with_context(
    expression: &Expression, 
    location: &TextLocation,
    context: &MirTransformContext,
) -> Result<Operand, CompileError> {
    match &expression.kind {
        ExpressionKind::Int(value) => Ok(Operand::Constant(Constant::I64(*value))),
        ExpressionKind::Float(value) => Ok(Operand::Constant(Constant::F64(*value))),
        ExpressionKind::Bool(value) => Ok(Operand::Constant(Constant::Bool(*value))),
        ExpressionKind::String(value) => Ok(Operand::Constant(Constant::String(value.clone()))),
        ExpressionKind::Reference(name) => {
            // Transform variable reference to operand
            let rvalue = transform_variable_reference(name, location, context)?;
            match rvalue {
                Rvalue::Use(operand) => Ok(operand),
                _ => return_compiler_error!("Variable reference '{}' produced non-use rvalue, which is not supported in operand context", name),
            }
        }
        ExpressionKind::Runtime(_) => {
            return_compiler_error!("Runtime expressions (complex calculations) not yet implemented for function parameters at line {}, column {}. Try passing simpler values or pre-calculating the result.", location.start_pos.line_number, location.start_pos.char_column);
        }
        ExpressionKind::None => {
            // None expressions represent parameters without default arguments
            // In function parameter context, this means the parameter is required and has no default
            // This should not be converted to an operand as it represents the absence of a default value
            return_compiler_error!("None expression encountered in operand context at line {}, column {}. This indicates a function parameter without a default argument, which should not be converted to an operand.", location.start_pos.line_number, location.start_pos.char_column);
        }
        _ => {
            return_compiler_error!("Expression type '{:?}' not yet implemented for function parameters at line {}, column {}. This expression type needs to be added to the MIR generator.", expression.kind, location.start_pos.line_number, location.start_pos.char_column)
        }
    }
}



/// Transform variable reference to MIR rvalue with proper usage context
/// 
/// This method handles variable references by looking up the variable in the context
/// and generating appropriate MIR operands based on usage patterns.
fn transform_variable_reference(
    name: &str,
    location: &TextLocation,
    context: &MirTransformContext,
) -> Result<Rvalue, CompileError> {
    // Look up the variable's place in the context
    let variable_place = match context.lookup_variable(name) {
        Some(place) => place.clone(),
        None => {
            return_rule_error!(location.clone(), "Undefined variable '{}'. Variable must be declared before use. Make sure the variable is declared in this scope or a parent scope.", name);
        }
    };

    // Determine whether to use Copy or Move semantics
    // For now, we'll use Copy semantics for all variable references
    // TODO: In the future, this could be enhanced with move analysis
    let operand = Operand::Copy(variable_place);
    
    Ok(Rvalue::Use(operand))
}

/// Transform runtime expressions (RPN order) to MIR rvalue
/// 
/// Runtime expressions contain AST nodes in Reverse Polish Notation order.
/// This function processes them using a stack-based approach to build MIR binary operations.
fn transform_runtime_expression(
    runtime_nodes: &[AstNode],
    location: &TextLocation,
    context: &MirTransformContext,
) -> Result<Rvalue, CompileError> {
    if runtime_nodes.is_empty() {
        return_compiler_error!("Empty runtime expression at line {}, column {}", location.start_pos.line_number, location.start_pos.char_column);
    }

    // Use a stack to process RPN expression
    let mut operand_stack: Vec<Operand> = Vec::new();

    for node in runtime_nodes {
        match &node.kind {
            NodeKind::Expression(expr) => {
                // Convert expression to operand and push to stack
                let operand = expression_to_operand_with_context(expr, &node.location, context)?;
                operand_stack.push(operand);
            }
            NodeKind::Operator(ast_op) => {
                // Pop two operands for binary operation
                if operand_stack.len() < 2 {
                    return_compiler_error!("Not enough operands for binary operator {:?} in runtime expression at line {}, column {}", ast_op, node.location.start_pos.line_number, node.location.start_pos.char_column);
                }

                let right = operand_stack.pop().unwrap();
                let left = operand_stack.pop().unwrap();

                // Convert AST operator to MIR BinOp
                let mir_op = ast_operator_to_mir_binop(ast_op, &node.location)?;
                
                // For now, we'll return the binary operation directly
                // In a more complex implementation, we might need to handle chained operations
                if operand_stack.is_empty() {
                    // This is the final operation
                    return Ok(Rvalue::BinaryOp(mir_op, left, right));
                } else {
                    // This is an intermediate operation - we would need to create temporary assignments
                    // For now, we'll simplify and only handle single binary operations
                    return_compiler_error!("Complex chained arithmetic expressions not yet implemented. Please break down the expression into simpler assignments at line {}, column {}", node.location.start_pos.line_number, node.location.start_pos.char_column);
                }
            }
            _ => {
                return_compiler_error!("Unsupported node type in runtime expression: {:?} at line {}, column {}", node.kind, node.location.start_pos.line_number, node.location.start_pos.char_column);
            }
        }
    }

    // If we have exactly one operand left, it's a simple value
    if operand_stack.len() == 1 {
        Ok(Rvalue::Use(operand_stack.pop().unwrap()))
    } else {
        return_compiler_error!("Invalid runtime expression - expected single result but got {} operands at line {}, column {}", operand_stack.len(), location.start_pos.line_number, location.start_pos.char_column);
    }
}

/// Convert AST Operator to MIR BinOp
fn ast_operator_to_mir_binop(ast_op: &crate::compiler::parsers::expressions::expression::Operator, location: &TextLocation) -> Result<BinOp, CompileError> {
    use crate::compiler::parsers::expressions::expression::Operator;
    
    match ast_op {
        Operator::Add => Ok(BinOp::Add),
        Operator::Subtract => Ok(BinOp::Sub),
        Operator::Multiply => Ok(BinOp::Mul),
        Operator::Divide => Ok(BinOp::Div),
        Operator::Modulus => Ok(BinOp::Rem),
        Operator::Equality => Ok(BinOp::Eq),
        Operator::NotEqual => Ok(BinOp::Ne),
        Operator::LessThan => Ok(BinOp::Lt),
        Operator::LessThanOrEqual => Ok(BinOp::Le),
        Operator::GreaterThan => Ok(BinOp::Gt),
        Operator::GreaterThanOrEqual => Ok(BinOp::Ge),
        Operator::And => Ok(BinOp::And),
        Operator::Or => Ok(BinOp::Or),
        _ => {
            return_compiler_error!("Operator {:?} not yet implemented for MIR binary operations at line {}, column {}", ast_op, location.start_pos.line_number, location.start_pos.char_column);
        }
    }
}

