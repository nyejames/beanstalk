//! Frontend semantic type model.
//!
//! WHAT: defines AST/frontend type identity before HIR type interning.
//! WHY: AST needs a rich type surface for named types, unresolved placeholders,
//! templates, choices, constants, and frontend-only wrappers.
//!
//! Access/mutability/owned-vs-reference state does not live in `DataType`.
//! That state belongs to expressions, declarations, call arguments, HIR locals,
//! and borrow-analysis facts.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::declaration_syntax::choice::{ChoiceVariant, ChoiceVariantPayload};
use crate::compiler_frontend::external_packages::ExternalTypeId;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::CompileTimePathKind;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use rustc_hash::FxHashSet;

/// Type-level distinction for compile-time path values.
///
/// WHAT: carries file vs directory classification inside the type system.
/// WHY: future path operations (trailing-slash coercion, join semantics,
///      metadata inspection) need this distinction at the type level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathTypeKind {
    File,
    Directory,
}

impl From<CompileTimePathKind> for PathTypeKind {
    fn from(kind: CompileTimePathKind) -> Self {
        match kind {
            CompileTimePathKind::File => PathTypeKind::File,
            CompileTimePathKind::Directory => PathTypeKind::Directory,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinScalarReceiver {
    Int,
    Float,
    Bool,
    String,
    Char,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReceiverKey {
    Struct(InternedPath),
    BuiltinScalar(BuiltinScalarReceiver),
}

#[derive(Debug, Clone)]
pub enum DataType {
    // Meta-types used during earlier frontend stages.
    // Type is inferred, This only exists before the type checking stage.
    // All 'inferred' variables must be evaluated to other types after the AST stage for the program to compile.
    // At the header parsing stage, 'inferred' is used where a symbol type is not yet known (as the type might be another header).
    Inferred,
    // AST-only placeholder for nominal types that are resolved once the module declaration set is known.
    NamedType(StringId),

    // Container and composite runtime types.
    Collection(Box<DataType>),
    Struct {
        nominal_path: InternedPath,
        fields: Vec<Declaration>,
        const_record: bool,
    },
    Reference(Box<DataType>),
    Range, // Iterable that must always be owned.
    Returns(Vec<DataType>),
    Function(Box<Option<ReceiverKey>>, FunctionSignature), // Receiver, signature

    // Compile-time/frontend-specific composite values.
    // Compile-time path value (file or directory).
    #[allow(dead_code)] // Will be needed for path expressions in the future
    Path(PathTypeKind),
    Template,

    // Scalar/runtime-leaf types.
    Bool,
    Int,
    Float,
    #[allow(dead_code)] // Planned: decimal numeric type support in parser/lowering.
    Decimal,
    StringSlice, // UTF-8 read-only string slice
    Char,
    BuiltinErrorKind,

    // Reserved or not-yet-wired variants kept for planned language work.
    #[allow(dead_code)] // Planned: explicit parameter/record type surfaces.
    Parameters(Vec<Declaration>), // Struct definitions and parameters

    Choices {
        nominal_path: InternedPath,
        variants: Vec<ChoiceVariant>,
    }, // Choice declaration identity + variant list
    /// Opaque external type provided by a platform package (e.g. `IO`, `Canvas`).
    /// Cannot be constructed with struct literals or field-accessed.
    External {
        type_id: ExternalTypeId,
    },
    #[allow(dead_code)] // Planned: Option<T> language-level type support.
    Option(Box<DataType>), // Shorthand for a choice of a type or None
    Result {
        ok: Box<DataType>,
        err: Box<DataType>,
    },
    TemplateWrapper, // Foldable template with a slot (becomes two string slices)
    #[allow(dead_code)] // Planned: explicit None literal/type flows.
    None, // The None result of an option, or empty argument
    #[allow(dead_code)] // Planned: boolean literal singleton typing extensions.
    True,
    #[allow(dead_code)] // Planned: boolean literal singleton typing extensions.
    False,
}

// NOTE: DataType owns type structure and structural helper methods only.
// Compatibility policy (what type is accepted in what position) lives exclusively
// in `type_coercion::compatibility::is_type_compatible`.
// Contextual numeric promotion logic lives in `type_coercion::numeric`.
impl DataType {
    pub fn runtime_struct(nominal_path: InternedPath, fields: Vec<Declaration>) -> Self {
        Self::Struct {
            nominal_path,
            fields,
            const_record: false,
        }
    }

    pub fn const_struct_record(nominal_path: InternedPath, fields: Vec<Declaration>) -> Self {
        Self::Struct {
            nominal_path,
            fields,
            const_record: true,
        }
    }

    pub fn receiver_key_from_type(&self) -> Option<ReceiverKey> {
        match self {
            DataType::Struct {
                nominal_path,
                const_record,
                ..
            } if !const_record => Some(ReceiverKey::Struct(nominal_path.to_owned())),
            DataType::Int => Some(ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Int)),
            DataType::Float => Some(ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Float)),
            DataType::Bool => Some(ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Bool)),
            DataType::StringSlice => {
                Some(ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::String))
            }
            DataType::Char => Some(ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Char)),
            _ => None,
        }
    }

    pub fn struct_nominal_path(&self) -> Option<&InternedPath> {
        match self {
            DataType::Struct { nominal_path, .. } => Some(nominal_path),
            _ => None,
        }
    }

    pub fn struct_fields(&self) -> Option<&[Declaration]> {
        match self {
            DataType::Struct { fields, .. } => Some(fields.as_slice()),
            _ => None,
        }
    }

    pub fn is_const_record_struct(&self) -> bool {
        match self {
            DataType::Struct { const_record, .. } => *const_record,
            _ => false,
        }
    }

    pub fn is_result(&self) -> bool {
        matches!(self, DataType::Result { .. })
    }

    pub fn result_ok_type(&self) -> Option<&DataType> {
        match self {
            DataType::Result { ok, .. } => Some(ok.as_ref()),
            _ => None,
        }
    }

    pub fn result_error_type(&self) -> Option<&DataType> {
        match self {
            DataType::Result { err, .. } => Some(err.as_ref()),
            _ => None,
        }
    }

    pub fn is_numerical(&self) -> bool {
        matches!(self, DataType::Float | DataType::Int | DataType::Decimal)
    }

    pub fn is_textual_cast_input(&self) -> bool {
        matches!(self, DataType::StringSlice | DataType::Template)
    }

    /// Returns true if values of this type can be compared with structural equality.
    ///
    /// WHAT: Recursively checks whether the type and all nested types support `is` / `is not`.
    /// WHY: Choice structural equality requires every payload field type to also support
    /// structural equality. This helper encodes the Alpha contract conservatively.
    ///
    /// Supported types:
    /// - Scalar types: Bool, Int, Float, Char, StringSlice
    /// - Choices whose payload fields all support structural equality
    /// - References to supported types
    ///
    /// Rejected types:
    /// - Structs, Collections, Functions, External opaque types, Templates
    /// - Recursive choices (cycle-safe via visited set)
    pub fn supports_structural_equality(&self) -> bool {
        self.supports_structural_equality_with_visited(&mut FxHashSet::default())
    }

    fn supports_structural_equality_with_visited(
        &self,
        visited: &mut FxHashSet<InternedPath>,
    ) -> bool {
        match self {
            DataType::Bool
            | DataType::Int
            | DataType::Float
            | DataType::Char
            | DataType::StringSlice => true,

            DataType::Reference(inner) => inner.supports_structural_equality_with_visited(visited),

            DataType::Choices {
                nominal_path,
                variants,
            } => {
                if !visited.insert(nominal_path.clone()) {
                    // Cycle detected: recursive choice equality is not supported.
                    return false;
                }
                let supported = variants.iter().all(|variant| match &variant.payload {
                    ChoiceVariantPayload::Unit => true,
                    ChoiceVariantPayload::Record { fields } => fields.iter().all(|field| {
                        field
                            .value
                            .data_type
                            .supports_structural_equality_with_visited(visited)
                    }),
                });
                visited.remove(nominal_path);
                supported
            }

            // Options and Results do not yet have frontend equality support.
            // Defer them until dedicated Option/Result comparison lowering is implemented.
            DataType::Option(_) | DataType::Result { .. } => false,

            _ => false,
        }
    }

    /// Display the DataType with proper string resolution for interned strings.
    /// This method should be used instead of Display when a StringTable is available.
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        match self {
            DataType::Reference(inner_type) => {
                format!("{} Reference", inner_type.display_with_table(string_table),)
            }
            DataType::Inferred => "Inferred".to_string(),
            DataType::NamedType(name) => string_table.resolve(*name).to_string(),
            DataType::Bool => "Bool".to_string(),
            DataType::StringSlice => "String".to_string(),
            DataType::TemplateWrapper => "String".to_string(),
            DataType::Char => "Char".to_string(),
            DataType::BuiltinErrorKind => "ErrorKind".to_string(),
            DataType::Float => "Float".to_string(),
            DataType::Int => "Int".to_string(),
            DataType::Decimal => "Decimal".to_string(),
            DataType::Collection(inner_type) => {
                format!("{} Collection", inner_type.display_with_table(string_table))
            }
            DataType::Parameters(args) => {
                let mut arg_str = String::new();
                for arg in args {
                    let name = arg.id.to_string(string_table);
                    arg_str.push_str(&format!(
                        "{}: {}, ",
                        name,
                        arg.value.data_type.display_with_table(string_table)
                    ));
                }
                format!("Parameters({arg_str})")
            }
            DataType::Struct {
                nominal_path,
                const_record,
                ..
            } => {
                let bare_name = nominal_path
                    .name_str(string_table)
                    .unwrap_or("<anonymous struct>");
                if *const_record {
                    format!("#{bare_name}")
                } else {
                    bare_name.to_owned()
                }
            }
            DataType::External { type_id } => {
                // External types are opaque; display uses the stable ID.
                format!("External({})", type_id.0)
            }
            DataType::Returns(returns) => {
                let mut returns_string = String::new();
                for return_type in returns {
                    returns_string
                        .push_str(&return_type.display_with_table(string_table).to_string());
                }
                format!("Returns({returns_string})")
            }
            DataType::Function(_, signature) => {
                let mut arg_str = String::new();
                let mut returns_string = String::new();
                for arg in &signature.parameters {
                    let name = arg.id.to_string(string_table);
                    arg_str.push_str(&format!(
                        "{}: {}, ",
                        name,
                        arg.value.data_type.display_with_table(string_table)
                    ));
                }
                for return_type in &signature.returns {
                    returns_string.push_str(&format!(
                        "{}, ",
                        return_type.data_type().display_with_table(string_table)
                    ));
                }
                format!("Function({arg_str} -> {returns_string})")
            }

            DataType::Path(PathTypeKind::File) => "Path(File)".to_string(),
            DataType::Path(PathTypeKind::Directory) => "Path(Directory)".to_string(),
            DataType::Template => "Template".to_string(),
            DataType::None => "None".to_string(),
            DataType::True => "True".to_string(),
            DataType::False => "False".to_string(),
            DataType::Range => "Range".to_string(),
            DataType::Option(inner_type) => {
                format!("Option({})", inner_type.display_with_table(string_table))
            }
            DataType::Result { ok, err } => {
                format!(
                    "Result({}, {})",
                    ok.display_with_table(string_table),
                    err.display_with_table(string_table)
                )
            }
            DataType::Choices {
                nominal_path,
                variants,
            } => {
                let name = nominal_path
                    .name_str(string_table)
                    .unwrap_or("<choice>")
                    .to_owned();
                if variants.is_empty() {
                    format!("{name}::{{}}")
                } else {
                    let variant_names: Vec<String> = variants
                        .iter()
                        .map(|v| {
                            let base = string_table.resolve(v.id).to_owned();
                            match &v.payload {
                                ChoiceVariantPayload::Unit => base,
                                ChoiceVariantPayload::Record { .. } => format!("{base}(...)"),
                            }
                        })
                        .collect();
                    format!("{name}::{{{}}}", variant_names.join(", "))
                }
            }
        }
    }
}

