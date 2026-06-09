//! Type definition shapes stored in `TypeEnvironment`.
//!
//! WHAT: describes the payload stored for each canonical type.
//! WHY: nominal definitions (structs, choices) live here instead of being
//!      cloned into every expression node.

use crate::compiler_frontend::external_packages::ExternalTypeId;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::ids::TraitId;

use super::ids::{
    BuiltinTypeKey, FunctionTypeId, GenericInstanceKey, GenericParameterId, GenericParameterListId,
    NominalTypeId, TypeConstructor, TypeId,
};

// -----------------------------------------------------------
//  Type Definitions
// -----------------------------------------------------------

/// The full definition of a type in the environment.
#[derive(Debug, Clone)]
pub enum TypeDefinition {
    Builtin(BuiltinTypeDefinition),
    Struct(StructTypeDefinition),
    Choice(ChoiceTypeDefinition),
    Constructed(ConstructedTypeDefinition),
    Function(FunctionTypeDefinition),
    External(ExternalTypeDefinition),
    GenericParameter(GenericParameterDefinition),
    GenericInstance(GenericInstanceDefinition),
    DynamicTrait(DynamicTraitTypeDefinition),
}

/// Builtin scalar type definition.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinTypeDefinition {
    pub key: BuiltinTypeKey,
}

/// Struct type definition.
#[derive(Debug, Clone)]
pub struct StructTypeDefinition {
    pub id: NominalTypeId,
    pub path: InternedPath,
    pub fields: Box<[FieldDefinition]>,
    pub generic_parameters: Option<GenericParameterListId>,
    pub const_record: bool,
}

/// Choice type definition.
#[derive(Debug, Clone)]
pub struct ChoiceTypeDefinition {
    pub id: NominalTypeId,
    pub path: InternedPath,
    pub variants: Box<[ChoiceVariantDefinition]>,
    pub generic_parameters: Option<GenericParameterListId>,
}

/// Field inside a struct or choice payload record.
#[derive(Debug, Clone)]
pub struct FieldDefinition {
    pub name: InternedPath,
    pub type_id: TypeId,
    pub location: SourceLocation,
}

/// Variant inside a choice definition.
#[derive(Debug, Clone)]
pub struct ChoiceVariantDefinition {
    pub name: StringId,
    pub tag: usize,
    pub payload: ChoiceVariantPayloadDefinition,
    pub location: SourceLocation,
}

/// Payload shape of a choice variant.
#[derive(Debug, Clone)]
pub enum ChoiceVariantPayloadDefinition {
    Unit,
    Record { fields: Box<[FieldDefinition]> },
}

/// Constructed type definition (collection, option, result, nominal generic instance).
#[derive(Debug, Clone)]
pub struct ConstructedTypeDefinition {
    pub constructor: TypeConstructor,
    pub arguments: Box<[TypeId]>,
}

/// Function type definition.
#[derive(Debug, Clone)]
pub struct FunctionTypeDefinition {
    pub id: FunctionTypeId,
    pub parameters: Box<[FunctionParameterDefinition]>,
    pub returns: Box<[TypeId]>,
    pub error_return: Option<TypeId>,
}

/// Parameter inside a function type definition.
#[derive(Debug, Clone)]
pub struct FunctionParameterDefinition {
    pub name: Option<StringId>,
    pub type_id: TypeId,
}

/// External opaque type definition.
#[derive(Debug, Clone, Copy)]
pub struct ExternalTypeDefinition {
    pub type_id: ExternalTypeId,
}

/// Generic parameter definition.
#[derive(Debug, Clone, Copy)]
pub struct GenericParameterDefinition {
    pub id: GenericParameterId,
    pub name: StringId,
}

/// A concrete generic instance (e.g. `Box of Int`).
#[derive(Debug, Clone)]
pub struct GenericInstanceDefinition {
    pub base: NominalTypeId,
    pub arguments: Box<[TypeId]>,
    pub source_key: GenericInstanceKey,
}

/// Dynamic trait value type identity.
///
/// WHAT: records the type-level identity for values whose concrete implementor is erased.
/// WHY: trait declarations and evidence stay in `TraitEnvironment`; `TypeEnvironment` owns only
/// the runtime value type identity needed by annotations and diagnostics.
#[derive(Debug, Clone, Copy)]
pub struct DynamicTraitTypeDefinition {
    pub(crate) trait_id: TraitId,
    pub(crate) name: StringId,
}
