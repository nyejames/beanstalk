//! Parameter Analysis - Simplified Parameter Lifetime Inference (No Reference Returns)
//!
//! This module implements **simplified parameter lifetime analysis** without reference
//! returns. The current implementation assumes all function returns are value returns,
//! which allows the lifetime inference fix to focus on core correctness issues while
//! deferring the complexity of reference returns to future implementation phases.
//!
//! ## Reference Return Limitation - Design Decision
//!
//! The decision to exclude reference returns from the current implementation is
//! **intentional and strategic**:
//!
//! ### Why Reference Returns Are Deferred
//! 1. **Complexity Isolation**: Reference returns add significant complexity to lifetime
//!    inference that would obscure the core algorithmic improvements
//! 2. **Incremental Development**: Fixing the fundamental issues first provides a solid
//!    foundation for future reference return implementation
//! 3. **Language Subset**: The current Beanstalk language subset doesn't heavily rely
//!    on reference returns, making this a reasonable temporary limitation
//! 4. **Correctness First**: Ensuring the algebraic approach works correctly for the
//!    common case before tackling the complex case
//!
//! ### Current Behavior
//! - **All Returns**: Treated as value returns (ownership transfer)
//! - **Parameter Lifetimes**: Scoped to function boundaries only
//! - **No Cross-Function**: Parameter lifetimes don't propagate across calls
//! - **Conservative Analysis**: When uncertain, extend lifetimes to function exit
//!
//! ### Impact on Algebraic Approach
//! This simplification allows the algebraic approach to focus on its core strengths:
//! - **Intra-Function Analysis**: Perfect for function-scoped borrow tracking
//! - **Set Operations**: Efficient for parameter borrow propagation within functions
//! - **Fixpoint Convergence**: Guaranteed convergence without cross-function complexity
//! - **Clear Semantics**: Unambiguous lifetime boundaries at function calls
//!
//! ## Current Limitations (Intentional Simplifications)
//!
//! - **No Reference Returns**: All function returns treated as value returns
//! - **Function-Scoped Analysis**: Parameter lifetimes limited to function CFG boundaries
//! - **Conservative Approach**: When uncertain, extend lifetimes to function exit
//! - **No Cross-Function Analysis**: Each function analyzed independently
//!
//! ## Future Implementation (Deferred)
//!
//! When the compiler pipeline is ready for reference returns, this module will be
//! extended to support:
//! - Return origin tracking (`-> param_name` syntax)
//! - Parameter-to-return lifetime relationships
//! - Reference return validation and soundness checking
//! - Cross-function lifetime propagation
//!
//! ## Design Rationale
//!
//! The previous implementation attempted to handle reference returns but did so
//! incorrectly, extending parameter lifetimes to all CFG exits in an over-conservative
//! manner that violated language semantics. This simplified approach provides
//! correct behavior for the current language subset while establishing a clean
//! foundation for future reference return support.

use crate::compiler::borrow_checker::lifetime_inference::borrow_live_sets::BorrowLiveSets;
use crate::compiler::borrow_checker::types::{BorrowId, CfgNodeId};
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::hir::nodes::{HirKind, HirNode};
use crate::compiler::hir::place::{Place, PlaceRoot};
use crate::compiler::parsers::statements::functions::FunctionSignature;

use std::collections::HashMap;

/// Parameter lifetime information for a single function
///
/// Contains the computed lifetime information for all parameters in a function,
/// scoped to that function's CFG boundaries only.
#[derive(Debug, Clone)]
pub(crate) struct FunctionParameterInfo {
    /// Function identifier
    pub(crate) function_id: CfgNodeId, // Using CFG node ID as function identifier

    /// Parameter borrows tracked within this function
    pub(crate) parameter_borrows: HashMap<Place, Vec<BorrowId>>,

    /// Lifetime spans for each parameter (within function scope only)
    pub(crate) parameter_lifetimes: HashMap<Place, ParameterLifetime>,

    /// Whether this function has any parameter borrows
    pub(crate) has_parameter_borrows: bool,
}

/// Lifetime information for a single parameter
///
/// Represents the computed lifetime of a parameter within its function scope.
/// Does not extend beyond function boundaries in the current implementation.
#[derive(Debug, Clone)]
pub(crate) struct ParameterLifetime {
    /// Parameter place
    pub(crate) place: Place,

    /// CFG nodes where this parameter is borrowed
    pub(crate) borrow_points: Vec<CfgNodeId>,

