//! Canonical builtin `Error` type manifest and helpers.
//!
//! WHAT: owns the language-level builtin error declarations, reserved symbols, field names,
//! error-code constants, and lookup helpers used across AST/HIR/backend lowering.
//! WHY: builtin error metadata should be centralized so parser/lowering/backend code cannot drift
//! on type names, field names, or error-code mapping.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::ContainsReferences;
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) const ERROR_TYPE_NAME: &str = "Error";
pub(crate) const ERROR_KIND_TYPE_NAME: &str = "ErrorKind";
pub(crate) const ERROR_LOCATION_TYPE_NAME: &str = "ErrorLocation";
pub(crate) const STACK_FRAME_TYPE_NAME: &str = "StackFrame";

pub(crate) const ERROR_FIELD_KIND: &str = "kind";
pub(crate) const ERROR_FIELD_CODE: &str = "code";
pub(crate) const ERROR_FIELD_MESSAGE: &str = "message";
pub(crate) const ERROR_FIELD_LOCATION: &str = "location";
pub(crate) const ERROR_FIELD_TRACE: &str = "trace";

pub(crate) const ERROR_LOCATION_FIELD_FILE: &str = "file";
pub(crate) const ERROR_LOCATION_FIELD_LINE: &str = "line";
pub(crate) const ERROR_LOCATION_FIELD_COLUMN: &str = "column";
pub(crate) const ERROR_LOCATION_FIELD_FUNCTION: &str = "function";

pub(crate) const STACK_FRAME_FIELD_FUNCTION: &str = "function";
pub(crate) const STACK_FRAME_FIELD_LOCATION: &str = "location";

pub(crate) const ERROR_HELPER_WITH_LOCATION: &str = "with_location";
pub(crate) const ERROR_HELPER_PUSH_TRACE: &str = "push_trace";
pub(crate) const ERROR_HELPER_BUBBLE: &str = "bubble";

pub(crate) const ERROR_HELPER_WITH_LOCATION_HOST: &str = "__bs_error_with_location";
pub(crate) const ERROR_HELPER_PUSH_TRACE_HOST: &str = "__bs_error_push_trace";
pub(crate) const ERROR_HELPER_BUBBLE_HOST: &str = "__bs_error_bubble";

pub(crate) const ERROR_CODE_COLLECTION_EXPECTED_ORDERED_COLLECTION: &str =
    "collection.expected_ordered_collection";
pub(crate) const ERROR_CODE_COLLECTION_INDEX_OUT_OF_BOUNDS: &str = "collection.index_out_of_bounds";
pub(crate) const ERROR_CODE_INT_PARSE_INVALID_FORMAT: &str = "int.parse_invalid_format";
pub(crate) const ERROR_CODE_INT_PARSE_OUT_OF_RANGE: &str = "int.parse_out_of_range";
pub(crate) const ERROR_CODE_FLOAT_PARSE_INVALID_FORMAT: &str = "float.parse_invalid_format";
pub(crate) const ERROR_CODE_FLOAT_PARSE_OUT_OF_RANGE: &str = "float.parse_out_of_range";

#[allow(dead_code)] // Not all variants are emitted in v1, but the builtin surface is fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BuiltinErrorKind {
    InvalidArgument,
    OutOfBounds,
    NotFound,
    Parse,
    Io,
    InvalidState,
    Unsupported,
    Other,
}

impl BuiltinErrorKind {
    pub(crate) fn as_runtime_tag(self) -> &'static str {
        match self {
            BuiltinErrorKind::InvalidArgument => "InvalidArgument",
            BuiltinErrorKind::OutOfBounds => "OutOfBounds",
            BuiltinErrorKind::NotFound => "NotFound",
            BuiltinErrorKind::Parse => "Parse",
            BuiltinErrorKind::Io => "Io",
            BuiltinErrorKind::InvalidState => "InvalidState",
            BuiltinErrorKind::Unsupported => "Unsupported",
            BuiltinErrorKind::Other => "Other",
        }
    }
}

/// Canonical builtin error declarations and visibility metadata.
///
/// WHAT: bundles every AST-time registration artifact required for builtin error type support.
/// WHY: AST orchestration should consume one manifest instead of manually reconstructing builtin
/// declarations and field maps.
pub(crate) struct BuiltinErrorManifest {
    #[allow(dead_code)]
    // Reserved-symbol paths are part of the manifest contract for future checks.
    pub(crate) reserved_symbol_paths: FxHashSet<InternedPath>,
    pub(crate) visible_symbol_paths: FxHashSet<InternedPath>,
    pub(crate) declarations: Vec<Declaration>,
    pub(crate) resolved_struct_fields_by_path: FxHashMap<InternedPath, Vec<Declaration>>,
    pub(crate) struct_source_by_path: FxHashMap<InternedPath, InternedPath>,
    pub(crate) ast_struct_nodes: Vec<AstNode>,
}

