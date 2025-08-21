// Re-export all MIR components from sibling modules
pub use crate::compiler::mir::place::*;
pub use crate::compiler::mir::mir_nodes::*;

use crate::compiler::compiler_errors::{CompileError, ErrorType};
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind, Arg};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::template::TemplateContent;
use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
use crate::{return_compiler_error, return_rule_error};
use std::collections::HashMap;

/// Context for AST-to-MIR transformation with WASM-aware place management
#[derive(Debug)]
pub struct MirTransformContext {
    /// Place manager for WASM memory layout
    place_manager: PlaceManager,
    /// Current function being processed
    current_function_id: Option<u32>,
    /// Variable name to place mapping (scoped)
    variable_scopes: Vec<HashMap<String, Place>>,
    /// Function name to ID mapping
    function_names: HashMap<String, u32>,
    /// Next function ID to allocate
    next_function_id: u32,
    /// Next block ID to allocate
    next_block_id: u32,
    /// Whether we're in global scope
    is_global_scope: bool,
    /// Program point generator for sequential allocation
    program_point_generator: ProgramPointGenerator,
    /// Loan tracking for borrow checking
    loans: Vec<Loan>,
    /// Next loan ID to allocate
    next_loan_id: u32,
    /// Events per program point for dataflow analysis
    events_map: HashMap<ProgramPoint, Events>,
    /// Use counts per place for last-use analysis
    use_counts: HashMap<Place, usize>,
    /// Variable use counts from AST analysis (before place allocation)
    variable_use_counts: HashMap<String, usize>,
}

impl MirTransformContext {
    /// Create a new transformation context
    pub fn new() -> Self {
        Self {
            place_manager: PlaceManager::new(),
            current_function_id: None,
            variable_scopes: vec![HashMap::new()], // Start with global scope
            function_names: HashMap::new(),
            next_function_id: 0,
            next_block_id: 0,
            is_global_scope: true,
            program_point_generator: ProgramPointGenerator::new(),
            loans: Vec::new(),
            next_loan_id: 0,
            events_map: HashMap::new(),
            use_counts: HashMap::new(),
            variable_use_counts: HashMap::new(),
        }
    }

    /// Enter a function scope
    pub fn enter_function(&mut self, function_id: u32) {
        self.current_function_id = Some(function_id);
        self.is_global_scope = false;
        self.variable_scopes.push(HashMap::new()); // New function scope
    }

    /// Exit function scope
    pub fn exit_function(&mut self) {
        self.current_function_id = None;
        self.is_global_scope = true;
        if self.variable_scopes.len() > 1 {
            self.variable_scopes.pop();
        }
    }

    /// Register a variable with a place
    pub fn register_variable(&mut self, name: String, place: Place) {
        // Initialize use count for this place based on AST analysis
        self.initialize_place_use_count(place.clone(), &name);
        
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

    /// Allocate the next program point in sequence
    pub fn allocate_program_point(&mut self) -> ProgramPoint {
        self.program_point_generator.allocate_next()
    }

    /// Get all allocated program points for dataflow analysis
    pub fn get_all_program_points(&self) -> &[ProgramPoint] {
        self.program_point_generator.get_all_points()
    }

    /// Get program point count
    pub fn program_point_count(&self) -> usize {
        self.program_point_generator.count()
    }

    /// Allocate a new loan ID
    pub fn allocate_loan_id(&mut self) -> LoanId {
        let id = LoanId::new(self.next_loan_id);
        self.next_loan_id += 1;
        id
    }

    /// Add a loan to the context
    pub fn add_loan(&mut self, loan: Loan) {
        self.loans.push(loan);
    }

    /// Get all loans
    pub fn get_loans(&self) -> &[Loan] {
        &self.loans
    }

    /// Store events for a program point
    pub fn store_events(&mut self, program_point: ProgramPoint, events: Events) {
        self.events_map.insert(program_point, events);
    }

    /// Get events for a program point
    pub fn get_events(&self, program_point: &ProgramPoint) -> Option<&Events> {
        self.events_map.get(program_point)
    }

    /// Get all events
    pub fn get_all_events(&self) -> &HashMap<ProgramPoint, Events> {
        &self.events_map
    }

    /// Initialize use count for a place
    pub fn set_use_count(&mut self, place: Place, count: usize) {
        self.use_counts.insert(place, count);
    }

    /// Decrement use count and check if this is a candidate last use
    pub fn decrement_use_count(&mut self, place: &Place) -> bool {
        if let Some(count) = self.use_counts.get_mut(place) {
            *count = count.saturating_sub(1);
            *count == 0
        } else {
            // If we don't have a count, assume this could be a last use
            true
        }
    }

    /// Get current use count for a place
    pub fn get_use_count(&self, place: &Place) -> usize {
        self.use_counts.get(place).copied().unwrap_or(0)
    }

    /// Store variable use counts from AST analysis
    pub fn store_variable_use_counts(&mut self, counts: HashMap<String, usize>) {
        self.variable_use_counts = counts;
    }

    /// Get variable use count by name
    pub fn get_variable_use_count(&self, var_name: &str) -> usize {
        self.variable_use_counts.get(var_name).copied().unwrap_or(0)
    }

    /// Initialize place use count from variable name when place is allocated
    pub fn initialize_place_use_count(&mut self, place: Place, var_name: &str) {
        let count = self.get_variable_use_count(var_name);
        if count > 0 {
            self.use_counts.insert(place, count);
        }
    }
}

/// Use counter for tracking variable and field/index uses in AST
#[derive(Debug)]
struct UseCounter {
    /// Variable use counts (simple variable names)
    variable_counts: HashMap<String, usize>,
    /// Field access counts (variable.field)
    field_access_counts: HashMap<String, usize>,
    /// Index access counts (variable[index])
    index_access_counts: HashMap<String, usize>,
}

impl UseCounter {
    /// Create a new use counter
    fn new() -> Self {
        Self {
            variable_counts: HashMap::new(),
            field_access_counts: HashMap::new(),
            index_access_counts: HashMap::new(),
        }
    }

    /// Count uses in a single AST node
    fn count_node_uses(&mut self, node: &AstNode) -> Result<(), CompileError> {
        match &node.kind {
            NodeKind::Declaration(_, expression, _) => {
                self.count_expression_uses(expression)?;
            }
            NodeKind::Expression(expression) => {
                self.count_expression_uses(expression)?;
            }
            NodeKind::FunctionCall(_, args, _, _) => {
                for arg in args {
                    self.count_expression_uses(arg)?;
                }
            }
            NodeKind::Print(expression) => {
                self.count_expression_uses(expression)?;
            }
            NodeKind::Return(expressions) => {
                for expr in expressions {
                    self.count_expression_uses(expr)?;
                }
            }
            NodeKind::If(condition, then_block, else_block) => {
                self.count_expression_uses(condition)?;
                self.count_block_uses(&then_block.ast)?;
                if let Some(else_block) = else_block {
                    self.count_block_uses(&else_block.ast)?;
                }
            }
            NodeKind::Match(subject, arms, default_arm) => {
                self.count_expression_uses(subject)?;
                for (pattern, block) in arms {
                    self.count_expression_uses(pattern)?;
                    self.count_block_uses(&block.ast)?;
                }
                if let Some(default_block) = default_arm {
                    self.count_block_uses(&default_block.ast)?;
                }
            }
            NodeKind::ForLoop(arg, collection, body) => {
                self.count_expression_uses(&arg.value)?;
                self.count_expression_uses(collection)?;
                self.count_block_uses(&body.ast)?;
            }
            NodeKind::WhileLoop(condition, body) => {
                self.count_expression_uses(condition)?;
                self.count_block_uses(&body.ast)?;
            }
            _ => {
                // Other node types don't contain variable uses
            }
        }
        Ok(())
    }

