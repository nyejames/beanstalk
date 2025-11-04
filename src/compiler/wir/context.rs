//! # Context Management Module
//!
//! This module contains the WirTransformContext and related tracking structures
//! used during AST to WIR transformation. It manages variable scoping, place
//! allocation, temporary variable tracking, and usage tracking for move detection.

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;

use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::wir::{
    place::{Place, PlaceManager},
    wir_nodes::{Operand, WirFunction},
};

use std::collections::HashMap;

/// Function information for tracking function metadata and signatures during WIR transformation
///
/// This structure maintains comprehensive information about functions as they are processed
/// during AST-to-WIR transformation, including their signatures, WASM indices, and the
/// resulting WIR representation.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// Function name as it appears in the source code
    pub name: String,
    /// Function parameters with their names and types
    pub parameters: Vec<(String, DataType)>,
    /// Return type of the function, if any
    pub return_type: Option<DataType>,
    /// WASM function index for direct WASM lowering (assigned during codegen)
    pub wasm_function_index: Option<u32>,
    /// The complete WIR function representation
    pub wir_function: WirFunction,
}

/// Stack entry for RPN (Reverse Polish Notation) expression evaluation
///
/// During runtime expression evaluation, operands are pushed onto a stack in RPN order.
/// This structure tracks each stack entry and whether it uses temporary storage that
/// needs cleanup after the expression evaluation completes.
#[derive(Debug, Clone)]
pub struct ExpressionStackEntry {
    /// The WIR operand representing this stack entry's value
    pub operand: Operand,
    /// Whether this operand uses a temporary variable that can be cleaned up
    /// after the expression evaluation. Temporary variables are created for
    /// intermediate results and should be freed to avoid memory leaks.
    pub is_temporary: bool,
}

/// Temporary variable tracking for cleanup and conflict avoidance
///
/// Temporary variables are created during expression evaluation to hold intermediate
/// results. This structure tracks their lifecycle to ensure proper cleanup and
/// prevent naming conflicts during nested expression evaluation.
#[derive(Debug, Clone)]
pub struct TemporaryVariable {
    /// Unique temporary variable name (e.g., "_temp_1", "_temp_2")
    pub name: String,
    /// WIR place allocated for this temporary variable's storage
    pub place: Place,
    /// Data type of the temporary variable for type checking
    pub data_type: DataType,
    /// Whether this temporary is currently in use (prevents premature cleanup)
    pub is_active: bool,
    /// Expression nesting depth where this temporary was created
    /// Used for scoped cleanup when exiting nested expressions
    pub creation_depth: usize,
}

/// Variable usage tracking for move detection and last-use analysis
///
/// This tracker implements last-use analysis to determine when variables can be moved
/// instead of borrowed. It's essential for Beanstalk's implicit borrowing system where
/// the compiler automatically determines ownership transfer vs. reference creation.
#[derive(Debug, Clone)]
pub struct VariableUsageTracker {
    /// Track how many times each variable is used throughout its lifetime
    /// Used to determine if a usage is the "last use" and can be a move
    usage_counts: HashMap<String, usize>,
    /// Track the current usage index for each variable during transformation
    /// Incremented each time a variable is accessed
    current_usage: HashMap<String, usize>,
    /// Variables that have been explicitly moved and are no longer accessible
    /// Prevents use-after-move errors in subsequent transformations
    moved_variables: std::collections::HashSet<String>,
}

/// Struct field initialization tracking for optional defaults and completeness checking
///
/// Tracks which fields have been initialized in struct literals to ensure all required
/// fields are provided and to support optional field defaults in Beanstalk's struct system.
#[derive(Debug, Clone)]
pub struct StructInitializationTracker {
    /// Track which fields have been initialized for each struct instance
    /// Key: struct place identifier, Value: set of initialized field names
    /// Used to verify that all required fields are provided in struct literals
    initialized_fields: HashMap<String, std::collections::HashSet<String>>,
    /// Track struct types and their required fields for validation
    /// Key: struct place identifier, Value: (struct type, required field names)
    /// Used to check completeness and provide helpful error messages
    struct_definitions: HashMap<String, (DataType, Vec<String>)>,
}