pub(crate) fn is_reserved_builtin_symbol(name: &str) -> bool {
    matches!(
        name,
        ERROR_TYPE_NAME | ERROR_KIND_TYPE_NAME | ERROR_LOCATION_TYPE_NAME | STACK_FRAME_TYPE_NAME
    )
}

pub(crate) fn builtin_error_type_path(string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(ERROR_TYPE_NAME, string_table)
}

pub(crate) fn builtin_error_kind_type_path(string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(ERROR_KIND_TYPE_NAME, string_table)
}

pub(crate) fn builtin_error_location_type_path(string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(ERROR_LOCATION_TYPE_NAME, string_table)
}

pub(crate) fn builtin_stack_frame_type_path(string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(STACK_FRAME_TYPE_NAME, string_table)
}

#[allow(dead_code)] // Shared canonical field-name helper for parser/backend call sites.
pub(crate) fn builtin_error_message_field_name() -> &'static str {
    ERROR_FIELD_MESSAGE
}

#[allow(dead_code)] // Shared canonical field-name helper for parser/backend call sites.
pub(crate) fn builtin_error_code_field_name() -> &'static str {
    ERROR_FIELD_CODE
}

#[allow(dead_code)] // Shared canonical field-name helper for parser/backend call sites.
pub(crate) fn builtin_error_kind_field_name() -> &'static str {
    ERROR_FIELD_KIND
}

#[allow(dead_code)] // Shared canonical field-name helper for parser/backend call sites.
pub(crate) fn builtin_error_location_field_name() -> &'static str {
    ERROR_FIELD_LOCATION
}

#[allow(dead_code)] // Shared canonical field-name helper for parser/backend call sites.
pub(crate) fn builtin_error_trace_field_name() -> &'static str {
    ERROR_FIELD_TRACE
}

pub(crate) fn builtin_error_kind_for_code(code: &str) -> BuiltinErrorKind {
    match code {
        ERROR_CODE_COLLECTION_EXPECTED_ORDERED_COLLECTION => BuiltinErrorKind::InvalidArgument,
        ERROR_CODE_COLLECTION_INDEX_OUT_OF_BOUNDS => BuiltinErrorKind::OutOfBounds,
        ERROR_CODE_INT_PARSE_INVALID_FORMAT
        | ERROR_CODE_INT_PARSE_OUT_OF_RANGE
        | ERROR_CODE_FLOAT_PARSE_INVALID_FORMAT
        | ERROR_CODE_FLOAT_PARSE_OUT_OF_RANGE => BuiltinErrorKind::Parse,
        _ => BuiltinErrorKind::Other,
    }
}

pub(crate) fn is_builtin_error_data_type(
    data_type: &DataType,
    string_table: &mut StringTable,
) -> bool {
    let Some(nominal_path) = data_type.struct_nominal_path() else {
        return false;
    };

    nominal_path == &builtin_error_type_path(string_table)
}

