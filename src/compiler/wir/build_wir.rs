//! # WIR Construction Module
//!
//! This module transforms the Abstract Syntax Tree (AST) into WASM Intermediate
//! Representation (WIR) optimized for WASM generation. The WIR provides a simplified,
//! place-based representation that enables efficient borrow checking and direct
//! WASM lowering.
//!
//! ## Design Philosophy
//!
//! The WIR construction follows these principles:
//! - **Place-Based**: All memory locations are represented as places with precise types
//! - **WASM-Optimized**: WIR operations map directly to efficient WASM instruction sequences
//! - **Borrow-Aware**: Generates facts for Polonius-based borrow checking
//! - **Type-Preserving**: Maintains type information throughout the transformation
//!
//! ## Key Transformations
//!
//! - **Variable Declarations**: AST variables → WIR places with lifetime tracking
//! - **Expressions**: AST expressions → WIR rvalues with proper operand handling
//! - **Function Calls**: AST calls → WIR call statements with argument lowering
//! - **Control Flow**: AST if/else → WIR blocks with structured terminators
//!
//! ## Usage
//!
//! ```rust
//! let mut context = WirTransformContext::new();
//! let wir = context.transform_ast_to_wir(&ast)?;
//! ```

// Re-export all WIR components from sibling modules
pub use crate::compiler::wir::place::*;
pub use crate::compiler::wir::wir_nodes::*;

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::statements::branching::MatchArm;
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
use crate::{
    ir_log, return_compiler_error, return_rule_error, return_type_error,
    return_type_mismatch_error, return_undefined_function_error, return_undefined_variable_error,
    return_unimplemented_feature_error,
};
use std::collections::HashMap;

/// Run borrow checking on all functions in the WIR
fn run_borrow_checking_on_wir(wir: &mut WIR) -> Result<(), CompileError> {
    use crate::compiler::wir::borrow_checker::run_unified_borrow_checking;
    use crate::compiler::wir::extract::BorrowFactExtractor;

    for function in &mut wir.functions {
        // Ensure events are generated for all statements and terminators
        regenerate_events_for_function(function);

        // Extract borrow facts from the function
        let mut extractor = BorrowFactExtractor::new();
        extractor.extract_function(function).map_err(|e| {
            CompileError::compiler_error(&format!(
                "Failed to extract borrow facts for function '{}': {}",
                function.name, e
            ))
        })?;

        // Update the function's events with the loans that were created
        extractor.update_function_events(function);

        // Run unified borrow checking
        let borrow_results = run_unified_borrow_checking(function, &extractor).map_err(|e| {
            CompileError::compiler_error(&format!(
                "Borrow checking failed for function '{}': {}",
                function.name, e
            ))
        })?;

        // Handle borrow checking errors with proper diagnostics
        if !borrow_results.errors.is_empty() {
            // Report the first error with detailed context
            let first_error = &borrow_results.errors[0];

            // Create a more detailed error message
            let detailed_message = format!(
                "Borrow checking error in function '{}': {}. This error occurred at program point {}.",
                function.name, first_error.message, first_error.point
            );

            // Use the error location if available, otherwise use a default location
            let error_location = if first_error.location != TextLocation::default() {
                first_error.location.clone()
            } else {
                TextLocation::default() // TODO: Map program points to source locations
            };

            return Err(CompileError::new_rule_error(
                detailed_message,
                error_location,
            ));
        }

        // Log warnings with function context
        for warning in &borrow_results.warnings {
            eprintln!(
                "Borrow checker warning in function '{}': {}",
                function.name, warning.message
            );
        }

        // Log successful borrow checking for debugging
        ir_log!(
            "Borrow checking completed successfully for function '{}'",
            function.name
        );
    }

    Ok(())
}

/// Generate events for all statements in a function during WIR construction
fn generate_events_for_function(function: &mut WirFunction, _context: &mut WirTransformContext) {
    regenerate_events_for_function(function);
}

/// Regenerate events for all statements and terminators in a function
///
/// This function ensures that all WIR statements and terminators have proper
/// event generation for borrow checking. It's called both during WIR construction
/// and before borrow checking to ensure events are up-to-date.
fn regenerate_events_for_function(function: &mut WirFunction) {
    // Clear existing events to regenerate them
    function.events.clear();

    // Collect all events first to avoid borrowing conflicts
    let mut all_events = Vec::new();

    for block in &function.blocks {
        // Generate events for statements
        for (stmt_index, statement) in block.statements.iter().enumerate() {
            let program_point = ProgramPoint::new(block.id * 1000 + stmt_index as u32);
            let events = statement.generate_events();

            // Log event generation for debugging
            ir_log!(
                "Generated events for statement at {}: uses={:?}, moves={:?}, reassigns={:?}, start_loans={:?}",
                program_point,
                events.uses,
                events.moves,
                events.reassigns,
                events.start_loans
            );

            all_events.push((program_point, events));
        }

        // Generate events for terminator
        let terminator_point = ProgramPoint::new(block.id * 1000 + 999);
        let terminator_events = block.terminator.generate_events();

        // Log terminator event generation for debugging
        ir_log!(
            "Generated events for terminator at {}: uses={:?}, moves={:?}, reassigns={:?}, start_loans={:?}",
            terminator_point,
            terminator_events.uses,
            terminator_events.moves,
            terminator_events.reassigns,
            terminator_events.start_loans
        );

        all_events.push((terminator_point, terminator_events));
    }

    // Store all events in the function
    for (program_point, events) in all_events {
        function.store_events(program_point, events);
    }

    ir_log!(
        "Event generation completed for function '{}' with {} program points",
        function.name,
        function.events.len()
    );
}

/// Function information for tracking function metadata and signatures
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub parameters: Vec<(String, DataType)>,
    pub return_type: Option<DataType>,
    pub wasm_function_index: Option<u32>,
    pub wir_function: WirFunction,
}

/// Context for AST-to-WIR transformation with place-based memory management
///
/// The `WirTransformContext` orchestrates the conversion from AST to WIR by maintaining:
/// - **Place Management**: Tracks memory locations and their types
/// - **Scope Tracking**: Manages variable visibility and lifetime scopes
/// - **Function Registry**: Maps function names to WIR function IDs
/// - **Program Points**: Generates unique points for borrow checking analysis
///
/// ## Memory Model
///
/// The context uses a place-based approach where all memory locations are represented
/// as [`Place`] objects with precise type information. This enables:
/// - Efficient WASM local variable allocation
/// - Accurate borrow checking with lifetime analysis
/// - Direct mapping to WASM memory operations
///
/// ## Scope Management
///
/// Variable scopes are managed as a stack of hash maps, allowing proper handling of:
/// - Function parameter scoping
/// - Block-level variable declarations
/// - Variable shadowing and lifetime tracking
#[derive(Debug)]
pub struct WirTransformContext {
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

    /// Host function imports used in this module
    host_imports:
        std::collections::HashSet<crate::compiler::host_functions::registry::HostFunctionDef>,
    /// WASIX function registry for WASIX import mapping
    wasix_registry: crate::compiler::host_functions::wasix_registry::WasixFunctionRegistry,
    /// WASI compatibility layer for automatic migration
    wasi_compatibility: crate::compiler::host_functions::wasi_compatibility::WasiCompatibilityLayer,
    /// Migration diagnostics for WASI usage detection and guidance
    migration_diagnostics:
        crate::compiler::host_functions::migration_diagnostics::MigrationDiagnostics,
    /// Fallback mechanisms for runtime compatibility
    fallback_mechanisms: crate::compiler::host_functions::fallback_mechanisms::FallbackMechanisms,
    /// Pending return operands for the current block
    pending_return: Option<Vec<Operand>>,
}

impl WirTransformContext {
    /// Create a new transformation context
    pub fn new() -> Self {
        use crate::compiler::host_functions::fallback_mechanisms::create_fallback_mechanisms;
        use crate::compiler::host_functions::migration_diagnostics::create_migration_diagnostics;
        use crate::compiler::host_functions::wasi_compatibility::create_wasi_compatibility_layer;
        use crate::compiler::host_functions::wasix_registry::create_wasix_registry;

        let wasix_registry = create_wasix_registry().unwrap_or_default();
        let wasi_compatibility = create_wasi_compatibility_layer(wasix_registry.clone())
            .unwrap_or_else(|_| {
                #[cfg(feature = "verbose_codegen_logging")]
                println!("WIR: Failed to create WASI compatibility layer, using default");
                crate::compiler::host_functions::wasi_compatibility::WasiCompatibilityLayer::new(
                    wasix_registry.clone(),
                )
            });
        let migration_diagnostics = create_migration_diagnostics(wasi_compatibility.clone());
        let fallback_mechanisms = create_fallback_mechanisms(wasi_compatibility.clone());

        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WIR: Created WASIX registry with {} functions",
            wasix_registry.count()
        );

