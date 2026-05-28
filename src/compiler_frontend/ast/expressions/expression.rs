//! AST expression values and constructor helpers used before HIR lowering.
//!
//! WHAT: defines frontend expression kinds plus the factory methods that build typed AST values.
//! WHY: parser and folding code should create expressions through one readable surface instead of
//! manually reassembling `Expression` fields at each call site.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
pub use crate::compiler_frontend::ast::expressions::expression_kind::{ExpressionKind, Operator};
pub use crate::compiler_frontend::ast::expressions::expression_types::{
    BuiltinCastKind, ConstRecordState, ConstValueKind, FallibleCarrierVariant, FallibleHandling,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::generic_identity_bridge::GenericInstantiationKey;
use crate::compiler_frontend::datatypes::ids::{TypeId, builtin_type_ids};
use crate::compiler_frontend::datatypes::{DataType, ReceiverKey, diagnostic_type_spelling};
use crate::compiler_frontend::external_packages::ExternalFunctionId;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

/// The kind determines the runtime shape and constant-foldability of the value.
///
/// `Runtime` carries a small AST fragment for expressions that could not be folded.
/// `ExpressionKind` mirrors core datatypes that produce values, omitting structural
/// types that do not represent first-class runtime values.
#[derive(Clone, Debug)]
pub struct Expression {
    pub kind: ExpressionKind,
    /// Canonical `TypeId` for the expression's resolved semantic type.
    ///
    /// WHAT: expression factories seed builtin scalar IDs immediately; callers that build
    ///       nominal, constructed, or function values must provide/resolve the canonical ID
    ///       while they still hold the AST type interner.
    /// WHY: HIR expression lowering consumes these IDs directly from the final module
    ///      `TypeEnvironment`; AST finalization only debug-validates that no orphan IDs remain.
    pub type_id: TypeId,
    /// Non-authoritative type spelling kept for diagnostics and parse-era declarations.
    ///
    /// WHAT: preserves the written type spelling for diagnostics and debug output.
    /// WHY: semantic identity for executable expressions is `type_id`; this field is
    ///      display-only and must never be used for semantic type decisions.
    pub diagnostic_type: DataType,
    /// Receiver metadata for function declarations.
    ///
    /// WHAT: records whether an `ExpressionKind::Function` was declared with a
    /// receiver parameter.
    /// WHY: receiver lookup must distinguish free functions from receiver
    /// methods without inspecting diagnostic-only `DataType` structure.
    pub function_receiver: Option<ReceiverKey>,
    /// Frontend access classification for this expression.
    ///
    /// WHY: mutability and reference semantics are tracked separately from the
    ///      diagnostic type so that lowering stages can make ownership decisions.
    pub value_mode: ValueMode,
    /// Source location where this expression was parsed.
    pub location: SourceLocation,
    /// Explicit value-level const-record classification.
    ///
    /// WHAT: `ConstRecord` means this expression is a compile-time member group
    ///       constructed in a `#=` constant context.
    /// WHY: const-record status is a value fact, not a type identity. Semantic
    ///      decisions must inspect this field, not `diagnostic_type`.
    pub const_record_state: ConstRecordState,

    /// Tracks whether this value was derived from regular division (`/`).
    ///
    /// WHY: explicit `Int` contexts should emit a targeted diagnostic when a
    /// value comes from `/`, even when constant folding removed the original
    /// operator node.
    pub contains_regular_division: bool,
}

/// Canonical type data for call expressions built after semantic resolution.
///
/// WHAT: computes the canonical `TypeId` and a display-only `DataType` from the
/// resolved return TypeIds so call constructors do not need separate diagnostic vectors.
/// WHY: typed call constructors should compute all call typing state up front instead of
/// building placeholder IDs and recomputing them later.
struct ResolvedCallTypes {
    result_type_ids: Vec<TypeId>,
    expression_type_id: TypeId,
    diagnostic_type: DataType,
}

/// Input struct for `Expression::handled_fallible_host_function_call_with_typed_arguments`
/// to avoid a long parameter list.
pub(crate) struct HandledFallibleHostFunctionCallInput {
    pub(crate) id: ExternalFunctionId,
    pub(crate) args: Vec<CallArgument>,
    pub(crate) result_type_ids: Vec<TypeId>,
    pub(crate) error_type_id: TypeId,
    pub(crate) handling: FallibleHandling,
    pub(crate) location: SourceLocation,
}

impl ResolvedCallTypes {
    fn new(result_type_ids: Vec<TypeId>, type_environment: &mut TypeEnvironment) -> Self {
        let expression_type_id = match result_type_ids.as_slice() {
            [] => type_environment.builtins().none,
            [single] => *single,
            multiple => type_environment.intern_tuple(multiple.to_vec()),
        };
        let diagnostic_type = diagnostic_type_spelling(expression_type_id, type_environment);

        Self {
            result_type_ids,
            expression_type_id,
            diagnostic_type,
        }
    }
}

/// Best-effort bridge for parse/header paths that still start from diagnostic spelling.
///
/// WHAT: maps syntax/display-only `DataType` values to builtin TypeId hints when the caller does
///      not yet have enough context to resolve through `TypeEnvironment`.
/// WHY: this is a transitional parse-boundary helper, not a semantic equality path. Executable AST
///      and HIR should carry canonical TypeIds resolved through the active type environment.
pub(crate) fn type_id_hint_for_diagnostic_type(data_type: &DataType) -> TypeId {
    match data_type {
        DataType::Bool | DataType::True | DataType::False => builtin_type_ids::BOOL,
        DataType::Int => builtin_type_ids::INT,
        DataType::Float => builtin_type_ids::FLOAT,
        DataType::Decimal => builtin_type_ids::DECIMAL,
        DataType::StringSlice
        | DataType::Template
        | DataType::TemplateWrapper
        | DataType::Path(_) => builtin_type_ids::STRING,
        DataType::Char => builtin_type_ids::CHAR,
        DataType::Range => builtin_type_ids::RANGE,
        DataType::None | DataType::Inferred => builtin_type_ids::NONE,
        DataType::Struct { type_id, .. } | DataType::Choices { type_id, .. } => *type_id,
        _ => builtin_type_ids::NONE,
    }
}

/// Input struct for `Expression::choice_construct` to avoid a long parameter list.
pub struct ChoiceConstructInput {
    pub nominal_path: InternedPath,
    pub variant: StringId,
    pub tag: usize,
    pub fields: Vec<Declaration>,
    pub diagnostic_type: DataType,
    pub type_id: TypeId,
    pub location: SourceLocation,
    pub value_mode: ValueMode,
}

impl Expression {
    /// Returns the narrow string projection used by compile-time folding/debug paths.
    ///
    /// WHAT: converts literal-like expression shapes into their folded text representation and
    /// returns an empty string for non-renderable runtime constructs.
    /// WHY: this is not a user-facing renderer. Runtime output and public path strings have
    /// stronger formatting contracts owned by template lowering and path formatting.
    pub fn as_string(&self, string_table: &StringTable) -> String {
        match &self.kind {
            // Scalar literals
            ExpressionKind::StringSlice(interned_string) => {
                string_table.resolve(*interned_string).to_owned()
            }
            ExpressionKind::Int(int) => int.to_string(),
            ExpressionKind::Float(float) => float.to_string(),
            ExpressionKind::Bool(bool) => bool.to_string(),
            ExpressionKind::Char(char) => char.to_string(),

            // Paths and references
            ExpressionKind::Path(ct_paths) => {
                // WHAT: This returns a bare public-path view without origin.
                // WHY: It is an intermediate representation only (diagnostics/debug/folding
                // contexts that have not crossed the runtime/public formatting boundary).
                // Final runtime/public path strings must go through the shared path formatter
                // (`format_compile_time_path(s)`), where leading `/`, trailing `/`, and
                // `#origin` are applied exactly once. Do not re-prefix origin onto strings
                // that may already be formatted, or origin components can stack.
                ct_paths
                    .paths
                    .iter()
                    .map(|p| p.public_path.to_portable_string(string_table))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
            ExpressionKind::Reference(interned_name) => interned_name.to_string(string_table),

            // Opaque / non-renderable
            ExpressionKind::Copy(..) => String::new(),
            ExpressionKind::Template(..) => String::new(),

            // Aggregates
            ExpressionKind::Collection(items, ..) => {
                let mut result = String::new();
                for item in items {
                    result.push_str(&item.as_string(string_table));
                }
                result
            }
            ExpressionKind::StructInstance(args) | ExpressionKind::StructDefinition(args) => {
                let mut result = String::new();
                for arg in args {
                    result.push_str(&arg.value.as_string(string_table));
                }
                result
            }
            ExpressionKind::Range(lower, upper) => {
                format!(
                    "{} to {}",
                    lower.as_string(string_table),
                    upper.as_string(string_table)
                )
            }
            ExpressionKind::ChoiceConstruct { variant, .. } => {
                string_table.resolve(*variant).to_owned()
            }

            // Functions and calls
            ExpressionKind::Function(..) => String::new(),
            ExpressionKind::FunctionCall { .. } => String::new(),
            ExpressionKind::HandledFallibleFunctionCall { .. } => String::new(),
            ExpressionKind::HandledFallibleHostFunctionCall { .. } => String::new(),
            ExpressionKind::BuiltinCast { .. } => String::new(),
            ExpressionKind::HandledFallibleExpression { .. } => String::new(),
            ExpressionKind::OptionPropagation { .. } => String::new(),
            ExpressionKind::HostFunctionCall { .. } => String::new(),

            // Carriers and special forms
            ExpressionKind::FallibleCarrierConstruct { variant, value } => match variant {
                FallibleCarrierVariant::Success => value.as_string(string_table),
                FallibleCarrierVariant::Error => String::new(),
            },
            ExpressionKind::Runtime(..) => String::new(),
            ExpressionKind::NoValue => String::new(),
            ExpressionKind::OptionNone => String::new(),
            ExpressionKind::Coerced { value, .. } => value.as_string(string_table),

            ExpressionKind::ValueBlock { .. } => String::new(),
        }
    }

    /// Generic constructor for an expression with the given kind and type metadata.
    pub fn new(
        kind: ExpressionKind,
        location: SourceLocation,
        type_id: TypeId,
        diagnostic_type: DataType,
        value_mode: ValueMode,
    ) -> Self {
        Self {
            type_id,
            diagnostic_type,
            function_receiver: None,
            kind,
            location,
            value_mode,
            const_record_state: ConstRecordState::RuntimeValue,
            contains_regular_division: false,
        }
    }

    /// Returns true if this expression represents a const-record value.
    pub fn is_const_record_value(&self) -> bool {
        matches!(self.const_record_state, ConstRecordState::ConstRecord)
    }

    /// Marks whether this expression originates from a regular division operator.
    pub fn with_regular_division_provenance(mut self, contains: bool) -> Self {
        self.contains_regular_division = contains;
        self
    }

    /// Centralises scalar literal construction so literal factories stay structurally identical.
    fn scalar_literal(
        kind: ExpressionKind,
        type_id: TypeId,
        diagnostic_type: DataType,
        location: SourceLocation,
        value_mode: ValueMode,
    ) -> Self {
        Self::new(kind, location, type_id, diagnostic_type, value_mode)
    }

    fn call_expression_with_resolved_types(
        kind: ExpressionKind,
        resolved_types: ResolvedCallTypes,
        location: SourceLocation,
    ) -> Self {
        let ResolvedCallTypes {
            result_type_ids: _,
            expression_type_id,
            diagnostic_type,
        } = resolved_types;

        Self::new(
            kind,
            location,
            expression_type_id,
            diagnostic_type,
            // Planned: derive ownership from alias-aware return signatures once
            // signature alias metadata is threaded through expression construction.
            // If the return signature is a reference (the name of a parameter passed in),
            // then this is a reference to that parameter.
            ValueMode::MutableOwned,
        )
    }

    /// Wraps a list of runtime AST nodes into a single runtime expression.
    pub fn runtime_with_type_id(
        expressions: Vec<AstNode>,
        data_type: DataType,
        type_id: TypeId,
        location: SourceLocation,
        value_mode: ValueMode,
    ) -> Self {
        let contains_regular_division = expressions.iter().any(Self::node_has_regular_division);

        Self::new(
            ExpressionKind::Runtime(expressions),
            location,
            type_id,
            data_type,
            value_mode,
        )
        .with_regular_division_provenance(contains_regular_division)
    }

    /// Constructs an integer literal expression.
    pub fn int(value: i64, location: SourceLocation, value_mode: ValueMode) -> Self {
        Self::scalar_literal(
            ExpressionKind::Int(value),
            builtin_type_ids::INT,
            DataType::Int,
            location,
            value_mode,
        )
    }

    /// Constructs a floating-point literal expression.
    pub fn float(value: f64, location: SourceLocation, value_mode: ValueMode) -> Self {
        Self::scalar_literal(
            ExpressionKind::Float(value),
            builtin_type_ids::FLOAT,
            DataType::Float,
            location,
            value_mode,
        )
    }

    /// Constructs a string slice literal expression.
    pub fn string_slice(value: StringId, location: SourceLocation, value_mode: ValueMode) -> Self {
        Self::scalar_literal(
            ExpressionKind::StringSlice(value),
            builtin_type_ids::STRING,
            DataType::StringSlice,
            location,
            value_mode,
        )
    }

    /// Constructs a boolean literal expression.
    pub fn bool(value: bool, location: SourceLocation, value_mode: ValueMode) -> Self {
        Self::scalar_literal(
            ExpressionKind::Bool(value),
            builtin_type_ids::BOOL,
            DataType::Bool,
            location,
            value_mode,
        )
    }

    /// Constructs a character literal expression.
    pub fn char(value: char, location: SourceLocation, value_mode: ValueMode) -> Self {
        Self::scalar_literal(
            ExpressionKind::Char(value),
            builtin_type_ids::CHAR,
            DataType::Char,
            location,
            value_mode,
        )
    }

    /// Constructs a reference expression from an interned path.
    pub fn reference_with_type_id(
        id: InternedPath,
        data_type: DataType,
        type_id: TypeId,
        location: SourceLocation,
        value_mode: ValueMode,
        const_record_state: ConstRecordState,
    ) -> Self {
        let mut expression = Self::new(
            ExpressionKind::Reference(id),
            location,
            type_id,
            data_type,
            value_mode,
        );
        expression.const_record_state = const_record_state;
        expression
    }

    /// Constructs a function expression with an optional receiver.
    pub fn function(
        receiver: Option<ReceiverKey>,
        signature: FunctionSignature,
        body: Vec<AstNode>,
        type_id: TypeId,
        location: SourceLocation,
    ) -> Self {
        let function_data_type = DataType::Function(Box::new(receiver.clone()), signature.clone());
        let mut expression = Self::new(
            ExpressionKind::Function(signature, body),
            location,
            type_id,
            function_data_type,
            ValueMode::ImmutableReference,
        );
        expression.function_receiver = receiver;
        expression
    }

    /// Constructs a resolved function call expression.
    pub(crate) fn function_call_with_typed_arguments(
        name: InternedPath,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        type_environment: &mut TypeEnvironment,
        location: SourceLocation,
    ) -> Self {
        let resolved_types = ResolvedCallTypes::new(result_type_ids, type_environment);

        let result_type_ids = resolved_types.result_type_ids.clone();
        Self::call_expression_with_resolved_types(
            ExpressionKind::FunctionCall {
                name,
                args,
                result_type_ids,
            },
            resolved_types,
            location,
        )
    }

    /// Constructs a resolved fallible function call with explicit error handling.
    pub(crate) fn handled_fallible_function_call_with_typed_arguments(
        name: InternedPath,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        handling: FallibleHandling,
        type_environment: &mut TypeEnvironment,
        location: SourceLocation,
    ) -> Self {
        let resolved_types = ResolvedCallTypes::new(result_type_ids, type_environment);

        let result_type_ids = resolved_types.result_type_ids.clone();
        Self::call_expression_with_resolved_types(
            ExpressionKind::HandledFallibleFunctionCall {
                name,
                args,
                result_type_ids,
                handling,
            },
            resolved_types,
            location,
        )
    }

    /// Constructs a resolved host function call expression.
    pub(crate) fn host_function_call_with_typed_arguments(
        id: ExternalFunctionId,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        type_environment: &mut TypeEnvironment,
        location: SourceLocation,
    ) -> Self {
        let resolved_types = ResolvedCallTypes::new(result_type_ids, type_environment);

        let result_type_ids = resolved_types.result_type_ids.clone();
        Self::call_expression_with_resolved_types(
            ExpressionKind::HostFunctionCall {
                id,
                args,
                result_type_ids,
            },
            resolved_types,
            location,
        )
    }

    /// Constructs a resolved fallible host function call with explicit error handling.
    pub(crate) fn handled_fallible_host_function_call_with_typed_arguments(
        input: HandledFallibleHostFunctionCallInput,
        type_environment: &mut TypeEnvironment,
    ) -> Self {
        let HandledFallibleHostFunctionCallInput {
            id,
            args,
            result_type_ids,
            error_type_id,
            handling,
            location,
        } = input;
        let resolved_types = ResolvedCallTypes::new(result_type_ids, type_environment);

        let result_type_ids = resolved_types.result_type_ids.clone();
        Self::call_expression_with_resolved_types(
            ExpressionKind::HandledFallibleHostFunctionCall {
                id,
                args,
                result_type_ids,
                error_type_id,
                handling,
            },
            resolved_types,
            location,
        )
    }

    /// Internal helper for building builtin cast expressions.
    ///
    /// Casts are fallible operations that must be handled immediately.
    /// HIR lowering turns the carrier `result_type_id` into explicit success/error control flow.
    /// `diagnostic_type` is display-only; semantic identity comes from `result_type_id`.
    fn builtin_cast(
        value: Expression,
        kind: BuiltinCastKind,
        result_type_id: TypeId,
        type_environment: &mut TypeEnvironment,
        location: SourceLocation,
    ) -> Self {
        let contains_regular_division = value.contains_regular_division;
        Self::new(
            ExpressionKind::BuiltinCast {
                kind,
                value: Box::new(value),
            },
            location,
            result_type_id,
            diagnostic_type_spelling(result_type_id, type_environment),
            ValueMode::ImmutableOwned,
        )
        .with_regular_division_provenance(contains_regular_division)
    }

    /// Constructs a builtin integer cast expression.
    pub fn builtin_int_cast(
        value: Expression,
        error_type_id: TypeId,
        type_environment: &mut TypeEnvironment,
        location: SourceLocation,
    ) -> Self {
        let result_type_id = type_environment
            .intern_fallible_carrier(type_environment.builtins().int, error_type_id);
        Self::builtin_cast(
            value,
            BuiltinCastKind::Int,
            result_type_id,
            type_environment,
            location,
        )
    }

    /// Constructs a builtin float cast expression.
    pub fn builtin_float_cast(
        value: Expression,
        error_type_id: TypeId,
        type_environment: &mut TypeEnvironment,
        location: SourceLocation,
    ) -> Self {
        let result_type_id = type_environment
            .intern_fallible_carrier(type_environment.builtins().float, error_type_id);
        Self::builtin_cast(
            value,
            BuiltinCastKind::Float,
            result_type_id,
            type_environment,
            location,
        )
    }

    /// Build an explicit contextual coercion node.
    ///
    /// WHAT: wraps `value` in a `Coerced` expression kind that carries the
    /// target type explicitly in the AST.
    /// WHY: contextual conversions such as `Int` → `Float` and `T` → `T?` must
    /// be represented deliberately so lowering stages can emit the correct
    /// conversion rather than silently mistyping the inner value.
    pub fn coerced(value: Expression, to_type: TypeId) -> Self {
        let location = value.location.clone();
        let value_mode = value.value_mode.to_owned();
        let contains_regular_division = value.contains_regular_division;
        let const_record_state = value.const_record_state;
        let mut expression = Self::new(
            ExpressionKind::Coerced {
                value: Box::new(value),
                to_type,
            },
            location,
            to_type,
            // diagnostic_type is non-authoritative; Inferred is sufficient for a
            // coercion node because semantic identity comes from type_id alone.
            DataType::Inferred,
            value_mode,
        )
        .with_regular_division_provenance(contains_regular_division);
        expression.const_record_state = const_record_state;
        expression
    }

    /// Constructs a fallible carrier (result) expression.
    pub fn result_construct_with_type_id(
        variant: FallibleCarrierVariant,
        value: Expression,
        diagnostic_type: DataType,
        type_id: TypeId,
        location: SourceLocation,
        value_mode: ValueMode,
    ) -> Self {
        let contains_regular_division = value.contains_regular_division;
        Self::new(
            ExpressionKind::FallibleCarrierConstruct {
                variant,
                value: Box::new(value),
            },
            location,
            type_id,
            diagnostic_type,
            value_mode,
        )
        .with_regular_division_provenance(contains_regular_division)
    }

    /// Wraps a fallible expression with explicit handling.
    pub fn handled_result_with_type_id(
        value: Expression,
        handling: FallibleHandling,
        result_type_id: TypeId,
        diagnostic_type: DataType,
        location: SourceLocation,
    ) -> Self {
        let contains_regular_division = value.contains_regular_division;
        Self::new(
            ExpressionKind::HandledFallibleExpression {
                value: Box::new(value),
                handling,
            },
            location,
            result_type_id,
            diagnostic_type,
            ValueMode::ImmutableOwned,
        )
        .with_regular_division_provenance(contains_regular_division)
    }

    /// Wraps an optional expression with postfix `?` propagation.
    pub fn option_propagation_with_type_id(
        value: Expression,
        inner_type_id: TypeId,
        diagnostic_type: DataType,
        location: SourceLocation,
    ) -> Self {
        let contains_regular_division = value.contains_regular_division;
        Self::new(
            ExpressionKind::OptionPropagation {
                value: Box::new(value),
            },
            location,
            inner_type_id,
            diagnostic_type,
            ValueMode::ImmutableOwned,
        )
        .with_regular_division_provenance(contains_regular_division)
    }

    /// Constructs a collection expression with a resolved element type.
    pub(crate) fn collection_with_type_id(
        items: Vec<Expression>,
        inner_type_id: TypeId,
        inner_diagnostic_type: DataType,
        type_environment: &mut TypeEnvironment,
        location: SourceLocation,
        value_mode: ValueMode,
    ) -> Self {
        let collection_type_id = type_environment.intern_collection(inner_type_id);
        let contains_regular_division = items.iter().any(|item| item.contains_regular_division);
        // `diagnostic_type` is display-only; semantic identity comes from `collection_type_id`.
        Self::new(
            ExpressionKind::Collection(items),
            location,
            collection_type_id,
            DataType::collection(inner_diagnostic_type),
            value_mode,
        )
        .with_regular_division_provenance(contains_regular_division)
    }

    /// Constructs a struct instance expression.
    pub fn struct_instance(
        nominal_path: InternedPath,
        args: Vec<Declaration>,
        location: SourceLocation,
        value_mode: ValueMode,
        const_record: bool,
        generic_instance_key: Option<GenericInstantiationKey>,
        type_id: TypeId,
    ) -> Self {
        let contains_regular_division = args.iter().any(|arg| arg.value.contains_regular_division);
        let struct_type = if let Some(key) = generic_instance_key {
            DataType::Struct {
                nominal_path: nominal_path.clone(),
                type_id,
                const_record,
                generic_instance_key: Some(key),
            }
        } else if const_record {
            DataType::const_struct_record(nominal_path, type_id)
        } else {
            DataType::runtime_struct(nominal_path, type_id)
        };
        let mut expression = Self::new(
            ExpressionKind::StructInstance(args),
            location,
            type_id,
            struct_type,
            value_mode,
        )
        .with_regular_division_provenance(contains_regular_division);
        if const_record {
            expression.const_record_state = ConstRecordState::ConstRecord;
        }
        expression
    }

    /// Constructs a struct definition expression.
    pub fn struct_definition(
        args: Vec<Declaration>,
        location: SourceLocation,
        value_mode: ValueMode,
    ) -> Self {
        Self::new(
            ExpressionKind::StructDefinition(args),
            location,
            builtin_type_ids::NONE,
            DataType::Inferred,
            value_mode,
        )
    }

    /// Constructs a template expression.
    pub fn template(template: Template, value_mode: ValueMode) -> Self {
        let location = template.location.to_owned();
        Self::new(
            ExpressionKind::Template(Box::new(template)),
            location,
            builtin_type_ids::STRING,
            DataType::Template,
            value_mode,
        )
    }

    /// Constructs a copy expression from an AST node place.
    pub fn copy_with_type_id(
        place: AstNode,
        data_type: DataType,
        type_id: TypeId,
        location: SourceLocation,
        value_mode: ValueMode,
    ) -> Self {
        Self::new(
            ExpressionKind::Copy(Box::new(place)),
            location,
            type_id,
            data_type,
            value_mode.as_owned(),
        )
    }

    /// Internal sentinel used for declarations/signature defaults that do not
    /// provide a value expression in source.
    pub fn no_value(location: SourceLocation, data_type: DataType, value_mode: ValueMode) -> Self {
        let type_id = type_id_hint_for_diagnostic_type(&data_type);
        Self::no_value_with_type_id(location, data_type, type_id, value_mode)
    }

    /// Internal sentinel for declarations whose canonical type is already known.
    ///
    /// Constructed and nominal types cannot be recovered from diagnostic spelling alone, so parser
    /// sites that already resolved a `TypeId` should preserve it here for later expression typing.
    pub fn no_value_with_type_id(
        location: SourceLocation,
        data_type: DataType,
        type_id: TypeId,
        value_mode: ValueMode,
    ) -> Self {
        Self::new(
            ExpressionKind::NoValue,
            location,
            type_id,
            data_type,
            value_mode,
        )
    }

    /// Constructs a `none` literal for an optional type.
    pub fn option_none_with_type_id(
        inner_type_id: TypeId,
        inner_diagnostic_type: DataType,
        type_environment: &mut TypeEnvironment,
        location: SourceLocation,
    ) -> Self {
        let option_type_id = type_environment.intern_option(inner_type_id);
        // `diagnostic_type` is display-only; semantic identity comes from `option_type_id`.
        Self::new(
            ExpressionKind::OptionNone,
            location,
            option_type_id,
            DataType::Option(Box::new(inner_diagnostic_type)),
            ValueMode::ImmutableOwned,
        )
    }

    /// Constructs a choice variant instance.
    pub fn choice_construct(input: ChoiceConstructInput) -> Self {
        let contains_regular_division = input
            .fields
            .iter()
            .any(|field| field.value.contains_regular_division);
        Self::new(
            ExpressionKind::ChoiceConstruct {
                nominal_path: input.nominal_path,
                variant: input.variant,
                tag: input.tag,
                fields: input.fields,
            },
            input.location,
            input.type_id,
            input.diagnostic_type,
            input.value_mode,
        )
        .with_regular_division_provenance(contains_regular_division)
    }

    /// Returns true if this expression is a compile-time constant.
    pub fn is_compile_time_constant(&self) -> bool {
        self.const_value_kind().is_compile_time_value()
    }

    /// Returns true if this expression represents a function declaration that has
    /// a receiver parameter (`this` or `This`).
    pub(crate) fn is_receiver_function(&self) -> bool {
        self.function_receiver.is_some()
    }

    /// Checks whether every expression in a slice is a compile-time constant.
    fn expressions_are_constant(expressions: &[Expression]) -> bool {
        expressions.iter().all(Expression::is_compile_time_constant)
    }

    /// Checks whether every declaration's value in a slice is a compile-time constant.
    fn declarations_are_constant(declarations: &[Declaration]) -> bool {
        declarations
            .iter()
            .all(|declaration| declaration.value.is_compile_time_constant())
    }

    /// Classifies the compile-time const-ness of this expression.
    pub fn const_value_kind(&self) -> ConstValueKind {
        match &self.kind {
            // Literal scalars are always compile-time constants.
            ExpressionKind::Int(_)
            | ExpressionKind::Float(_)
            | ExpressionKind::StringSlice(_)
            | ExpressionKind::Bool(_)
            | ExpressionKind::Char(_)
            | ExpressionKind::Path(_) => ConstValueKind::Literal,

            // Composite values are constant only when every sub-field is constant.
            ExpressionKind::ChoiceConstruct { fields, .. } => {
                if fields.is_empty() {
                    ConstValueKind::Literal
                } else if Self::declarations_are_constant(fields) {
                    ConstValueKind::Composite
                } else {
                    ConstValueKind::NonConst
                }
            }

            ExpressionKind::Collection(items) => {
                if Self::expressions_are_constant(items) {
                    ConstValueKind::Composite
                } else {
                    ConstValueKind::NonConst
                }
            }

            ExpressionKind::StructInstance(fields) => {
                if Self::declarations_are_constant(fields) {
                    ConstValueKind::Composite
                } else {
                    ConstValueKind::NonConst
                }
            }

            ExpressionKind::Range(start, end) => {
                if start.is_compile_time_constant() && end.is_compile_time_constant() {
                    ConstValueKind::Composite
                } else {
                    ConstValueKind::NonConst
                }
            }

            // Template const classification is delegated to the template.
            ExpressionKind::Template(template) => match template.const_value_kind() {
                TemplateConstValueKind::RenderableString => ConstValueKind::RenderableTemplate,
                TemplateConstValueKind::WrapperTemplate => ConstValueKind::TemplateWrapper,
                TemplateConstValueKind::SlotInsertHelper => ConstValueKind::SlotInsertTemplate,
                TemplateConstValueKind::NonConst => ConstValueKind::NonConst,
            },

            // Fallible carriers preserve const-ness of the wrapped value.
            ExpressionKind::FallibleCarrierConstruct { value, .. } => {
                if value.is_compile_time_constant() {
                    ConstValueKind::Composite
                } else {
                    ConstValueKind::NonConst
                }
            }

            // Everything else is non-const by default.
            ExpressionKind::Reference(_)
            | ExpressionKind::Copy(_)
            | ExpressionKind::Runtime(_)
            | ExpressionKind::Function(..)
            | ExpressionKind::FunctionCall { .. }
            | ExpressionKind::BuiltinCast { .. }
            | ExpressionKind::HandledFallibleExpression { .. }
            | ExpressionKind::OptionPropagation { .. }
            | ExpressionKind::HandledFallibleFunctionCall { .. }
            | ExpressionKind::HandledFallibleHostFunctionCall { .. }
            | ExpressionKind::HostFunctionCall { .. }
            | ExpressionKind::StructDefinition(..)
            | ExpressionKind::NoValue
            | ExpressionKind::OptionNone
            | ExpressionKind::ValueBlock { .. } => ConstValueKind::NonConst,

            // Delegate const classification to the wrapped value — the coercion
            // does not change whether an expression is compile-time foldable.
            ExpressionKind::Coerced { value, .. } => value.const_value_kind(),
        }
    }

    /// Checks whether an AST node contains a regular division operator.
    fn node_has_regular_division(node: &AstNode) -> bool {
        match &node.kind {
            NodeKind::Operator(Operator::Divide) => true,
            NodeKind::Rvalue(expr) => expr.contains_regular_division,
            _ => false,
        }
    }

    /// Remap all interned string IDs in this expression recursively.
    ///
    /// WHAT: updates `diagnostic_type`, `function_receiver`, `location`, and the
    ///       expression kind, including nested declarations, AST nodes, and templates.
    /// WHY: per-file header parsing produces expression defaults using local string tables;
    ///      remapping keeps them valid after merge into the module/global table.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.diagnostic_type.remap_string_ids(remap);
        if let Some(receiver) = &mut self.function_receiver {
            receiver.remap_string_ids(remap);
        }
        self.location.remap_string_ids(remap);
        self.kind.remap_string_ids(remap);
    }
}