    /// Count uses in a block of AST nodes
    fn count_block_uses(&mut self, nodes: &[AstNode]) -> Result<(), CompileError> {
        for node in nodes {
            self.count_node_uses(node)?;
        }
        Ok(())
    }

    /// Count uses in an expression
    fn count_expression_uses(&mut self, expression: &Expression) -> Result<(), CompileError> {
        match &expression.kind {
            ExpressionKind::Reference(var_name) => {
                // Simple variable reference
                *self.variable_counts.entry(var_name.clone()).or_insert(0) += 1;
            }
            ExpressionKind::Runtime(runtime_nodes) => {
                // Count uses in runtime expression nodes
                for runtime_node in runtime_nodes {
                    self.count_node_uses(runtime_node)?;
                }
            }
            ExpressionKind::Function(args, body, _) => {
                // Count uses in function arguments
                for arg in args {
                    self.count_expression_uses(&arg.value)?;
                }
                // Count uses in function body
                self.count_block_uses(body)?;
            }
            ExpressionKind::Collection(items) => {
                // Count uses in collection items
                for item in items {
                    self.count_expression_uses(item)?;
                }
            }
            ExpressionKind::Struct(args) => {
                // Count uses in struct field values
                for arg in args {
                    self.count_expression_uses(&arg.value)?;
                }
            }
            ExpressionKind::Template(content, _, _) => {
                // Count uses in template content
                self.count_template_uses(content)?;
            }
            _ => {
                // Other expression types (literals) don't contain variable references
            }
        }
        Ok(())
    }

    /// Count uses in template content
    fn count_template_uses(&mut self, _content: &TemplateContent) -> Result<(), CompileError> {
        // Template use counting would be implemented here
        // For now, we'll skip this as it's complex and not critical for the basic implementation
        Ok(())
    }

    /// Get all use counts combined
    fn get_use_counts(&self) -> HashMap<String, usize> {
        let mut combined = self.variable_counts.clone();
        
        // Add field access counts
        for (key, count) in &self.field_access_counts {
            *combined.entry(key.clone()).or_insert(0) += count;
        }
        
        // Add index access counts
        for (key, count) in &self.index_access_counts {
            *combined.entry(key.clone()).or_insert(0) += count;
        }
        
        combined
    }
}

/// Transform AST to WASM-optimized MIR
pub fn ast_to_mir(ast: AstBlock) -> Result<MIR, CompileError> {
    let mut mir = MIR::new();
    let mut context = MirTransformContext::new();

    // First pass: count uses of each place for last-use analysis
    count_ast_uses(&ast, &mut context)?;

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
        context.enter_function(main_function_id);
    }

    // Create control flow graph builder (simplified for now)
    // This will be implemented in later tasks
    
    // Transform all AST nodes to MIR
    let _main_block_id = 0; // Placeholder block ID
    let mut current_block = MirBlock::new(_main_block_id); // Placeholder block

    for node in &ast.ast {
        let statements = transform_ast_node_to_mir(node, &mut context)?;
        for (statement_index, statement) in statements.into_iter().enumerate() {
            // Generate program point and events for each statement
            let program_point = generate_program_point_and_events(&statement, &mut context);
            
            // Add statement with program point to block
            current_block.add_statement_with_program_point(statement, program_point);
            
            // Track program point in function if we have one
            if let Some(function_id) = context.current_function_id {
                if let Some(function) = mir.get_function_mut(function_id) {
                    function.add_program_point(program_point, current_block.id, statement_index);
                }
            }
        }
    }

    // Set terminator for the main block with program point
    let terminator = Terminator::Return { values: vec![] };
    let terminator_point = generate_terminator_program_point(&terminator, &mut context);
    current_block.set_terminator_with_program_point(terminator, terminator_point);
    
    // Add terminator program point to function
    if let Some(function_id) = context.current_function_id {
        if let Some(function) = mir.get_function_mut(function_id) {
            // Add terminator program point (no statement index for terminators)
            function.add_program_point(terminator_point, current_block.id, usize::MAX);
        }
    }

    // Add the block to the current function
    if let Some(function_id) = context.current_function_id {
        if let Some(function) = mir.get_function_mut(function_id) {
            function.add_block(current_block.into());
        }
    }

    if ast.is_entry_point {
        context.exit_function();
    }

    // Build control flow graph between program points
    mir.build_control_flow_graph();

    // Borrow checking will be implemented in later tasks
    // For now, just return the MIR without borrow checking

    // Validate WASM constraints
    mir.validate_wasm_constraints().map_err(|e| {
        CompileError {
            msg: e,
            location: crate::compiler::parsers::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
        }
    })?;

    Ok(mir)
}

/// Transform a single AST node to MIR statements and generate program points/events
fn transform_ast_node_to_mir(
    node: &AstNode,
    context: &mut MirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    match &node.kind {
        NodeKind::Declaration(name, expression, visibility) => {
            transform_declaration_to_mir(name, expression, visibility, context)
        }
        NodeKind::Expression(expression) => {
            // Regular expression - evaluate and potentially assign
            let (statements, _place) = transform_expression_to_mir(expression, context)?;
            Ok(statements)
        }
        NodeKind::Comment(_) | NodeKind::Newline | NodeKind::Spaces(_) | NodeKind::Empty => {
            // These nodes don't generate MIR statements
            Ok(vec![])
        }
        _ => {
            return_compiler_error!(
                "Unsupported AST node type for MIR generation: {:?} at {}:{}",
                node.kind,
                node.location.start_pos.line_number,
                node.location.start_pos.char_column
            )
        }
    }
}

/// Generate program point and events for a statement
fn generate_program_point_and_events(
    statement: &Statement,
    context: &mut MirTransformContext,
) -> ProgramPoint {
    // Allocate the next program point in sequence
    let program_point = context.allocate_program_point();
    
    // Generate events for this statement at this program point
    generate_statement_events(statement, program_point, context);
    
    program_point
}

