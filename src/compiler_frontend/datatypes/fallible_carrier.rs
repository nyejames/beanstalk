//! Internal fallible carrier helpers.
//!
//! WHAT: owns construction and slot queries for the temporary carrier used to move
//! fallible-operation results between AST, HIR, and backend lowering.
//! WHY: public Beanstalk has fallible control flow, not first-class `Result`
//! values. Keeping carrier construction here makes each remaining use explicit
//! implementation machinery rather than a public type surface.

use super::definitions::TypeDefinition;
use super::ids::{BuiltinTypeConstructor, TypeConstructor, TypeId};
use super::{DataType, TypeEnvironment};

pub(crate) fn fallible_carrier_constructor() -> TypeConstructor {
    TypeConstructor::Builtin(BuiltinTypeConstructor::FallibleCarrier)
}

impl DataType {
    pub fn fallible_carrier(success: DataType, error: DataType) -> Self {
        Self::FallibleCarrier {
            success: Box::new(success),
            error: Box::new(error),
        }
    }

    /// Returns true for the diagnostic-only fallible carrier.
    ///
    /// Do not use this for semantic checks when a `TypeId` and `TypeEnvironment`
    /// are available; use `TypeEnvironment::is_fallible_carrier` instead.
    pub fn is_fallible_carrier(&self) -> bool {
        matches!(self, DataType::FallibleCarrier { .. })
    }

    /// Returns the diagnostic success side of an internal fallible carrier.
    ///
    /// New semantic checks should use `TypeEnvironment::fallible_carrier_slots`
    /// on a canonical `TypeId`. This helper remains for diagnostic/test-support
    /// paths that still carry pre-lowering `DataType::FallibleCarrier` values.
    pub fn fallible_carrier_success_type(&self) -> Option<&DataType> {
        match self {
            DataType::FallibleCarrier { success, .. } => Some(success.as_ref()),
            _ => None,
        }
    }

    /// Returns the diagnostic error side of an internal fallible carrier.
    ///
    /// New semantic checks should use `TypeEnvironment::fallible_carrier_slots`
    /// on a canonical `TypeId`. This helper remains for diagnostic/test-support
    /// paths that still carry pre-lowering `DataType::FallibleCarrier` values.
    pub fn fallible_carrier_error_type(&self) -> Option<&DataType> {
        match self {
            DataType::FallibleCarrier { error, .. } => Some(error.as_ref()),
            _ => None,
        }
    }
}

impl TypeEnvironment {
    /// Interns the temporary fallible carrier used only at handled fallible boundaries.
    pub fn intern_fallible_carrier(&mut self, success: TypeId, error: TypeId) -> TypeId {
        self.intern_constructed(fallible_carrier_constructor(), Box::new([success, error]))
    }

    /// Returns true if the type is the temporary internal fallible carrier.
    ///
    /// Public Beanstalk does not expose first-class `Result` values. This query is retained only
    /// for fallible-operation validation and HIR lowering while those operations are immediately
    /// consumed by postfix `!` or boundary `catch`.
    pub fn is_fallible_carrier(&self, id: TypeId) -> bool {
        self.fallible_carrier_slots(id).is_some()
    }

    /// Returns the (success, error) slots of the temporary internal fallible carrier, if any.
    pub fn fallible_carrier_slots(&self, id: TypeId) -> Option<(TypeId, TypeId)> {
        match self.get(id) {
            Some(TypeDefinition::Constructed(constructed))
                if constructed.constructor == fallible_carrier_constructor() =>
            {
                if let [success, error] = constructed.arguments.as_ref() {
                    Some((*success, *error))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Returns the success type of a fallible carrier, if any.
    pub fn fallible_success_type(&self, id: TypeId) -> Option<TypeId> {
        self.fallible_carrier_slots(id).map(|(success, _)| success)
    }

    /// Returns the error type of a fallible carrier, if any.
    pub fn fallible_error_type(&self, id: TypeId) -> Option<TypeId> {
        self.fallible_carrier_slots(id).map(|(_, error)| error)
    }
}
