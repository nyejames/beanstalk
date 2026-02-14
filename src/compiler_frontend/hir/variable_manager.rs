//! Variable Manager for HIR Builder
//!
//! This module implements the VariableManager component that handles variable
//! declarations, references, assignments, and scope tracking during HIR generation.
//!
//! The manager ensures that:
//! - Variables are properly declared before use
//! - Mutability is tracked correctly
//! - Scope boundaries are maintained for drop insertion
//! - Conservative ownership capability tracking is performed
//!
//! ## Key Design Principles
//!
//! - Variables are tracked by symbol identity (name/ID), not place-based projections
//! - Ownership capability is tracked conservatively (may be incomplete or wrong)
//! - The borrow checker is the authority for ownership decisions
//! - All compiler_frontend-introduced locals are treated exactly like user locals

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::build_hir::HirBuilderContext;
use crate::compiler_frontend::hir::nodes::{
    HirExpr, HirExprKind, HirKind, HirNode, HirPlace, HirStmt,
};
use crate::compiler_frontend::parsers::tokenizer::tokens::TextLocation;
use crate::compiler_frontend::string_interning::InternedString;
use crate::return_compiler_error;
use std::collections::HashMap;

/// Scope level type for tracking variable scope depth
pub type ScopeLevel = usize;

/// Information about a declared variable
#[derive(Debug, Clone)]
pub struct VariableInfo {
    /// The data type of the variable
    pub data_type: DataType,
    /// Whether the variable is mutable
    pub is_mutable: bool,
    /// The scope level where the variable was declared
    pub scope_level: ScopeLevel,
    /// Source location of the declaration
    pub location: TextLocation,
    /// Whether this variable can potentially be owned
    pub ownership_capable: bool,
}

/// The VariableManager component handles variable declarations, references,
/// assignments, and scope tracking during HIR generation.
///
/// This component operates on borrowed HirBuilderContext rather than owning
/// independent state, ensuring a single authoritative HIR state per module.
#[derive(Debug, Default)]
pub struct VariableManager {
    /// Maps variable names to their scope levels
    variable_scopes: HashMap<InternedString, ScopeLevel>,

    /// Maps variable names to their mutability
    mutability_tracking: HashMap<InternedString, bool>,

    /// Maps variable names to whether they can be owned
    /// CONSERVATIVE: This may be incomplete or wrong - borrow checker is authority
    ownership_capability: HashMap<InternedString, bool>,

    /// Maps variable names to their full info
    variable_info: HashMap<InternedString, VariableInfo>,

    /// Current scope level
    current_scope: ScopeLevel,

    /// Stack of variables declared at each scope level
    scope_variables: Vec<Vec<InternedString>>,
}

impl VariableManager {
    /// Creates a new VariableManager
    pub fn new() -> Self {
        VariableManager {
            variable_scopes: HashMap::new(),
            mutability_tracking: HashMap::new(),
            ownership_capability: HashMap::new(),
            variable_info: HashMap::new(),
            current_scope: 0,
            scope_variables: vec![Vec::new()], // Start with scope 0
        }
    }

    // =========================================================================
    // Scope Management
    // =========================================================================

    /// Enters a new scope
    pub fn enter_scope(&mut self) {
        self.current_scope += 1;
        self.scope_variables.push(Vec::new());
    }

    /// Exits the current scope and returns variables that went out of scope
    pub fn exit_scope(&mut self) -> Vec<InternedString> {
        if self.current_scope == 0 {
            return Vec::new();
        }

        let exited_vars = self.scope_variables.pop().unwrap_or_default();

        // Clean up tracking for exited variables
        for var in &exited_vars {
            self.variable_scopes.remove(var);
            self.mutability_tracking.remove(var);
            self.ownership_capability.remove(var);
            self.variable_info.remove(var);
        }

        self.current_scope -= 1;
        exited_vars
    }

    /// Gets the current scope level
    pub fn current_scope_level(&self) -> ScopeLevel {
        self.current_scope
    }

    // =========================================================================
    // Variable Declaration
    // =========================================================================