/// Generate events for a statement at a program point
fn generate_statement_events(
    statement: &Statement,
    program_point: ProgramPoint,
    context: &mut MirTransformContext,
) {
    // Get or create events for this program point
    let mut events = Events::default();
    
    // Extract events based on statement type
    match statement {
        Statement::Assign { place, rvalue } => {
            // Generate events for the rvalue
            generate_rvalue_events(rvalue, program_point, &mut events, context);
            
            // The assignment itself generates a reassign event for the place
            events.reassigns.push(place.clone());
        }
        Statement::Call { args, destination, .. } => {
            // Generate use events for all arguments
            for arg in args {
                generate_operand_events_with_context(arg, program_point, &mut events, context);
            }
            
            // If there's a destination, it gets reassigned
            if let Some(dest_place) = destination {
                events.reassigns.push(dest_place.clone());
            }
        }
        Statement::InterfaceCall { receiver, args, destination, .. } => {
            // Generate use event for receiver
            generate_operand_events_with_context(receiver, program_point, &mut events, context);
            
            // Generate use events for all arguments
            for arg in args {
                generate_operand_events_with_context(arg, program_point, &mut events, context);
            }
            
            // If there's a destination, it gets reassigned
            if let Some(dest_place) = destination {
                events.reassigns.push(dest_place.clone());
            }
        }
        Statement::Drop { place } => {
            // Generate drop event - this is an end-of-lifetime point
            // For now, we'll track this as a use (the place is being accessed to drop it)
            events.uses.push(place.clone());
        }
        Statement::Store { place, value, .. } => {
            // Store operations reassign the place and use the value
            events.reassigns.push(place.clone());
            generate_operand_events_with_context(value, program_point, &mut events, context);
        }
        Statement::Alloc { place, size, .. } => {
            // Allocation reassigns the place and uses the size operand
            events.reassigns.push(place.clone());
            generate_operand_events_with_context(size, program_point, &mut events, context);
        }
        Statement::Dealloc { place } => {
            // Deallocation uses the place (to free it)
            events.uses.push(place.clone());
        }
        Statement::Nop | Statement::MemoryOp { .. } => {
            // These don't generate events for basic borrow checking
        }
    }
    
    // Store events in context for later use by dataflow analysis
    context.store_events(program_point, events);
}

/// Generate events for rvalue operations
fn generate_rvalue_events(
    rvalue: &Rvalue,
    program_point: ProgramPoint,
    events: &mut Events,
    context: &mut MirTransformContext,
) {
    match rvalue {
        Rvalue::Use(operand) => {
            generate_operand_events_with_context(operand, program_point, events, context);
        }
        Rvalue::BinaryOp { left, right, .. } => {
            generate_operand_events_with_context(left, program_point, events, context);
            generate_operand_events_with_context(right, program_point, events, context);
        }
        Rvalue::UnaryOp { operand, .. } => {
            generate_operand_events_with_context(operand, program_point, events, context);
        }
        Rvalue::Cast { source, .. } => {
            generate_operand_events_with_context(source, program_point, events, context);
        }
        Rvalue::Ref { place, borrow_kind } => {
            // Generate start_loan event for borrows
            let loan_id = generate_loan_for_borrow(place, borrow_kind, program_point, context);
            events.start_loans.push(loan_id);
            
            // The place being borrowed is also used (read access)
            events.uses.push(place.clone());
        }
        Rvalue::Deref { place } => {
            // Generate use event for the place being dereferenced
            events.uses.push(place.clone());
        }
        Rvalue::Array { elements, .. } => {
            for element in elements {
                generate_operand_events_with_context(element, program_point, events, context);
            }
        }
        Rvalue::Struct { fields, .. } => {
            for (_, operand) in fields {
                generate_operand_events_with_context(operand, program_point, events, context);
            }
        }
        Rvalue::Load { place, .. } => {
            // Generate use event for the place being loaded
            events.uses.push(place.clone());
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            generate_operand_events_with_context(receiver, program_point, events, context);
            for arg in args {
                generate_operand_events_with_context(arg, program_point, events, context);
            }
        }
        Rvalue::MemorySize => {
            // Memory size doesn't use any places
        }
        Rvalue::MemoryGrow { pages } => {
            generate_operand_events_with_context(pages, program_point, events, context);
        }
    }
}

/// Generate program point for a terminator
fn generate_terminator_program_point(
    terminator: &Terminator,
    context: &mut MirTransformContext,
) -> ProgramPoint {
    // Allocate the next program point in sequence
    let program_point = context.allocate_program_point();
    
    // Generate events for terminator operands
    let mut events = Events::default();
    
    match terminator {
        Terminator::If { condition, .. } => {
            generate_operand_events_with_context(condition, program_point, &mut events, context);
        }
        Terminator::Switch { discriminant, .. } => {
            generate_operand_events_with_context(discriminant, program_point, &mut events, context);
        }
        Terminator::Return { values } => {
            for value in values {
                generate_operand_events_with_context(value, program_point, &mut events, context);
            }
        }
        _ => {
            // Other terminators don't have operands
        }
    }
    
    // Store events for this program point
    context.store_events(program_point, events);
    
    program_point
}

/// Generate a loan for a borrow operation
fn generate_loan_for_borrow(
    place: &Place,
    borrow_kind: &BorrowKind,
    program_point: ProgramPoint,
    context: &mut MirTransformContext,
) -> LoanId {
    let loan_id = context.allocate_loan_id();
    
    let loan = Loan {
        id: loan_id,
        owner: place.clone(),
        kind: borrow_kind.clone(),
        origin_stmt: program_point,
    };
    
    context.add_loan(loan);
    loan_id
}

/// Generate events for operands
fn generate_operand_events(
    operand: &Operand,
    _program_point: ProgramPoint,
    events: &mut Events,
) {
    match operand {
        Operand::Copy(place) => {
            // Generate use event for the place (non-consuming read)
            events.uses.push(place.clone());
        }
        Operand::Move(place) => {
            // Generate move event for the place (consuming read)
            events.moves.push(place.clone());
        }
        Operand::Constant(_) => {
            // Constants don't generate events
        }
        Operand::FunctionRef(_) | Operand::GlobalRef(_) => {
            // References don't generate events
        }
    }
}

/// Generate events for operands with candidate last use tracking
fn generate_operand_events_with_context(
    operand: &Operand,
    _program_point: ProgramPoint,
    events: &mut Events,
    context: &mut MirTransformContext,
) {
    match operand {
        Operand::Copy(place) => {
            // Generate use event for the place (non-consuming read)
            events.uses.push(place.clone());
            
            // Check if this is a candidate last use
            if context.decrement_use_count(place) {
                events.candidate_last_uses.push(place.clone());
            }
        }
        Operand::Move(place) => {
            // Generate move event for the place (consuming read)
            events.moves.push(place.clone());
            
            // Moves are always last uses
            events.candidate_last_uses.push(place.clone());
        }
        Operand::Constant(_) => {
            // Constants don't generate events
        }
        Operand::FunctionRef(_) | Operand::GlobalRef(_) => {
            // References don't generate events
        }
    }
}

/// Transform variable declaration to MIR
fn transform_declaration_to_mir(
    name: &str,
    expression: &Expression,
    visibility: &VarVisibility,
    context: &mut MirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    let mut statements = Vec::new();

    // Transform the expression first
    let (expr_statements, expr_place) = transform_expression_to_mir(expression, context)?;
    statements.extend(expr_statements);

    // Determine if this should be a global or local variable
    let is_global = context.is_global_scope || matches!(visibility, VarVisibility::Exported);

    // Allocate appropriate place for the variable
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

    // Create assignment statement
    let assign_statement = Statement::Assign {
        place: variable_place,
        rvalue: match expr_place {
            Some(place) => Rvalue::Use(Operand::Copy(place)),
            None => {
                // Expression didn't produce a place (e.g., constant)
                // Convert expression to rvalue
                expression_to_rvalue(expression)?
            }
        },
    };

    statements.push(assign_statement);
    Ok(statements)
}

