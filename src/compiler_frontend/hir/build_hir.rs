//! HIR Builder - Core Infrastructure
//!
//! This module implements the HIR (High-Level Intermediate Representation) builder
//! for the Beanstalk compiler_frontend. The HIR builder converts the fully typed AST into
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

use crate::compiler_frontend::ast::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, Var};
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::hir::control_flow_linearizer::ControlFlowLinearizer;
use crate::compiler_frontend::hir::expression_linearizer::ExpressionLinearizer;
use crate::compiler_frontend::hir::function_transformer::FunctionTransformer;
use crate::compiler_frontend::hir::memory_management::drop_point_inserter::DropPointInserter;
use crate::compiler_frontend::hir::nodes::{
    BlockId, HirBlock, HirExpr, HirExprKind, HirKind, HirModule, HirNode, HirNodeId, HirPlace,
    HirStmt,
};
use crate::compiler_frontend::hir::struct_handler::StructHandler;
use crate::compiler_frontend::hir::template_processor::TemplateProcessor;
use crate::compiler_frontend::hir::variable_manager::{VariableManager, is_type_ownership_capable};
use crate::compiler_frontend::string_interning::{InternedString, StringTable};
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use std::collections::{HashMap, HashSet};
// Re-export validator types for backward compatibility
use crate::backends::host_function_registry::{CallTarget, HostFunctionId};
pub use crate::compiler_frontend::hir::validator::{HirValidationError, HirValidator};
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
    /// Temporary variables introduced by the compiler_frontend (all treated as user locals)
    pub temporary_variables: HashMap<InternedString, HirExprKind>,
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

    /// Generates a unique name for a compiler_frontend-introduced temporary variable
    pub fn generate_temp_name(&mut self) -> String {
        let name = format!("__tmp_{}", self.temp_var_counter);
        self.temp_var_counter += 1;
        name
    }

    /// Registers a temporary variable with its type
    pub fn register_temporary(&mut self, name: InternedString, kind: HirExprKind) {
        self.temporary_variables.insert(name, kind);
    }

    /// Checks if a variable is a compiler_frontend-introduced temporary
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
    struct_definitions: HashMap<InternedString, Vec<Var>>,

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

    /// Expression linearizer component
    expression_linearizer: ExpressionLinearizer,

    /// Control flow linearizer component
    control_flow_linearizer: ControlFlowLinearizer,

    /// Variable manager component
    variable_manager: VariableManager,

    /// Drop point inserter component
    drop_inserter: DropPointInserter,

    /// Function transformer component
    function_transformer: FunctionTransformer,

    /// Struct handler component
    struct_handler: StructHandler,

    /// Template processor component
    template_processor: TemplateProcessor,
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
            expression_linearizer: ExpressionLinearizer::new(),
            control_flow_linearizer: ControlFlowLinearizer::new(),
            variable_manager: VariableManager::new(),
            drop_inserter: DropPointInserter::new(),
            function_transformer: FunctionTransformer::new(),
            struct_handler: StructHandler::new(),
            template_processor: TemplateProcessor::new(),
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
    pub fn register_struct(&mut self, name: InternedString, fields: Vec<Var>) {
        self.struct_definitions.insert(name, fields);
    }

    /// Gets a registered struct definition
    pub fn get_struct_definition(&self, name: &InternedString) -> Option<&Vec<Var>> {
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

    /// Gets a mutable reference to the function transformer
    pub fn function_transformer_mut(
        &mut self,
    ) -> &mut crate::compiler_frontend::hir::function_transformer::FunctionTransformer {
        &mut self.function_transformer
    }

    /// Gets a reference to the function transformer
    pub fn function_transformer(
        &self,
    ) -> &crate::compiler_frontend::hir::function_transformer::FunctionTransformer {
        &self.function_transformer
    }

    /// Gets a mutable reference to the struct handler
    pub fn struct_handler_mut(&mut self) -> &mut StructHandler {
        &mut self.struct_handler
    }

    /// Gets a reference to the struct handler
    pub fn struct_handler(&self) -> &StructHandler {
        &self.struct_handler
    }

    /// Processes a single AST node and generates corresponding HIR
    fn process_ast_node(&mut self, node: &AstNode) -> Result<Vec<HirNode>, CompilerError> {
        match &node.kind {
            NodeKind::Function(name, signature, body) => {
                // We need to work around the borrow checker here
                // Take the transformer temporarily
                let mut transformer =
                    std::mem::replace(&mut self.function_transformer, FunctionTransformer::new());

                let result = transformer.transform_function_definition(
                    *name,
                    signature.clone(),
                    body,
                    self,
                    node.location.clone(),
                );

                // Put it back
                self.function_transformer = transformer;

                let func_node = result?;
                self.functions.push(func_node.clone());
                Ok(vec![func_node])
            }
            NodeKind::FunctionCall {
                name,
                args,
                returns,
                location,
            } => {
                let mut transformer =
                    std::mem::replace(&mut self.function_transformer, FunctionTransformer::new());

                let result =
                    transformer.transform_function_call_as_stmt(*name, args, self, location);

                self.function_transformer = transformer;
                result
            }
            NodeKind::HostFunctionCall {
                host_function_id,
                args,
                returns,
                location,
            } => {
                let mut transformer =
                    std::mem::replace(&mut self.function_transformer, FunctionTransformer::new());

                let result = transformer.transform_host_function_call_as_stmt(
                    *host_function_id,
                    args,
                    self,
                    location,
                );

                self.function_transformer = transformer;
                result
            }
            NodeKind::Return(exprs) => {
                let mut transformer =
                    std::mem::replace(&mut self.function_transformer, FunctionTransformer::new());

                let result = transformer.transform_return(exprs, self, &node.location);

                self.function_transformer = transformer;
                result
            }
            NodeKind::StructDefinition(name, fields) => {
                // Take the struct handler temporarily to work around the borrow checker
                let mut handler = std::mem::replace(&mut self.struct_handler, StructHandler::new());

                let result =
                    handler.transform_struct_definition(*name, fields, self, node.location.clone());

                // Put it back
                self.struct_handler = handler;

                let struct_node = result?;
                self.structs.push(struct_node.clone());
                Ok(vec![struct_node])
            }

            // Variable declarations
            NodeKind::VariableDeclaration(arg) => {
                // Use expression linearizer to process the value
                let mut linearizer =
                    std::mem::replace(&mut self.expression_linearizer, ExpressionLinearizer::new());

                let (value_nodes, value_expr) =
                    linearizer.linearize_expression(&arg.value, self)?;
                self.expression_linearizer = linearizer;

                let mut nodes = value_nodes;

                // Create the assignment node for the declaration
                let is_mutable = arg.value.ownership.is_mutable();
                let node_id = self.allocate_node_id();
                let build_context = self.create_build_context(node.location.clone());
                self.record_node_context(node_id, build_context);

                let assign_node = HirNode {
                    kind: HirKind::Stmt(HirStmt::Assign {
                        target: HirPlace::Var(arg.id),
                        value: value_expr,
                        is_mutable,
                    }),
                    location: node.location.clone(),
                    id: node_id,
                };

                // Track the variable in variable manager
                self.variable_manager.enter_scope();

                // Mark as potentially owned if applicable
                if is_type_ownership_capable(&arg.value.data_type) {
                    self.mark_potentially_owned(arg.id);
                    self.add_drop_candidate(arg.id, node.location.clone());
                }

                nodes.push(assign_node);
                Ok(nodes)
            }

            // Assignments
            NodeKind::Assignment { target, value } => {
                // Use expression linearizer to process the value
                let mut linearizer =
                    std::mem::replace(&mut self.expression_linearizer, ExpressionLinearizer::new());

                let (value_nodes, value_expr) = linearizer.linearize_expression(value, self)?;
                self.expression_linearizer = linearizer;

                let mut nodes = value_nodes;

                // Convert target to HirPlace
                let hir_place = self.convert_target_to_place(target)?;

                let node_id = self.allocate_node_id();
                let build_context = self.create_build_context(node.location.clone());
                self.record_node_context(node_id, build_context);

                let assign_node = HirNode {
                    kind: HirKind::Stmt(HirStmt::Assign {
                        target: hir_place,
                        value: value_expr,
                        is_mutable: true,
                    }),
                    location: node.location.clone(),
                    id: node_id,
                };

                nodes.push(assign_node);
                Ok(nodes)
            }

            // If statements
            NodeKind::If(condition, then_body, else_body) => {
                let mut linearizer = std::mem::replace(
                    &mut self.control_flow_linearizer,
                    ControlFlowLinearizer::new(),
                );

                let result = linearizer.linearize_if_statement(
                    condition,
                    then_body,
                    else_body.as_deref(),
                    &node.location,
                    self,
                );

                self.control_flow_linearizer = linearizer;
                result
            }

            // For loops
            NodeKind::ForLoop(binding, iterator, body) => {
                let mut linearizer = std::mem::replace(
                    &mut self.control_flow_linearizer,
                    ControlFlowLinearizer::new(),
                );

                let result =
                    linearizer.linearize_for_loop(binding, iterator, body, &node.location, self);

                self.control_flow_linearizer = linearizer;
                result
            }

            // While loops
            NodeKind::WhileLoop(condition, body) => {
                let mut linearizer = std::mem::replace(
                    &mut self.control_flow_linearizer,
                    ControlFlowLinearizer::new(),
                );

                let result = linearizer.linearize_while_loop(condition, body, &node.location, self);

                self.control_flow_linearizer = linearizer;
                result
            }

            // Match expressions
            NodeKind::Match(scrutinee, arms, default) => {
                let mut linearizer = std::mem::replace(
                    &mut self.control_flow_linearizer,
                    ControlFlowLinearizer::new(),
                );

                let result = linearizer.linearize_match(
                    scrutinee,
                    arms,
                    default.as_deref(),
                    &node.location,
                    self,
                );

                self.control_flow_linearizer = linearizer;
                result
            }

            // R-values (expressions as statements)
            NodeKind::Rvalue(expr) => {
                let mut linearizer =
                    std::mem::replace(&mut self.expression_linearizer, ExpressionLinearizer::new());

                let (mut nodes, result_expr) = linearizer.linearize_expression(expr, self)?;
                self.expression_linearizer = linearizer;

                // Create an expression statement for the result
                let node_id = self.allocate_node_id();
                let build_context = self.create_build_context(node.location.clone());
                self.record_node_context(node_id, build_context);

                let expr_stmt = HirNode {
                    kind: HirKind::Stmt(HirStmt::ExprStmt(result_expr)),
                    location: node.location.clone(),
                    id: node_id,
                };

                nodes.push(expr_stmt);
                Ok(nodes)
            }

            // Print statements (legacy support)
            NodeKind::Print(expr) => {
                let mut linearizer =
                    std::mem::replace(&mut self.expression_linearizer, ExpressionLinearizer::new());

                let (mut nodes, result_expr) = linearizer.linearize_expression(expr, self)?;
                self.expression_linearizer = linearizer;

                // Create a call to the host_io_functions host function
                let io_name = self.string_table.intern("host_io_functions");
                let node_id = self.allocate_node_id();
                let build_context = self.create_build_context(node.location.clone());
                self.record_node_context(node_id, build_context);

                let call_node = HirNode {
                    kind: HirKind::Stmt(HirStmt::Call {
                        target: CallTarget::HostFunction(HostFunctionId::Io),
                        args: vec![result_expr],
                    }),
                    location: node.location.clone(),
                    id: node_id,
                };

                nodes.push(call_node);
                Ok(nodes)
            }

            // Empty nodes - no HIR generated
            NodeKind::Empty | NodeKind::Newline | NodeKind::Spaces(_) => Ok(Vec::new()),

            // Warnings are passed through (no HIR generated)
            NodeKind::Warning(_) => Ok(Vec::new()),

            // Operators should be handled within expressions
            NodeKind::Operator(_) => Ok(Vec::new()),

            // Field access as a statement
            NodeKind::FieldAccess {
                base,
                field,
                data_type,
                ..
            } => {
                let mut linearizer =
                    std::mem::replace(&mut self.expression_linearizer, ExpressionLinearizer::new());

                // Linearize the base expression
                let (mut nodes, base_expr) = linearizer.linearize_ast_node(base, self)?;
                self.expression_linearizer = linearizer;

                // Extract base variable name
                let base_var = self.extract_base_var_from_expr(&base_expr)?;

                let field_expr = HirExpr {
                    kind: HirExprKind::Field {
                        base: base_var,
                        field: *field,
                    },
                    location: node.location.clone(),
                };

                let node_id = self.allocate_node_id();
                let build_context = self.create_build_context(node.location.clone());
                self.record_node_context(node_id, build_context);

                let expr_stmt = HirNode {
                    kind: HirKind::Stmt(HirStmt::ExprStmt(field_expr)),
                    location: node.location.clone(),
                    id: node_id,
                };

                nodes.push(expr_stmt);
                Ok(nodes)
            }

            // Other node kinds - return empty for now
            _ => {
                // For unsupported nodes, return empty
                // This allows gradual implementation
                Ok(Vec::new())
            }
        }
    }

    // ========================================================================
    // Helper Methods for AST Processing
    // ========================================================================

    /// Converts an AST target node to an HirPlace
    fn convert_target_to_place(&self, target: &AstNode) -> Result<HirPlace, CompilerError> {
        match &target.kind {
            NodeKind::Rvalue(expr) => match &expr.kind {
                ExpressionKind::Reference(name) => Ok(HirPlace::Var(name.to_owned())),
                _ => {
                    crate::return_compiler_error!("Unsupported assignment target expression")
                }
            },
            NodeKind::FieldAccess { base, field, .. } => {
                let base_place = self.convert_target_to_place(base)?;
                Ok(HirPlace::Field {
                    base: Box::new(base_place),
                    field: *field,
                })
            }
            _ => {
                crate::return_compiler_error!("Unsupported assignment target node kind")
            }
        }
    }

    /// Extracts the base variable name from an HIR expression
    fn extract_base_var_from_expr(&self, expr: &HirExpr) -> Result<InternedString, CompilerError> {
        match &expr.kind {
            HirExprKind::Load(place) => self.extract_var_from_place(place),
            HirExprKind::Field { base, .. } => Ok(*base),
            _ => {
                crate::return_compiler_error!("Cannot extract base variable from expression")
            }
        }
    }

    /// Extracts the variable name from an HIR place
    fn extract_var_from_place(&self, place: &HirPlace) -> Result<InternedString, CompilerError> {
        match place {
            HirPlace::Var(name) => Ok(*name),
            HirPlace::Field { base, .. } => self.extract_var_from_place(base),
            HirPlace::Index { base, .. } => self.extract_var_from_place(base),
        }
    }

    /// Processes a variable declaration and returns the corresponding HIR nodes.
    /// This is a helper method that can be called from function transformers or other components.
    pub fn process_variable_declaration(
        &mut self,
        arg: &Var,
        location: &TextLocation,
    ) -> Result<Vec<HirNode>, CompilerError> {
        // Use expression linearizer to process the value
        let mut linearizer =
            std::mem::replace(&mut self.expression_linearizer, ExpressionLinearizer::new());

        let (value_nodes, value_expr) = linearizer.linearize_expression(&arg.value, self)?;
        self.expression_linearizer = linearizer;

        let mut nodes = value_nodes;

        // Create the assignment node for the declaration
        let is_mutable = arg.value.ownership.is_mutable();
        let node_id = self.allocate_node_id();
        let build_context = self.create_build_context(location.clone());
        self.record_node_context(node_id, build_context);

        let assign_node = HirNode {
            kind: HirKind::Stmt(HirStmt::Assign {
                target: HirPlace::Var(arg.id),
                value: value_expr,
                is_mutable,
            }),
            location: location.clone(),
            id: node_id,
        };

        // Track the variable in variable manager
        self.variable_manager.enter_scope();

        // Mark as potentially owned if applicable
        if is_type_ownership_capable(&arg.value.data_type) {
            self.mark_potentially_owned(arg.id);
            self.add_drop_candidate(arg.id, location.clone());
        }

        nodes.push(assign_node);
        Ok(nodes)
    }
}
