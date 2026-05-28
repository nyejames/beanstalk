//! Fallible success-slot helpers.
//!
//! WHAT: extracts success-slot type IDs from a fallible carrier type.
//! WHY: catch parsing needs the success arity to activate the shared value-production
//! target before the handler body is parsed.

use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;

/// Builds the list of success type IDs for a handled fallible expression.
///
/// WHAT: given a fallible carrier type, returns the success-slot type IDs it contains.
/// WHY: multi-success fallible calls return tuples, and the fallback path needs the same arity
/// to align produced values with the success path.
pub(crate) fn fallible_success_type_ids(
    result_type_id: TypeId,
    type_environment: &TypeEnvironment,
) -> Vec<TypeId> {
    let Some((success_type_id, _error_type_id)) =
        type_environment.fallible_carrier_slots(result_type_id)
    else {
        // Not a fallible carrier type: no success slots to extract.
        return vec![];
    };

    if success_type_id == type_environment.builtins().none {
        // The success slot is explicitly None: the handled expression produces no value.
        return vec![];
    }

    if let Some(tuple_fields) = type_environment.tuple_field_ids(success_type_id) {
        // Multi-success call: expand the tuple into its individual field types.
        return tuple_fields.to_vec();
    }

    // Single success value: return it as a one-element vector so callers have uniform arity.
    vec![success_type_id]
}
