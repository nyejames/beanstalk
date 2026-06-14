//! Definitions for external functions, types, constants, and packages.
//!
//! WHAT: describes the metadata for individual external symbols and the packages that group them.
//! WHY: the registry stores these definitions so the frontend and backends can query signatures
//! and lowering metadata without re-parsing binding files.

use super::abi::{ExternalAbiType, ExternalParameter, ExternalReturnAlias, ExternalSignatureType};
use super::ids::{ExternalPackageId, ExternalPackageOrigin};
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
    RuntimeFunction(String),
    /// Emit an inline JS expression by substituting lowered arguments into a template.
    InlineExpression(String),
    /// Provider-created ES module export that generated HTML glue must import and call.
    ExternalModuleExport { export_name: String },
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
    pub name: String,
    pub parameters: Vec<ExternalParameter>,
    /// Success-channel return slots exposed to Beanstalk callers.
    ///
    /// WHAT: external functions use the same success/error signature shape as source
    /// functions, but the signature is supplied by builder metadata rather than parsed
    /// source.
    /// WHY: fallible external calls must be handled with postfix `!` or `catch` without
    /// manufacturing public raw `Result` values.
    pub returns: Vec<ExternalReturnSlot>,
    /// Optional final error slot. This maps to `T!` on source functions.
    pub error_return_type: Option<ExternalSignatureType>,
    /// Backend-specific lowering metadata.
    pub lowerings: ExternalFunctionLowerings,
}

impl ExternalFunctionDef {
    pub(crate) fn success_return_data_types(&self) -> Vec<DataType> {
        self.returns
            .iter()
            .filter_map(|slot| slot.value_type.to_datatype())
            .collect()
    }

    pub(crate) fn success_return_type_ids(
        &self,
        type_environment: &mut crate::compiler_frontend::datatypes::environment::TypeEnvironment,
        builtin_error_type_id: crate::compiler_frontend::datatypes::ids::TypeId,
    ) -> Vec<crate::compiler_frontend::datatypes::ids::TypeId> {
        self.returns
            .iter()
            .filter_map(|slot| {
                slot.value_type
                    .to_type_id(type_environment, builtin_error_type_id)
            })
            .collect()
    }

    pub(crate) fn error_return_type_id(
        &self,
        type_environment: &mut crate::compiler_frontend::datatypes::environment::TypeEnvironment,
        builtin_error_type_id: crate::compiler_frontend::datatypes::ids::TypeId,
    ) -> Option<crate::compiler_frontend::datatypes::ids::TypeId> {
        self.error_return_type
            .as_ref()
            .and_then(|error_type| error_type.to_type_id(type_environment, builtin_error_type_id))
    }

    pub(crate) fn is_fallible(&self) -> bool {
        self.error_return_type.is_some()
    }

    /// Alias behavior for the HIR call result local.
    ///
    /// Fallible external calls return a backend-boundary carrier object. The carrier is
    /// always fresh; any aliasing metadata belongs to the success payload after the
    /// fallible branch unwraps it.
    pub(crate) fn hir_return_alias(&self) -> ExternalReturnAlias {
        if self.is_fallible() {
            return ExternalReturnAlias::Fresh;
        }

        match self.returns.as_slice() {
            [single] => single.alias.clone(),
            _ => ExternalReturnAlias::Fresh,
        }
    }
}

/// One success-channel return slot for an external function.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalReturnSlot {
    pub value_type: ExternalSignatureType,
    pub alias: ExternalReturnAlias,
}

impl ExternalReturnSlot {
    pub fn fresh(value_type: impl Into<ExternalSignatureType>) -> Self {
        Self {
            value_type: value_type.into(),
            alias: ExternalReturnAlias::Fresh,
        }
    }

    pub fn alias_args(
        value_type: impl Into<ExternalSignatureType>,
        parameter_indices: Vec<usize>,
    ) -> Self {
        Self {
            value_type: value_type.into(),
            alias: ExternalReturnAlias::AliasArgs(parameter_indices),
        }
    }
}

/// Builder-friendly signature constructor for one-slot success metadata.
pub fn external_success_returns(
    success_type: ExternalAbiType,
    success_alias: ExternalReturnAlias,
) -> Vec<ExternalReturnSlot> {
    match success_type.to_datatype() {
        Some(_) => vec![ExternalReturnSlot {
            value_type: success_type.into(),
            alias: success_alias,
        }],
        None => Vec::new(),
    }
}

/// Definition of a single opaque external type exposed by a virtual package.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalTypeDef {
    pub name: String,
    /// Stable package ID rather than a static string so dynamic and built-in packages
    /// share the same identity model.
    pub package_id: ExternalPackageId,
    pub abi_type: ExternalAbiType,
}

/// Compile-time value for an external package constant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExternalConstantValue {
    Float(f64),
    Int(i32),
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
    pub name: String,
    pub data_type: ExternalAbiType,
    pub value: ExternalConstantValue,
}

/// A single virtual package provided by a project builder.
#[derive(Clone, Debug)]
pub struct ExternalPackage {
    pub id: ExternalPackageId,
    pub path: String,
    pub origin: ExternalPackageOrigin,
    pub functions: HashMap<String, ExternalFunctionDef>,
    pub types: HashMap<String, ExternalTypeDef>,
    pub constants: HashMap<String, ExternalConstantDef>,
}

impl ExternalPackage {
    pub(crate) fn new(
        id: ExternalPackageId,
        path: impl Into<String>,
        origin: ExternalPackageOrigin,
    ) -> Self {
        Self {
            id,
            path: path.into(),
            origin,
            functions: HashMap::new(),
            types: HashMap::new(),
            constants: HashMap::new(),
        }
    }
}

/// Builder-friendly spec for registering an external function.
///
/// WHAT: carries the metadata needed to register a function without forcing
/// the caller to construct the full `ExternalFunctionDef` and pick a stable ID.
/// WHY: builder packages should not need to hardcode `ExternalFunctionId` enum variants.
#[derive(Debug, Clone)]
pub struct ExternalFunctionSpec {
    pub name: String,
    pub parameters: Vec<ExternalParameter>,
    pub returns: Vec<ExternalReturnSlot>,
    pub error_return_type: Option<ExternalSignatureType>,
    pub lowerings: ExternalFunctionLowerings,
}

impl From<ExternalFunctionSpec> for ExternalFunctionDef {
    fn from(spec: ExternalFunctionSpec) -> Self {
        ExternalFunctionDef {
            name: spec.name,
            parameters: spec.parameters,
            returns: spec.returns,
            error_return_type: spec.error_return_type,
            lowerings: spec.lowerings,
        }
    }
}

/// Builder-friendly spec for registering an external type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalTypeSpec {
    pub name: String,
    pub abi_type: ExternalAbiType,
}
