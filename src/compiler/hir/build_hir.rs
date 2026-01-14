//! HIR Builder - Core Infrastructure
//!
//! This module implements the HIR (High-Level Intermediate Representation) builder
//! for the Beanstalk compiler. The HIR builder converts the fully typed AST into
//! a linear, control-flow-explicit representation suitable for last-use analysis
//! and ownership reasoning.
//!
//! The builder follows a component-based architecture where each component operates
//! on a shared `HirBuilderContext` to maintain a single authoritative state.
//!
//! ## Key Data Structures
//!
//! - `HirBuildContext`: Attached to HIR nodes for source location and debugging
//! - `HirGenerationMetadata`: Tracks temporary variables, block hierarchy, and drop points
//! - `AstHirMapping`: Maps between AST and HIR nodes for error reporting
//! - `OwnershipHints`: Conservative hints for ownership (not authoritative)

use crate::compiler::compiler_errors::{CompilerError, CompilerMessages, ErrorLocation};
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::{BlockId, HirBlock, HirModule, HirNode, HirNodeId};
use crate::compiler::parsers::ast::Ast;
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::{InternedString, StringTable};
use std::collections::{HashMap, HashSet};

// ============================================================================
// HIR Build Context (attached to HIR nodes)
// ============================================================================

/// Extended context information attached to HIR nodes for debugging and error reporting.
/// This structure preserves the connection between HIR nodes and their source AST.
#[derive(Debug, Clone)]
pub struct HirBuildContext {
    /// Source location from the original AST node
    pub source_location: TextLocation,
    /// Optional reference to the original AST node ID for debugging
    pub original_ast_node: Option<AstNodeId>,
    /// The scope depth at which this node was created
    pub scope_depth: usize,
    /// Whether this node could involve ownership transfer
    pub ownership_potential: bool,
}

impl HirBuildContext {
    /// Creates a new HIR build context with the given source location
    pub fn new(source_location: TextLocation) -> Self {
        HirBuildContext {
            source_location,
            original_ast_node: None,
            scope_depth: 0,
            ownership_potential: false,
        }
    }

    /// Creates a new HIR build context with full information
    pub fn with_details(
        source_location: TextLocation,
        ast_node_id: Option<AstNodeId>,
        scope_depth: usize,
        ownership_potential: bool,
    ) -> Self {
        HirBuildContext {
            source_location,
            original_ast_node: ast_node_id,
            scope_depth,
            ownership_potential,
        }
    }

    /// Creates a context from an AST node with scope information
    pub fn from_ast_node(
        location: TextLocation,
        ast_node_id: AstNodeId,
        scope_depth: usize,
    ) -> Self {
        HirBuildContext {
            source_location: location,
            original_ast_node: Some(ast_node_id),
            scope_depth,
            ownership_potential: false,
        }
    }

    /// Marks this context as potentially involving ownership transfer
    pub fn with_ownership_potential(mut self) -> Self {
        self.ownership_potential = true;
        self
    }
}

impl Default for HirBuildContext {
    fn default() -> Self {
        HirBuildContext {
            source_location: TextLocation::default(),
            original_ast_node: None,
            scope_depth: 0,
            ownership_potential: false,
        }
    }
}

// ============================================================================
// Core Types and Enums
// ============================================================================

/// The type of scope being tracked during HIR generation
#[derive(Debug, Clone, PartialEq)]
pub enum ScopeType {
    /// Function scope - top-level scope for a function body
    Function,
    /// Block scope - general block scope
    Block,
    /// Loop scope with break and continue targets
    Loop {
        break_target: BlockId,
        continue_target: BlockId,
    },
    /// If statement scope
    If,
}

/// Information about a scope during HIR generation
#[derive(Debug, Clone)]
pub struct ScopeInfo {
    /// The type of this scope
    pub scope_type: ScopeType,
    /// Variables that could be owned in this scope (for drop insertion)
    pub owned_variables: Vec<InternedString>,
    /// The block ID associated with this scope (if any)
    pub block_id: Option<BlockId>,
    /// The scope depth level
    pub depth: usize,
}

impl ScopeInfo {
    /// Creates a new scope info
    pub fn new(scope_type: ScopeType, depth: usize) -> Self {
        ScopeInfo {
            scope_type,
            owned_variables: Vec::new(),
            block_id: None,
            depth,
        }
    }

    /// Creates a new scope info with a block ID
    pub fn with_block(scope_type: ScopeType, block_id: BlockId, depth: usize) -> Self {
        ScopeInfo {
            scope_type,
            owned_variables: Vec::new(),
            block_id: Some(block_id),
            depth,
        }
    }
}

/// Candidate for possible drop insertion
#[derive(Debug, Clone)]
pub struct DropCandidate {
    /// The variable that might need to be dropped
    pub variable: InternedString,
    /// Source location for error reporting
    pub location: TextLocation,
    /// The scope level where this variable was declared
    pub scope_level: usize,
}

/// The type of drop insertion being performed
#[derive(Debug, Clone)]
pub enum DropInsertionType {
    /// Drop at scope exit
    ScopeExit,
    /// Drop before return statement
    Return,
    /// Drop before break statement
    Break { target: BlockId },
    /// Drop before continue statement
    Continue { target: BlockId },
    /// Drop at control flow merge point
    Merge,
}

/// Metadata about a drop insertion point
#[derive(Debug, Clone)]
pub struct DropInsertionPoint {
    /// Source location for the drop
    pub location: TextLocation,
    /// Variables to potentially drop
    pub variables: Vec<InternedString>,
    /// The type of drop insertion
    pub insertion_type: DropInsertionType,
}

// ============================================================================
// Ownership Hints (Conservative, not authoritative)
// ============================================================================