    /// CFG nodes where this parameter is used (last-use analysis)
    pub(crate) usage_points: Vec<CfgNodeId>,

    /// Computed last-use point within function scope
    pub(crate) last_use_point: Option<CfgNodeId>,

    /// Whether this parameter could potentially be returned (future feature)
    /// Currently always false since reference returns are not supported
    pub(crate) potentially_returned: bool,
}

/// Complete parameter lifetime analysis result
///
/// Contains parameter lifetime information for all functions analyzed,
/// organized for efficient lookup and integration with other components.
#[derive(Debug, Clone)]
pub(crate) struct ParameterLifetimeInfo {
    /// Parameter information per function
    pub(crate) functions: HashMap<CfgNodeId, FunctionParameterInfo>,

    /// Global parameter borrow tracking (for integration with live sets)
    pub(crate) all_parameter_borrows: HashMap<BorrowId, ParameterLifetime>,

    /// Statistics for debugging and validation
    pub(crate) total_functions_analyzed: usize,
    pub(crate) total_parameter_borrows: usize,
}

/// Parameter lifetime analysis engine
///
/// Implements simplified parameter lifetime analysis that focuses on
/// function-scoped parameter usage without reference return complexity.
pub(crate) struct ParameterAnalysis {
    /// Current analysis state
    functions: HashMap<CfgNodeId, FunctionParameterInfo>,

    /// Global parameter borrow tracking
    parameter_borrows: HashMap<BorrowId, ParameterLifetime>,
}

impl ParameterAnalysis {
    /// Create a new parameter analysis engine
    pub(crate) fn new() -> Self {
        Self {
            functions: HashMap::new(),
            parameter_borrows: HashMap::new(),
        }
    }

    /// Analyze parameter lifetimes for all functions in the HIR
    ///
    /// This is the main entry point that processes all function definitions
    /// and computes parameter lifetime information within function scope.
    ///
    /// ## Current Limitations (Reference Returns Deferred)
    /// - Only analyzes parameters within function boundaries
    /// - Does not track parameter-to-return relationships
    /// - Assumes all function returns are value returns
    ///
    /// TODO: When reference returns are implemented, extend this to:
    /// - Analyze return origin declarations (`-> param_name` syntax)
    /// - Track which parameters can be returned as references
    /// - Validate that returned references originate from declared parameters
    /// - Compute cross-function lifetime relationships
    /// - Integrate with call-site lifetime propagation
    pub(crate) fn analyze_parameters(
        &mut self,
        hir_nodes: &[HirNode],
        live_sets: &BorrowLiveSets,
    ) -> Result<ParameterLifetimeInfo, CompilerMessages> {
        // Starting parameter lifetime analysis

        // Clear any previous analysis state
        self.functions.clear();
        self.parameter_borrows.clear();

        // Process each function definition
        for node in hir_nodes {
            if let HirKind::FunctionDef { signature, .. } = &node.kind {
                self.analyze_function_parameters(node.id, signature, live_sets)?;
            }
        }

        // Build final result
        let result = ParameterLifetimeInfo {
            functions: self.functions.clone(),
            all_parameter_borrows: self.parameter_borrows.clone(),
            total_functions_analyzed: self.functions.len(),
            total_parameter_borrows: self.parameter_borrows.len(),
        };

        // Parameter analysis complete

        Ok(result)
    }

    /// Analyze parameters for a single function
    ///
    /// Processes all parameters in a function and computes their lifetimes
    /// within the function's CFG boundaries only.
    fn analyze_function_parameters(
        &mut self,
        function_id: CfgNodeId,
        signature: &FunctionSignature,
        live_sets: &BorrowLiveSets,
    ) -> Result<(), CompilerMessages> {
        let mut function_info = FunctionParameterInfo {
            function_id,
            parameter_borrows: HashMap::new(),
            parameter_lifetimes: HashMap::new(),
            has_parameter_borrows: false,
        };

        // Process each parameter in the function signature
        for param in &signature.parameters {
            let param_place = Place {
                root: PlaceRoot::Param(param.id), // Use parameter name as ID
                projections: Vec::new(),
            };

            // Find all borrows of this parameter in the live sets
            let param_borrows = self.find_parameter_borrows(&param_place, live_sets);

            if !param_borrows.is_empty() {
                // Compute lifetime information for this parameter
                let param_lifetime = self.compute_parameter_lifetime(
                    param_place.clone(),
                    &param_borrows,
                    live_sets,
                )?;

                function_info
                    .parameter_borrows
                    .insert(param_place.clone(), param_borrows.clone());
                function_info
                    .parameter_lifetimes
                    .insert(param_place.clone(), param_lifetime.clone());
                function_info.has_parameter_borrows = true;

                // Add to global parameter borrow tracking
                for &borrow_id in &param_borrows {
                    self.parameter_borrows
                        .insert(borrow_id, param_lifetime.clone());
                }
            }
        }

        // Store function information
        self.functions.insert(function_id, function_info);

        Ok(())
    }