impl PartialEq for DataType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DataType::Inferred, DataType::Inferred) => true,
            (DataType::NamedType(a), DataType::NamedType(b)) => a == b,
            (DataType::Reference(a), DataType::Reference(b)) => a == b,
            (DataType::Bool, DataType::Bool) => true,
            (DataType::Range, DataType::Range) => true,
            (DataType::None, DataType::None) => true,
            (DataType::True, DataType::True) => true,
            (DataType::False, DataType::False) => true,
            (DataType::StringSlice, DataType::StringSlice) => true,
            (DataType::Char, DataType::Char) => true,
            (DataType::BuiltinErrorKind, DataType::BuiltinErrorKind) => true,
            (DataType::Float, DataType::Float) => true,
            (DataType::Int, DataType::Int) => true,
            (DataType::Decimal, DataType::Decimal) => true,
            (
                DataType::Result {
                    ok: ok_a,
                    err: err_a,
                },
                DataType::Result {
                    ok: ok_b,
                    err: err_b,
                },
            ) => ok_a == ok_b && err_a == err_b,
            (DataType::Collection(a), DataType::Collection(b)) => a == b,
            (DataType::Path(a), DataType::Path(b)) => a == b,
            (DataType::Template, DataType::Template) => true,
            (DataType::Option(a), DataType::Option(b)) => a == b,
            // For Args, Struct, Function, and Choices, we compare by name/structure
            // but not by the actual Arg values since they contain Expressions
            (DataType::Parameters(a), DataType::Parameters(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(arg_a, arg_b)| arg_a.id == arg_b.id)
            }
            (
                DataType::Struct {
                    nominal_path: path_a,
                    const_record: const_a,
                    ..
                },
                DataType::Struct {
                    nominal_path: path_b,
                    const_record: const_b,
                    ..
                },
            ) => path_a == path_b && const_a == const_b,
            (DataType::Function(_, signature1), DataType::Function(_, signature2)) => {
                // If both functions have the same signature.returns types,
                // then they are equal
                signature1.returns.len() == signature2.returns.len()
                    && signature1
                        .returns
                        .iter()
                        .zip(signature2.returns.iter())
                        .all(|(return1, return2)| return1.data_type() == return2.data_type())
            }
            (
                DataType::Choices {
                    nominal_path: path_a,
                    ..
                },
                DataType::Choices {
                    nominal_path: path_b,
                    ..
                },
            ) => path_a == path_b,
            (DataType::External { type_id: id_a }, DataType::External { type_id: id_b }) => {
                id_a == id_b
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DataType;
    use crate::compiler_frontend::ast::ast_nodes::Declaration;
    use crate::compiler_frontend::declaration_syntax::choice::{
        ChoiceVariant, ChoiceVariantPayload,
    };
    use crate::compiler_frontend::interned_path::InternedPath;
    use crate::compiler_frontend::symbols::string_interning::StringTable;

    #[test]
    fn choice_equality_is_purely_nominal() {
        let mut table = StringTable::new();
        let path_a = InternedPath::from_single_str("Status", &mut table);
        let path_b = InternedPath::from_single_str("OtherStatus", &mut table);
        let ready = ChoiceVariant {
            id: table.intern("Ready"),
            payload: ChoiceVariantPayload::Unit,
            location: Default::default(),
        };
        let busy = ChoiceVariant {
            id: table.intern("Busy"),
            payload: ChoiceVariantPayload::Unit,
            location: Default::default(),
        };

        let status_a = DataType::Choices {
            nominal_path: path_a.clone(),
            variants: vec![ready.clone(), busy.clone()],
        };
        let status_b = DataType::Choices {
            nominal_path: path_a.clone(),
            variants: vec![ready.clone()],
        };
        let other = DataType::Choices {
            nominal_path: path_b.clone(),
            variants: vec![ready.clone(), busy.clone()],
        };

        assert_eq!(
            status_a, status_b,
            "same nominal path should make choices equal regardless of variant shape"
        );
        assert_ne!(
            status_a, other,
            "different nominal paths should make choices unequal even with identical variants"
        );
    }

    #[test]
    fn scalar_types_support_structural_equality() {
        assert!(DataType::Int.supports_structural_equality());
        assert!(DataType::Float.supports_structural_equality());
        assert!(DataType::Bool.supports_structural_equality());
        assert!(DataType::Char.supports_structural_equality());
        assert!(DataType::StringSlice.supports_structural_equality());
    }

    #[test]
    fn unsupported_types_do_not_support_structural_equality() {
        assert!(
            !DataType::Struct {
                nominal_path: InternedPath::new(),
                fields: vec![],
                const_record: false,
            }
            .supports_structural_equality()
        );
        assert!(!DataType::Collection(Box::new(DataType::Int)).supports_structural_equality());
        assert!(
            !DataType::External {
                type_id: crate::compiler_frontend::external_packages::ExternalTypeId(0)
            }
            .supports_structural_equality()
        );
    }

    #[test]
    fn unit_choice_supports_structural_equality() {
        let mut table = StringTable::new();
        let path = InternedPath::from_single_str("Status", &mut table);
        let status = DataType::Choices {
            nominal_path: path,
            variants: vec![ChoiceVariant {
                id: table.intern("Ready"),
                payload: ChoiceVariantPayload::Unit,
                location: Default::default(),
            }],
        };
        assert!(status.supports_structural_equality());
    }

    #[test]
    fn payload_choice_supports_structural_equality_when_fields_do() {
        let mut table = StringTable::new();
        let path = InternedPath::from_single_str("Response", &mut table);
        let response = DataType::Choices {
            nominal_path: path,
            variants: vec![ChoiceVariant {
                id: table.intern("Err"),
                payload: ChoiceVariantPayload::Record {
                    fields: vec![Declaration {
                        id: InternedPath::from_single_str("message", &mut table),
                        value: crate::compiler_frontend::ast::expressions::expression::Expression {
                            kind: crate::compiler_frontend::ast::expressions::expression::ExpressionKind::NoValue,
                            data_type: DataType::StringSlice,
                            value_mode: crate::compiler_frontend::value_mode::ValueMode::ImmutableOwned,
                            location: Default::default(),
                            contains_regular_division: false,
                        },
                    }],
                },
                location: Default::default(),
            }],
        };
        assert!(response.supports_structural_equality());
    }

    #[test]
    fn payload_choice_rejects_structural_equality_when_fields_do_not() {
        let mut table = StringTable::new();
        let path = InternedPath::from_single_str("Response", &mut table);
        let response = DataType::Choices {
            nominal_path: path,
            variants: vec![ChoiceVariant {
                id: table.intern("Err"),
                payload: ChoiceVariantPayload::Record {
                    fields: vec![Declaration {
                        id: InternedPath::from_single_str("items", &mut table),
                        value: crate::compiler_frontend::ast::expressions::expression::Expression {
                            kind: crate::compiler_frontend::ast::expressions::expression::ExpressionKind::NoValue,
                            data_type: DataType::Collection(Box::new(DataType::Int)),
                            value_mode: crate::compiler_frontend::value_mode::ValueMode::ImmutableOwned,
                            location: Default::default(),
                            contains_regular_division: false,
                        },
                    }],
                },
                location: Default::default(),
            }],
        };
        assert!(!response.supports_structural_equality());
    }
}
