//! Definitions for external functions, types, constants, and packages.
//!
//! WHAT: describes the metadata for individual external symbols and the packages that group them.
//! WHY: the registry stores these definitions so the frontend and backends can query signatures
//! and lowering metadata without re-parsing binding files.

use super::abi::{ExternalAbiType, ExternalAccessKind, ExternalParameter, ExternalReturnAlias};
use crate::compiler_frontend::ast::statements::functions::{FunctionReturn, ReturnSlot};
use crate::compiler_frontend::datatypes::DataType;
use std::collections::HashMap;

/// Backend-specific lowering metadata for an external function.
#[derive(Debug, Clone, Default)]
pub struct ExternalFunctionLowerings {
    pub js: Option<ExternalJsLowering>,
    pub wasm: Option<ExternalWasmLowering>,
}

/// JavaScript backend lowering strategy for an external function.
#[derive(Debug, Clone)]
pub enum ExternalJsLowering {
    /// Emit a call to a named JS runtime helper function.
    RuntimeFunction(&'static str),
    /// Emit an inline JS expression (not used yet, reserved for future optimization).
    InlineExpression(&'static str),
}

/// Wasm backend lowering strategy for an external function.
/// Placeholder: Wasm external support is still experimental.
#[derive(Debug, Clone)]
pub enum ExternalWasmLowering {
    HostFunction(&'static str),
}

/// Full definition of a single external function.
#[derive(Debug, Clone)]
pub struct ExternalFunctionDef {
    pub name: &'static str,
    pub parameters: Vec<ExternalParameter>,
    pub return_type: ExternalAbiType,
    pub return_alias: ExternalReturnAlias,
    /// If this function is a receiver method, the ABI type of the receiver.
    /// The first entry in `parameters` is the receiver argument.
    pub receiver_type: Option<ExternalAbiType>,
    /// Access kind required for the receiver when this is a method.
    pub receiver_access: ExternalAccessKind,
    /// Backend-specific lowering metadata.
    pub lowerings: ExternalFunctionLowerings,
}

impl ExternalFunctionDef {
    pub(crate) fn return_type_to_datatype(&self) -> Option<DataType> {
        self.return_type.to_datatype()
    }

    pub(crate) fn return_slots(&self) -> Vec<ReturnSlot> {
        let Some(return_data_type) = self.return_type_to_datatype() else {
            return Vec::new();
        };

        match self.return_alias {
            ExternalReturnAlias::Fresh => {
                vec![ReturnSlot::success(FunctionReturn::Value(return_data_type))]
            }
            ExternalReturnAlias::AliasArgs(ref parameter_indices) => {
                vec![ReturnSlot::success(FunctionReturn::AliasCandidates {
                    parameter_indices: parameter_indices.clone(),
                    data_type: return_data_type,
                })]
            }
        }
    }

    pub(crate) fn return_data_types(&self) -> Vec<DataType> {
        self.return_slots()
            .iter()
            .map(|slot| slot.data_type().clone())
            .collect()
    }
}

/// Definition of a single opaque external type exposed by a virtual package.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalTypeDef {
    pub name: &'static str,
    pub package: &'static str,
    pub abi_type: ExternalAbiType,
}

/// Compile-time value for an external package constant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExternalConstantValue {
    Float(f64),
    Int(i64),
    StringSlice(&'static str),
    Bool(bool),
}

impl ExternalConstantValue {
    /// Returns true for scalar values that are valid in const contexts.
    pub fn is_scalar(self) -> bool {
        matches!(self, Self::Float(_) | Self::Int(_) | Self::Bool(_))
    }
}

/// Definition of a single external constant exposed by a virtual package.
#[derive(Debug, Clone)]
pub struct ExternalConstantDef {
    pub name: &'static str,
    pub data_type: ExternalAbiType,
    pub value: ExternalConstantValue,
}

/// A single virtual package provided by a project builder.
#[derive(Clone, Debug, Default)]
pub struct ExternalPackage {
    pub path: &'static str,
    pub functions: HashMap<&'static str, ExternalFunctionDef>,
    pub types: HashMap<&'static str, ExternalTypeDef>,
    pub constants: HashMap<&'static str, ExternalConstantDef>,
}

impl ExternalPackage {
    pub fn new(path: &'static str) -> Self {
        Self {
            path,
            functions: HashMap::new(),
            types: HashMap::new(),
            constants: HashMap::new(),
        }
    }

    pub fn with_function(mut self, function: ExternalFunctionDef) -> Self {
        self.functions.insert(function.name, function);
        self
    }

    pub fn with_type(mut self, type_def: ExternalTypeDef) -> Self {
        self.types.insert(type_def.name, type_def);
        self
    }
}

/// Builder-friendly spec for registering an external function.
///
/// WHAT: carries the metadata needed to register a function without forcing
/// the caller to construct the full `ExternalFunctionDef` and pick a stable ID.
/// WHY: builder packages should not need to hardcode `ExternalFunctionId` enum variants.
#[derive(Debug, Clone)]
pub struct ExternalFunctionSpec {
    pub name: &'static str,
    pub parameters: Vec<ExternalParameter>,
    pub return_type: ExternalAbiType,
    pub return_alias: ExternalReturnAlias,
    pub receiver_type: Option<ExternalAbiType>,
    pub receiver_access: ExternalAccessKind,
    pub lowerings: ExternalFunctionLowerings,
}

impl From<ExternalFunctionSpec> for ExternalFunctionDef {
    fn from(spec: ExternalFunctionSpec) -> Self {
        ExternalFunctionDef {
            name: spec.name,
            parameters: spec.parameters,
            return_type: spec.return_type,
            return_alias: spec.return_alias,
            receiver_type: spec.receiver_type,
            receiver_access: spec.receiver_access,
            lowerings: spec.lowerings,
        }
    }
}

/// Builder-friendly spec for registering an external type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalTypeSpec {
    pub name: &'static str,
    pub abi_type: ExternalAbiType,
}