    /// Find all borrows of a specific parameter in the live sets
    ///
    /// Searches through all live sets to find borrows that target the given parameter.
    fn find_parameter_borrows(
        &self,
        param_place: &Place,
        live_sets: &BorrowLiveSets,
    ) -> Vec<BorrowId> {
        let mut param_borrows = Vec::new();

        // Check all borrows to see which ones target this parameter
        for borrow_id in live_sets.all_borrows() {
            if let Some(borrow_place) = live_sets.borrow_place(borrow_id)
                && self.places_match(param_place, borrow_place)
            {
                param_borrows.push(borrow_id);
            }
        }

        param_borrows
    }

    /// Check if two places match for parameter analysis
    ///
    /// Determines if a borrow place corresponds to a parameter place,
    /// handling projections (field access, array indexing) appropriately.
    fn places_match(&self, param_place: &Place, borrow_place: &Place) -> bool {
        // For now, use simple equality check
        // A more sophisticated implementation would handle projections
        // and determine if the borrow place is derived from the parameter
        param_place == borrow_place || self.is_derived_from_parameter(param_place, borrow_place)
    }

    /// Check if a borrow place is derived from a parameter
    ///
    /// Determines if a place like `param.field` or `param[index]` is derived
    /// from a parameter place like `param`.
    fn is_derived_from_parameter(&self, param_place: &Place, borrow_place: &Place) -> bool {
        // Check if the borrow place has the same root as the parameter
        if param_place.root != borrow_place.root {
            return false;
        }

        // Check if the borrow place is a projection of the parameter
        // (e.g., param.field is derived from param)
        borrow_place.projections.len() >= param_place.projections.len()
            && borrow_place
                .projections
                .starts_with(&param_place.projections)
    }

    /// Compute lifetime information for a parameter
    ///
    /// Analyzes the usage patterns of a parameter within its function scope
    /// and computes appropriate lifetime information.
    fn compute_parameter_lifetime(
        &self,
        param_place: Place,
        param_borrows: &[BorrowId],
        live_sets: &BorrowLiveSets,
    ) -> Result<ParameterLifetime, CompilerMessages> {
        let mut borrow_points = Vec::new();
        let mut usage_points = Vec::new();

        // Collect borrow and usage points for all borrows of this parameter
        for &borrow_id in param_borrows {
            // Get creation point (borrow point)
            if let Some(creation_point) = live_sets.creation_point(borrow_id) {
                borrow_points.push(creation_point);
            }

            // Get all usage points
            let borrow_usage_points = live_sets.usage_points(borrow_id);
            usage_points.extend(borrow_usage_points);
        }

        // Remove duplicates and sort
        borrow_points.sort_unstable();
        borrow_points.dedup();
        usage_points.sort_unstable();
        usage_points.dedup();

        // Compute last-use point (latest usage within function scope)
        let last_use_point = usage_points.iter().max().copied();

        // Note: potentially_returned is always false in current implementation
        // This is a deliberate simplification to focus on core lifetime inference correctness

        // TODO: When reference returns are supported, implement return analysis:
        // 1. Scan function return statements for references to this parameter
        // 2. Check if parameter is declared as return origin (`-> param_name`)
        // 3. Validate that all reference returns originate from declared parameter
        // 4. Compute parameter-to-return lifetime relationships
        // 5. Integrate with cross-function lifetime propagation
        let potentially_returned = false;

        Ok(ParameterLifetime {
            place: param_place,
            borrow_points,
            usage_points,
            last_use_point,
            potentially_returned,
        })
    }

    /// Get parameter lifetime information for a specific function
    pub(crate) fn get_function_parameters(
        &self,
        function_id: CfgNodeId,
    ) -> Option<&FunctionParameterInfo> {
        self.functions.get(&function_id)
    }