    /// Declares a new variable in the current scope.
    ///
    /// Returns an HIR node for the variable declaration.
    pub fn declare_variable(
        &mut self,
        name: InternedString,
        data_type: DataType,
        is_mutable: bool,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<HirNode, CompilerError> {
        // Check if variable already exists in current scope
        if let Some(&existing_scope) = self.variable_scopes.get(&name) {
            if existing_scope == self.current_scope {
                return_compiler_error!("Variable already declared in current scope");
            }
        }

        // Determine ownership capability based on type
        let ownership_capable = is_type_ownership_capable(&data_type);

        // Store variable info
        let info = VariableInfo {
            data_type: data_type.clone(),
            is_mutable,
            scope_level: self.current_scope,
            location: location.clone(),
            ownership_capable,
        };

        self.variable_scopes.insert(name, self.current_scope);
        self.mutability_tracking.insert(name, is_mutable);
        self.ownership_capability.insert(name, ownership_capable);
        self.variable_info.insert(name, info);

        // Add to current scope's variable list
        if let Some(scope_vars) = self.scope_variables.last_mut() {
            scope_vars.push(name);
        }

        // Register with context for drop tracking if ownership capable
        if ownership_capable {
            ctx.add_drop_candidate(name, location.clone());
            ctx.mark_potentially_owned(name);
        }

        // Create the HIR assignment node (declarations are assignments in HIR)
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        // Create a placeholder value for the declaration
        // The actual value will be set by a subsequent assignment
        let placeholder_value = HirExpr {
            kind: HirExprKind::Int(0), // Placeholder
            location: location.clone(),
        };

        Ok(HirNode {
            kind: HirKind::Stmt(HirStmt::Assign {
                target: HirPlace::Var(name),
                value: placeholder_value,
                is_mutable,
            }),
            location,
            id: node_id,
        })
    }

    // =========================================================================
    // Variable Reference
    // =========================================================================

    /// Creates an HIR expression for referencing a variable.
    ///
    /// This creates a Load expression for the variable.
    pub fn reference_variable(
        &mut self,
        name: InternedString,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<HirExpr, CompilerError> {
        // Record potential last use for ownership tracking
        ctx.record_potential_last_use(name, location.clone());

        Ok(HirExpr {
            kind: HirExprKind::Load(HirPlace::Var(name)),
            location,
        })
    }

    // =========================================================================
    // Variable Assignment
    // =========================================================================

    /// Creates an HIR node for assigning a value to a variable.
    pub fn assign_variable(
        &mut self,
        target: HirPlace,
        value: HirExpr,
        is_mutable: bool,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<HirNode, CompilerError> {
        // Extract the variable name from the target
        let var_name = extract_var_name(&target)?;

        // Check if variable exists
        if !self.variable_scopes.contains_key(&var_name) {
            return_compiler_error!("Cannot assign to undeclared variable");
        }

        // Check mutability for reassignment
        if let Some(&existing_mutable) = self.mutability_tracking.get(&var_name) {
            if !existing_mutable && !is_mutable {
                // Reassigning to an immutable variable
                return_compiler_error!("Cannot reassign to immutable variable");
            }
        }

        // Create the assignment node
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        Ok(HirNode {
            kind: HirKind::Stmt(HirStmt::Assign {
                target,
                value,
                is_mutable,
            }),
            location,
            id: node_id,
        })
    }

    // =========================================================================
    // Potential Move Handling
    // =========================================================================

    /// Marks a potential ownership consumption point.
    ///
    /// CONSERVATIVE: This marks where ownership could potentially be consumed,
    /// but the actual ownership decision is made by the borrow checker.
    pub fn mark_potential_move(
        &mut self,
        name: InternedString,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<HirExpr, CompilerError> {
        // Check if variable exists
        let info = match self.variable_info.get(&name) {
            Some(info) => info.clone(),
            None => {
                return_compiler_error!("Variable not found for potential move");
            }
        };

        // Only ownership-capable variables can be moved
        if !info.ownership_capable {
            // Return a regular load for non-ownership-capable variables
            return Ok(HirExpr {
                kind: HirExprKind::Load(HirPlace::Var(name)),
                location,
            });
        }

        // Mark as potentially consumed in context
        ctx.mark_potentially_consumed(name);
        ctx.record_potential_last_use(name, location.clone());

        // Return a Move expression
        Ok(HirExpr {
            kind: HirExprKind::Move(HirPlace::Var(name)),
            location,
        })
    }

    // =========================================================================
    // Query Methods
    // =========================================================================
    /// Checks if a variable is mutable
    pub fn is_variable_mutable(&self, name: InternedString) -> bool {
        self.mutability_tracking
            .get(&name)
            .copied()
            .unwrap_or(false)
    }

    /// Checks if a variable is ownership capable
    pub fn is_ownership_capable(&self, name: InternedString) -> bool {
        self.ownership_capability
            .get(&name)
            .copied()
            .unwrap_or(false)
    }

    /// Gets the variable info for a variable
    pub fn get_variable_info(&self, name: InternedString) -> Option<&VariableInfo> {
        self.variable_info.get(&name)
    }

    /// Checks if a variable exists in any scope
    pub fn variable_exists(&self, name: InternedString) -> bool {
        self.variable_scopes.contains_key(&name)
    }

    /// Gets the scope level where a variable was declared
    pub fn get_variable_scope(&self, name: InternedString) -> Option<ScopeLevel> {
        self.variable_scopes.get(&name).copied()
    }

    /// Gets all variables in the current scope
    pub fn get_current_scope_variables(&self) -> &[InternedString] {
        self.scope_variables
            .last()
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Gets all ownership-capable variables in the current scope
    pub fn get_ownership_capable_variables(&self) -> Vec<InternedString> {
        self.get_current_scope_variables()
            .iter()
            .filter(|&name| self.is_ownership_capable(*name))
            .copied()
            .collect()
    }
}

// =========================================================================
// Helper Functions
// =========================================================================

/// Determines if a type is ownership-capable.
///
/// CONSERVATIVE: This is a heuristic - the borrow checker makes final decisions.
pub fn is_type_ownership_capable(data_type: &DataType) -> bool {
    match data_type {
        // Primitive types are typically not ownership capable (copy semantics)
        DataType::Int | DataType::Float | DataType::Bool | DataType::Char => false,

        // String slices are borrowed, not owned
        DataType::String => false,

        // None type is not ownership-capable
        DataType::None => false,

        // Collections and structs are ownership-capable
        DataType::Collection(_, _) => true,
        DataType::Struct(_, _) => true,
        DataType::Parameters(_) => true,

        // Templates can be ownership-capable
        DataType::Template => true,

        // Functions are typically not ownership-capable
        DataType::Function(_, _) => false,

        // References depend on what they reference
        DataType::Reference(inner) => is_type_ownership_capable(inner),

        // Inferred types are conservatively ownership-capable
        DataType::Inferred => true,

        // Other types are conservatively ownership-capable
        _ => true,
    }
}

/// Extracts the variable name from an HIR place.
pub fn extract_var_name(place: &HirPlace) -> Result<InternedString, CompilerError> {
    match place {
        HirPlace::Var(name) => Ok(*name),
        HirPlace::Field { base, .. } => extract_var_name(base),
        HirPlace::Index { base, .. } => extract_var_name(base),
    }
}