        Self {
            place_manager: PlaceManager::new(),
            variable_scopes: vec![HashMap::new()], // Start with global scope
            function_names: HashMap::new(),
            next_function_id: 0,
            next_block_id: 0,

            host_imports: std::collections::HashSet::new(),
            wasix_registry,
            wasi_compatibility,
            migration_diagnostics,
            fallback_mechanisms,
            pending_return: None,
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

    /// Add a host function to the imports set
    pub fn add_host_import(
        &mut self,
        host_function: crate::compiler::host_functions::registry::HostFunctionDef,
    ) {
        self.host_imports.insert(host_function);
    }

    /// Get all host function imports
    pub fn get_host_imports(
        &self,
    ) -> &std::collections::HashSet<crate::compiler::host_functions::registry::HostFunctionDef>
    {
        &self.host_imports
    }

    /// Get mutable access to migration diagnostics
    pub fn get_migration_diagnostics_mut(
        &mut self,
    ) -> &mut crate::compiler::host_functions::migration_diagnostics::MigrationDiagnostics {
        &mut self.migration_diagnostics
    }

    /// Get migration diagnostics
    pub fn get_migration_diagnostics(
        &self,
    ) -> &crate::compiler::host_functions::migration_diagnostics::MigrationDiagnostics {
        &self.migration_diagnostics
    }

    /// Generate and print migration report if there are any WASI detections
    pub fn print_migration_report(&self) {
        let report = self.migration_diagnostics.generate_migration_report();
        if report.total_wasi_detections > 0 {
            println!("\n{}", report.format_report());
        }
    }

    /// Get mutable access to fallback mechanisms
    pub fn get_fallback_mechanisms_mut(
        &mut self,
    ) -> &mut crate::compiler::host_functions::fallback_mechanisms::FallbackMechanisms {
        &mut self.fallback_mechanisms
    }

    /// Get fallback mechanisms
    pub fn get_fallback_mechanisms(
        &self,
    ) -> &crate::compiler::host_functions::fallback_mechanisms::FallbackMechanisms {
        &self.fallback_mechanisms
    }

    /// Generate and print compatibility report
    pub fn print_compatibility_report(&self) {
        let report = self.fallback_mechanisms.generate_compatibility_report();
        println!("\n{}", report.format_report());
    }

    /// Get similar variable names for error suggestions
    pub fn get_similar_variable_names(&self, target: &str) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Check all scopes for similar names
        for scope in &self.variable_scopes {
            for var_name in scope.keys() {
                if Self::is_similar_name(target, var_name) {
                    suggestions.push(var_name.clone());
                }
            }
        }

        // Limit to top 3 suggestions
        suggestions.truncate(3);
        suggestions
    }

    /// Get similar function names for error suggestions
    pub fn get_similar_function_names(&self, target: &str) -> Vec<String> {
        let mut suggestions = Vec::new();

        for func_name in self.function_names.keys() {
            if Self::is_similar_name(target, func_name) {
                suggestions.push(func_name.clone());
            }
        }

        // Limit to top 3 suggestions
        suggestions.truncate(3);
        suggestions
    }

    /// Check if two names are similar (simple edit distance)
    fn is_similar_name(target: &str, candidate: &str) -> bool {
        // Simple similarity check - could be enhanced with proper edit distance
        if target == candidate {
            return false; // Don't suggest exact matches
        }

        // Check for common typos
        if target.len() == candidate.len() {
            let diff_count = target
                .chars()
                .zip(candidate.chars())
                .filter(|(a, b)| a != b)
                .count();
            return diff_count <= 2; // Allow up to 2 character differences
        }

        // Check for length differences of 1 (insertion/deletion)
        if (target.len() as i32 - candidate.len() as i32).abs() == 1 {
            let (shorter, longer) = if target.len() < candidate.len() {
                (target, candidate)
            } else {
                (candidate, target)
            };

            // Check if shorter is a subsequence of longer
            let mut shorter_chars = shorter.chars();
            let mut current_char = shorter_chars.next();

            for longer_char in longer.chars() {
                if let Some(ch) = current_char {
                    if ch == longer_char {
                        current_char = shorter_chars.next();
                    }
                }
            }

            return current_char.is_none();
        }

        false
    }

    /// Store events for a program point in the current function
    pub fn store_events_for_statement(
        &mut self,
        function: &mut WirFunction,
        program_point: ProgramPoint,
        statement: &Statement,
    ) {
        let events = statement.generate_events();
        function.store_events(program_point, events);
    }

    /// Transform function definition from expression to WIR
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

        // Convert parameters to WIR places and register them
        let mut wir_parameters = Vec::new();
        let mut param_info = Vec::new();

        for param in parameters {
            // Allocate a local place for the parameter
            let param_place = self
                .get_place_manager()
                .allocate_local(&param.value.data_type);

            // Register parameter in current scope
            self.register_variable(param.name.clone(), param_place.clone());

            wir_parameters.push(param_place);
            param_info.push((param.name.clone(), param.value.data_type.clone()));
        }

        // Convert return types to WASM types
        let wasm_return_types: Vec<crate::compiler::wir::place::WasmType> = return_types
            .iter()
            .map(|dt| self.datatype_to_wasm_type(dt))
            .collect::<Result<Vec<_>, _>>()?;

        // Create WIR function
        let mut wir_function = WirFunction::new(
            function_index,
            name.to_string(),
            wir_parameters.clone(),
            wasm_return_types,
        );

        // Transform function body
        let main_block_id = 0;
        let mut current_block = crate::compiler::wir::wir_nodes::WirBlock::new(main_block_id);

        for node in body {
            let statements = transform_ast_node_to_wir(node, self)?;
            for statement in statements {
                current_block.add_statement(statement);
            }
        }

        // Set terminator for the function block
        let terminator = if let Some(return_operands) = self.pending_return.take() {
            crate::compiler::wir::wir_nodes::Terminator::Return {
                values: return_operands,
            }
        } else if return_types.is_empty() {
            crate::compiler::wir::wir_nodes::Terminator::Return { values: vec![] }
        } else {
            // For now, we'll use an empty return - proper return value handling will be added later
            crate::compiler::wir::wir_nodes::Terminator::Return { values: vec![] }
        };
        current_block.set_terminator(terminator);

        // Add the block to the function
        wir_function.add_block(current_block);

        // Add local variables to function
        if let Some(current_scope) = self.variable_scopes.last() {
            for (var_name, var_place) in current_scope {
                if matches!(var_place, crate::compiler::wir::place::Place::Local { .. }) {
                    wir_function.add_local(var_name.clone(), var_place.clone());
                }
            }
        }

        // Generate events for all statements in the function
        generate_events_for_function(&mut wir_function, self);

        self.exit_scope(); // Exit function scope

        Ok(FunctionInfo {
            name: name.to_string(),
            parameters: param_info,
            return_type: return_types.first().cloned(), // For now, support single return type
            wasm_function_index: Some(function_index),
            wir_function,
        })
    }

    /// Transform function definition to WIR
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

        // Convert parameters to WIR places and register them
        let mut wir_parameters = Vec::new();
        let mut param_info = Vec::new();

        for param in parameters {
            // Allocate a local place for the parameter
            let param_place = self
                .get_place_manager()
                .allocate_local(&param.value.data_type);

            // Register parameter in current scope
            self.register_variable(param.name.clone(), param_place.clone());

            wir_parameters.push(param_place);
            param_info.push((param.name.clone(), param.value.data_type.clone()));
        }

        // Convert return types to WASM types
        let wasm_return_types: Vec<crate::compiler::wir::place::WasmType> = return_types
            .iter()
            .map(|dt| self.datatype_to_wasm_type(dt))
            .collect::<Result<Vec<_>, _>>()?;

        // Create WIR function
        let mut wir_function = WirFunction::new(
            function_index,
            name.to_string(),
            wir_parameters.clone(),
            wasm_return_types,
        );

        // Transform function body
        let main_block_id = 0;
        let mut current_block = crate::compiler::wir::wir_nodes::WirBlock::new(main_block_id);

        for node in &body.ast {
            let statements = transform_ast_node_to_wir(node, self)?;
            for statement in statements {
                current_block.add_statement(statement);
            }
        }

        // Set terminator for the function block
        let terminator = if let Some(return_operands) = self.pending_return.take() {
            crate::compiler::wir::wir_nodes::Terminator::Return {
                values: return_operands,
            }
        } else if return_types.is_empty() {
            crate::compiler::wir::wir_nodes::Terminator::Return { values: vec![] }
        } else {
            // For now, we'll use an empty return - proper return value handling will be added later
            crate::compiler::wir::wir_nodes::Terminator::Return { values: vec![] }
        };
        current_block.set_terminator(terminator);

        // Add the block to the function
        wir_function.add_block(current_block);

        // Add local variables to function
        if let Some(current_scope) = self.variable_scopes.last() {
            for (var_name, var_place) in current_scope {
                if matches!(var_place, crate::compiler::wir::place::Place::Local { .. }) {
                    wir_function.add_local(var_name.clone(), var_place.clone());
                }
            }
        }

        // Generate events for all statements in the function
        generate_events_for_function(&mut wir_function, self);

        self.exit_scope(); // Exit function scope

        Ok(FunctionInfo {
            name: name.to_string(),
            parameters: param_info,
            return_type: return_types.first().cloned(), // For now, support single return type
            wasm_function_index: Some(function_index),
            wir_function,
        })
    }

    /// Convert DataType to WasmType
    fn datatype_to_wasm_type(
        &self,
        data_type: &DataType,
    ) -> Result<crate::compiler::wir::place::WasmType, CompileError> {
        use crate::compiler::wir::place::WasmType;

        match data_type {
            DataType::Int(_) => Ok(WasmType::I32),
            DataType::Float(_) => Ok(WasmType::F32),
            DataType::Bool(_) => Ok(WasmType::I32),
            DataType::String(_) => Ok(WasmType::I32), // String pointer
            _ => {
                return_compiler_error!(
                    "DataType {:?} not yet supported for WASM type conversion",
                    data_type
                );
            }
        }
    }
}

