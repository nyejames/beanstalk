//! Compact type identifiers and canonical keys.
//!
//! WHAT: defines all dense ID newtypes and the stable keys used for interning.
//! WHY: `TypeId` equality is the canonical semantic type equality check.
//!      Deterministic lookup comes from stable keys, not from numeric IDs.

use crate::compiler_frontend::external_packages::ExternalTypeId;

// -----------------------------------------------------------
//  Compact Type Identifiers
// -----------------------------------------------------------

/// Dense module-local type identifier.
///
/// Valid only with the `TypeEnvironment` that created it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeId(pub u32);

/// Builtin `TypeId` constants matching the seeding order of `TypeEnvironment::new()`.
///
/// WHAT: deterministic IDs for builtin types so that factory methods and tests
/// can construct expressions without threading a full `TypeEnvironment`.
///
/// WHY: `Expression::int()` and similar factories should not need `&TypeEnvironment`
/// just to refer to `Int`. These constants are valid for any `TypeEnvironment`
/// created via `TypeEnvironment::new()` or `Default::default()`.
///
/// WARNING: if `TypeEnvironment::new()` changes its builtin seeding order,
/// these constants must be updated to match.
pub mod builtin_type_ids {
    use super::TypeId;

    pub const BOOL: TypeId = TypeId(0);
    pub const INT: TypeId = TypeId(1);
    pub const FLOAT: TypeId = TypeId(2);
    pub const DECIMAL: TypeId = TypeId(3);
    pub const STRING: TypeId = TypeId(4);
    pub const CHAR: TypeId = TypeId(5);
    pub const RANGE: TypeId = TypeId(6);
    pub const NONE: TypeId = TypeId(7);
}

/// Dense identifier for a nominal struct or choice definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NominalTypeId(pub u32);

/// Dense identifier for a single generic parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenericParameterId(pub u32);

/// Dense identifier for a list of generic parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenericParameterListId(pub u32);

/// Dense identifier for a function type definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionTypeId(pub u32);

// -----------------------------------------------------------
//  Canonical Keys
// -----------------------------------------------------------

/// Stable key for canonical type lookup.
///
/// WHAT: encodes everything needed to decide whether a type already exists.
/// WHY: two types with the same `TypeKey` must share the same `TypeId`.
#[allow(dead_code)] // Planned: stable canonical type lookup key for TypeEnvironment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeKey {
    Builtin(BuiltinTypeKey),
    Nominal(NominalTypeId),
    Constructed(ConstructedTypeKey),
    GenericParameter(GenericParameterId),
    Function(FunctionTypeKey),
    External(ExternalTypeId),
}

/// Keys for builtin scalar types.
///
/// These are seeded once when `TypeEnvironment` is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinTypeKey {
    Bool,
    Int,
    Float,
    Decimal,
    String,
    Char,
    Range,
    None,
}

/// A type constructor paired with its arguments.
///
/// Used for collections, options, results, and nominal generic instances.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeConstructor {
    Builtin(BuiltinTypeConstructor),
    Nominal(NominalTypeId),
    External(ExternalTypeId),
}

/// Builtin type constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinTypeConstructor {
    Collection,
    Option,
    FallibleCarrier,
    Tuple,
}

/// Key for a constructed type (collection, option, result, or nominal instance).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstructedTypeKey {
    pub constructor: TypeConstructor,
    pub arguments: Box<[TypeId]>,
}

/// Key for a generic nominal instance.
///
/// WHAT: base nominal + concrete argument IDs.
/// WHY: canonicalises `Box of Int` so repeated uses share one `TypeId`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericInstanceKey {
    pub base: NominalTypeId,
    pub arguments: Box<[TypeId]>,
}

/// Key for a function type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionTypeKey {
    pub parameters: Box<[TypeId]>,
    pub returns: Box<[TypeId]>,
    pub error_return: Option<TypeId>,
}
