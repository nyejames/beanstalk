//! Backend-agnostic ABI types for the external-call boundary.
//!
//! WHAT: defines the type system that external functions use to describe their parameters
//! and return values to the frontend. This is a narrower vocabulary than the full Beanstalk
//! type system because host boundaries are intentionally restricted.
//! WHY: the frontend needs to know how to validate and lower arguments without embedding
//! backend-specific knowledge into the AST.

use crate::compiler_frontend::datatypes::DataType;

/// Backend-agnostic ABI values that currently cross the host boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternalAbiType {
    I32,
    F64,
    Utf8Str,
    Void,
    /// Opaque handle to an external type (lowers to `i32` in Wasm, object reference in JS).
    Handle,
    /// Parameter accepts any language type (used for polymorphic external functions
    /// such as collection helpers and `io()` during the transition to explicit ABI types).
    Inferred,
}

impl ExternalAbiType {
    /// Maps this ABI type to the corresponding frontend `DataType` when one exists.
    pub(crate) fn to_datatype(&self) -> Option<DataType> {
        match self {
            ExternalAbiType::I32 => Some(DataType::Int),
            ExternalAbiType::F64 => Some(DataType::Float),
            ExternalAbiType::Utf8Str => Some(DataType::StringSlice),
            ExternalAbiType::Void => None,
            ExternalAbiType::Handle => None,
            ExternalAbiType::Inferred => Some(DataType::Inferred),
        }
    }
}

/// A single external-call parameter definition.
#[derive(Debug, Clone)]
pub struct ExternalParameter {
    /// What the Beanstalk language accepts.
    pub language_type: ExternalAbiType,
    /// Borrow access mode required for this argument.
    pub access_kind: ExternalAccessKind,
}

/// Borrow access mode for an external parameter or receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalAccessKind {
    Shared,
    Mutable,
}

/// Describes how an external function's return value aliases its arguments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternalReturnAlias {
    /// Return value is freshly allocated and does not alias any argument.
    Fresh,
    /// Return value may alias the arguments at the given parameter indices.
    AliasArgs(Vec<usize>),
}