/// Transform AST to simplified WIR
///
/// Transform an Abstract Syntax Tree (AST) into Mid-level Intermediate Representation (WIR)
///
/// This is the core transformation function that converts the parsed AST into a place-based
/// WIR suitable for borrow checking and WASM generation. The transformation process:
///
/// ## Two-Pass Algorithm
///
/// 1. **Function Collection Pass**: Identifies all function definitions and builds a registry
///    for forward references and recursive calls
/// 2. **Statement Transformation Pass**: Converts AST nodes to WIR statements with proper
///    place allocation and type tracking
///
/// ## Key Transformations
///
/// - **Variables**: AST variable declarations → WIR places with lifetime tracking
/// - **Expressions**: AST expressions → WIR rvalues with operand lowering
/// - **Functions**: AST function definitions → WIR functions with parameter mapping
/// - **Control Flow**: AST if/else statements → WIR blocks with structured terminators
///
/// ## Memory Safety
///
/// The transformation generates facts for Polonius-based borrow checking, ensuring:
/// - No use-after-move violations
/// - No multiple mutable borrows
/// - Proper lifetime relationships between places
///
/// ## Error Handling
///
/// Returns [`CompileError`] for:
/// - Undefined variable references
/// - Type mismatches in expressions
/// - Invalid function signatures
/// - Unimplemented language features
pub fn ast_to_wir(ast: AstBlock) -> Result<WIR, CompileError> {
    let mut wir = WIR::new();
    let mut context = WirTransformContext::new();
    let mut defined_functions = Vec::new();

    // First pass: collect all function definitions
    for node in &ast.ast {
        if let NodeKind::Declaration(name, expression, _visibility) = &node.kind {
            if let crate::compiler::parsers::expressions::expression::ExpressionKind::Function(
                parameters,
                body,
                return_types,
            ) = &expression.kind
            {
                let function_info = context.transform_function_definition_from_expression(
                    name,
                    parameters,
                    return_types,
                    body,
                )?;
                defined_functions.push(function_info);
            }
        }
    }

    // Add all defined functions to WIR
    for function_info in defined_functions {
        wir.add_function(function_info.wir_function);
    }

    // Create main function if this is an entry point
    if ast.is_entry_point {
        let main_function_id = context.allocate_function_id();
        context
            .function_names
            .insert("main".to_string(), main_function_id);

        let main_function = WirFunction::new(
            main_function_id,
            "main".to_string(),
            vec![], // No parameters
            vec![], // No return values for main
        );

        wir.add_function(main_function);
        context.enter_scope(); // Enter function scope
    }

    // Transform all non-function AST nodes to WIR
    let main_block_id = 0;
    let mut current_block = WirBlock::new(main_block_id);

    for node in &ast.ast {
        // Skip function declarations as they were already processed
        if let NodeKind::Declaration(_, expression, _) = &node.kind {
            if matches!(
                expression.kind,
                crate::compiler::parsers::expressions::expression::ExpressionKind::Function(..)
            ) {
                continue;
            }
        }

        let statements = transform_ast_node_to_wir(node, &mut context)?;

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
    let terminator = if let Some(return_operands) = context.pending_return.take() {
        Terminator::Return {
            values: return_operands,
        }
    } else {
        Terminator::Return { values: vec![] }
    };
    current_block.set_terminator(terminator);

    // Add the block to the current function
    if ast.is_entry_point {
        if let Some(function) = wir.functions.last_mut() {
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

    // Add host function imports to WIR
    wir.add_host_imports(context.get_host_imports());

    // Run borrow checking on all functions
    run_borrow_checking_on_wir(&mut wir)?;

    Ok(wir)
}

/// Transform a single AST node to WIR statements
fn transform_ast_node_to_wir(
    node: &AstNode,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    match &node.kind {
        NodeKind::Declaration(name, expression, visibility) => {
            ast_declaration_to_wir(name, expression, visibility, context)
        }
        NodeKind::Mutation(name, expression) => {
            ast_mutation_to_wir(name, expression, &node.location, context)
        }
        NodeKind::FunctionCall(name, params, return_types, ..) => {
            ast_function_call_to_wir(name, params, return_types, &node.location, context)
        }
        NodeKind::HostFunctionCall(name, params, return_types, ..) => {
            ast_host_function_call_to_wir(name, params, return_types, &node.location, context)
        }
        NodeKind::If(condition, then_body, else_body) => {
            ast_if_statement_to_wir(condition, then_body, else_body, &node.location, context)
        }
        NodeKind::Return(expressions) => {
            ast_return_statement_to_wir(expressions, &node.location, context)
        }
        NodeKind::Expression(expression) => {
            ast_expression_to_wir(expression, &node.location, context)
        }
        NodeKind::Print(expression) => ast_print_to_wir(expression, &node.location, context),
        NodeKind::ForLoop(item, collection, body) => {
            ast_for_loop_to_wir(item, collection, body, &node.location, context)
        }
        NodeKind::WhileLoop(condition, body) => {
            ast_while_loop_to_wir(condition, body, &node.location, context)
        }
        NodeKind::Match(subject, arms, else_body) => {
            ast_match_to_wir(subject, arms, else_body, &node.location, context)
        }
        NodeKind::Warning(message) => ast_warning_to_wir(message, &node.location, context),
        NodeKind::Config(args) => ast_config_to_wir(args, &node.location, context),
        NodeKind::Use(path) => ast_use_to_wir(path, &node.location, context),
        NodeKind::Free(path) => ast_free_to_wir(path, &node.location, context),
        NodeKind::Access => ast_access_to_wir(&node.location, context),
        NodeKind::JS(code) => ast_js_to_wir(code, &node.location, context),
        NodeKind::Css(code) => ast_css_to_wir(code, &node.location, context),
        NodeKind::TemplateFormatter => ast_template_formatter_to_wir(&node.location, context),
        NodeKind::Slot => ast_slot_to_wir(&node.location, context),
        NodeKind::Operator(_) => {
            // Operators should only appear within runtime expressions, not as standalone statements
            return_rule_error!(
                node.location.clone(),
                "Operator cannot be used as a standalone statement. Operators must be part of expressions or assignments."
            );
        }
        NodeKind::Newline | NodeKind::Spaces(_) | NodeKind::Empty => {
            // These nodes don't generate WIR statements
            Ok(vec![])
        }
    }
}

/// Transform variable declaration to WIR
fn ast_declaration_to_wir(
    name: &str,
    expression: &Expression,
    visibility: &VarVisibility,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // Check if this is a function declaration
    if let crate::compiler::parsers::expressions::expression::ExpressionKind::Function(
        parameters,
        body,
        return_types,
    ) = &expression.kind
    {
        // Transform function declaration
        let _function_info = context.transform_function_definition_from_expression(
            name,
            parameters,
            return_types,
            body,
        )?;
        // Function declarations don't generate statements in the current block
        return Ok(vec![]);
    }

    let mut statements = Vec::new();

    // Check if variable is already declared in current scope
    if let Some(current_scope) = context.variable_scopes.last() {
        if current_scope.contains_key(name) {
            return_rule_error!(
                expression.location.clone(),
                "Variable '{}' is already declared in this scope. Shadowing is not supported in Beanstalk - each variable name can only be used once per scope. Try using a different name like '{}_2' or '{}_{}'.",
                name,
                name,
                name,
                "new"
            );
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

/// Transform variable mutation to WIR
fn ast_mutation_to_wir(
    name: &str,
    expression: &Expression,
    location: &TextLocation,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // Look up the existing variable place
    let variable_place = match context.lookup_variable(name) {
        Some(place) => place.clone(),
        None => {
            // Get similar variable names for suggestions
            let suggestions = context.get_similar_variable_names(name);
            if !suggestions.is_empty() {
                return_undefined_variable_error!(location.clone(), name, suggestions);
            } else {
                return_rule_error!(
                    location.clone(),
                    "Cannot mutate undefined variable '{}'. Variable must be declared before mutation. Did you mean to declare it first with 'let {} = ...' or '{}~= ...'?",
                    name,
                    name,
                    name
                );
            }
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

/// Transform function call to WIR
fn ast_function_call_to_wir(
    name: &str,
    params: &[Expression],
    _return_types: &[DataType],
    location: &TextLocation,
    context: &mut WirTransformContext,
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
        // Get similar function names for suggestions
        let suggestions = context.get_similar_function_names(name);
        return_undefined_function_error!(location.clone(), name, suggestions);
    };

    // Create call statement
    let call_statement = Statement::Call {
        func: func_operand,
        args,
        destination: None, // For now, don't handle return values
    };

    Ok(vec![call_statement])
}

/// Transform host function call to WIR
fn ast_host_function_call_to_wir(
    name: &str,
    params: &[Expression],
    return_types: &[DataType],
    _location: &TextLocation,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // Verbose logging for host function call generation
    #[cfg(feature = "verbose_codegen_logging")]
    println!(
        "WIR: Processing host function call '{}' with {} parameters",
        name,
        params.len()
    );

    // Get the host function definition from the registry
    // For now, we'll create a placeholder HostFunctionDef since we need access to the registry
    // This will be properly implemented when the registry is integrated into the context

    // Convert parameters to operands with context for variable references
    let mut args = Vec::new();
    for param in params {
        let operand = expression_to_operand_with_context(param, &param.location, context)?;
        args.push(operand);
    }

    // Create a placeholder host function definition
    // In a complete implementation, this would come from the host function registry
    use crate::compiler::host_functions::registry::HostFunctionDef;
    use crate::compiler::parsers::ast_nodes::Arg;
    use crate::compiler::parsers::expressions::expression::Expression as AstExpression;

    // Create parameter definitions for the host function
    let mut host_params = Vec::new();
    for (i, param) in params.iter().enumerate() {
        let param_arg = Arg {
            name: format!("param_{}", i),
            value: AstExpression::new(
                crate::compiler::parsers::expressions::expression::ExpressionKind::None,
                param.location.clone(),
                param.data_type.clone(),
            ),
        };
        host_params.push(param_arg);
    }

    // Detect runtime environment if not already done
    let current_environment = context
        .get_fallback_mechanisms()
        .get_detected_environment()
        .cloned()
        .unwrap_or_else(|| {
            // Default to mixed environment for now - in a real implementation,
            // this would be detected from available imports
            use crate::compiler::host_functions::fallback_mechanisms::RuntimeEnvironment;
            RuntimeEnvironment::Mixed
        });

    // Check for WASI compatibility and automatic migration using diagnostics and fallbacks
    let (final_name, migration_guidance, fallback_info) =
        if context.wasi_compatibility.is_wasi_function(name) {
            // Detect WASI usage and record for diagnostics
            let guidance = context
                .get_migration_diagnostics_mut()
                .detect_wasi_usage(name, Some("wasi_snapshot_preview1"), _location)
                .unwrap_or(None);

            // Get fallback information for this function in the current environment
            let fallback = context
                .get_fallback_mechanisms()
                .get_fallback_for_function(name, &current_environment)
                .unwrap_or_else(|_| {
                    use crate::compiler::host_functions::fallback_mechanisms::{
                        FunctionFallback, FunctionFallbackType,
                    };
                    FunctionFallback {
                        function_name: name.to_string(),
                        fallback_type: FunctionFallbackType::Unknown,
                        fallback_module: "beanstalk_io".to_string(),
                        fallback_function: name.to_string(),
                        warning_message: Some(format!("Unknown fallback for function '{}'", name)),
                        error_message: None,
                    }
                });

            // This is a WASI function that needs migration
            match context.wasi_compatibility.migrate_function_name(name) {
                Ok(wasix_name) => {
                    #[cfg(feature = "verbose_codegen_logging")]
                    println!(
                        "WIR: Migrating WASI function '{}' to WASIX function '{}'",
                        name, wasix_name
                    );

                    (wasix_name, guidance, Some(fallback))
                }
                Err(_e) => {
                    #[cfg(feature = "verbose_codegen_logging")]
                    println!("WIR: Failed to migrate WASI function '{}': {}", name, _e);

                    // Check if this is a supported function and emit error if configured
                    let _ = context
                        .get_migration_diagnostics()
                        .check_function_support(name, _location);

                    (name.to_string(), guidance, Some(fallback))
                }
            }
        } else {
            // Check fallback for non-WASI functions
            let fallback = context
                .get_fallback_mechanisms()
                .get_fallback_for_function(name, &current_environment)
                .ok();

            (name.to_string(), None, fallback)
        };

    // Emit migration guidance if available
    if let Some(_guidance) = migration_guidance {
        if context.wasi_compatibility.should_emit_warnings() {
            #[cfg(feature = "verbose_codegen_logging")]
            println!(
                "WIR: WASI Migration Guidance:\n{}",
                _guidance.format_message()
            );
        }
    }

    // Handle fallback warnings and errors
    if let Some(ref fallback) = fallback_info {
        if let Some(ref _warning) = fallback.warning_message {
            #[cfg(feature = "verbose_codegen_logging")]
            println!("WIR: Fallback Warning: {}", _warning);
        }

        if let Some(ref _error) = fallback.error_message {
            use crate::compiler::host_functions::fallback_mechanisms::FunctionFallbackType;
            match fallback.fallback_type {
                FunctionFallbackType::Error => {
                    #[cfg(feature = "verbose_codegen_logging")]
                    println!("WIR: Fallback Error: {}", _error);
                    // In a complete implementation, this might return an error
                }
                _ => {
                    #[cfg(feature = "verbose_codegen_logging")]
                    println!("WIR: Fallback Note: {}", _error);
                }
            }
        }
    }

    // Check if there's a WASIX mapping for this function (using potentially migrated name)
    let is_wasix_function = final_name == "print" && context.wasix_registry.has_function("print");

    let host_function = if is_wasix_function {
        // Use WASIX mapping for print function
        let wasix_func = context.wasix_registry.get_function("print").unwrap();

        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WIR: Using WASIX mapping for host function '{}': module={}, name={}",
            final_name, wasix_func.module, wasix_func.name
        );

        // For WASIX functions, we don't add them to host imports because they have
        // different signatures (low-level WASM vs high-level Beanstalk)
        // The WASIX registry handles the import generation separately
        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WIR: Skipping host import for WASIX function '{}' - handled by WASIX registry",
            final_name
        );

        HostFunctionDef::new(
            &final_name,
            host_params,
            return_types.to_vec(),
            &wasix_func.module, // Use WASIX module name
            &wasix_func.name,   // Use WASIX function name
            &format!("Host function: {} (WASIX)", final_name),
        )
    } else {
        // Use legacy module for other functions
        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WIR: Using legacy module for host function '{}'",
            final_name
        );

        let host_func = HostFunctionDef::new(
            &final_name,
            host_params,
            return_types.to_vec(),
            "beanstalk_io", // Default module for legacy functions
            &final_name,    // Use same name for import
            &format!("Host function: {}", final_name),
        );

        // Only add non-WASIX functions to host imports
        context.add_host_import(host_func.clone());

        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WIR: Added host function '{}' to imports, module: {}",
            final_name, host_func.module
        );

        host_func
    };

    // Determine destination place if there's a return value
    let destination = if !return_types.is_empty() {
        // Allocate a local place for the return value
        let return_place = context.get_place_manager().allocate_local(&return_types[0]);
        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WIR: Allocated return place for host function '{}': {:?}",
            final_name, return_place
        );
        Some(return_place)
    } else {
        #[cfg(feature = "verbose_codegen_logging")]
        println!("WIR: Host function '{}' has no return value", final_name);
        None
    };

    // Create appropriate statement based on function type
    let call_statement = if is_wasix_function {
        // For WASIX functions, use WasixCall statement
        #[cfg(feature = "verbose_codegen_logging")]
        println!("WIR: Generated WASIX call statement for '{}'", final_name);

        Statement::WasixCall {
            function_name: final_name.clone(),
            args,
            destination,
        }
    } else {
        // For regular host functions, use HostCall statement
        #[cfg(feature = "verbose_codegen_logging")]
        println!("WIR: Generated host call statement for '{}'", final_name);

        Statement::HostCall {
            function: host_function,
            args,
            destination,
        }
    };

    Ok(vec![call_statement])
}

/// Transform if statement to WIR with proper control flow
///
/// This function creates the WIR representation for if/else statements by:
/// 1. Converting the condition expression to an operand
/// 2. Validating that the condition is boolean type
/// 3. Creating block IDs for then and else branches
/// 4. Generating a Terminator::If for structured control flow
fn ast_if_statement_to_wir(
    condition: &Expression,
    then_body: &AstBlock,
    else_body: &Option<AstBlock>,
    location: &TextLocation,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // Validate that condition is boolean type
    if !matches!(condition.data_type, DataType::Bool(_)) {
        return_type_mismatch_error!(
            condition.location.clone(),
            "Bool",
            &format!("{:?}", condition.data_type),
            "if condition"
        );
    }

    // Convert condition expression to operand
    let _condition_operand =
        expression_to_operand_with_context(condition, &condition.location, context)?;

    // Allocate block IDs for control flow
    let _then_block_id = context.allocate_block_id();
    let _else_block_id = context.allocate_block_id();

    // Suppress unused variable warnings - these will be used when full control flow is implemented
    let _ = (then_body, else_body, location);

    // For now, we'll create a simplified WIR representation
    // In a full implementation, we would need to:
    // 1. Create separate WIR blocks for then/else branches
    // 2. Transform the body statements into those blocks
    // 3. Handle block merging and continuation

    // This is a placeholder implementation that demonstrates the structure
    // The actual block creation and statement transformation will be implemented
    // when we have full block-based WIR construction

    // Create a nop statement as a placeholder for the if logic
    //
    // IMPLEMENTATION NOTE: Full if/else support requires:
    // 1. Transform then_body and else_body into separate WIR blocks
    // 2. Create proper Terminator::If with block references
    // 3. Handle block merging and continuation flow
    //
    // This placeholder maintains compilation compatibility while
    // the control flow system is being implemented.
    let if_placeholder = Statement::Nop;

    Ok(vec![if_placeholder])
}

/// Convert expression to rvalue for basic types
fn expression_to_rvalue(
    expression: &Expression,
    location: &TextLocation,
) -> Result<Rvalue, CompileError> {
    match &expression.kind {
        ExpressionKind::Int(value) => {
            Ok(Rvalue::Use(Operand::Constant(Constant::I32(*value as i32))))
        }
        ExpressionKind::Float(value) => {
            Ok(Rvalue::Use(Operand::Constant(Constant::F32(*value as f32))))
        }
        ExpressionKind::Bool(value) => Ok(Rvalue::Use(Operand::Constant(Constant::Bool(*value)))),
        ExpressionKind::String(value) => Ok(Rvalue::Use(Operand::Constant(Constant::String(
            value.clone(),
        )))),
        ExpressionKind::Reference(name) => {
            return_unimplemented_feature_error!(
                &format!("Variable references in expressions for variable '{}'", name),
                Some(location.clone()),
                Some(
                    "use transform_variable_reference instead or assign the variable to a temporary first"
                )
            );
        }
        ExpressionKind::Runtime(_) => {
            return_unimplemented_feature_error!(
                "Runtime expressions (complex calculations)",
                Some(location.clone()),
                Some("break down complex expressions into simpler assignments")
            );
        }
        ExpressionKind::Template(_) => {
            return_unimplemented_feature_error!(
                "Template expressions in rvalue context",
                Some(location.clone()),
                Some("use expression_to_rvalue_with_context for template support")
            );
        }
        _ => {
            return_compiler_error!(
                "Expression type '{:?}' not yet implemented for rvalue conversion at line {}, column {}. This expression type needs to be added to the WIR generator.",
                expression.kind,
                location.start_pos.line_number,
                location.start_pos.char_column
            )
        }
    }
}

/// Convert expression to rvalue with context for variable references
fn expression_to_rvalue_with_context(
    expression: &Expression,
    location: &TextLocation,
    context: &WirTransformContext,
) -> Result<Rvalue, CompileError> {
    match &expression.kind {
        ExpressionKind::Int(value) => {
            Ok(Rvalue::Use(Operand::Constant(Constant::I32(*value as i32))))
        }
        ExpressionKind::Float(value) => {
            Ok(Rvalue::Use(Operand::Constant(Constant::F32(*value as f32))))
        }
        ExpressionKind::Bool(value) => Ok(Rvalue::Use(Operand::Constant(Constant::Bool(*value)))),
        ExpressionKind::String(value) => Ok(Rvalue::Use(Operand::Constant(Constant::String(
            value.clone(),
        )))),
        ExpressionKind::Reference(name) => {
            // Transform variable reference using context
            transform_variable_reference(name, location, context)
        }
        ExpressionKind::Runtime(runtime_nodes) => {
            // Transform runtime expressions (RPN order) to WIR
            transform_runtime_expression(runtime_nodes, location, context)
        }
        ExpressionKind::Template(template) => {
            // Transform template to WIR statements for string creation
            transform_template_to_rvalue(template, location, context)
        }
        ExpressionKind::None => {
            // None expressions represent parameters without default arguments
            // In the context of function parameters, this indicates the parameter must be provided
            return_compiler_error!(
                "None expression encountered in rvalue context at line {}, column {}. This typically indicates a function parameter without a default argument being used in an invalid context.",
                location.start_pos.line_number,
                location.start_pos.char_column
            );
        }
        _ => {
            return_compiler_error!(
                "Expression type '{:?}' not yet implemented for rvalue conversion at line {}, column {}. This expression type needs to be added to the WIR generator.",
                expression.kind,
                location.start_pos.line_number,
                location.start_pos.char_column
            )
        }
    }
}

/// Transform template to rvalue for string creation
fn transform_template_to_rvalue(
    template: &Template,
    location: &TextLocation,
    context: &WirTransformContext,
) -> Result<Rvalue, CompileError> {
    use crate::compiler::parsers::template::TemplateType;

    match template.kind {
        TemplateType::CompileTimeString => {
            // Template can be folded at compile time - convert to string constant
            let mut folded_template = template.clone();
            let folded_string = folded_template.fold(&None).map_err(|e| {
                CompileError::compiler_error(&format!(
                    "Failed to fold compile-time template: {:?}",
                    e
                ))
            })?;

            Ok(Rvalue::Use(Operand::Constant(Constant::String(
                folded_string,
            ))))
        }
        TemplateType::StringFunction => {
            // Template requires runtime evaluation - generate string concatenation
            transform_runtime_template_to_rvalue(template, location, context)
        }
        TemplateType::Comment => {
            // Comments become empty strings
            Ok(Rvalue::Use(Operand::Constant(Constant::String(
                String::new(),
            ))))
        }
        TemplateType::Slot => {
            // Slots are not valid in expression context
            return_compiler_error!(
                "Template slots cannot be used in expression context at line {}, column {}. Slots are only valid within template bodies.",
                location.start_pos.line_number,
                location.start_pos.char_column
            );
        }
    }
}

/// Transform runtime template to rvalue with string concatenation
fn transform_runtime_template_to_rvalue(
    template: &Template,
    location: &TextLocation,
    context: &WirTransformContext,
) -> Result<Rvalue, CompileError> {
    // Check if template has any variable references that need runtime evaluation
    let has_variable_references = template
        .content
        .flatten()
        .iter()
        .any(|expr| matches!(expr.kind, ExpressionKind::Reference(_)));

    if !has_variable_references {
        // No variables - can concatenate at compile time
        let mut result_parts = Vec::new();

        for expr in template.content.flatten() {
            match &expr.kind {
                ExpressionKind::String(s) => {
                    result_parts.push(s.clone());
                }
                ExpressionKind::Int(i) => {
                    result_parts.push(i.to_string());
                }
                ExpressionKind::Float(f) => {
                    result_parts.push(f.to_string());
                }
                ExpressionKind::Bool(b) => {
                    result_parts.push(b.to_string());
                }
                _ => {
                    return_compiler_error!(
                        "Unsupported expression type in template content: {:?} at line {}, column {}. Only simple values are supported in basic templates.",
                        expr.kind,
                        location.start_pos.line_number,
                        location.start_pos.char_column
                    );
                }
            }
        }

        let concatenated = result_parts.join("");
        return Ok(Rvalue::Use(Operand::Constant(Constant::String(
            concatenated,
        ))));
    }

    // Template has variable references - generate string concatenation operations
    transform_template_with_variable_interpolation(template, location, context)
}

/// Transform template with variable interpolation to string concatenation operations
fn transform_template_with_variable_interpolation(
    template: &Template,
    location: &TextLocation,
    context: &WirTransformContext,
) -> Result<Rvalue, CompileError> {
    // Process both before (head variables) and after (body content) vectors
    let before_parts = &template.content.before;
    let after_parts = &template.content.after;

    // If both vectors are empty, return empty string
    if before_parts.is_empty() && after_parts.is_empty() {
        return Ok(Rvalue::Use(Operand::Constant(Constant::String(
            String::new(),
        ))));
    }

    // Collect all parts in order: before first, then after
    let mut all_parts = Vec::new();
    all_parts.extend(before_parts.iter());
    all_parts.extend(after_parts.iter());

    // If only one part total, convert directly
    if all_parts.len() == 1 {
        return convert_template_expression_to_string_rvalue(all_parts[0], location, context);
    }

    // Multiple parts - generate string concatenation
    // Convert all parts to string operands
    let mut string_operands = Vec::new();
    for expr in &all_parts {
        match convert_template_expression_to_string_operand(expr, location, context)? {
            Some(operand) => string_operands.push(operand),
            None => {
                return_compiler_error!(
                    "Template expression could not be converted to string operand: {:?} at line {}, column {}. Complex expressions in templates are not yet supported.",
                    expr.kind,
                    location.start_pos.line_number,
                    location.start_pos.char_column
                );
            }
        }
    }

    // If we have no operands after conversion, return empty string
    if string_operands.is_empty() {
        return Ok(Rvalue::Use(Operand::Constant(Constant::String(
            String::new(),
        ))));
    }

    // If we have only one operand after conversion, return it directly
    if string_operands.len() == 1 {
        return Ok(Rvalue::Use(string_operands.into_iter().next().unwrap()));
    }

    // For multiple operands, we need to implement string concatenation
    // For now, we'll create a binary operation chain for string concatenation
    // This is a simplified approach - in a full implementation, we'd have a dedicated
    // string concatenation operation or use a more efficient approach

    // Start with the first operand
    let mut result_operand = string_operands[0].clone();

    // Chain binary string concatenation operations for the remaining operands
    for operand in string_operands.iter().skip(1) {
        // Create a binary operation for string concatenation
        // Note: This is a simplified approach. In a real implementation,
        // we would have a dedicated string concatenation operation
        result_operand = Operand::Constant(Constant::String(format!(
            "{}{}",
            extract_string_from_operand(&result_operand)?,
            extract_string_from_operand(operand)?
        )));
    }

    Ok(Rvalue::Use(result_operand))
}

/// Check if a place can be coerced to string at compile time
fn can_coerce_place_to_string_at_compile_time(place: &Place) -> bool {
    // Check if the place represents a type that can be converted to string
    // This function determines whether we can perform string coercion at runtime

    match place.wasm_type() {
        crate::compiler::wir::place::WasmType::I32 | crate::compiler::wir::place::WasmType::I64 => {
            // Integer types can be coerced to strings via runtime conversion
            true
        }
        crate::compiler::wir::place::WasmType::F32 | crate::compiler::wir::place::WasmType::F64 => {
            // Floating point types can be coerced to strings via runtime conversion
            true
        }
        crate::compiler::wir::place::WasmType::ExternRef => {
            // External reference types (strings, objects) can potentially be coerced
            // Strings can be used directly, objects may have string representations
            true
        }
        crate::compiler::wir::place::WasmType::FuncRef => {
            // Function references cannot be coerced to strings
            false
        }
    }
}

/// Generate string coercion operation for a variable place
fn generate_string_coercion_rvalue(
    place: Place,
    location: &TextLocation,
    _context: &WirTransformContext,
) -> Result<Rvalue, CompileError> {
    // Generate appropriate string coercion based on the place's WASM type
    // This creates the proper WIR operations for runtime string conversion

    match place.wasm_type() {
        crate::compiler::wir::place::WasmType::I32 => {
            // For I32 values, we need to generate a string conversion operation
            // In a full implementation, this would call a runtime string conversion function
            // For now, we'll use a copy operation and rely on WASM codegen to handle conversion
            Ok(Rvalue::Use(Operand::Copy(place)))
        }
        crate::compiler::wir::place::WasmType::I64 => {
            // For I64 values, similar to I32
            Ok(Rvalue::Use(Operand::Copy(place)))
        }
        crate::compiler::wir::place::WasmType::F32 => {
            // For F32 values, floating point to string conversion
            Ok(Rvalue::Use(Operand::Copy(place)))
        }
        crate::compiler::wir::place::WasmType::F64 => {
            // For F64 values, floating point to string conversion
            Ok(Rvalue::Use(Operand::Copy(place)))
        }
        crate::compiler::wir::place::WasmType::ExternRef => {
            // For external reference types (strings, objects), check if it's already a string
            // If it's a string reference, use it directly
            // If it's another object type, we need to call its string representation method
            Ok(Rvalue::Use(Operand::Copy(place)))
        }
        crate::compiler::wir::place::WasmType::FuncRef => {
            // Function references cannot be converted to strings
            return_type_error!(
                location.clone(),
                "Function references cannot be converted to string in template context. Functions cannot be used as values in template heads."
            );
        }
    }
}

/// Extract string value from an operand for concatenation
fn extract_string_from_operand(operand: &Operand) -> Result<String, CompileError> {
    match operand {
        Operand::Constant(Constant::String(s)) => Ok(s.clone()),
        Operand::Constant(Constant::I32(i)) => Ok(i.to_string()),
        Operand::Constant(Constant::I64(i)) => Ok(i.to_string()),
        Operand::Constant(Constant::F32(f)) => Ok(f.to_string()),
        Operand::Constant(Constant::F64(f)) => Ok(f.to_string()),
        Operand::Constant(Constant::Bool(b)) => Ok(b.to_string()),
        _ => {
            return_compiler_error!(
                "Cannot extract string from operand: {:?}. Only constant values can be extracted for string concatenation at compile time.",
                operand
            );
        }
    }
}

/// Convert template expression to string rvalue
fn convert_template_expression_to_string_rvalue(
    expr: &Expression,
    location: &TextLocation,
    context: &WirTransformContext,
) -> Result<Rvalue, CompileError> {
    match &expr.kind {
        ExpressionKind::String(s) => {
            Ok(Rvalue::Use(Operand::Constant(Constant::String(s.clone()))))
        }
        ExpressionKind::Int(i) => Ok(Rvalue::Use(Operand::Constant(Constant::String(
            i.to_string(),
        )))),
        ExpressionKind::Float(f) => Ok(Rvalue::Use(Operand::Constant(Constant::String(
            f.to_string(),
        )))),
        ExpressionKind::Bool(b) => Ok(Rvalue::Use(Operand::Constant(Constant::String(
            b.to_string(),
        )))),
        ExpressionKind::Reference(name) => {
            // Look up variable and convert to string
            let variable_place = match context.lookup_variable(name) {
                Some(place) => place.clone(),
                None => {
                    let suggestions = context.get_similar_variable_names(name);
                    return_undefined_variable_error!(location.clone(), name, suggestions);
                }
            };

            // Generate string coercion operation for the variable
            // This creates a runtime string conversion from the variable's value
            generate_string_coercion_rvalue(variable_place, location, context)
        }
        ExpressionKind::Template(nested_template) => {
            // Handle nested templates recursively
            transform_template_to_rvalue(nested_template, location, context)
        }
        ExpressionKind::Runtime(runtime_nodes) => {
            // Handle runtime expressions by transforming them first
            let _runtime_rvalue = transform_runtime_expression(runtime_nodes, location, context)?;

            // Then convert the result to string
            // For now, we'll assume the runtime expression produces a value that can be coerced to string
            // In a full implementation, we'd need to track the type of the runtime expression
            return_compiler_error!(
                "Runtime expressions in templates not yet fully supported at line {}, column {}. Complex calculations in template heads need additional type tracking.",
                location.start_pos.line_number,
                location.start_pos.char_column
            );
        }
        _ => {
            return_compiler_error!(
                "Unsupported expression type in template: {:?} at line {}, column {}. Only simple values, variable references, and nested templates are supported.",
                expr.kind,
                location.start_pos.line_number,
                location.start_pos.char_column
            );
        }
    }
}

/// Convert template expression to string operand (if possible)
fn convert_template_expression_to_string_operand(
    expr: &Expression,
    location: &TextLocation,
    context: &WirTransformContext,
) -> Result<Option<Operand>, CompileError> {
    match &expr.kind {
        ExpressionKind::String(s) => Ok(Some(Operand::Constant(Constant::String(s.clone())))),
        ExpressionKind::Int(i) => Ok(Some(Operand::Constant(Constant::String(i.to_string())))),
        ExpressionKind::Float(f) => Ok(Some(Operand::Constant(Constant::String(f.to_string())))),
        ExpressionKind::Bool(b) => Ok(Some(Operand::Constant(Constant::String(b.to_string())))),
        ExpressionKind::Reference(name) => {
            // Look up variable
            let variable_place = match context.lookup_variable(name) {
                Some(place) => place.clone(),
                None => {
                    let suggestions = context.get_similar_variable_names(name);
                    return_undefined_variable_error!(location.clone(), name, suggestions);
                }
            };

            // Check if the variable's type can be coerced to string
            if !can_coerce_place_to_string_at_compile_time(&variable_place) {
                return_type_error!(
                    location.clone(),
                    "Variable '{}' of type {:?} cannot be converted to string in template context. Only primitive types (int, float, bool) and strings can be used in template heads.",
                    name,
                    variable_place.wasm_type()
                );
            }

            // Return the place for runtime string conversion
            Ok(Some(Operand::Copy(variable_place)))
        }
        ExpressionKind::Template(_nested_template) => {
            // Nested templates need to be processed recursively
            // For now, we'll indicate they can't be converted to simple operands
            Ok(None)
        }
        ExpressionKind::Runtime(_) => {
            // Runtime expressions cannot be converted to simple operands
            Ok(None)
        }
        _ => {
            // Other expression types not supported yet
            Ok(None)
        }
    }
}

/// Convert expression to operand with context for variable references
fn expression_to_operand_with_context(
    expression: &Expression,
    location: &TextLocation,
    context: &WirTransformContext,
) -> Result<Operand, CompileError> {
    match &expression.kind {
        ExpressionKind::Int(value) => Ok(Operand::Constant(Constant::I32(*value as i32))),
        ExpressionKind::Float(value) => Ok(Operand::Constant(Constant::F32(*value as f32))),
        ExpressionKind::Bool(value) => Ok(Operand::Constant(Constant::Bool(*value))),
        ExpressionKind::String(value) => Ok(Operand::Constant(Constant::String(value.clone()))),
        ExpressionKind::Reference(name) => {
            // Transform variable reference to operand
            let rvalue = transform_variable_reference(name, location, context)?;
            match rvalue {
                Rvalue::Use(operand) => Ok(operand),
                _ => return_compiler_error!(
                    "Variable reference '{}' produced non-use rvalue, which is not supported in operand context",
                    name
                ),
            }
        }
        ExpressionKind::Runtime(runtime_nodes) => {
            // For operand context, we need to create a temporary assignment for runtime expressions
            // This is a simplified approach - in a full implementation, we'd integrate this better
            let rvalue = transform_runtime_expression(runtime_nodes, location, context)?;
            match rvalue {
                Rvalue::Use(operand) => Ok(operand),
                _ => return_compiler_error!(
                    "Runtime expression produced non-use rvalue, which is not supported in operand context"
                ),
            }
        }
        ExpressionKind::Template(template) => {
            // For operand context, try to fold the template to constant if possible
            match template.kind {
                crate::compiler::parsers::template::TemplateType::CompileTimeString => {
                    let mut folded_template = template.as_ref().clone();
                    let folded_string = folded_template.fold(&None).map_err(|e| {
                        CompileError::compiler_error(&format!(
                            "Failed to fold compile-time template: {:?}",
                            e
                        ))
                    })?;
                    Ok(Operand::Constant(Constant::String(folded_string)))
                }
                _ => {
                    return_compiler_error!(
                        "Runtime template expressions not supported in operand context at line {}, column {}. Templates should be assigned to variables first.",
                        location.start_pos.line_number,
                        location.start_pos.char_column
                    );
                }
            }
        }
        ExpressionKind::None => {
            // None expressions represent parameters without default arguments
            // In function parameter context, this means the parameter is required and has no default
            // This should not be converted to an operand as it represents the absence of a default value

            return_compiler_error!(
                "None expression encountered in operand context at line {}, column {}. This indicates a function parameter without a default argument, which should not be converted to an operand.",
                location.start_pos.line_number,
                location.start_pos.char_column
            );
        }
        _ => {
            return_compiler_error!(
                "Expression type '{:?}' not yet implemented for function parameters at line {}, column {}. This expression type needs to be added to the WIR generator.",
                expression.kind,
                location.start_pos.line_number,
                location.start_pos.char_column
            )
        }
    }
}

/// Transform variable reference to WIR rvalue with proper usage context
///
/// This method handles variable references by looking up the variable in the context
/// and generating appropriate WIR operands based on usage patterns.
fn transform_variable_reference(
    name: &str,
    location: &TextLocation,
    context: &WirTransformContext,
) -> Result<Rvalue, CompileError> {
    // Look up the variable's place in the context
    let variable_place = match context.lookup_variable(name) {
        Some(place) => place.clone(),
        None => {
            // Get similar variable names for suggestions
            let suggestions = context.get_similar_variable_names(name);
            return_undefined_variable_error!(location.clone(), name, suggestions);
        }
    };

    // TODO: Implement Beanstalk's implicit borrowing semantics
    //
    // BEANSTALK SEMANTICS:
    // - `x = y` should create Rvalue::Ref { place: y, borrow_kind: BorrowKind::Shared }
    // - `x ~= y` should create Rvalue::Ref { place: y, borrow_kind: BorrowKind::Mut }
    // - `x ~= ~y` should create Rvalue::Use(Operand::Move(y))
    //
    // CURRENT STATUS: Using Copy as placeholder until AST supports ~ syntax
    let operand = Operand::Copy(variable_place);

    Ok(Rvalue::Use(operand))
}

/// Transform runtime expressions (RPN order) to WIR rvalue
///
/// Runtime expressions contain AST nodes in Reverse Polish Notation order.
/// This function processes them using a stack-based approach to build WIR binary operations.
fn transform_runtime_expression(
    runtime_nodes: &[AstNode],
    location: &TextLocation,
    context: &WirTransformContext,
) -> Result<Rvalue, CompileError> {
    if runtime_nodes.is_empty() {
        return_compiler_error!(
            "Empty runtime expression at line {}, column {}",
            location.start_pos.line_number,
            location.start_pos.char_column
        );
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
                    return_compiler_error!(
                        "Not enough operands for binary operator {:?} in runtime expression at line {}, column {}",
                        ast_op,
                        node.location.start_pos.line_number,
                        node.location.start_pos.char_column
                    );
                }

                let right = operand_stack.pop().unwrap();
                let left = operand_stack.pop().unwrap();

                // Convert AST operator to WIR BinOp
                let wir_op = ast_operator_to_wir_binop(ast_op, &node.location)?;

                // For now, we'll return the binary operation directly
                // In a more complex implementation, we might need to handle chained operations
                if operand_stack.is_empty() {
                    // This is the final operation
                    return Ok(Rvalue::BinaryOp(wir_op, left, right));
                } else {
                    // This is an intermediate operation - we would need to create temporary assignments
                    // For now, we'll simplify and only handle single binary operations
                    return_compiler_error!(
                        "Complex chained arithmetic expressions not yet implemented. Please break down the expression into simpler assignments at line {}, column {}",
                        node.location.start_pos.line_number,
                        node.location.start_pos.char_column
                    );
                }
            }
            _ => {
                return_compiler_error!(
                    "Unsupported node type in runtime expression: {:?} at line {}, column {}",
                    node.kind,
                    node.location.start_pos.line_number,
                    node.location.start_pos.char_column
                );
            }
        }
    }

    // If we have exactly one operand left, it's a simple value
    if operand_stack.len() == 1 {
        Ok(Rvalue::Use(operand_stack.pop().unwrap()))
    } else {
        return_compiler_error!(
            "Invalid runtime expression - expected single result but got {} operands at line {}, column {}",
            operand_stack.len(),
            location.start_pos.line_number,
            location.start_pos.char_column
        );
    }
}