/// Transform expression to MIR statements and return the result place
fn transform_expression_to_mir(
    expression: &Expression,
    context: &mut MirTransformContext,
) -> Result<(Vec<Statement>, Option<Place>), CompileError> {
    match &expression.kind {
        ExpressionKind::Int(_) | ExpressionKind::Float(_) | ExpressionKind::Bool(_) => {
            // Constants don't need places, they're embedded in operands
            Ok((vec![], None))
        }
        ExpressionKind::String(value) => {
            // Strings need memory allocation in linear memory
            let string_place = context.get_place_manager().allocate_heap(
                &expression.data_type,
                value.len() as u32 + 8, // +8 for length prefix
            );
            Ok((vec![], Some(string_place)))
        }
        ExpressionKind::Reference(name) => {
            // Variable reference
            if let Some(place) = context.lookup_variable(name) {
                Ok((vec![], Some(place.clone())))
            } else {
                return_rule_error!(
                    expression.location.clone(),
                    "Undefined variable '{}'. Variable must be declared before use.",
                    name
                )
            }
        }
        ExpressionKind::Runtime(runtime_nodes) => {
            // Transform runtime expression to three-address form
            transform_runtime_expression_to_three_address_form(runtime_nodes, expression, context)
        }
        ExpressionKind::Function(args, body, return_types) => {
            // Function expressions need special handling
            transform_function_expression_to_mir(args, body, return_types, expression, context)
        }
        ExpressionKind::Collection(items) => {
            // Collection expressions need to be broken down
            transform_collection_expression_to_mir(items, expression, context)
        }
        ExpressionKind::Struct(args) => {
            // Struct expressions need to be broken down
            transform_struct_expression_to_mir(args, expression, context)
        }
        _ => {
            return_compiler_error!(
                "Unsupported expression kind for MIR generation: {:?}",
                expression.kind
            )
        }
    }
}

/// Convert expression to rvalue
fn expression_to_rvalue(expression: &Expression) -> Result<Rvalue, CompileError> {
    match &expression.kind {
        ExpressionKind::Int(value) => Ok(Rvalue::Use(Operand::Constant(Constant::I64(*value)))),
        ExpressionKind::Float(value) => Ok(Rvalue::Use(Operand::Constant(Constant::F64(*value)))),
        ExpressionKind::Bool(value) => Ok(Rvalue::Use(Operand::Constant(Constant::Bool(*value)))),
        ExpressionKind::String(value) => Ok(Rvalue::Use(Operand::Constant(Constant::String(
            value.clone(),
        )))),
        _ => {
            return_compiler_error!("Cannot convert expression to rvalue: {:?}", expression.kind)
        }
    }
}

/// Transform runtime expression to three-address form
/// 
/// This function takes a runtime expression (which contains RPN-ordered AST nodes)
/// and breaks it down into separate MIR statements, ensuring each operand read/write
/// is in a separate statement.
/// 
/// Example: `x = foo(y + z*2)` becomes:
/// ```
/// t1 = z * 2
/// t2 = y + t1  
/// t3 = call foo(t2)
/// x = t3
/// ```
fn transform_runtime_expression_to_three_address_form(
    runtime_nodes: &[AstNode],
    expression: &Expression,
    context: &mut MirTransformContext,
) -> Result<(Vec<Statement>, Option<Place>), CompileError> {
    let mut statements = Vec::new();
    let mut operand_stack: Vec<Operand> = Vec::new();
    
    // Process RPN nodes to build three-address form statements
    for node in runtime_nodes {
        match &node.kind {
            NodeKind::Expression(expr) => {
                match &expr.kind {
                    ExpressionKind::Reference(var_name) => {
                        // Variable reference - emit Copy operand initially
                        if let Some(place) = context.lookup_variable(var_name) {
                            let operand = Operand::Copy(place.clone());
                            operand_stack.push(operand);
                        } else {
                            return_rule_error!(
                                expr.location.clone(),
                                "Undefined variable '{}'. Variable must be declared before use.",
                                var_name
                            );
                        }
                    }
                    ExpressionKind::Int(value) => {
                        operand_stack.push(Operand::Constant(Constant::I64(*value)));
                    }
                    ExpressionKind::Float(value) => {
                        operand_stack.push(Operand::Constant(Constant::F64(*value)));
                    }
                    ExpressionKind::Bool(value) => {
                        operand_stack.push(Operand::Constant(Constant::Bool(*value)));
                    }
                    ExpressionKind::String(value) => {
                        operand_stack.push(Operand::Constant(Constant::String(value.clone())));
                    }
                    _ => {
                        return_compiler_error!(
                            "Unsupported expression in runtime nodes: {:?}",
                            expr.kind
                        );
                    }
                }
            }
            NodeKind::Operator(op) => {
                // Operator - pop operands based on operator type, create temporary, push result
                match op {
                    // Binary operators
                    crate::compiler::parsers::expressions::expression::Operator::Add |
                    crate::compiler::parsers::expressions::expression::Operator::Subtract |
                    crate::compiler::parsers::expressions::expression::Operator::Multiply |
                    crate::compiler::parsers::expressions::expression::Operator::Divide |
                    crate::compiler::parsers::expressions::expression::Operator::Modulus |
                    crate::compiler::parsers::expressions::expression::Operator::And => {
                        // Binary operation - pop two operands, create temporary, push result
                        if operand_stack.len() < 2 {
                            return_compiler_error!(
                                "Not enough operands for binary operation: {:?}",
                                op
                            );
                        }
                        
                        let right = operand_stack.pop().unwrap();
                        let left = operand_stack.pop().unwrap();
                        
                        // Create temporary place for result
                        let temp_place = context.get_place_manager().allocate_local(&expression.data_type);
                        
                        // Convert AST operator to MIR BinOp
                        let mir_op = convert_ast_operator_to_mir_binop(op)?;
                        
                        // Create assignment statement
                        let assign_stmt = Statement::Assign {
                            place: temp_place.clone(),
                            rvalue: Rvalue::BinaryOp {
                                op: mir_op,
                                left,
                                right,
                            },
                        };
                        
                        statements.push(assign_stmt);
                        
                        // Push result operand onto stack
                        operand_stack.push(Operand::Copy(temp_place));
                    }
                    // Unary operators would go here if we had any
                    _ => {
                        return_compiler_error!(
                            "Unsupported operator in runtime expression: {:?}",
                            op
                        );
                    }
                }
            }
            NodeKind::FunctionCall(func_name, args, _, _) => {
                // Function call - process arguments and create call statement
                let mut call_args = Vec::new();
                
                // Process arguments (they should already be on the stack from RPN evaluation)
                for _ in 0..args.len() {
                    if operand_stack.is_empty() {
                        return_compiler_error!(
                            "Not enough operands for function call arguments"
                        );
                    }
                    call_args.insert(0, operand_stack.pop().unwrap()); // Insert at front to maintain order
                }
                
                // Create temporary place for result
                let temp_place = context.get_place_manager().allocate_local(&expression.data_type);
                
                // Look up function ID
                let func_id = context.function_names.get(func_name).copied().unwrap_or_else(|| {
                    // If function not found, allocate new ID (for external functions)
                    let id = context.allocate_function_id();
                    context.function_names.insert(func_name.clone(), id);
                    id
                });
                
                // Create call statement
                let call_stmt = Statement::Call {
                    func: Operand::FunctionRef(func_id),
                    args: call_args,
                    destination: Some(temp_place.clone()),
                };
                
                statements.push(call_stmt);
                
                // Push result operand onto stack
                operand_stack.push(Operand::Copy(temp_place));
            }
            _ => {
                return_compiler_error!(
                    "Unsupported AST node in runtime expression: {:?}",
                    node.kind
                );
            }
        }
    }
    
    // The final result should be the last operand on the stack
    let result_place = if operand_stack.len() == 1 {
        match operand_stack.pop().unwrap() {
            Operand::Copy(place) | Operand::Move(place) => Some(place),
            _ => None, // Constants don't have places
        }
    } else if operand_stack.is_empty() {
        None // No result (e.g., void expression)
    } else {
        return_compiler_error!(
            "Runtime expression evaluation left {} operands on stack, expected 1",
            operand_stack.len()
        );
    };
    
    Ok((statements, result_place))
}