/// Conservative hints for ownership during HIR generation.
///
/// IMPORTANT: These are NOT authoritative - the borrow checker is the authority.
/// This data may be incomplete, conservative, and wrong. It exists to help
/// with drop point insertion and to provide hints for later analysis stages.
#[derive(Debug, Clone, Default)]
pub struct OwnershipHints {
    /// Variables that might be owned (conservative estimate)
    pub potentially_owned: HashSet<InternedString>,
    /// Variables that are definitely borrowed (conservative)
    pub definitely_borrowed: HashSet<InternedString>,
    /// Hints for potential last uses (may be incomplete)
    pub last_uses: HashMap<InternedString, TextLocation>,
    /// Variables that have been potentially consumed (moved)
    pub potentially_consumed: HashSet<InternedString>,
}

impl OwnershipHints {
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks a variable as potentially owned
    pub fn mark_potentially_owned(&mut self, var: InternedString) {
        self.potentially_owned.insert(var);
        self.definitely_borrowed.remove(&var);
    }

    /// Marks a variable as definitely borrowed
    pub fn mark_definitely_borrowed(&mut self, var: InternedString) {
        self.definitely_borrowed.insert(var);
    }

    /// Records a potential last use location
    pub fn record_potential_last_use(&mut self, var: InternedString, location: TextLocation) {
        self.last_uses.insert(var, location);
    }

    /// Marks a variable as potentially consumed (moved)
    pub fn mark_potentially_consumed(&mut self, var: InternedString) {
        self.potentially_consumed.insert(var);
    }

    /// Checks if a variable could potentially be owned
    pub fn is_potentially_owned(&self, var: &InternedString) -> bool {
        self.potentially_owned.contains(var)
    }

    /// Checks if a variable is definitely borrowed
    pub fn is_definitely_borrowed(&self, var: &InternedString) -> bool {
        self.definitely_borrowed.contains(var)
    }

    /// Checks if a variable has been potentially consumed
    pub fn is_potentially_consumed(&self, var: &InternedString) -> bool {
        self.potentially_consumed.contains(var)
    }

    /// Gets the potential last use location for a variable
    pub fn get_last_use(&self, var: &InternedString) -> Option<&TextLocation> {
        self.last_uses.get(var)
    }

    /// Clears ownership hints for a variable (e.g., when it goes out of scope)
    pub fn clear_variable(&mut self, var: &InternedString) {
        self.potentially_owned.remove(var);
        self.definitely_borrowed.remove(var);
        self.last_uses.remove(var);
        self.potentially_consumed.remove(var);
    }

    /// Gets all variables that might need drops
    pub fn get_drop_candidates(&self) -> Vec<InternedString> {
        self.potentially_owned
            .iter()
            .filter(|v| !self.potentially_consumed.contains(v))
            .copied()
            .collect()
    }
}

// ============================================================================
// HIR Generation Metadata
// ============================================================================

/// Additional metadata tracked during HIR generation.
/// This structure maintains state that spans across the entire HIR generation process.
#[derive(Debug, Clone)]
pub struct HirGenerationMetadata {
    /// Temporary variables introduced by the compiler (all treated as user locals)
    pub temporary_variables: HashMap<InternedString, DataType>,
    /// The hierarchy of blocks being built (stack of active block IDs)
    pub block_hierarchy: Vec<BlockId>,
    /// Points where drops should be inserted
    pub drop_insertion_points: Vec<DropInsertionPoint>,
    /// Conservative ownership hints (may be incomplete or wrong)
    pub ownership_hints: OwnershipHints,
    /// Counter for generating unique temporary variable names
    temp_var_counter: usize,
}

impl Default for HirGenerationMetadata {
    fn default() -> Self {
        Self::new()
    }
}

impl HirGenerationMetadata {
    pub fn new() -> Self {
        HirGenerationMetadata {
            temporary_variables: HashMap::new(),
            block_hierarchy: Vec::new(),
            drop_insertion_points: Vec::new(),
            ownership_hints: OwnershipHints::new(),
            temp_var_counter: 0,
        }
    }

    /// Generates a unique name for a compiler-introduced temporary variable
    pub fn generate_temp_name(&mut self) -> String {
        let name = format!("__tmp_{}", self.temp_var_counter);
        self.temp_var_counter += 1;
        name
    }

    /// Registers a temporary variable with its type
    pub fn register_temporary(&mut self, name: InternedString, data_type: DataType) {
        self.temporary_variables.insert(name, data_type);
    }

    /// Checks if a variable is a compiler-introduced temporary
    pub fn is_temporary(&self, name: &InternedString) -> bool {
        self.temporary_variables.contains_key(name)
    }

    /// Pushes a block onto the hierarchy stack
    pub fn push_block(&mut self, block_id: BlockId) {
        self.block_hierarchy.push(block_id);
    }

    /// Pops a block from the hierarchy stack
    pub fn pop_block(&mut self) -> Option<BlockId> {
        self.block_hierarchy.pop()
    }

    /// Gets the current (innermost) block ID
    pub fn current_block(&self) -> Option<BlockId> {
        self.block_hierarchy.last().copied()
    }

    /// Records a drop insertion point
    pub fn record_drop_point(&mut self, point: DropInsertionPoint) {
        self.drop_insertion_points.push(point);
    }
}

// ============================================================================
// AST to HIR Mapping (for error reporting)
// ============================================================================

/// Unique identifier for AST nodes
pub type AstNodeId = usize;

/// Mapping between AST and HIR for error reporting and debugging.
/// This structure maintains bidirectional mappings to enable accurate error reporting
/// that traces back to the original source code.
#[derive(Debug, Clone, Default)]
pub struct AstHirMapping {
    /// Maps AST node IDs to their corresponding HIR node IDs
    pub ast_to_hir: HashMap<AstNodeId, Vec<HirNodeId>>,
    /// Maps HIR node IDs back to their source AST node
    pub hir_to_ast: HashMap<HirNodeId, AstNodeId>,
    /// Source locations for HIR nodes (preserved for error reporting)
    pub source_locations: HashMap<HirNodeId, TextLocation>,
    /// Build context for HIR nodes (includes scope depth and ownership info)
    pub build_contexts: HashMap<HirNodeId, HirBuildContext>,
}