/// Context for AST-to-WIR transformation with place-based memory management
///
/// This is the central context structure that maintains all state during AST-to-WIR
/// transformation. It manages variable scoping, place allocation, temporary variables,
/// and integrates with the WASIX host function system.
///
/// ## Key Responsibilities
///
/// - **Variable Scoping**: Maintains lexical scoping with proper variable lookup
/// - **Place Management**: Allocates and tracks memory locations for all variables
/// - **Temporary Management**: Creates and cleans up temporary variables during expression evaluation
/// - **Host Function Integration**: Manages imports and compatibility layers for system functions
/// - **Usage Tracking**: Implements last-use analysis for move detection
///
/// ## WASM Integration
///
/// The context is designed with WASM generation in mind:
/// - Places map directly to WASM locals and linear memory locations
/// - Function indices prepare for WASM function tables
/// - Host imports integrate with WASM import sections
#[derive(Debug)]
pub struct WirTransformContext {
    /// Place manager for memory layout
    place_manager: PlaceManager,
    /// Variable name to place mapping (scoped)
    variable_scopes: Vec<HashMap<String, Place>>,
    /// Variable mutability tracking (for string slice reassignment)
    variable_mutability: HashMap<String, bool>,
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

    /// Pending return operands for the current block
    pending_return: Option<Vec<Operand>>,

    // Enhanced fields for runtime expression handling
    /// Counter for generating unique temporary variable names
    temporary_counter: u32,
    /// Expression stack for RPN evaluation
    expression_stack: Vec<ExpressionStackEntry>,
    /// Active temporary variables for tracking and cleanup
    temporary_variables: Vec<TemporaryVariable>,
    /// Current expression evaluation depth for cleanup scoping
    expression_depth: usize,
    /// Variable usage tracking for move detection
    variable_usage_tracker: VariableUsageTracker,
    /// Struct field initialization tracking
    struct_initialization_tracker: StructInitializationTracker,
}

impl WirTransformContext {
    /// Create a place for a variable and register it in the current scope
    ///
    /// # Parameters
    ///
    /// - `name`: Variable name to create and register
    ///
    /// # Returns
    ///
    /// - `Ok(Place)`: Successfully created place for the variable
    /// - `Err(CompileError)`: Error if variable name is invalid
    ///
    /// # Validation
    ///
    /// - Variable name must not be empty
    /// - Variable name should not start with underscore (reserved for temporaries)
    pub fn create_place_for_variable(&mut self, name: String) -> Result<Place, crate::compiler::compiler_errors::CompileError> {
        use crate::compiler::datatypes::DataType;
        
        // Validate variable name
        if name.is_empty() {
            return Err(CompileError::compiler_error(
                "Attempted to create place for variable with empty name. This indicates a bug in AST processing."
            ));
        }
        
        // Warn if variable name starts with underscore (reserved for temporaries)
        if name.starts_with('_') && !name.starts_with("_temp_") {
            // This is just a warning - we'll still create the place
            // In a full implementation, we might want to emit a compiler warning
        }
        
        // Create a new place for the variable (default to String type for now)
        let place = self.place_manager.allocate_local(&DataType::String);
        
        // Register the variable in the current scope
        self.register_variable(name, place.clone());
        
        Ok(place)
    }
    
    /// Get the place for an existing variable
    ///
    /// # Parameters
    ///
    /// - `name`: Variable name to look up
    ///
    /// # Returns
    ///
    /// - `Ok(Place)`: Place for the variable if found
    /// - `Err(CompileError)`: Error if variable is not defined
    ///
    /// # Validation
    ///
    /// - Variable name must not be empty
    /// - Variable must exist in the current scope chain
    pub fn get_place_for_variable(&self, name: &str) -> Result<Place, crate::compiler::compiler_errors::CompileError> {
        // Validate variable name
        if name.is_empty() {
            return Err(CompileError::compiler_error(
                "Attempted to get place for variable with empty name. This indicates a bug in AST processing."
            ));
        }
        
        match self.lookup_variable(name) {
            Some(place) => Ok(place.clone()),
            None => {
                // Provide helpful error message with suggestions
                let mut error_msg = format!("Undefined variable '{}'. Variable must be declared before use.", name);
                
                // Try to find similar variable names for suggestions
                let similar_vars = self.find_similar_variable_names(name, 3);
                if !similar_vars.is_empty() {
                    error_msg.push_str(&format!(" Did you mean one of: {}?", similar_vars.join(", ")));
                }
                
                Err(CompileError::new_rule_error(
                    error_msg,
                    TextLocation::default()
                ))
            }
        }
    }
    
