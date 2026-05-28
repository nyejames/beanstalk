//! Named-type walkers for parsed and diagnostic type surfaces.
//!
//! WHAT: visits nominal type names embedded in parsed refs and DataType diagnostics.
//! WHY: dependency discovery and validation need traversal without taking ownership of
//! type-resolution policy.

use super::*;

/// Visit every named type reference inside a `ParsedTypeRef`.
pub(crate) fn for_each_named_type_in_parsed_ref(
    parsed: &ParsedTypeRef,
    visitor: &mut impl FnMut(StringId),
) {
    match parsed {
        ParsedTypeRef::Named { name, .. } => visitor(*name),
        ParsedTypeRef::Applied {
            base, arguments, ..
        } => {
            for_each_named_type_in_parsed_ref(base, visitor);
            for argument in arguments {
                for_each_named_type_in_parsed_ref(argument, visitor);
            }
        }
        ParsedTypeRef::Collection { element, .. }
        | ParsedTypeRef::Optional { inner: element, .. } => {
            for_each_named_type_in_parsed_ref(element, visitor);
        }
        ParsedTypeRef::Result { ok, err, .. } => {
            for_each_named_type_in_parsed_ref(ok, visitor);
            for_each_named_type_in_parsed_ref(err, visitor);
        }
        _ => {}
    }
}