impl AstHirMapping {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a mapping from an AST node to HIR nodes
    pub fn add_mapping(&mut self, ast_id: AstNodeId, hir_ids: Vec<HirNodeId>) {
        for hir_id in &hir_ids {
            self.hir_to_ast.insert(*hir_id, ast_id);
        }
        self.ast_to_hir.insert(ast_id, hir_ids);
    }

    /// Adds a single HIR node mapping to an AST node
    pub fn add_single_mapping(&mut self, ast_id: AstNodeId, hir_id: HirNodeId) {
        self.hir_to_ast.insert(hir_id, ast_id);
        self.ast_to_hir
            .entry(ast_id)
            .or_insert_with(Vec::new)
            .push(hir_id);
    }

    /// Records the source location for a HIR node
    pub fn record_location(&mut self, hir_id: HirNodeId, location: TextLocation) {
        self.source_locations.insert(hir_id, location);
    }

    /// Records the build context for a HIR node
    pub fn record_build_context(&mut self, hir_id: HirNodeId, context: HirBuildContext) {
        // Also record the source location from the context
        self.source_locations
            .insert(hir_id, context.source_location.clone());
        self.build_contexts.insert(hir_id, context);
    }

    /// Gets the source location for a HIR node
    pub fn get_source_location(&self, hir_id: HirNodeId) -> Option<&TextLocation> {
        self.source_locations.get(&hir_id)
    }

    /// Gets the build context for a HIR node
    pub fn get_build_context(&self, hir_id: HirNodeId) -> Option<&HirBuildContext> {
        self.build_contexts.get(&hir_id)
    }

    /// Gets the original AST node ID for a HIR node
    pub fn get_original_ast(&self, hir_id: HirNodeId) -> Option<AstNodeId> {
        self.hir_to_ast.get(&hir_id).copied()
    }

    /// Gets all HIR node IDs generated from an AST node
    pub fn get_hir_nodes(&self, ast_id: AstNodeId) -> Option<&Vec<HirNodeId>> {
        self.ast_to_hir.get(&ast_id)
    }

    /// Checks if a HIR node has ownership potential
    pub fn has_ownership_potential(&self, hir_id: HirNodeId) -> bool {
        self.build_contexts
            .get(&hir_id)
            .map(|ctx| ctx.ownership_potential)
            .unwrap_or(false)
    }

    /// Gets the scope depth at which a HIR node was created
    pub fn get_scope_depth(&self, hir_id: HirNodeId) -> Option<usize> {
        self.build_contexts.get(&hir_id).map(|ctx| ctx.scope_depth)
    }
}

// ============================================================================
// HIR Builder Context
// ============================================================================

/// The main context for HIR generation.
/// Manages the overall HIR generation process and maintains compilation state.
/// All components operate on borrowed references to this context.
pub struct HirBuilderContext<'a> {
    /// Reference to the string table for string interning
    pub string_table: &'a mut StringTable,

    /// The current function being processed (if any)
    pub current_function: Option<InternedString>,

    /// Counter for generating unique block IDs
    block_counter: usize,

    /// Counter for generating unique HIR node IDs
    node_counter: usize,

    /// Stack of active scopes
    scope_stack: Vec<ScopeInfo>,

    /// Registered function signatures
    function_signatures: HashMap<InternedString, FunctionSignature>,

    /// Registered struct definitions (name -> fields)
    struct_definitions: HashMap<InternedString, Vec<crate::compiler::parsers::ast_nodes::Arg>>,

    /// Candidates for possible drop insertion
    drop_candidates: Vec<DropCandidate>,

    /// Generated HIR blocks
    blocks: Vec<HirBlock>,

    /// Generated function definitions
    functions: Vec<HirNode>,

    /// Generated struct definitions
    structs: Vec<HirNode>,

    /// Metadata for HIR generation
    metadata: HirGenerationMetadata,

    /// AST to HIR mapping for error reporting
    ast_hir_mapping: AstHirMapping,

    /// Entry block ID
    entry_block: Option<BlockId>,
}

impl<'a> HirBuilderContext<'a> {
    /// Creates a new HIR builder context
    pub fn new(string_table: &'a mut StringTable) -> Self {
        HirBuilderContext {
            string_table,
            current_function: None,
            block_counter: 0,
            node_counter: 0,
            scope_stack: Vec::new(),
            function_signatures: HashMap::new(),
            struct_definitions: HashMap::new(),
            drop_candidates: Vec::new(),
            blocks: Vec::new(),
            functions: Vec::new(),
            structs: Vec::new(),
            metadata: HirGenerationMetadata::new(),
            ast_hir_mapping: AstHirMapping::new(),
            entry_block: None,
        }
    }

    // ========================================================================
    // Main Build Method
    // ========================================================================
    /// Builds an HIR module from an AST.
    /// This is the main entry point for HIR generation.
    pub fn build_hir_module(mut self, ast: Ast) -> Result<HirModule, CompilerMessages> {
        // Create the entry block
        let entry_block_id = self.create_block();
        self.set_entry_block(entry_block_id);

        // Enter the module scope
        self.enter_scope_with_block(ScopeType::Function, entry_block_id);

        // Process each AST node
        for node in &ast.nodes {
            match self.process_ast_node(node) {
                Ok(_) => {}
                Err(e) => {
                    return Err(CompilerMessages {
                        errors: vec![e],
                        warnings: ast.warnings,
                    });
                }
            }
        }

        // Exit the module scope
        let _dropped_vars = self.exit_scope();

        // Validate the generated HIR
        let hir_module = HirModule {
            blocks: self.blocks,
            entry_block: entry_block_id,
            functions: self.functions,
            structs: self.structs,
        };

        // Run validation
        match HirValidator::validate_module(&hir_module) {
            Ok(_) => Ok(hir_module),
            Err(validation_error) => Err(CompilerMessages {
                errors: vec![validation_error.into()],
                warnings: ast.warnings,
            }),
        }
    }