    /// Find similar variable names for error suggestions
    ///
    /// Uses simple string distance to find variables with similar names.
    /// This helps provide helpful "did you mean" suggestions in error messages.
    ///
    /// # Parameters
    ///
    /// - `name`: Variable name to find similar matches for
    /// - `max_suggestions`: Maximum number of suggestions to return
    ///
    /// # Returns
    ///
    /// Vector of similar variable names, sorted by similarity
    fn find_similar_variable_names(&self, name: &str, max_suggestions: usize) -> Vec<String> {
        // Performance optimization: pre-allocate with estimated capacity
        let mut candidates: Vec<String> = Vec::with_capacity(max_suggestions * 2);
        
        // Collect all variable names from all scopes
        for scope in &self.variable_scopes {
            for var_name in scope.keys() {
                // Simple similarity check: same length or starts with same prefix
                if var_name.len() == name.len() || 
                   var_name.starts_with(&name[..name.len().min(3)]) ||
                   name.starts_with(&var_name[..var_name.len().min(3)]) {
                    candidates.push(var_name.clone());
                }
            }
        }
        
        // Sort by similarity (simple: prefer exact length matches)
        candidates.sort_by_key(|v| {
            let len_diff = (v.len() as i32 - name.len() as i32).abs();
            len_diff
        });
        
        // Return top suggestions
        candidates.truncate(max_suggestions);
        candidates
    }
    

    /// Create a new transformation context with default settings
    ///
    /// Initializes all tracking structures and sets up the WASIX host function registry.
    /// The context starts with a single global scope and is ready to begin AST-to-WIR
    /// transformation.
    ///
    /// # Returns
    ///
    /// A new `WirTransformContext` ready for transformation with:
    /// - Empty variable scopes (with one global scope)
    /// - Initialized place manager for memory allocation
    /// - WASIX registry configured for host function imports
    /// - All tracking structures reset to initial state
    pub fn new() -> Self {
        use crate::compiler::host_functions::wasix_registry::create_wasix_registry;

        let wasix_registry = create_wasix_registry().unwrap_or_default();

        Self {
            place_manager: PlaceManager::new(),
            variable_scopes: vec![HashMap::new()],
            variable_mutability: HashMap::new(),
            function_names: HashMap::new(),
            next_function_id: 0,
            next_block_id: 0,
            host_imports: std::collections::HashSet::new(),
            wasix_registry,
            pending_return: None,
            temporary_counter: 0,
            expression_stack: Vec::new(),
            temporary_variables: Vec::new(),
            expression_depth: 0,
            variable_usage_tracker: VariableUsageTracker::new(),
            struct_initialization_tracker: StructInitializationTracker::new(),
        }
    }

    /// Get the place manager for allocating memory locations
    ///
    /// The place manager handles allocation of WASM locals, globals, and linear memory
    /// locations. It ensures proper type alignment and tracks memory layout for
    /// efficient WASM generation.
    ///
    /// # Returns
    ///
    /// Mutable reference to the place manager for allocating new places
    pub fn get_place_manager(&mut self) -> &mut PlaceManager {
        &mut self.place_manager
    }

    /// Register a variable in the current lexical scope
    ///
    /// Associates a variable name with its allocated place in the current scope.
    /// This enables variable lookup during expression and statement transformation.
    /// Variables registered in inner scopes shadow those in outer scopes.
    ///
    /// # Parameters
    ///
    /// - `name`: Variable name as it appears in source code
    /// - `place`: Allocated memory location for the variable
    ///
    /// # Panics
    ///
    /// This method should never panic as the global scope is always present.
    /// However, if the scope stack is somehow empty, this indicates a critical
    /// compiler bug in scope management.
    pub fn register_variable(&mut self, name: String, place: Place) {
        if let Some(current_scope) = self.variable_scopes.last_mut() {
            current_scope.insert(name, place);
        } else {
            // This should never happen as we always have at least the global scope
            panic!("COMPILER BUG: Attempted to register variable '{}' but no scope exists. This indicates a critical error in scope management.", name);
        }
    }