/// Convert AST Operator to WIR BinOp
fn ast_operator_to_wir_binop(
    ast_op: &crate::compiler::parsers::expressions::expression::Operator,
    location: &TextLocation,
) -> Result<BinOp, CompileError> {
    use crate::compiler::parsers::expressions::expression::Operator;

    match ast_op {
        Operator::Add => Ok(BinOp::Add),
        Operator::Subtract => Ok(BinOp::Sub),
        Operator::Multiply => Ok(BinOp::Mul),
        Operator::Divide => Ok(BinOp::Div),
        Operator::Modulus => Ok(BinOp::Rem),
        Operator::Equality => Ok(BinOp::Eq),
        Operator::Not => Ok(BinOp::Ne),
        Operator::LessThan => Ok(BinOp::Lt),
        Operator::LessThanOrEqual => Ok(BinOp::Le),
        Operator::GreaterThan => Ok(BinOp::Gt),
        Operator::GreaterThanOrEqual => Ok(BinOp::Ge),
        Operator::And => Ok(BinOp::And),
        Operator::Or => Ok(BinOp::Or),
        _ => {
            return_compiler_error!(
                "Operator {:?} not yet implemented for WIR binary operations at line {}, column {}",
                ast_op,
                location.start_pos.line_number,
                location.start_pos.char_column
            );
        }
    }
}