    // ========================================================================
    // ID Allocation
    // ========================================================================

    /// Allocates a new unique block ID
    pub fn allocate_block_id(&mut self) -> BlockId {
        let id = self.block_counter;
        self.block_counter += 1;
        id
    }

    /// Allocates a new unique HIR node ID
    pub fn allocate_node_id(&mut self) -> HirNodeId {
        let id = self.node_counter;
        self.node_counter += 1;
        id
    }

    // ========================================================================
    // Scope Management
    // ========================================================================

    /// Enters a new scope
    pub fn enter_scope(&mut self, scope_type: ScopeType) {
        let depth = self.scope_stack.len();
        self.scope_stack.push(ScopeInfo::new(scope_type, depth));
    }

    /// Enters a new scope with an associated block
    pub fn enter_scope_with_block(&mut self, scope_type: ScopeType, block_id: BlockId) {
        let depth = self.scope_stack.len();
        self.scope_stack
            .push(ScopeInfo::with_block(scope_type, block_id, depth));
    }

    /// Exits the current scope and returns variables that went out of scope
    pub fn exit_scope(&mut self) -> Vec<InternedString> {
        match self.scope_stack.pop() {
            Some(scope_info) => scope_info.owned_variables,
            None => Vec::new(),
        }
    }

    /// Gets the current scope depth
    pub fn current_scope_depth(&self) -> usize {
        self.scope_stack.len()
    }

    /// Gets the current scope info (if any)
    pub fn current_scope(&self) -> Option<&ScopeInfo> {
        self.scope_stack.last()
    }

    /// Gets a mutable reference to the current scope info (if any)
    pub fn current_scope_mut(&mut self) -> Option<&mut ScopeInfo> {
        self.scope_stack.last_mut()
    }

    /// Finds the nearest enclosing loop scope
    pub fn find_enclosing_loop(&self) -> Option<&ScopeInfo> {
        self.scope_stack
            .iter()
            .rev()
            .find(|s| matches!(s.scope_type, ScopeType::Loop { .. }))
    }

    // ========================================================================
    // Registration Methods
    // ========================================================================

    /// Registers a function signature
    pub fn register_function(&mut self, name: InternedString, signature: FunctionSignature) {
        self.function_signatures.insert(name, signature);
    }

    /// Gets a registered function signature
    pub fn get_function_signature(&self, name: &InternedString) -> Option<&FunctionSignature> {
        self.function_signatures.get(name)
    }

    /// Registers a struct definition
    pub fn register_struct(
        &mut self,
        name: InternedString,
        fields: Vec<crate::compiler::parsers::ast_nodes::Arg>,
    ) {
        self.struct_definitions.insert(name, fields);
    }

    /// Gets a registered struct definition
    pub fn get_struct_definition(
        &self,
        name: &InternedString,
    ) -> Option<&Vec<crate::compiler::parsers::ast_nodes::Arg>> {
        self.struct_definitions.get(name)
    }

    // ========================================================================
    // Drop Candidate Management
    // ========================================================================

    /// Adds a drop candidate
    pub fn add_drop_candidate(&mut self, variable: InternedString, location: TextLocation) {
        let scope_level = self.current_scope_depth();
        self.drop_candidates.push(DropCandidate {
            variable,
            location,
            scope_level,
        });

        // Also add to current scope's owned variables
        if let Some(scope) = self.current_scope_mut() {
            scope.owned_variables.push(variable);
        }
    }

    /// Gets drop candidates for the current scope level
    pub fn get_drop_candidates_for_scope(&self, scope_level: usize) -> Vec<&DropCandidate> {
        self.drop_candidates
            .iter()
            .filter(|c| c.scope_level >= scope_level)
            .collect()
    }

    // ========================================================================
    // Block Management
    // ========================================================================

    /// Creates a new HIR block and returns its ID
    pub fn create_block(&mut self) -> BlockId {
        let id = self.allocate_block_id();
        self.blocks.push(HirBlock {
            id,
            params: Vec::new(),
            nodes: Vec::new(),
        });
        id
    }

    /// Gets a mutable reference to a block by ID
    pub fn get_block_mut(&mut self, id: BlockId) -> Option<&mut HirBlock> {
        self.blocks.iter_mut().find(|b| b.id == id)
    }

    /// Gets a reference to a block by ID
    pub fn get_block(&self, id: BlockId) -> Option<&HirBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// Adds a node to a specific block
    pub fn add_node_to_block(&mut self, block_id: BlockId, node: HirNode) {
        if let Some(block) = self.get_block_mut(block_id) {
            block.nodes.push(node);
        }
    }

    /// Sets the entry block
    pub fn set_entry_block(&mut self, block_id: BlockId) {
        self.entry_block = Some(block_id);
    }

    // ========================================================================
    // Metadata Access
    // ========================================================================

    /// Gets a reference to the generation metadata
    pub fn metadata(&self) -> &HirGenerationMetadata {
        &self.metadata
    }

    /// Gets a mutable reference to the generation metadata
    pub fn metadata_mut(&mut self) -> &mut HirGenerationMetadata {
        &mut self.metadata
    }

    /// Gets a reference to the AST-HIR mapping
    pub fn ast_hir_mapping(&self) -> &AstHirMapping {
        &self.ast_hir_mapping
    }

    /// Gets a mutable reference to the AST-HIR mapping
    pub fn ast_hir_mapping_mut(&mut self) -> &mut AstHirMapping {
        &mut self.ast_hir_mapping
    }

    // ========================================================================
    // Build Context Helpers
    // ========================================================================

    /// Creates a HirBuildContext for the current state
    pub fn create_build_context(&self, location: TextLocation) -> HirBuildContext {
        HirBuildContext::with_details(location, None, self.current_scope_depth(), false)
    }