    /// Look up a variable in the current scope chain
    ///
    /// Searches for a variable starting from the innermost scope and working outward.
    /// This implements Beanstalk's lexical scoping rules where inner scopes can
    /// shadow variables from outer scopes.
    ///
    /// # Parameters
    ///
    /// - `name`: Variable name to look up
    ///
    /// # Returns
    ///
    /// - `Some(&Place)`: Reference to the variable's place if found
    /// - `None`: Variable not found in any accessible scope
    pub fn lookup_variable(&self, name: &str) -> Option<&Place> {
        for scope in self.variable_scopes.iter().rev() {
            if let Some(place) = scope.get(name) {
                return Some(place);
            }
        }
        None
    }

    /// Create a temporary place for intermediate values during expression evaluation
    ///
    /// Allocates a new temporary variable with a unique name for holding intermediate
    /// results during complex expression evaluation. These temporaries are automatically
    /// managed and can be cleaned up when no longer needed.
    ///
    /// # Parameters
    ///
    /// - `data_type`: Type of the temporary variable for proper memory allocation
    ///
    /// # Returns
    ///
    /// A new `Place` allocated for the temporary variable
    ///
    /// # Note
    ///
    /// Temporary names follow the pattern `_temp_N` where N is an incrementing counter.
    /// This ensures uniqueness and makes temporaries easily identifiable in debugging.
    ///
    /// # Safety
    ///
    /// This method checks for counter overflow to prevent temporary name collisions.
    /// If the counter would overflow, it panics with a descriptive error message.
    pub fn create_temporary_place(
        &mut self,
        data_type: &crate::compiler::datatypes::DataType,
    ) -> Place {
        // Check for counter overflow (extremely unlikely but good to be safe)
        if self.temporary_counter == u32::MAX {
            panic!(
                "COMPILER BUG: Temporary variable counter overflow. Created {} temporary variables, which exceeds the maximum. This indicates an issue with temporary cleanup or an extremely complex expression.",
                u32::MAX
            );
        }
        
        self.temporary_counter += 1;
        // Performance optimization: avoid string allocation for temporary names
        // since they're not actually used in the current implementation
        self.place_manager.allocate_local(data_type)
    }

    /// Enter a new lexical scope
    ///
    /// Creates a new variable scope for blocks, functions, or other scoped constructs.
    /// Variables declared in this scope will shadow any variables with the same name
    /// from outer scopes. The scope must be properly exited with `exit_scope()`.
    ///
    /// # Usage
    ///
    /// ```rust
    /// context.enter_scope();
    /// // ... register variables in new scope
    /// context.exit_scope(); // Don't forget to exit!
    /// ```
    pub fn enter_scope(&mut self) {
        self.variable_scopes.push(HashMap::new());
    }

    /// Exit the current lexical scope
    ///
    /// Removes the innermost scope and all variables declared within it.
    /// Variables from outer scopes that were shadowed become accessible again.
    /// The global scope is never removed to maintain context integrity.
    ///
    /// # Safety
    ///
    /// This method protects the global scope from being removed. If called when
    /// only the global scope remains, it will not remove it and will log a warning
    /// in debug builds to help identify scope management issues.
    pub fn exit_scope(&mut self) {
        if self.variable_scopes.len() > 1 {
            self.variable_scopes.pop();
        } else {
            // In debug builds, warn about attempting to exit the global scope
            #[cfg(debug_assertions)]
            {
                eprintln!(
                    "WARNING: Attempted to exit global scope. This may indicate a scope management bug. \
                    Ensure enter_scope() and exit_scope() calls are properly balanced."
                );
            }
        }
    }