/// Transform return statement to WIR terminator
///
/// Return statements become WIR terminators that end the current basic block.
/// The return values are converted to operands and included in the terminator.
fn ast_return_statement_to_wir(
    expressions: &[Expression],
    location: &TextLocation,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // Convert return value expressions to operands
    let mut return_operands = Vec::new();

    for expr in expressions {
        let operand = expression_to_operand_with_context(expr, location, context)?;
        return_operands.push(operand);
    }

    // Return statements don't generate regular statements, they set the terminator
    // We need to set the terminator on the current block
    // For now, we'll return an empty statement list and handle terminator setting elsewhere
    // This is a limitation of the current architecture where statements and terminators are handled separately

    // TODO: Improve architecture to handle terminators properly
    // For now, we'll create a special statement that indicates a return
    // This will need to be handled specially in the block building logic

    // Since we can't directly set the terminator here, we'll return an empty statement list
    // The terminator will need to be set by the caller or through a different mechanism

    // Actually, let's create a way to signal that this block should end with a return
    // We can do this by storing the return information in the context
    context.pending_return = Some(return_operands);

    Ok(vec![])
}

/// Transform expression statement to WIR
///
/// Expression statements are expressions that are evaluated for their side effects.
/// The result is typically discarded unless it's assigned to a variable.
fn ast_expression_to_wir(
    expression: &Expression,
    location: &TextLocation,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // For expression statements, we evaluate the expression but don't assign the result
    // This is useful for function calls that have side effects but no return value we care about

    match &expression.kind {
        ExpressionKind::Function(..) => {
            // Function expressions should be handled as declarations, not statements
            return_rule_error!(
                location.clone(),
                "Function definitions cannot be used as expression statements. Use 'let function_name = |params| -> ReturnType: body;' syntax instead."
            );
        }
        _ => {
            // Convert expression to rvalue and create a temporary assignment
            // This ensures the expression is evaluated even if the result is discarded
            let rvalue = expression_to_rvalue_with_context(expression, location, context)?;

            // Allocate a temporary place for the result
            let temp_place = context
                .get_place_manager()
                .allocate_local(&expression.data_type);

            // Create assignment to temporary (result will be discarded)
            let assign_statement = Statement::Assign {
                place: temp_place,
                rvalue,
            };

            Ok(vec![assign_statement])
        }
    }
}