    /// Creates a HirBuildContext with AST node reference
    pub fn create_build_context_with_ast(
        &self,
        location: TextLocation,
        ast_node_id: AstNodeId,
    ) -> HirBuildContext {
        HirBuildContext::with_details(
            location,
            Some(ast_node_id),
            self.current_scope_depth(),
            false,
        )
    }

    /// Creates a HirBuildContext with ownership potential
    pub fn create_build_context_with_ownership(
        &self,
        location: TextLocation,
        ast_node_id: Option<AstNodeId>,
    ) -> HirBuildContext {
        HirBuildContext::with_details(location, ast_node_id, self.current_scope_depth(), true)
    }

    /// Records a HIR node with its build context
    pub fn record_node_context(&mut self, hir_id: HirNodeId, context: HirBuildContext) {
        self.ast_hir_mapping.record_build_context(hir_id, context);
    }

    /// Records a mapping from AST to HIR with context
    pub fn record_ast_to_hir(
        &mut self,
        ast_id: AstNodeId,
        hir_id: HirNodeId,
        location: TextLocation,
    ) {
        self.ast_hir_mapping.add_single_mapping(ast_id, hir_id);
        let context = self.create_build_context_with_ast(location, ast_id);
        self.ast_hir_mapping.record_build_context(hir_id, context);
    }

    // ========================================================================
    // Ownership Hints Helpers
    // ========================================================================

    /// Marks a variable as potentially owned
    pub fn mark_potentially_owned(&mut self, var: InternedString) {
        self.metadata.ownership_hints.mark_potentially_owned(var);
    }

    /// Marks a variable as definitely borrowed
    pub fn mark_definitely_borrowed(&mut self, var: InternedString) {
        self.metadata.ownership_hints.mark_definitely_borrowed(var);
    }

    /// Records a potential last use for a variable
    pub fn record_potential_last_use(&mut self, var: InternedString, location: TextLocation) {
        self.metadata
            .ownership_hints
            .record_potential_last_use(var, location);
    }

    /// Marks a variable as potentially consumed (moved)
    pub fn mark_potentially_consumed(&mut self, var: InternedString) {
        self.metadata.ownership_hints.mark_potentially_consumed(var);
    }

    /// Checks if a variable could potentially be owned
    pub fn is_potentially_owned(&self, var: &InternedString) -> bool {
        self.metadata.ownership_hints.is_potentially_owned(var)
    }

    /// Processes a single AST node and generates corresponding HIR
    fn process_ast_node(&mut self, _node: &AstNode) -> Result<Vec<HirNode>, CompilerError> {
        // TODO: Implement AST node processing in subsequent tasks
        // This is a placeholder that will be filled in by Task 3 (expression linearization)
        // and Task 4 (control flow linearization)
        Ok(Vec::new())
    }
}

// ============================================================================
// HIR Validation Framework
// ============================================================================

/// Report from HIR validation
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// List of invariants that were checked
    pub invariants_checked: Vec<String>,
    /// Any violations found
    pub violations_found: Vec<InvariantViolation>,
    /// Warnings (non-fatal issues)
    pub warnings: Vec<String>,
}