/// Transform function expression to MIR
fn transform_function_expression_to_mir(
    _args: &[Arg],
    _body: &[AstNode],
    _return_types: &[DataType],
    _expression: &Expression,
    _context: &mut MirTransformContext,
) -> Result<(Vec<Statement>, Option<Place>), CompileError> {
    // Function expressions will be implemented in later tasks
    return_compiler_error!("Function expressions not yet implemented in MIR generation");
}

/// Transform collection expression to MIR
fn transform_collection_expression_to_mir(
    items: &[Expression],
    expression: &Expression,
    context: &mut MirTransformContext,
) -> Result<(Vec<Statement>, Option<Place>), CompileError> {
    let mut statements = Vec::new();
    let mut element_operands = Vec::new();
    
    // Transform each item to three-address form
    for item in items {
        let (item_statements, item_place) = transform_expression_to_mir(item, context)?;
        statements.extend(item_statements);
        
        // Convert place to operand
        let operand = if let Some(place) = item_place {
            Operand::Copy(place)
        } else {
            // Convert constant expression to operand
            match &item.kind {
                ExpressionKind::Int(value) => Operand::Constant(Constant::I64(*value)),
                ExpressionKind::Float(value) => Operand::Constant(Constant::F64(*value)),
                ExpressionKind::Bool(value) => Operand::Constant(Constant::Bool(*value)),
                ExpressionKind::String(value) => Operand::Constant(Constant::String(value.clone())),
                _ => {
                    return_compiler_error!("Cannot convert collection item to operand: {:?}", item.kind);
                }
            }
        };
        
        element_operands.push(operand);
    }
    
    // Create temporary place for the collection
    let collection_place = context.get_place_manager().allocate_local(&expression.data_type);
    
    // Determine element type (simplified for now)
    let element_type = if !items.is_empty() {
        convert_datatype_to_wasm_type(&items[0].data_type)?
    } else {
        WasmType::I32 // Default for empty collections
    };
    
    // Create array assignment statement
    let array_stmt = Statement::Assign {
        place: collection_place.clone(),
        rvalue: Rvalue::Array {
            elements: element_operands,
            element_type,
        },
    };
    
    statements.push(array_stmt);
    
    Ok((statements, Some(collection_place)))
}

/// Transform struct expression to MIR
fn transform_struct_expression_to_mir(
    args: &[Arg],
    expression: &Expression,
    context: &mut MirTransformContext,
) -> Result<(Vec<Statement>, Option<Place>), CompileError> {
    let mut statements = Vec::new();
    let mut field_operands = Vec::new();
    
    // Transform each field value to three-address form
    for (field_id, arg) in args.iter().enumerate() {
        let (field_statements, field_place) = transform_expression_to_mir(&arg.value, context)?;
        statements.extend(field_statements);
        
        // Convert place to operand
        let operand = if let Some(place) = field_place {
            Operand::Copy(place)
        } else {
            // Convert constant expression to operand
            match &arg.value.kind {
                ExpressionKind::Int(value) => Operand::Constant(Constant::I64(*value)),
                ExpressionKind::Float(value) => Operand::Constant(Constant::F64(*value)),
                ExpressionKind::Bool(value) => Operand::Constant(Constant::Bool(*value)),
                ExpressionKind::String(value) => Operand::Constant(Constant::String(value.clone())),
                _ => {
                    return_compiler_error!("Cannot convert struct field to operand: {:?}", arg.value.kind);
                }
            }
        };
        
        field_operands.push((field_id as u32, operand));
    }
    
    // Create temporary place for the struct
    let struct_place = context.get_place_manager().allocate_local(&expression.data_type);
    
    // Create struct assignment statement
    let struct_stmt = Statement::Assign {
        place: struct_place.clone(),
        rvalue: Rvalue::Struct {
            fields: field_operands,
            struct_type: 0, // Simplified struct type ID for now
        },
    };
    
    statements.push(struct_stmt);
    
    Ok((statements, Some(struct_place)))
}

/// Convert AST binary operator to MIR BinOp
fn convert_ast_operator_to_mir_binop(op: &crate::compiler::parsers::expressions::expression::Operator) -> Result<BinOp, CompileError> {
    use crate::compiler::parsers::expressions::expression::Operator;
    
    match op {
        Operator::Add => Ok(BinOp::Add),
        Operator::Subtract => Ok(BinOp::Sub),
        Operator::Multiply => Ok(BinOp::Mul),
        Operator::Divide => Ok(BinOp::Div),
        Operator::Modulus => Ok(BinOp::Rem),
        Operator::And => Ok(BinOp::And),
        _ => {
            return_compiler_error!("Unsupported binary operator for MIR: {:?}", op);
        }
    }
}

/// Convert AST unary operator to MIR UnOp
fn convert_ast_operator_to_mir_unop(op: &crate::compiler::parsers::expressions::expression::Operator) -> Result<UnOp, CompileError> {
    use crate::compiler::parsers::expressions::expression::Operator;
    
    match op {
        // Note: AST operators might not have direct unary equivalents
        // This is a simplified mapping for now
        _ => {
            return_compiler_error!("Unsupported unary operator for MIR: {:?}", op);
        }
    }
}

/// Convert DataType to WasmType (simplified)
fn convert_datatype_to_wasm_type(data_type: &DataType) -> Result<WasmType, CompileError> {
    match data_type {
        DataType::Int(_) => Ok(WasmType::I64),
        DataType::Float(_) => Ok(WasmType::F64),
        DataType::Bool(_) => Ok(WasmType::I32),
        DataType::String(_) => Ok(WasmType::I32), // Pointer to linear memory
        _ => {
            return_compiler_error!("Cannot convert DataType to WasmType: {:?}", data_type);
        }
    }
}