    /// Get parameter lifetime information for a specific borrow
    pub(crate) fn get_parameter_lifetime(&self, borrow_id: BorrowId) -> Option<&ParameterLifetime> {
        self.parameter_borrows.get(&borrow_id)
    }

    /// Check if a borrow is a parameter borrow
    pub(crate) fn is_parameter_borrow(&self, borrow_id: BorrowId) -> bool {
        self.parameter_borrows.contains_key(&borrow_id)
    }

    /// Get all functions that have parameter borrows
    pub(crate) fn functions_with_parameter_borrows(&self) -> impl Iterator<Item = CfgNodeId> + '_ {
        self.functions
            .iter()
            .filter(|(_, info)| info.has_parameter_borrows)
            .map(|(&function_id, _)| function_id)
    }

    /// Validate parameter lifetime soundness
    ///
    /// Ensures that computed parameter lifetimes are sound and conservative
    /// within the current implementation's constraints.
    pub(crate) fn validate_parameter_lifetimes(&self) -> Result<(), CompilerMessages> {
        let mut errors = Vec::new();

        for (borrow_id, param_lifetime) in &self.parameter_borrows {
            // Check 1: All borrow points should come before usage points
            for &borrow_point in &param_lifetime.borrow_points {
                for &usage_point in &param_lifetime.usage_points {
                    if borrow_point > usage_point {
                        let error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
                            msg: format!(
                                "Parameter lifetime soundness violation: Borrow {:?} has borrow point {:?} after usage point {:?}",
                                borrow_id, borrow_point, usage_point
                            ),
                            location: crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(),
                            error_type: crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                            metadata: std::collections::HashMap::new(),
                        };
                        errors.push(error);
                    }
                }
            }

            // Check 2: Last-use point should be the latest usage point
            if let Some(last_use) = param_lifetime.last_use_point
                && let Some(&latest_usage) = param_lifetime.usage_points.iter().max()
                && last_use != latest_usage
            {
                let error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
                    msg: format!(
                        "Parameter lifetime soundness violation: Borrow {:?} has incorrect last-use point {:?}, expected {:?}",
                        borrow_id, last_use, latest_usage
                    ),
                    location:
                        crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(
                        ),
                    error_type:
                        crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                    metadata: std::collections::HashMap::new(),
                };
                errors.push(error);
            }

            // Check 3: Reference returns should not be marked as potentially returned
            // (since they're not supported in current implementation)
            if param_lifetime.potentially_returned {
                let error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
                    msg: format!(
                        "Parameter lifetime error: Borrow {:?} marked as potentially returned, but reference returns are not supported in current implementation",
                        borrow_id
                    ),
                    location:
                        crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(
                        ),
                    error_type:
                        crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                    metadata: std::collections::HashMap::new(),
                };
                errors.push(error);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(CompilerMessages {
                errors,
                warnings: Vec::new(),
            })
        }
    }
}

impl Default for ParameterAnalysis {
    fn default() -> Self {
        Self::new()
    }
}

// TODO: Future reference return support
// When the compiler pipeline is ready for reference returns, add:

/*
/// Reference return analysis (FUTURE IMPLEMENTATION)
///
/// This will be implemented when the compiler pipeline supports reference returns.
/// The analysis will track parameter-to-return relationships and validate that
/// returned references originate from declared parameter origins.
pub(crate) struct ReferenceReturnAnalysis {
    /// Functions that return references
    returning_functions: HashMap<CfgNodeId, ReturnOriginInfo>,

    /// Parameter-to-return lifetime relationships
    return_lifetimes: HashMap<BorrowId, ReturnLifetime>,
}

/// Information about reference return origins (FUTURE)
#[derive(Debug, Clone)]
pub(crate) struct ReturnOriginInfo {
    /// Declared return origin parameter
    origin_parameter: Place,

    /// Return points in the function
    return_points: Vec<CfgNodeId>,

    /// Validation that all returns originate from the declared parameter
    origin_validated: bool,
}

/// Lifetime relationship between parameter and return (FUTURE)
#[derive(Debug, Clone)]
pub(crate) struct ReturnLifetime {
    /// Parameter that is returned
    parameter_place: Place,

    /// Borrow that is returned
    returned_borrow: BorrowId,

    /// Return points where this borrow is returned
    return_points: Vec<CfgNodeId>,
}
*/