pub(crate) fn register_builtin_error_types(string_table: &mut StringTable) -> BuiltinErrorManifest {
    let location = SourceLocation::default();

    let error_kind_path = builtin_error_kind_type_path(string_table);
    let error_location_path = builtin_error_location_type_path(string_table);
    let stack_frame_path = builtin_stack_frame_type_path(string_table);
    let error_path = builtin_error_type_path(string_table);

    let mut reserved_symbol_paths = FxHashSet::default();
    reserved_symbol_paths.insert(error_kind_path.to_owned());
    reserved_symbol_paths.insert(error_location_path.to_owned());
    reserved_symbol_paths.insert(stack_frame_path.to_owned());
    reserved_symbol_paths.insert(error_path.to_owned());

    let mut visible_symbol_paths = FxHashSet::default();
    visible_symbol_paths.extend(reserved_symbol_paths.iter().cloned());

    let error_location_fields = vec![
        required_field(
            error_location_path.join_str(ERROR_LOCATION_FIELD_FILE, string_table),
            DataType::StringSlice,
            location.clone(),
        ),
        required_field(
            error_location_path.join_str(ERROR_LOCATION_FIELD_LINE, string_table),
            DataType::Int,
            location.clone(),
        ),
        required_field(
            error_location_path.join_str(ERROR_LOCATION_FIELD_COLUMN, string_table),
            DataType::Int,
            location.clone(),
        ),
        defaulted_optional_field(
            error_location_path.join_str(ERROR_LOCATION_FIELD_FUNCTION, string_table),
            DataType::StringSlice,
            location.clone(),
        ),
    ];

    let error_location_type = DataType::runtime_struct(
        error_location_path.to_owned(),
        error_location_fields.to_owned(),
        Ownership::MutableOwned,
    );

    let stack_frame_fields = vec![
        required_field(
            stack_frame_path.join_str(STACK_FRAME_FIELD_FUNCTION, string_table),
            DataType::StringSlice,
            location.clone(),
        ),
        defaulted_optional_field(
            stack_frame_path.join_str(STACK_FRAME_FIELD_LOCATION, string_table),
            error_location_type.to_owned(),
            location.clone(),
        ),
    ];

    let stack_frame_type = DataType::runtime_struct(
        stack_frame_path.to_owned(),
        stack_frame_fields.to_owned(),
        Ownership::MutableOwned,
    );

    let error_fields = vec![
        required_field(
            error_path.join_str(ERROR_FIELD_KIND, string_table),
            DataType::BuiltinErrorKind,
            location.clone(),
        ),
        required_field(
            error_path.join_str(ERROR_FIELD_CODE, string_table),
            DataType::StringSlice,
            location.clone(),
        ),
        required_field(
            error_path.join_str(ERROR_FIELD_MESSAGE, string_table),
            DataType::StringSlice,
            location.clone(),
        ),
        defaulted_optional_field(
            error_path.join_str(ERROR_FIELD_LOCATION, string_table),
            error_location_type.to_owned(),
            location.clone(),
        ),
        defaulted_optional_field(
            error_path.join_str(ERROR_FIELD_TRACE, string_table),
            DataType::Collection(
                Box::new(stack_frame_type.to_owned()),
                Ownership::ImmutableOwned,
            ),
            location.clone(),
        ),
    ];

    let mut declarations = Vec::with_capacity(4);
    declarations.push(type_declaration(
        error_kind_path.to_owned(),
        DataType::BuiltinErrorKind,
        location.clone(),
    ));
    declarations.push(type_declaration(
        error_location_path.to_owned(),
        error_location_type.to_owned(),
        location.clone(),
    ));
    declarations.push(type_declaration(
        stack_frame_path.to_owned(),
        stack_frame_type.to_owned(),
        location.clone(),
    ));
    declarations.push(type_declaration(
        error_path.to_owned(),
        DataType::runtime_struct(
            error_path.to_owned(),
            error_fields.to_owned(),
            Ownership::MutableOwned,
        ),
        location.clone(),
    ));

    let mut resolved_struct_fields_by_path = FxHashMap::default();
    resolved_struct_fields_by_path.insert(error_location_path.to_owned(), error_location_fields);
    resolved_struct_fields_by_path.insert(stack_frame_path.to_owned(), stack_frame_fields);
    resolved_struct_fields_by_path.insert(error_path.to_owned(), error_fields);

    let mut struct_source_by_path = FxHashMap::default();
    struct_source_by_path.insert(error_location_path.to_owned(), InternedPath::new());
    struct_source_by_path.insert(stack_frame_path.to_owned(), InternedPath::new());
    struct_source_by_path.insert(error_path.to_owned(), InternedPath::new());

    let ast_struct_nodes = vec![
        AstNode {
            kind: NodeKind::StructDefinition(
                error_location_path.to_owned(),
                resolved_struct_fields_by_path
                    .get(&error_location_path)
                    .cloned()
                    .unwrap_or_default(),
            ),
            location: location.clone(),
            scope: error_location_path.to_owned(),
        },
        AstNode {
            kind: NodeKind::StructDefinition(
                stack_frame_path.to_owned(),
                resolved_struct_fields_by_path
                    .get(&stack_frame_path)
                    .cloned()
                    .unwrap_or_default(),
            ),
            location: location.clone(),
            scope: stack_frame_path.to_owned(),
        },
        AstNode {
            kind: NodeKind::StructDefinition(
                error_path.to_owned(),
                resolved_struct_fields_by_path
                    .get(&error_path)
                    .cloned()
                    .unwrap_or_default(),
            ),
            location,
            scope: error_path.to_owned(),
        },
    ];

    BuiltinErrorManifest {
        reserved_symbol_paths,
        visible_symbol_paths,
        declarations,
        resolved_struct_fields_by_path,
        struct_source_by_path,
        ast_struct_nodes,
    }
}