/// Transform print statement to WIR host function call
///
/// Print statements are converted to host function calls to the runtime's print function.
fn ast_print_to_wir(
    expression: &Expression,
    location: &TextLocation,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // Convert the expression to an operand
    let operand = expression_to_operand_with_context(expression, location, context)?;

    // Create a host function definition for print
    use crate::compiler::host_functions::registry::HostFunctionDef;
    use crate::compiler::parsers::ast_nodes::Arg;
    use crate::compiler::parsers::expressions::expression::Expression as AstExpression;

    let print_param = Arg {
        name: "value".to_string(),
        value: AstExpression::new(
            crate::compiler::parsers::expressions::expression::ExpressionKind::None,
            location.clone(),
            expression.data_type.clone(),
        ),
    };

    // Check if there's a WASIX mapping for print function
    #[cfg(feature = "verbose_codegen_logging")]
    println!("WIR: Checking WASIX registry for 'print' function...");

    let has_wasix_print = context.wasix_registry.has_function("print");

    #[cfg(feature = "verbose_codegen_logging")]
    {
        println!(
            "WIR: WASIX registry has_function('print') = {}",
            has_wasix_print
        );
        if has_wasix_print {
            let wasix_func = context.wasix_registry.get_function("print").unwrap();
            println!(
                "WIR: Found WASIX function: module={}, name={}",
                wasix_func.module, wasix_func.name
            );
        }
    }

    let print_function = if has_wasix_print {
        // Use WASIX mapping - create a HostFunctionDef that matches the WASIX function
        let wasix_func = context.wasix_registry.get_function("print").unwrap();

        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WIR: Using WASIX mapping for print: module={}, name={}",
            wasix_func.module, wasix_func.name
        );

        // For WASIX functions, we don't add them to host imports because they have
        // different signatures (low-level WASM vs high-level Beanstalk)
        // The WASIX registry handles the import generation separately
        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WIR: Skipping host import for WASIX function 'print' - handled by WASIX registry"
        );

        HostFunctionDef::new(
            "print",
            vec![print_param],
            vec![],             // Print returns void (we handle the WASIX errno internally)
            &wasix_func.module, // Use WASIX module name
            &wasix_func.name,   // Use WASIX function name
            "Print a value to the console using WASIX fd_write",
        )
    } else {
        // Fall back to legacy beanstalk_io import
        #[cfg(feature = "verbose_codegen_logging")]
        println!("WIR: No WASIX mapping found for print, using legacy beanstalk_io");

        let host_func = HostFunctionDef::new(
            "print",
            vec![print_param],
            vec![], // Print returns void
            "beanstalk_io",
            "print",
            "Print a value to the console",
        );

        // Only add non-WASIX functions to host imports
        context.add_host_import(host_func.clone());
        host_func
    };

    // Create appropriate statement based on function type
    let print_statement = if has_wasix_print {
        // For WASIX functions, use WasixCall statement
        // This will be handled specially during WASM generation
        Statement::WasixCall {
            function_name: "print".to_string(),
            args: vec![operand],
            destination: None, // Print has no return value at Beanstalk level
        }
    } else {
        // For regular host functions, use Statement::HostCall
        Statement::HostCall {
            function: print_function,
            args: vec![operand],
            destination: None, // Print has no return value
        }
    };

    Ok(vec![print_statement])
}