/// Count uses of variables in AST for last-use analysis
fn count_ast_uses(ast: &AstBlock, context: &mut MirTransformContext) -> Result<(), CompileError> {
    let mut use_counter = UseCounter::new();
    
    // First pass: count all variable references and field/index accesses
    for node in &ast.ast {
        use_counter.count_node_uses(node)?;
    }
    
    // Store use counts in context for later use during MIR generation
    // Note: At this stage we only have variable names, not places yet.
    // The actual place-based counting will happen during MIR transformation.
    context.store_variable_use_counts(use_counter.get_use_counts());
    
    Ok(())
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mir_context_creation() {
        let context = MirTransformContext::new();
        assert!(context.is_global_scope);
        assert_eq!(context.variable_scopes.len(), 1);
    }

    #[test]
    fn test_function_scope_management() {
        let mut context = MirTransformContext::new();

        let func_id = context.allocate_function_id();
        context.enter_function(func_id);

        assert!(!context.is_global_scope);
        assert_eq!(context.current_function_id, Some(func_id));
        assert_eq!(context.variable_scopes.len(), 2);

        context.exit_function();
        assert!(context.is_global_scope);
        assert_eq!(context.current_function_id, None);
        assert_eq!(context.variable_scopes.len(), 1);
    }

    #[test]
    fn test_variable_registration() {
        let mut context = MirTransformContext::new();
        let place = context.get_place_manager().allocate_local(&DataType::Int(
            crate::compiler::datatypes::Ownership::ImmutableOwned(false),
        ));

        context.register_variable("test_var".to_string(), place.clone());

        let found_place = context.lookup_variable("test_var");
        assert!(found_place.is_some());
        assert_eq!(found_place.unwrap(), &place);
    }

    #[test]
    fn test_empty_ast_to_mir() {
        let ast = AstBlock {
            ast: vec![],
            is_entry_point: false,
            scope: std::path::PathBuf::new(),
        };

        let result = ast_to_mir(ast);
        assert!(result.is_ok());

        let mir = result.unwrap();
        assert!(mir.functions.is_empty());
    }

    #[test]
    fn test_entry_point_mir_generation() {
        let ast = AstBlock {
            ast: vec![],
            is_entry_point: true,
            scope: std::path::PathBuf::new(),
        };

        let result = ast_to_mir(ast);
        assert!(result.is_ok());

        let mir = result.unwrap();
        assert_eq!(mir.functions.len(), 1);
        assert_eq!(mir.functions[0].name, "main");
    }

    #[test]
    fn test_program_point_generation() {
        use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
        use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
        use crate::compiler::datatypes::DataType;

        // Create a simple AST with a variable declaration
        let expression = Expression {
            kind: ExpressionKind::Int(42),
            data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
            location: TextLocation::default(),
        };

        let declaration_node = AstNode {
            kind: NodeKind::Declaration("test_var".to_string(), expression, VarVisibility::Private),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let ast = AstBlock {
            ast: vec![declaration_node],
            is_entry_point: true,
            scope: std::path::PathBuf::new(),
        };

        let result = ast_to_mir(ast);
        assert!(result.is_ok());

        let mir = result.unwrap();
        assert_eq!(mir.functions.len(), 1);
        
        let main_function = &mir.functions[0];
        assert!(!main_function.program_points.is_empty());
        
        // Should have at least one program point for the assignment statement
        // and one for the return terminator
        assert!(main_function.program_points.len() >= 2);
        
        // Program points should be sequential
        for i in 1..main_function.program_points.len() {
            assert!(main_function.program_points[i-1].precedes(&main_function.program_points[i]));
        }
    }

    #[test]
    fn test_use_counting() {
        use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
        use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
        use crate::compiler::datatypes::DataType;

        // Create an AST with variable declarations and uses
        let var_decl = Expression {
            kind: ExpressionKind::Int(42),
            data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
            location: TextLocation::default(),
        };

        let var_use1 = Expression {
            kind: ExpressionKind::Reference("test_var".to_string()),
            data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
            location: TextLocation::default(),
        };

        let var_use2 = Expression {
            kind: ExpressionKind::Reference("test_var".to_string()),
            data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
            location: TextLocation::default(),
        };

        let declaration_node = AstNode {
            kind: NodeKind::Declaration("test_var".to_string(), var_decl, VarVisibility::Private),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let use_node1 = AstNode {
            kind: NodeKind::Expression(var_use1),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let use_node2 = AstNode {
            kind: NodeKind::Expression(var_use2),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let ast = AstBlock {
            ast: vec![declaration_node, use_node1, use_node2],
            is_entry_point: true,
            scope: std::path::PathBuf::new(),
        };

        // Test the use counter directly
        let mut use_counter = UseCounter::new();
        for node in &ast.ast {
            use_counter.count_node_uses(node).unwrap();
        }

        let use_counts = use_counter.get_use_counts();
        
        // Should have counted 2 uses of "test_var"
        assert_eq!(use_counts.get("test_var"), Some(&2));
    }

    #[test]
    fn test_use_counting_in_runtime_expressions() {
        use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
        use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
        use crate::compiler::datatypes::DataType;

        // Create a runtime expression that contains variable references
        let var_ref = AstNode {
            kind: NodeKind::Expression(Expression {
                kind: ExpressionKind::Reference("x".to_string()),
                data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
                location: TextLocation::default(),
            }),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let runtime_expr = Expression {
            kind: ExpressionKind::Runtime(vec![var_ref]),
            data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
            location: TextLocation::default(),
        };

        let expr_node = AstNode {
            kind: NodeKind::Expression(runtime_expr),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let ast = AstBlock {
            ast: vec![expr_node],
            is_entry_point: false,
            scope: std::path::PathBuf::new(),
        };

        // Test the use counter
        let mut use_counter = UseCounter::new();
        for node in &ast.ast {
            use_counter.count_node_uses(node).unwrap();
        }

        let use_counts = use_counter.get_use_counts();
        
        // Should have counted 1 use of "x" inside the runtime expression
        assert_eq!(use_counts.get("x"), Some(&1));
    }

    #[test]
    fn test_use_counting_integration() {
        use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
        use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
        use crate::compiler::datatypes::DataType;

        // Create an AST with variable declaration and use
        let var_decl = Expression {
            kind: ExpressionKind::Int(42),
            data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
            location: TextLocation::default(),
        };

        let var_use = Expression {
            kind: ExpressionKind::Reference("test_var".to_string()),
            data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
            location: TextLocation::default(),
        };

        let declaration_node = AstNode {
            kind: NodeKind::Declaration("test_var".to_string(), var_decl, VarVisibility::Private),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let use_node = AstNode {
            kind: NodeKind::Expression(var_use),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let ast = AstBlock {
            ast: vec![declaration_node, use_node],
            is_entry_point: true,
            scope: std::path::PathBuf::new(),
        };

        // Test full AST to MIR transformation
        let result = ast_to_mir(ast);
        assert!(result.is_ok());

        let mir = result.unwrap();
        assert_eq!(mir.functions.len(), 1);
        
        // The MIR should be generated successfully with use counting integrated
        let main_function = &mir.functions[0];
        assert!(!main_function.program_points.is_empty());
    }

    #[test]
    fn test_three_address_form_transformation() {
        use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
        use crate::compiler::parsers::tokens::TextLocation;
        use crate::compiler::datatypes::DataType;

        // Create a runtime expression that represents: x + y * 2
        // In RPN form this would be: [x, y, 2, *, +]
        let x_ref = AstNode {
            kind: NodeKind::Expression(Expression {
                kind: ExpressionKind::Reference("x".to_string()),
                data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
                location: TextLocation::default(),
            }),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let y_ref = AstNode {
            kind: NodeKind::Expression(Expression {
                kind: ExpressionKind::Reference("y".to_string()),
                data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
                location: TextLocation::default(),
            }),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let const_2 = AstNode {
            kind: NodeKind::Expression(Expression {
                kind: ExpressionKind::Int(2),
                data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
                location: TextLocation::default(),
            }),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let multiply_op = AstNode {
            kind: NodeKind::Operator(Operator::Multiply),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let add_op = AstNode {
            kind: NodeKind::Operator(Operator::Add),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        // Create runtime expression in RPN order: x, y, 2, *, +
        let runtime_expr = Expression {
            kind: ExpressionKind::Runtime(vec![x_ref, y_ref, const_2, multiply_op, add_op]),
            data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
            location: TextLocation::default(),
        };

        let mut context = MirTransformContext::new();
        
        // Register variables x and y
        let x_place = context.get_place_manager().allocate_local(&DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)));
        let y_place = context.get_place_manager().allocate_local(&DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)));
        context.register_variable("x".to_string(), x_place);
        context.register_variable("y".to_string(), y_place);

        // Transform the runtime expression to three-address form
        let runtime_nodes = match &runtime_expr.kind {
            ExpressionKind::Runtime(nodes) => nodes,
            _ => panic!("Expected runtime expression"),
        };
        
        let result = transform_runtime_expression_to_three_address_form(
            runtime_nodes,
            &runtime_expr,
            &mut context
        );

        // Should succeed and generate multiple statements
        assert!(result.is_ok());
        let (statements, result_place) = result.unwrap();
        
        // Should generate at least 2 statements:
        // 1. t1 = y * 2
        // 2. t2 = x + t1
        assert!(statements.len() >= 2);
        
        // Should have a result place
        assert!(result_place.is_some());
        
        // Verify the statements are assignment statements
        for statement in &statements {
            match statement {
                Statement::Assign { place: _, rvalue } => {
                    match rvalue {
                        Rvalue::BinaryOp { op: _, left: _, right: _ } => {
                            // This is what we expect for three-address form
                        }
                        _ => {
                            panic!("Expected binary operation in three-address form");
                        }
                    }
                }
                _ => {
                    panic!("Expected assignment statement in three-address form");
                }
            }
        }
    }

    #[test]
    fn test_program_point_generator() {
        let mut generator = ProgramPointGenerator::new();
        
        let point1 = generator.allocate_next();
        let point2 = generator.allocate_next();
        let point3 = generator.allocate_next();
        
        assert_eq!(point1.id(), 0);
        assert_eq!(point2.id(), 1);
        assert_eq!(point3.id(), 2);
        
        assert!(point1.precedes(&point2));
        assert!(point2.precedes(&point3));
        
        assert_eq!(generator.count(), 3);
        
        let all_points = generator.get_all_points();
        assert_eq!(all_points.len(), 3);
        assert_eq!(all_points[0], point1);
        assert_eq!(all_points[1], point2);
        assert_eq!(all_points[2], point3);
    }

    #[test]
    #[ignore]
    fn test_program_point_mapping() {
        let mut function = MirFunction::new(0, "test".to_string(), vec![], vec![]);
        
        let point1 = ProgramPoint::new(0);
        let point2 = ProgramPoint::new(1);
        
        // TODO: Re-implement in task 2
        // function.add_program_point(point1, 0, 0); // block 0, statement 0
        // function.add_program_point(point2, 0, 1); // block 0, statement 1
    }

    #[test]
    #[ignore]
    fn test_comprehensive_program_point_generation() {
        use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
        use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
        use crate::compiler::datatypes::DataType;

        // Create AST with multiple statements
        let expression1 = Expression {
            kind: ExpressionKind::Int(42),
            data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
            location: TextLocation::default(),
        };

        let expression2 = Expression {
            kind: ExpressionKind::Int(100),
            data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
            location: TextLocation::default(),
        };

        let declaration1 = AstNode {
            kind: NodeKind::Declaration("var1".to_string(), expression1, VarVisibility::Private),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let declaration2 = AstNode {
            kind: NodeKind::Declaration("var2".to_string(), expression2, VarVisibility::Private),
            location: TextLocation::default(),
            scope: std::path::PathBuf::new(),
        };

        let ast = AstBlock {
            ast: vec![declaration1, declaration2],
            is_entry_point: true,
            scope: std::path::PathBuf::new(),
        };

        let result = ast_to_mir(ast);
        assert!(result.is_ok());

        let mir = result.unwrap();
        assert_eq!(mir.functions.len(), 1);
        
        let main_function = &mir.functions[0];
        
        // Should have program points for:
        // - 2 assignment statements (var1 and var2)
        // - 1 return terminator
        assert_eq!(main_function.program_points.len(), 3);
        
        // Program points should be sequential
        for i in 1..main_function.program_points.len() {
            assert!(main_function.program_points[i-1].precedes(&main_function.program_points[i]));
        }
        
        // TODO: Re-implement in task 2
        // Check that each program point has proper mapping
        // for (i, &point) in main_function.program_points.iter().enumerate() {
        //     let block_id = main_function.get_block_for_program_point(&point);
        //     assert!(block_id.is_some());
        // }
        
        // Check block program points
        assert_eq!(main_function.blocks.len(), 1);
        let block = &main_function.blocks[0];
        
        // Block should have 2 statement program points
        assert_eq!(block.statement_program_points.len(), 2);
        
        // Block should have 1 terminator program point
        assert!(block.terminator_program_point.is_some());
        
        // All program points in block should be in function's program points
        for &stmt_point in &block.statement_program_points {
            assert!(main_function.program_points.contains(&stmt_point));
        }
        
        if let Some(term_point) = block.terminator_program_point {
            assert!(main_function.program_points.contains(&term_point));
        }
    }

    // TODO: Re-enable these tests in task 2 when event extraction is implemented
    #[test]
    fn test_event_extraction_for_statements() {
        use crate::compiler::mir::mir_nodes::*;
        use crate::compiler::mir::place::*;

        let mut context = MirTransformContext::new();
        
        // Test assignment statement event generation
        let place = Place::local(0, &DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)));
        let operand = Operand::Constant(Constant::I64(42));
        let statement = Statement::Assign {
            place: place.clone(),
            rvalue: Rvalue::Use(operand),
        };
        
        let point = generate_program_point_and_events(&statement, &mut context);
        
        // Should have generated events for the assignment
        let events = context.get_events(&point);
        assert!(events.is_some());
        
        let events = events.unwrap();
        // Assignment should generate a reassign event for the place
        assert_eq!(events.reassigns.len(), 1);
        assert_eq!(events.reassigns[0], place);
        
        // The rvalue is a constant, so no use events should be generated
        assert_eq!(events.uses.len(), 0);
        assert_eq!(events.moves.len(), 0);
        assert_eq!(events.start_loans.len(), 0);
    }

    #[test]
    fn test_event_extraction_for_borrows() {
        use crate::compiler::mir::mir_nodes::*;
        use crate::compiler::mir::place::*;

        let mut context = MirTransformContext::new();
        
        // Test borrow statement event generation
        let borrowed_place = Place::local(0, &DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)));
        let result_place = Place::local(1, &DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)));
        
        let statement = Statement::Assign {
            place: result_place.clone(),
            rvalue: Rvalue::Ref {
                place: borrowed_place.clone(),
                borrow_kind: BorrowKind::Shared,
            },
        };
        
        let point = generate_program_point_and_events(&statement, &mut context);
        
        // Should have generated start_loan and reassign events
        let events = context.get_events(&point);
        assert!(events.is_some());
        
        let events = events.unwrap();
        // Assignment should generate a reassign event for the result place
        assert_eq!(events.reassigns.len(), 1);
        assert_eq!(events.reassigns[0], result_place);
        
        // Borrow should generate a use event for the borrowed place
        assert_eq!(events.uses.len(), 1);
        assert_eq!(events.uses[0], borrowed_place);
        
        // Borrow should generate a start_loan event
        assert_eq!(events.start_loans.len(), 1);
        
        // Check that a loan was created
        let loans = context.get_loans();
        assert_eq!(loans.len(), 1);
        assert_eq!(loans[0].owner, borrowed_place);
        assert_eq!(loans[0].kind, BorrowKind::Shared);
        assert_eq!(loans[0].origin_stmt, point);
    }

    #[test]
    fn test_loan_creation_in_three_address_form() {
        use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
        use crate::compiler::parsers::tokens::TextLocation;
        use crate::compiler::datatypes::DataType;

        let mut context = MirTransformContext::new();
        
        // Register a variable to borrow from
        let borrowed_place = context.get_place_manager().allocate_local(&DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)));
        context.register_variable("x".to_string(), borrowed_place.clone());
        
        // Create a borrow expression: &x
        let borrow_expr = Expression {
            kind: ExpressionKind::Reference("x".to_string()),
            data_type: DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)),
            location: TextLocation::default(),
        };
        
        // Transform to MIR
        let result = transform_expression_to_mir(&borrow_expr, &mut context);
        assert!(result.is_ok());
        
        let (statements, result_place) = result.unwrap();
        
        // Should have a result place for the variable reference
        assert!(result_place.is_some());
        assert_eq!(result_place.unwrap(), borrowed_place);
        
        // For simple variable references, no statements should be generated
        // (the borrow would be generated when the reference is used in a borrow context)
        assert_eq!(statements.len(), 0);
    }

    #[test]
    fn test_event_extraction_for_function_calls() {
        use crate::compiler::mir::mir_nodes::*;
        use crate::compiler::mir::place::*;

        let mut context = MirTransformContext::new();
        
        // Test function call event generation
        let arg_place = Place::local(0, &DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)));
        let result_place = Place::local(1, &DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)));
        
        let statement = Statement::Call {
            func: Operand::FunctionRef(0),
            args: vec![Operand::Copy(arg_place.clone())],
            destination: Some(result_place.clone()),
        };
        
        let point = generate_program_point_and_events(&statement, &mut context);
        
        // Should have generated Use events for arguments and reassign for destination
        let events = context.get_events(&point);
        assert!(events.is_some());
        
        let events = events.unwrap();
        // Function call should generate a use event for the argument
        assert_eq!(events.uses.len(), 1);
        assert_eq!(events.uses[0], arg_place);
        
        // Function call should generate a reassign event for the destination
        assert_eq!(events.reassigns.len(), 1);
        assert_eq!(events.reassigns[0], result_place);
        
        // No moves or loans for this simple call
        assert_eq!(events.moves.len(), 0);
        assert_eq!(events.start_loans.len(), 0);
    }

    #[test]
    fn test_event_extraction_for_moves() {
        use crate::compiler::mir::mir_nodes::*;
        use crate::compiler::mir::place::*;

        let mut context = MirTransformContext::new();
        
        // Test move operation event generation
        let source_place = Place::local(0, &DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)));
        let dest_place = Place::local(1, &DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)));
        
        let statement = Statement::Assign {
            place: dest_place.clone(),
            rvalue: Rvalue::Use(Operand::Move(source_place.clone())),
        };
        
        let point = generate_program_point_and_events(&statement, &mut context);
        
        // Should have generated move and reassign events
        let events = context.get_events(&point);
        assert!(events.is_some());
        
        let events = events.unwrap();
        // Assignment should generate a reassign event for the destination
        assert_eq!(events.reassigns.len(), 1);
        assert_eq!(events.reassigns[0], dest_place);
        
        // Move operand should generate a move event
        assert_eq!(events.moves.len(), 1);
        assert_eq!(events.moves[0], source_place);
        
        // Move should also be a candidate last use
        assert_eq!(events.candidate_last_uses.len(), 1);
        assert_eq!(events.candidate_last_uses[0], source_place);
        
        // No uses or loans for this move
        assert_eq!(events.uses.len(), 0);
        assert_eq!(events.start_loans.len(), 0);
    }

    #[test]
    fn test_event_extraction_for_drops() {
        use crate::compiler::mir::mir_nodes::*;
        use crate::compiler::mir::place::*;

        let mut context = MirTransformContext::new();
        
        // Test drop statement event generation
        let place = Place::local(0, &DataType::Int(crate::compiler::datatypes::Ownership::ImmutableOwned(false)));
        
        let statement = Statement::Drop {
            place: place.clone(),
        };
        
        let point = generate_program_point_and_events(&statement, &mut context);
        
        // Should have generated use event for the dropped place
        let events = context.get_events(&point);
        assert!(events.is_some());
        
        let events = events.unwrap();
        // Drop should generate a use event (accessing the place to drop it)
        assert_eq!(events.uses.len(), 1);
        assert_eq!(events.uses[0], place);
        
        // No reassigns, moves, or loans for drop
        assert_eq!(events.reassigns.len(), 0);
        assert_eq!(events.moves.len(), 0);
        assert_eq!(events.start_loans.len(), 0);
    }

    #[test]
    #[ignore]
    fn test_event_validation_valid_borrow_sequence() {
        // TODO: Re-implement in task 2
        // use crate::compiler::mir::mir_nodes::*;
        
        // TODO: Re-implement in task 2
        // Validation should pass with no errors
        // let errors = borrow_checker.analyze();
    }

    #[test]
    #[ignore]
    fn test_borrow_checker_query_methods() {
        // TODO: Re-implement in task 2
        // use crate::compiler::mir::mir_nodes::*;
    }

    #[test]
    #[ignore]
    fn test_program_point_iteration_utilities() {
        let mut mir = MIR::new();
        
        // Add a function with some program points
        let mut function = MirFunction::new(0, "test".to_string(), vec![], vec![]);
        function.add_program_point(ProgramPoint::new(0), 0, 0);
        function.add_program_point(ProgramPoint::new(1), 0, 1);
        function.add_program_point(ProgramPoint::new(2), 0, usize::MAX);
        
        mir.add_function(function);
        
        // Test iteration utilities
        let all_points = mir.get_all_program_points();
        assert_eq!(all_points.len(), 3);
        
        // Points should be sorted
        for i in 1..all_points.len() {
            assert!(all_points[i-1] < all_points[i]);
        }
        
        // Test iterator
        let iter_points: Vec<_> = mir.iter_program_points().collect();
        assert_eq!(iter_points.len(), 3);
        
        // Test function-specific program points
        let func_points = mir.get_function_program_points(0);
        assert!(func_points.is_some());
        assert_eq!(func_points.unwrap().len(), 3);
        
        // Non-existent function should return None
        let no_func_points = mir.get_function_program_points(999);
        assert!(no_func_points.is_none());
    }
}
