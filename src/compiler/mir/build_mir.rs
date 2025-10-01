// Re-export all MIR components from sibling modules
pub use crate::compiler::mir::mir_nodes::*;
pub use crate::compiler::mir::place::*;

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
use crate::{ir_log, return_compiler_error, return_rule_error};
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
}

/// Transform AST to simplified MIR
///
/// This is the core MIR lowering function that focuses on correct transformation
/// without premature optimization.
pub fn ast_to_mir(ast: AstBlock) -> Result<MIR, CompileError> {
    let mut mir = MIR::new();
    let mut context = MirTransformContext::new();

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

    // Transform all AST nodes to MIR
    let main_block_id = 0;
    let mut current_block = MirBlock::new(main_block_id);

    for node in &ast.ast {
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
        if let Some(function) = mir.functions.get_mut(0) {
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

    // Convert expression to rvalue
    let rvalue = expression_to_rvalue(expression, &expression.location)?;

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

    // Convert expression to rvalue
    let rvalue = expression_to_rvalue(expression, location)?;

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
    // Convert parameters to operands
    let mut args = Vec::new();
    for param in params {
        let operand = expression_to_operand(param, &param.location)?;
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
            return_compiler_error!("Variable references in expressions not yet implemented for variable '{}' at line {}, column {}. This feature is coming soon - for now, try using the variable directly in assignments.", name, location.start_pos.line_number, location.start_pos.char_column);
        }
        ExpressionKind::Runtime(_) => {
            return_compiler_error!("Runtime expressions (complex calculations) not yet implemented for MIR generation at line {}, column {}. Try breaking down complex expressions into simpler assignments.", location.start_pos.line_number, location.start_pos.char_column);
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
            return_compiler_error!("Variable references in function parameters not yet implemented for variable '{}' at line {}, column {}. This feature is coming soon - for now, try passing literal values.", name, location.start_pos.line_number, location.start_pos.char_column);
        }
        ExpressionKind::Runtime(_) => {
            return_compiler_error!("Runtime expressions (complex calculations) not yet implemented for function parameters at line {}, column {}. Try passing simpler values or pre-calculating the result.", location.start_pos.line_number, location.start_pos.char_column);
        }
        _ => {
            return_compiler_error!("Expression type '{:?}' not yet implemented for function parameters at line {}, column {}. This expression type needs to be added to the MIR generator.", expression.kind, location.start_pos.line_number, location.start_pos.char_column)
        }
    }
}