/// Transform for loop to WIR control flow
///
/// For loops are converted to WIR blocks with iteration logic.
/// This is a complex transformation that will be implemented incrementally.
fn ast_for_loop_to_wir(
    _item: &Arg,
    _collection: &Expression,
    _body: &AstBlock,
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    return_unimplemented_feature_error!(
        "For loops",
        Some(location.clone()),
        Some("use while loops or manual iteration for now")
    );
}

/// Transform while loop to WIR control flow
///
/// While loops are converted to WIR blocks with conditional branching.
/// This requires proper block management and control flow.
fn ast_while_loop_to_wir(
    _condition: &Expression,
    _body: &AstBlock,
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    return_unimplemented_feature_error!(
        "While loops",
        Some(location.clone()),
        Some("use if statements and manual control flow for now")
    );
}

/// Transform match statement to WIR control flow
///
/// Match statements are converted to WIR blocks with pattern matching logic.
/// This is a complex feature that requires pattern analysis.
fn ast_match_to_wir(
    _subject: &Expression,
    _arms: &[MatchArm],
    _else_body: &Option<AstBlock>,
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    return_unimplemented_feature_error!(
        "Match expressions",
        Some(location.clone()),
        Some("use if-else chains for pattern matching")
    );
}

/// Transform warning node to WIR
///
/// Warning nodes are compiler-generated and don't produce runtime code.
/// They are used for diagnostics and analysis.
fn ast_warning_to_wir(
    message: &str,
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    // Warnings don't generate WIR statements, but we can log them
    #[cfg(feature = "verbose_codegen_logging")]
    println!(
        "WIR: Warning at {}:{}: {}",
        location.start_pos.line_number, location.start_pos.char_column, message
    );

    // Suppress unused variable warnings for release builds
    let _ = (message, location);

    // Return empty statement list - warnings are compile-time only
    Ok(vec![])
}

