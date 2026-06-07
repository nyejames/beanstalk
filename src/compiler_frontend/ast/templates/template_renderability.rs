//! Template-head value renderability classification.
//!
//! WHAT: determines whether a semantic type is allowed as a non-template,
//!       non-path expression in a template head.
//! WHY: template-head validation must use semantic `TypeId` identity through
//!      `TypeEnvironment`, not parse-time `DataType` representations.

use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;

/// Returns `true` if `type_id` is a scalar or textual type that can be
/// rendered directly into template output.
///
/// WHAT: accepts the built-in scalar/textual types that the compiler
///       supports for template rendering.
/// WHY: positive list keeps the policy explicit and easy to extend.
///
/// Allowed: String, Int, Float, Bool, Char.
/// Rejected: structs, const records, choices, collections, functions,
///           external opaque types, dynamic trait values, generic instances,
///           generic parameters, and other builtin types such as Range and None.
pub(crate) fn is_template_renderable_type(
    type_id: TypeId,
    type_environment: &TypeEnvironment,
) -> bool {
    let builtins = type_environment.builtins();
    type_id == builtins.string
        || type_id == builtins.int
        || type_id == builtins.float
        || type_id == builtins.bool
        || type_id == builtins.char
}