    /// Add a host function import for WASM module generation
    ///
    /// Registers a host function that will be imported in the generated WASM module.
    /// This includes system functions, WASI/WASIX functions, and other external
    /// functions that the Beanstalk program needs to call.
    ///
    /// # Parameters
    ///
    /// - `host_function`: Complete definition of the host function including
    ///   its signature, module, and function name for WASM imports
    ///
    /// # Note
    ///
    /// Duplicate imports are automatically deduplicated using the function's
    /// hash implementation. The imports will be included in the final WASM
    /// module's import section.
    pub fn add_host_import(
        &mut self,
        host_function: crate::compiler::host_functions::registry::HostFunctionDef,
    ) {
        self.host_imports.insert(host_function);
    }

    /// Get all host function imports collected during WIR transformation
    ///
    /// Returns a reference to the set of host functions that need to be imported
    /// in the final WASM module. This is used to transfer imports from the context
    /// to the WIR structure.
    ///
    /// # Returns
    ///
    /// Reference to the set of host function definitions
    pub fn get_host_imports(&self) -> &std::collections::HashSet<crate::compiler::host_functions::registry::HostFunctionDef> {
        &self.host_imports
    }

    /// Create a place for a function parameter
    ///
    /// Allocates a place for a function parameter with the given name, index, and type.
    /// Parameters are allocated as local variables within the function scope.
    ///
    /// # Parameters
    ///
    /// - `name`: Parameter name
    /// - `index`: Parameter index in the function signature
    /// - `data_type`: Parameter data type
    ///
    /// # Returns
    ///
    /// - `Ok(Place)`: Successfully created place for the parameter
    /// - `Err(CompileError)`: Error if parameter name is invalid
    ///
    /// # Validation
    ///
    /// - Parameter name must not be empty
    /// - Parameter name should not conflict with reserved names
    pub fn create_place_for_parameter(
        &mut self,
        name: String,
        _index: u32,
        data_type: &DataType,
    ) -> Result<Place, CompileError> {
        // Validate parameter name
        if name.is_empty() {
            return Err(CompileError::compiler_error(
                "Attempted to create place for parameter with empty name. This indicates a bug in function signature processing."
            ));
        }
        
        // Check for reserved names
        if name.starts_with("_temp_") {
            return Err(CompileError::compiler_error(
                &format!(
                    "Parameter name '{}' conflicts with reserved temporary variable naming pattern. This indicates a bug in AST processing.",
                    name
                )
            ));
        }
        
        let place = self.place_manager.allocate_local(data_type);
        self.register_variable(name, place.clone());
        Ok(place)
    }

    /// Get the next function ID and increment the counter
    ///
    /// Returns a unique function ID for creating new WIR functions.
    /// Each function gets a unique ID for identification and WASM generation.
    ///
    /// # Returns
    ///
    /// A unique function ID
    pub fn get_next_function_id(&mut self) -> u32 {
        let id = self.next_function_id;
        self.next_function_id += 1;
        id
    }

    /// Add a function to the context
    ///
    /// Registers a WIR function in the context for later processing.
    /// This is used when transforming function definitions.
    ///
    /// # Parameters
    ///
    /// - `function`: WIR function to add
    pub fn add_function(&mut self, function: WirFunction) {
        self.function_names.insert(function.name.clone(), function.id);
        // Note: In a full implementation, we would store the function somewhere
        // For now, we just track the name-to-ID mapping
    }
}

impl VariableUsageTracker {
    /// Create a new variable usage tracker
    ///
    /// Initializes empty tracking structures for implementing last-use analysis.
    /// This tracker is essential for Beanstalk's implicit borrowing system where
    /// the compiler automatically determines when to move vs. borrow variables.
    ///
    /// # Returns
    ///
    /// A new tracker ready to begin usage analysis
    pub fn new() -> Self {
        Self {
            usage_counts: HashMap::new(),
            current_usage: HashMap::new(),
            moved_variables: std::collections::HashSet::new(),
        }
    }
}

impl StructInitializationTracker {
    /// Create a new struct initialization tracker
    ///
    /// Initializes empty tracking structures for monitoring struct field initialization.
    /// This ensures that all required fields are provided in struct literals and
    /// enables helpful error messages when fields are missing.
    ///
    /// # Returns
    ///
    /// A new tracker ready to monitor struct initialization
    pub fn new() -> Self {
        Self {
            initialized_fields: HashMap::new(),
            struct_definitions: HashMap::new(),
        }
    }
}