/// Transform config node to WIR
///
/// Config nodes set compilation parameters and don't generate runtime code.
fn ast_config_to_wir(
    _args: &[Arg],
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    return_unimplemented_feature_error!(
        "Config statements",
        Some(location.clone()),
        Some("config is handled at the module level, not in function bodies")
    );
}

/// Transform use statement to WIR
///
/// Use statements import modules and don't generate runtime code.
/// They are handled at the module level during compilation.
fn ast_use_to_wir(
    _path: &std::path::PathBuf,
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    return_unimplemented_feature_error!(
        "Use statements in function bodies",
        Some(location.clone()),
        Some("use statements should be at module level, not inside functions")
    );
}

/// Transform free statement to WIR
///
/// Free statements are memory management hints inserted by the compiler.
/// They will be converted to appropriate memory deallocation operations.
fn ast_free_to_wir(
    _path: &std::path::PathBuf,
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    return_unimplemented_feature_error!(
        "Explicit memory management (free)",
        Some(location.clone()),
        Some("memory management is automatic in Beanstalk - explicit free is not needed")
    );
}

/// Transform access statement to WIR
///
/// Access statements are used for field access and indexing operations.
fn ast_access_to_wir(
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    return_unimplemented_feature_error!(
        "Access operations",
        Some(location.clone()),
        Some("use direct variable references or implement field access")
    );
}

/// Transform JavaScript code block to WIR
///
/// JavaScript blocks are embedded code that runs in web environments.
/// They don't generate WASM code directly.
fn ast_js_to_wir(
    _code: &str,
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    return_unimplemented_feature_error!(
        "JavaScript code blocks",
        Some(location.clone()),
        Some("JavaScript blocks are handled by the HTML5 codegen, not WASM")
    );
}

/// Transform CSS code block to WIR
///
/// CSS blocks are styling code that doesn't generate WASM instructions.
fn ast_css_to_wir(
    _code: &str,
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    return_unimplemented_feature_error!(
        "CSS code blocks",
        Some(location.clone()),
        Some("CSS blocks are handled by the HTML5 codegen, not WASM")
    );
}

/// Transform template formatter to WIR
///
/// Template formatters are used in template processing and don't generate WASM code.
fn ast_template_formatter_to_wir(
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    return_unimplemented_feature_error!(
        "Template formatters",
        Some(location.clone()),
        Some("template formatters are handled during template processing")
    );
}

/// Transform slot to WIR
///
/// Slots are template placeholders and don't generate WASM code directly.
fn ast_slot_to_wir(
    location: &TextLocation,
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    return_unimplemented_feature_error!(
        "Template slots",
        Some(location.clone()),
        Some("slots are handled during template processing")
    );
}