pub(crate) fn resolve_builtin_error_type(
    context: &ScopeContext,
    location: &SourceLocation,
    string_table: &mut StringTable,
) -> Result<DataType, CompilerError> {
    resolve_builtin_named_type(context, ERROR_TYPE_NAME, location, string_table)
}

pub(crate) fn resolve_builtin_error_location_type(
    context: &ScopeContext,
    location: &SourceLocation,
    string_table: &mut StringTable,
) -> Result<DataType, CompilerError> {
    resolve_builtin_named_type(context, ERROR_LOCATION_TYPE_NAME, location, string_table)
}

pub(crate) fn resolve_builtin_stack_frame_type(
    context: &ScopeContext,
    location: &SourceLocation,
    string_table: &mut StringTable,
) -> Result<DataType, CompilerError> {
    resolve_builtin_named_type(context, STACK_FRAME_TYPE_NAME, location, string_table)
}

fn resolve_builtin_named_type(
    context: &ScopeContext,
    type_name: &str,
    location: &SourceLocation,
    string_table: &mut StringTable,
) -> Result<DataType, CompilerError> {
    let symbol = string_table.intern(type_name);
    let Some(declaration) = context.get_reference(&symbol) else {
        return Err(CompilerError::compiler_error(format!(
            "Builtin type '{type_name}' is missing from this compilation context."
        )));
    };

    if declaration.value.data_type == DataType::Inferred {
        return Err(CompilerError::compiler_error(format!(
            "Builtin type '{type_name}' resolved to an inferred placeholder at {:?}.",
            location
        )));
    }

    Ok(declaration.value.data_type.to_owned())
}

fn type_declaration(
    id: InternedPath,
    data_type: DataType,
    location: SourceLocation,
) -> Declaration {
    Declaration {
        id,
        value: Expression::new(
            ExpressionKind::NoValue,
            location,
            data_type,
            Ownership::ImmutableReference,
        ),
    }
}

fn required_field(id: InternedPath, data_type: DataType, location: SourceLocation) -> Declaration {
    Declaration {
        id,
        value: Expression::no_value(location, data_type, Ownership::ImmutableOwned),
    }
}

fn defaulted_optional_field(
    id: InternedPath,
    inner_type: DataType,
    location: SourceLocation,
) -> Declaration {
    let optional_type = DataType::Option(Box::new(inner_type));
    Declaration {
        id,
        value: Expression::new(
            ExpressionKind::OptionNone,
            location,
            optional_type,
            Ownership::ImmutableOwned,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ERROR_FIELD_CODE, ERROR_FIELD_KIND, ERROR_FIELD_LOCATION, ERROR_FIELD_MESSAGE,
        ERROR_FIELD_TRACE, ERROR_KIND_TYPE_NAME, ERROR_LOCATION_TYPE_NAME, ERROR_TYPE_NAME,
        STACK_FRAME_TYPE_NAME, is_reserved_builtin_symbol, register_builtin_error_types,
    };
    use crate::compiler_frontend::string_interning::StringTable;

    #[test]
    fn registers_builtin_error_manifest_with_canonical_symbols() {
        let mut string_table = StringTable::new();
        let manifest = register_builtin_error_types(&mut string_table);

        assert_eq!(manifest.declarations.len(), 4);
        assert_eq!(manifest.reserved_symbol_paths.len(), 4);
        assert_eq!(manifest.visible_symbol_paths.len(), 4);

        let error_path = super::builtin_error_type_path(&mut string_table);
        let error_fields = manifest
            .resolved_struct_fields_by_path
            .get(&error_path)
            .expect("Error fields should be registered");

        let mut field_names = error_fields
            .iter()
            .map(|field| {
                field
                    .id
                    .name_str(&string_table)
                    .expect("field names should exist")
                    .to_owned()
            })
            .collect::<Vec<_>>();
        field_names.sort();

        assert_eq!(
            field_names,
            vec![
                ERROR_FIELD_CODE.to_owned(),
                ERROR_FIELD_KIND.to_owned(),
                ERROR_FIELD_LOCATION.to_owned(),
                ERROR_FIELD_MESSAGE.to_owned(),
                ERROR_FIELD_TRACE.to_owned(),
            ]
        );
    }

    #[test]
    fn reserves_builtin_error_symbol_names() {
        assert!(is_reserved_builtin_symbol(ERROR_TYPE_NAME));
        assert!(is_reserved_builtin_symbol(ERROR_KIND_TYPE_NAME));
        assert!(is_reserved_builtin_symbol(ERROR_LOCATION_TYPE_NAME));
        assert!(is_reserved_builtin_symbol(STACK_FRAME_TYPE_NAME));
        assert!(!is_reserved_builtin_symbol("UserError"));
    }
}