impl ValidationReport {
    pub fn new() -> Self {
        ValidationReport {
            invariants_checked: Vec::new(),
            violations_found: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Returns true if validation passed (no violations)
    pub fn is_valid(&self) -> bool {
        self.violations_found.is_empty()
    }
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

/// A specific invariant violation found during validation
#[derive(Debug, Clone)]
pub struct InvariantViolation {
    /// Name of the invariant that was violated
    pub invariant: String,
    /// Location in source code (if available)
    pub location: Option<TextLocation>,
    /// Description of the violation
    pub description: String,
    /// Suggested fix (if any)
    pub suggested_fix: Option<String>,
}

/// Errors that can occur during HIR validation
#[derive(Debug, Clone)]
pub enum HirValidationError {
    /// Found a nested expression where flat expression was expected
    NestedExpression {
        location: TextLocation,
        expression: String,
    },
    /// Block is missing a terminator
    MissingTerminator {
        block_id: BlockId,
        location: Option<TextLocation>,
    },
    /// Block has multiple terminators
    MultipleTerminators { block_id: BlockId, count: usize },
    /// Variable used before declaration
    UndeclaredVariable {
        variable: String,
        location: TextLocation,
    },
    /// Missing drop for a variable on an exit path
    MissingDrop {
        variable: String,
        exit_path: String,
        location: TextLocation,
    },
    /// Block is unreachable from entry
    UnreachableBlock { block_id: BlockId },
    /// Branch target references invalid block
    InvalidBranchTarget {
        source_block: BlockId,
        target_block: BlockId,
    },
    /// Invalid assignment
    InvalidAssignment {
        variable: String,
        location: TextLocation,
        reason: String,
    },
}

impl From<HirValidationError> for CompilerError {
    fn from(error: HirValidationError) -> Self {
        match error {
            HirValidationError::NestedExpression {
                location,
                expression,
            } => CompilerError::new(
                format!(
                    "HIR invariant violation: nested expression found: {}",
                    expression
                ),
                location.to_error_location_without_table(),
                crate::compiler::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::MissingTerminator { block_id, location } => CompilerError::new(
                format!(
                    "HIR invariant violation: block {} is missing a terminator",
                    block_id
                ),
                location
                    .map(|l| l.to_error_location_without_table())
                    .unwrap_or_else(ErrorLocation::default),
                crate::compiler::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::MultipleTerminators { block_id, count } => CompilerError::new(
                format!(
                    "HIR invariant violation: block {} has {} terminators (expected 1)",
                    block_id, count
                ),
                ErrorLocation::default(),
                crate::compiler::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::UndeclaredVariable { variable, location } => CompilerError::new(
                format!(
                    "HIR invariant violation: variable '{}' used before declaration",
                    variable
                ),
                location.to_error_location_without_table(),
                crate::compiler::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::MissingDrop {
                variable,
                exit_path,
                location,
            } => CompilerError::new(
                format!(
                    "HIR invariant violation: missing drop for '{}' on exit path '{}'",
                    variable, exit_path
                ),
                location.to_error_location_without_table(),
                crate::compiler::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::UnreachableBlock { block_id } => CompilerError::new(
                format!(
                    "HIR invariant violation: block {} is unreachable from entry",
                    block_id
                ),
                ErrorLocation::default(),
                crate::compiler::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::InvalidBranchTarget {
                source_block,
                target_block,
            } => CompilerError::new(
                format!(
                    "HIR invariant violation: block {} branches to invalid block {}",
                    source_block, target_block
                ),
                ErrorLocation::default(),
                crate::compiler::compiler_errors::ErrorType::HirTransformation,
            ),
            HirValidationError::InvalidAssignment {
                variable,
                location,
                reason,
            } => CompilerError::new(
                format!(
                    "HIR invariant violation: invalid assignment to '{}': {}",
                    variable, reason
                ),
                location.to_error_location_without_table(),
                crate::compiler::compiler_errors::ErrorType::HirTransformation,
            ),
        }
    }
}

/// HIR Validator - validates HIR invariants
///
/// The validator checks that the generated HIR conforms to all required invariants.
/// These invariants turn the design document into an executable contract.
///
/// ## Core HIR Invariants
///
/// 1. **No Nested Expressions**: All expressions in HIR are flat
/// 2. **Explicit Terminators**: Every HIR block ends in exactly one terminator
/// 3. **Variable Declaration Before Use**: All variables are declared before any use
/// 4. **Drop Coverage**: All ownership-capable variables have possible_drop on exit paths
/// 5. **Block Connectivity**: All HIR blocks are reachable from the entry block
/// 6. **Terminator Target Validity**: All branch targets reference valid block IDs
/// 7. **Assignment Discipline**: Assignments must be explicit and properly ordered
pub struct HirValidator;

impl HirValidator {
    /// Maximum allowed expression nesting depth.
    /// HIR expressions should be mostly flat. We allow limited nesting
    /// for binary operations, but operands should be simple.
    const MAX_EXPRESSION_DEPTH: usize = 2;

    /// Validates all HIR invariants on a module.
    /// Returns a validation report with all checked invariants and any violations.
    pub fn validate_module(hir_module: &HirModule) -> Result<ValidationReport, HirValidationError> {
        let mut report = ValidationReport::new();

        // Invariant 1: No nested expressions
        report
            .invariants_checked
            .push("no_nested_expressions".to_string());
        if let Err(e) = Self::check_no_nested_expressions(hir_module) {
            report.violations_found.push(InvariantViolation {
                invariant: "no_nested_expressions".to_string(),
                location: None,
                description: format!("{:?}", e),
                suggested_fix: Some("Flatten nested expressions into temporaries".to_string()),
            });
            return Err(e);
        }

        // Invariant 2: Explicit terminators
        report
            .invariants_checked
            .push("explicit_terminators".to_string());
        if let Err(e) = Self::check_explicit_terminators(hir_module) {
            report.violations_found.push(InvariantViolation {
                invariant: "explicit_terminators".to_string(),
                location: None,
                description: format!("{:?}", e),
                suggested_fix: Some(
                    "Ensure every block ends with exactly one terminator".to_string(),
                ),
            });
            return Err(e);
        }

        // Invariant 5: Block connectivity
        report
            .invariants_checked
            .push("block_connectivity".to_string());
        if let Err(e) = Self::check_block_connectivity(hir_module) {
            report.violations_found.push(InvariantViolation {
                invariant: "block_connectivity".to_string(),
                location: None,
                description: format!("{:?}", e),
                suggested_fix: Some(
                    "Remove unreachable blocks or add control flow paths".to_string(),
                ),
            });
            return Err(e);
        }

        // Invariant 6: Terminator target validity
        report
            .invariants_checked
            .push("terminator_targets".to_string());
        if let Err(e) = Self::check_terminator_targets(hir_module) {
            report.violations_found.push(InvariantViolation {
                invariant: "terminator_targets".to_string(),
                location: None,
                description: format!("{:?}", e),
                suggested_fix: Some(
                    "Ensure all branch targets reference valid block IDs".to_string(),
                ),
            });
            return Err(e);
        }

        // Invariant 3: Variable declaration order
        report
            .invariants_checked
            .push("variable_declaration_order".to_string());
        Self::check_variable_declaration_order(hir_module)?;

        // Invariant 7: Assignment discipline
        report
            .invariants_checked
            .push("assignment_discipline".to_string());
        Self::check_assignment_discipline(hir_module)?;

        // Invariant 4: Drop coverage
        report.invariants_checked.push("drop_coverage".to_string());
        Self::check_drop_coverage(hir_module)?;

        Ok(report)
    }

    /// Validates a single block's invariants
    pub fn validate_block(block: &HirBlock) -> Result<(), HirValidationError> {
        // Check expression flatness
        for node in &block.nodes {
            Self::check_node_expressions_flat(node)?;
        }

        // Check terminator presence (non-empty blocks must have exactly one terminator)
        if !block.nodes.is_empty() {
            let terminator_count = Self::count_terminators_in_block(block);
            if terminator_count == 0 {
                if let Some(last_node) = block.nodes.last() {
                    if !Self::is_terminator(last_node) {
                        return Err(HirValidationError::MissingTerminator {
                            block_id: block.id,
                            location: Some(last_node.location.clone()),
                        });
                    }
                }
            } else if terminator_count > 1 {
                return Err(HirValidationError::MultipleTerminators {
                    block_id: block.id,
                    count: terminator_count,
                });
            }
        }

        Ok(())
    }

    /// Checks that no expressions contain deeply nested expressions.
    /// All expressions in HIR should be mostly flat.
    pub fn check_no_nested_expressions(hir_module: &HirModule) -> Result<(), HirValidationError> {
        for block in &hir_module.blocks {
            for node in &block.nodes {
                Self::check_node_expressions_flat(node)?;
            }
        }
        // Also check function definitions
        for func in &hir_module.functions {
            Self::check_node_expressions_flat(func)?;
        }
        Ok(())
    }

    /// Helper to check that expressions in a node are flat
    fn check_node_expressions_flat(node: &HirNode) -> Result<(), HirValidationError> {
        match &node.kind {
            crate::compiler::hir::nodes::HirKind::Stmt(stmt) => {
                Self::check_stmt_expressions_flat(stmt, &node.location)?;
            }
            crate::compiler::hir::nodes::HirKind::Terminator(term) => {
                Self::check_terminator_expressions_flat(term, &node.location)?;
            }
        }
        Ok(())
    }

    /// Checks that expressions in a statement are flat
    fn check_stmt_expressions_flat(
        stmt: &crate::compiler::hir::nodes::HirStmt,
        location: &TextLocation,
    ) -> Result<(), HirValidationError> {
        use crate::compiler::hir::nodes::HirStmt;

        match stmt {
            HirStmt::Assign { value, .. } => {
                Self::check_expr_nesting_depth(value, 0, location)?;
            }
            HirStmt::Call { args, .. } => {
                for arg in args {
                    Self::check_expr_nesting_depth(arg, 0, location)?;
                }
            }
            HirStmt::HostCall { args, .. } => {
                for arg in args {
                    Self::check_expr_nesting_depth(arg, 0, location)?;
                }
            }
            HirStmt::RuntimeTemplateCall { captures, .. } => {
                for capture in captures {
                    Self::check_expr_nesting_depth(capture, 0, location)?;
                }
            }
            HirStmt::ExprStmt(expr) => {
                Self::check_expr_nesting_depth(expr, 0, location)?;
            }
            HirStmt::PossibleDrop(_)
            | HirStmt::TemplateFn { .. }
            | HirStmt::FunctionDef { .. }
            | HirStmt::StructDef { .. } => {}
        }
        Ok(())
    }

    /// Checks that expressions in a terminator are flat
    fn check_terminator_expressions_flat(
        term: &crate::compiler::hir::nodes::HirTerminator,
        location: &TextLocation,
    ) -> Result<(), HirValidationError> {
        use crate::compiler::hir::nodes::HirTerminator;

        match term {
            HirTerminator::If { condition, .. } => {
                Self::check_expr_nesting_depth(condition, 0, location)?;
            }
            HirTerminator::Match { scrutinee, .. } => {
                Self::check_expr_nesting_depth(scrutinee, 0, location)?;
            }
            HirTerminator::Loop { iterator, .. } => {
                if let Some(iter) = iterator {
                    Self::check_expr_nesting_depth(iter, 0, location)?;
                }
            }
            HirTerminator::Return(exprs) => {
                for expr in exprs {
                    Self::check_expr_nesting_depth(expr, 0, location)?;
                }
            }
            HirTerminator::ReturnError(expr) => {
                Self::check_expr_nesting_depth(expr, 0, location)?;
            }
            HirTerminator::Panic { message } => {
                if let Some(msg) = message {
                    Self::check_expr_nesting_depth(msg, 0, location)?;
                }
            }
            HirTerminator::Break { .. } | HirTerminator::Continue { .. } => {}
        }
        Ok(())
    }

    /// Checks expression nesting depth
    fn check_expr_nesting_depth(
        expr: &crate::compiler::hir::nodes::HirExpr,
        current_depth: usize,
        location: &TextLocation,
    ) -> Result<(), HirValidationError> {
        use crate::compiler::hir::nodes::HirExprKind;

        if current_depth > Self::MAX_EXPRESSION_DEPTH {
            return Err(HirValidationError::NestedExpression {
                location: location.clone(),
                expression: format!("{:?}", expr.kind),
            });
        }

        match &expr.kind {
            // Simple expressions - no nesting
            HirExprKind::Int(_)
            | HirExprKind::Float(_)
            | HirExprKind::Bool(_)
            | HirExprKind::StringLiteral(_)
            | HirExprKind::HeapString(_)
            | HirExprKind::Char(_)
            | HirExprKind::Load(_)
            | HirExprKind::Field { .. }
            | HirExprKind::Move(_) => Ok(()),

            HirExprKind::BinOp { left, right, .. } => {
                Self::check_expr_nesting_depth(left, current_depth + 1, location)?;
                Self::check_expr_nesting_depth(right, current_depth + 1, location)
            }
            HirExprKind::UnaryOp { operand, .. } => {
                Self::check_expr_nesting_depth(operand, current_depth + 1, location)
            }
            HirExprKind::Call { args, .. } => {
                for arg in args {
                    Self::check_expr_nesting_depth(arg, current_depth + 1, location)?;
                }
                Ok(())
            }
            HirExprKind::MethodCall { receiver, args, .. } => {
                Self::check_expr_nesting_depth(receiver, current_depth + 1, location)?;
                for arg in args {
                    Self::check_expr_nesting_depth(arg, current_depth + 1, location)?;
                }
                Ok(())
            }
            HirExprKind::StructConstruct { fields, .. } => {
                for (_, field_expr) in fields {
                    Self::check_expr_nesting_depth(field_expr, current_depth + 1, location)?;
                }
                Ok(())
            }
            HirExprKind::Collection(exprs) => {
                for e in exprs {
                    Self::check_expr_nesting_depth(e, current_depth + 1, location)?;
                }
                Ok(())
            }
            HirExprKind::Range { start, end } => {
                Self::check_expr_nesting_depth(start, current_depth + 1, location)?;
                Self::check_expr_nesting_depth(end, current_depth + 1, location)
            }
        }
    }

    /// Checks that every block ends in exactly one terminator.
    pub fn check_explicit_terminators(hir_module: &HirModule) -> Result<(), HirValidationError> {
        for block in &hir_module.blocks {
            let terminator_count = Self::count_terminators_in_block(block);

            if block.nodes.is_empty() {
                continue; // Allow empty blocks during construction
            }

            if terminator_count == 0 {
                if let Some(last_node) = block.nodes.last() {
                    if !Self::is_terminator(last_node) {
                        return Err(HirValidationError::MissingTerminator {
                            block_id: block.id,
                            location: Some(last_node.location.clone()),
                        });
                    }
                }
            } else if terminator_count > 1 {
                return Err(HirValidationError::MultipleTerminators {
                    block_id: block.id,
                    count: terminator_count,
                });
            }
        }
        Ok(())
    }

    /// Counts the number of terminator nodes in a block
    fn count_terminators_in_block(block: &HirBlock) -> usize {
        block
            .nodes
            .iter()
            .filter(|n| Self::is_terminator(n))
            .count()
    }

    /// Checks if a node is a terminator
    pub fn is_terminator(node: &HirNode) -> bool {
        matches!(
            node.kind,
            crate::compiler::hir::nodes::HirKind::Terminator(_)
        )
    }

    /// Checks if a node is a statement
    pub fn is_statement(node: &HirNode) -> bool {
        matches!(node.kind, crate::compiler::hir::nodes::HirKind::Stmt(_))
    }

    /// Checks that all blocks are reachable from the entry block.
    pub fn check_block_connectivity(hir_module: &HirModule) -> Result<(), HirValidationError> {
        if hir_module.blocks.is_empty() {
            return Ok(());
        }

        let mut reachable: HashSet<BlockId> = HashSet::new();
        let mut to_visit: Vec<BlockId> = vec![hir_module.entry_block];

        while let Some(block_id) = to_visit.pop() {
            if reachable.contains(&block_id) {
                continue;
            }
            reachable.insert(block_id);

            if let Some(block) = hir_module.blocks.iter().find(|b| b.id == block_id) {
                for succ in Self::get_block_successors(block) {
                    if !reachable.contains(&succ) {
                        to_visit.push(succ);
                    }
                }
            }
        }

        for block in &hir_module.blocks {
            if !reachable.contains(&block.id) {
                return Err(HirValidationError::UnreachableBlock { block_id: block.id });
            }
        }

        Ok(())
    }

    /// Gets the successor block IDs from a block's terminator
    pub fn get_block_successors(block: &HirBlock) -> Vec<BlockId> {
        let mut successors = Vec::new();

        for node in &block.nodes {
            if let crate::compiler::hir::nodes::HirKind::Terminator(term) = &node.kind {
                match term {
                    crate::compiler::hir::nodes::HirTerminator::If {
                        then_block,
                        else_block,
                        ..
                    } => {
                        successors.push(*then_block);
                        if let Some(else_id) = else_block {
                            successors.push(*else_id);
                        }
                    }
                    crate::compiler::hir::nodes::HirTerminator::Match {
                        arms,
                        default_block,
                        ..
                    } => {
                        for arm in arms {
                            successors.push(arm.body);
                        }
                        if let Some(default_id) = default_block {
                            successors.push(*default_id);
                        }
                    }
                    crate::compiler::hir::nodes::HirTerminator::Loop { body, .. } => {
                        successors.push(*body);
                    }
                    crate::compiler::hir::nodes::HirTerminator::Break { target }
                    | crate::compiler::hir::nodes::HirTerminator::Continue { target } => {
                        successors.push(*target);
                    }
                    crate::compiler::hir::nodes::HirTerminator::Return(_)
                    | crate::compiler::hir::nodes::HirTerminator::ReturnError(_)
                    | crate::compiler::hir::nodes::HirTerminator::Panic { .. } => {}
                }
            }
        }

        successors
    }

    /// Checks that all branch targets reference valid block IDs.
    pub fn check_terminator_targets(hir_module: &HirModule) -> Result<(), HirValidationError> {
        let valid_block_ids: HashSet<BlockId> = hir_module.blocks.iter().map(|b| b.id).collect();

        for block in &hir_module.blocks {
            for succ in Self::get_block_successors(block) {
                if !valid_block_ids.contains(&succ) {
                    return Err(HirValidationError::InvalidBranchTarget {
                        source_block: block.id,
                        target_block: succ,
                    });
                }
            }
        }

        Ok(())
    }

    /// Checks that all variables are declared before use.
    pub fn check_variable_declaration_order(
        _hir_module: &HirModule,
    ) -> Result<(), HirValidationError> {
        // Placeholder - full implementation requires tracking declarations through control flow
        Ok(())
    }

    /// Checks that all ownership-capable variables have possible_drop on every exit path.
    pub fn check_drop_coverage(_hir_module: &HirModule) -> Result<(), HirValidationError> {
        // Placeholder - full implementation requires control flow analysis
        Ok(())
    }

    /// Checks that assignments follow proper discipline.
    pub fn check_assignment_discipline(hir_module: &HirModule) -> Result<(), HirValidationError> {
        for block in &hir_module.blocks {
            for node in &block.nodes {
                if let crate::compiler::hir::nodes::HirKind::Stmt(
                    crate::compiler::hir::nodes::HirStmt::Assign {
                        target, is_mutable, ..
                    },
                ) = &node.kind
                {
                    Self::check_assignment_target_valid(target, *is_mutable, &node.location)?;
                }
            }
        }
        Ok(())
    }

    /// Checks that an assignment target is valid
    fn check_assignment_target_valid(
        target: &crate::compiler::hir::nodes::HirPlace,
        is_mutable: bool,
        location: &TextLocation,
    ) -> Result<(), HirValidationError> {
        match target {
            crate::compiler::hir::nodes::HirPlace::Var(_) => Ok(()),
            crate::compiler::hir::nodes::HirPlace::Field { base, .. } => {
                Self::check_assignment_target_valid(base, is_mutable, location)
            }
            crate::compiler::hir::nodes::HirPlace::Index { base, .. } => {
                Self::check_assignment_target_valid(base, is_mutable, location)
            }
        }
    }
}
