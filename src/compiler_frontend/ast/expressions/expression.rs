//! AST expression values and constructor helpers used before HIR lowering.
//!
//! WHAT: defines frontend expression kinds plus the factory methods that build typed AST values.
//! WHY: parser and folding code should create expressions through one readable surface instead of
//! manually reassembling `Expression` fields at each call site.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::datatypes::{DataType, Ownership, PathTypeKind, ReceiverKey};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::CompileTimePaths;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

// Expressions represent anything that will turn into a value
// Their kind will represent what their value is.
// Runtime expressions (couldn't be folded) are represented as 'runtime' kinds.
// These runtime expressions are small ASTs that must be represented at runtime.
// Expression kinds are like a subset of the core datatypes because some data types don't return values or represent more complex structures.
#[derive(Clone, Debug)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub data_type: DataType,
    pub ownership: Ownership,
    pub location: SourceLocation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstValueKind {
    Literal,
    Composite,
    RenderableTemplate,
    TemplateWrapper,
    SlotInsertTemplate,
    NonConst,
}

impl ConstValueKind {
    pub fn is_compile_time_value(self) -> bool {
        !matches!(self, Self::NonConst)
    }
}

#[derive(Clone, Debug)]
pub enum ResultCallHandling {
    Propagate,
    Fallback(Vec<Expression>),
    Handler {
        error_name: StringId,
        error_binding: InternedPath,
        fallback: Option<Vec<Expression>>,
        body: Vec<AstNode>,
    },
}

impl Expression {
    pub fn as_string(&self, string_table: &StringTable) -> String {
        match &self.kind {
            ExpressionKind::StringSlice(interned_string) => {
                string_table.resolve(*interned_string).to_owned()
            }
            ExpressionKind::Int(int) => int.to_string(),
            ExpressionKind::Float(float) => float.to_string(),
            ExpressionKind::Bool(bool) => bool.to_string(),
            ExpressionKind::Char(char) => char.to_string(),
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
            ExpressionKind::Copy(..) => String::new(),
            ExpressionKind::Template(..) => String::new(),
            ExpressionKind::Collection(items, ..) => {
                let mut all_items = String::new();
                for item in items {
                    all_items.push_str(&item.as_string(string_table));
                }
                all_items
            }
            ExpressionKind::StructInstance(args) | ExpressionKind::StructDefinition(args) => {
                let mut all_items = String::new();
                for arg in args {
                    all_items.push_str(&arg.value.as_string(string_table));
                }
                all_items
            }
            ExpressionKind::Function(..) => String::new(),
            ExpressionKind::FunctionCall(..) => String::new(),
            ExpressionKind::ResultHandledFunctionCall { .. } => String::new(),
            ExpressionKind::HostFunctionCall(..) => String::new(),
            ExpressionKind::Runtime(..) => String::new(),
            ExpressionKind::Range(lower, upper) => {
                format!(
                    "{} to {}",
                    lower.as_string(string_table),
                    upper.as_string(string_table)
                )
            }
            ExpressionKind::NoValue => String::new(),
            ExpressionKind::OptionNone => String::new(),
        }
    }

    pub fn new(
        kind: ExpressionKind,
        location: SourceLocation,
        data_type: DataType,
        ownership: Ownership,
    ) -> Self {
        Self {
            data_type,
            kind,
            location,
            ownership,
        }
    }

    /// Centralises scalar literal construction so literal factories stay structurally identical.
    fn scalar_literal(
        kind: ExpressionKind,
        data_type: DataType,
        location: SourceLocation,
        ownership: Ownership,
    ) -> Self {
        Self::new(kind, location, data_type, ownership)
    }

    /// Collapse function return signatures into the AST expression type model.
    ///
    /// WHY: single-return calls should stay ergonomic while multi-return calls preserve the full
    /// tuple-like `Returns` wrapper expected by later lowering stages.
    pub(crate) fn call_result_type(mut result_types: Vec<DataType>) -> DataType {
        if result_types.len() == 1 {
            result_types.pop().unwrap_or(DataType::None)
        } else {
            DataType::Returns(result_types)
        }
    }

    /// Build a function or host-function call with the shared return-type/ownership policy.
    fn call_expression(
        kind: ExpressionKind,
        result_types: Vec<DataType>,
        location: SourceLocation,
    ) -> Self {
        Self::new(
            kind,
            location,
            Self::call_result_type(result_types),
            // Planned: derive ownership from alias-aware return signatures once
            // signature alias metadata is threaded through expression construction.
            // If the return signature is a reference (the name of a parameter passed in),
            // then this is a reference to that parameter.
            Ownership::MutableOwned,
        )
    }
    pub fn runtime(
        expressions: Vec<AstNode>,
        data_type: DataType,
        location: SourceLocation,
        ownership: Ownership,
    ) -> Self {
        Self::new(
            ExpressionKind::Runtime(expressions),
            location,
            data_type,
            ownership,
        )
    }
    pub fn int(value: i64, location: SourceLocation, ownership: Ownership) -> Self {
        Self::scalar_literal(
            ExpressionKind::Int(value),
            DataType::Int,
            location,
            ownership,
        )
    }
    pub fn float(value: f64, location: SourceLocation, ownership: Ownership) -> Self {
        Self::scalar_literal(
            ExpressionKind::Float(value),
            DataType::Float,
            location,
            ownership,
        )
    }
    pub fn string_slice(value: StringId, location: SourceLocation, ownership: Ownership) -> Self {
        Self::scalar_literal(
            ExpressionKind::StringSlice(value),
            DataType::StringSlice,
            location,
            ownership,
        )
    }
    pub fn bool(value: bool, location: SourceLocation, ownership: Ownership) -> Self {
        Self::scalar_literal(
            ExpressionKind::Bool(value),
            DataType::Bool,
            location,
            ownership,
        )
    }
    pub fn char(value: char, location: SourceLocation, ownership: Ownership) -> Self {
        Self::scalar_literal(
            ExpressionKind::Char(value),
            DataType::Char,
            location,
            ownership,
        )
    }

    #[allow(dead_code)] // Planned: compile-time path literals in expressions.
    pub fn path(compile_time_paths: CompileTimePaths, location: SourceLocation) -> Self {
        // Derives the path type kind from the first resolved path.
        let path_type_kind = compile_time_paths
            .paths
            .first()
            .map(|p| PathTypeKind::from(p.kind.clone()))
            .unwrap_or(PathTypeKind::File);
        Self::new(
            ExpressionKind::Path(Box::new(compile_time_paths)),
            location,
            DataType::Path(path_type_kind),
            Ownership::ImmutableOwned,
        )
    }

    pub fn reference(
        id: InternedPath,
        data_type: DataType,
        location: SourceLocation,
        ownership: Ownership,
    ) -> Self {
        Self::new(
            ExpressionKind::Reference(id),
            location,
            data_type,
            ownership,
        )
    }

    // Creating Functions
    pub fn function(
        receiver: Option<ReceiverKey>,
        signature: FunctionSignature,
        body: Vec<AstNode>,
        location: SourceLocation,
    ) -> Self {
        let function_data_type = DataType::Function(Box::new(receiver), signature.clone());
        Self::new(
            ExpressionKind::Function(signature, body),
            location,
            function_data_type,
            Ownership::ImmutableReference,
        )
    }

    // Function calls
    pub fn function_call(
        name: InternedPath,
        args: Vec<Expression>,
        result_types: Vec<DataType>,
        location: SourceLocation,
    ) -> Self {
        Self::call_expression(
            ExpressionKind::FunctionCall(name, args),
            result_types,
            location,
        )
    }

    pub fn result_handled_function_call(
        name: InternedPath,
        args: Vec<Expression>,
        result_types: Vec<DataType>,
        handling: ResultCallHandling,
        location: SourceLocation,
    ) -> Self {
        Self::call_expression(
            ExpressionKind::ResultHandledFunctionCall {
                name,
                args,
                handling,
            },
            result_types,
            location,
        )
    }

    pub fn host_function_call(
        name: InternedPath,
        args: Vec<Expression>,
        result_types: Vec<DataType>,
        location: SourceLocation,
    ) -> Self {
        Self::call_expression(
            ExpressionKind::HostFunctionCall(name, args),
            result_types,
            location,
        )
    }

    pub fn collection(
        items: Vec<Expression>,
        location: SourceLocation,
        ownership: Ownership,
    ) -> Self {
        let inner_type = items
            .first()
            .map(|item| item.data_type.to_owned())
            .unwrap_or(DataType::Int);

        Self::new(
            ExpressionKind::Collection(items),
            location,
            DataType::Collection(Box::new(inner_type), ownership.to_owned()),
            ownership,
        )
    }
    pub fn struct_instance(
        nominal_path: InternedPath,
        args: Vec<Declaration>,
        location: SourceLocation,
        ownership: Ownership,
        const_record: bool,
    ) -> Self {
        let struct_type = if const_record {
            DataType::const_struct_record(nominal_path, args.to_owned())
        } else {
            DataType::runtime_struct(nominal_path, args.to_owned(), ownership.to_owned())
        };
        Self::new(
            ExpressionKind::StructInstance(args),
            location,
            struct_type,
            ownership,
        )
    }
    pub fn struct_definition(
        args: Vec<Declaration>,
        location: SourceLocation,
        ownership: Ownership,
    ) -> Self {
        Self::new(
            ExpressionKind::StructDefinition(args),
            location,
            DataType::Inferred,
            ownership,
        )
    }
    pub fn template(template: Template, ownership: Ownership) -> Self {
        let location = template.location.to_owned();
        Self::new(
            ExpressionKind::Template(Box::new(template)),
            location,
            DataType::Template,
            ownership,
        )
    }

    #[allow(dead_code)] // Planned: explicit range expression construction helpers.
    pub fn range(
        lower: Expression,
        upper: Expression,
        location: SourceLocation,
        ownership: Ownership,
    ) -> Self {
        Self::new(
            ExpressionKind::Range(Box::new(lower), Box::new(upper)),
            location,
            DataType::Inferred,
            ownership,
        )
    }

    pub fn copy(
        place: AstNode,
        data_type: DataType,
        location: SourceLocation,
        ownership: Ownership,
    ) -> Self {
        Self::new(
            ExpressionKind::Copy(Box::new(place)),
            location,
            data_type,
            ownership.get_owned(),
        )
    }

    /// Internal sentinel used for declarations/signature defaults that do not
    /// provide a value expression in source.
    pub fn no_value(location: SourceLocation, data_type: DataType, ownership: Ownership) -> Self {
        Self::new(ExpressionKind::NoValue, location, data_type, ownership)
    }

    /// User-facing `none` literal in an optional context.
    pub fn option_none(inner_type: DataType, location: SourceLocation) -> Self {
        Self::new(
            ExpressionKind::OptionNone,
            location,
            DataType::Option(Box::new(inner_type)),
            Ownership::ImmutableOwned,
        )
    }

    pub fn is_compile_time_constant(&self) -> bool {
        self.const_value_kind().is_compile_time_value()
    }

    pub fn const_value_kind(&self) -> ConstValueKind {
        match &self.kind {
            ExpressionKind::Int(_)
            | ExpressionKind::Float(_)
            | ExpressionKind::StringSlice(_)
            | ExpressionKind::Bool(_)
            | ExpressionKind::Char(_)
            | ExpressionKind::Path(_) => ConstValueKind::Literal,
            ExpressionKind::Collection(items) => {
                if items.iter().all(Expression::is_compile_time_constant) {
                    ConstValueKind::Composite
                } else {
                    ConstValueKind::NonConst
                }
            }
            ExpressionKind::StructInstance(fields) => {
                if fields
                    .iter()
                    .all(|field| field.value.is_compile_time_constant())
                {
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
            ExpressionKind::Template(template) => match template.const_value_kind() {
                TemplateConstValueKind::RenderableString => ConstValueKind::RenderableTemplate,
                TemplateConstValueKind::WrapperTemplate => ConstValueKind::TemplateWrapper,
                TemplateConstValueKind::SlotInsertHelper => ConstValueKind::SlotInsertTemplate,
                TemplateConstValueKind::NonConst => ConstValueKind::NonConst,
            },
            ExpressionKind::Reference(_)
            | ExpressionKind::Copy(_)
            | ExpressionKind::Runtime(_)
            | ExpressionKind::Function(..)
            | ExpressionKind::FunctionCall(..)
            | ExpressionKind::ResultHandledFunctionCall { .. }
            | ExpressionKind::HostFunctionCall(..)
            | ExpressionKind::StructDefinition(..)
            | ExpressionKind::NoValue
            | ExpressionKind::OptionNone => ConstValueKind::NonConst,
        }
    }
}
#[derive(Clone, Debug)]
pub enum ExpressionKind {
    /// Internal sentinel for "no source value was provided" (for example, a
    /// parameter default that is intentionally absent).
    NoValue,

    /// User-authored `none` literal in an explicit option context.
    OptionNone,

    Runtime(Vec<AstNode>),

    Int(i64),
    Float(f64),
    StringSlice(StringId),
    Bool(bool),
    Char(char),

    // Compile-time path literal(s) — one or more resolved paths from grouped syntax.
    #[allow(dead_code)] // Will be needed for path expressions in the future
    Path(Box<CompileTimePaths>),

    // Reference to a variable by name
    Reference(InternedPath),

    // Explicitly materialize a fresh value from an aliasing place.
    Copy(Box<AstNode>),

    // Because functions can all be values
    Function(
        FunctionSignature,
        Vec<AstNode>, // body
    ),

    FunctionCall(
        InternedPath,    // Function name
        Vec<Expression>, // Arguments
    ),

    ResultHandledFunctionCall {
        name: InternedPath,
        args: Vec<Expression>,
        handling: ResultCallHandling,
    },

    HostFunctionCall(InternedPath, Vec<Expression>),

    // Also equivalent to a String if it folds into a string
    Template(Box<Template>), // Template Body, Styles, ID

    Collection(Vec<Expression>),

    StructDefinition(Vec<Declaration>),
    StructInstance(Vec<Declaration>),

    // This is a special case for the range operator
    // This implementation will probably change in the future to be a more general operator
    // Upper and lower bounds are inclusive.
    // Instead of making this a function, it has its own special case to make constant folding easier
    Range(Box<Expression>, Box<Expression>),
}

impl ExpressionKind {
    pub fn is_foldable(&self) -> bool {
        matches!(
            self,
            ExpressionKind::Int(_)
                | ExpressionKind::Float(_)
                | ExpressionKind::Bool(_)
                | ExpressionKind::StringSlice(_)
                | ExpressionKind::Char(_)
                | ExpressionKind::Path(_)
        )
    }

    #[allow(dead_code)] // Planned: generic iterable checks for collection/range expansion.
    pub fn is_iterable(&self) -> bool {
        matches!(
            self,
            ExpressionKind::Collection(..)
                | ExpressionKind::Int(_)
                | ExpressionKind::Float(_)
                | ExpressionKind::StringSlice(_)
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulus,
    // Remainder,
    #[allow(dead_code)] // Planned: root operator for numeric extensions.
    Root,
    Exponent,

    // Logical
    And,
    Or,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Equality,
    NotEqual,
    Not,

    // Special
    Range,
}

impl Operator {
    pub fn required_values(&self) -> usize {
        match self {
            Operator::Add
            | Operator::Subtract
            | Operator::Multiply
            | Operator::Divide
            | Operator::Modulus
            | Operator::Root
            | Operator::Exponent
            | Operator::And
            | Operator::Or
            | Operator::GreaterThan
            | Operator::GreaterThanOrEqual
            | Operator::LessThan
            | Operator::LessThanOrEqual
            | Operator::Range
            | Operator::Equality
            | Operator::NotEqual => 2,

            // Not is a unary operator
            _ => 1,
        }
    }
    pub fn to_str(&self) -> &str {
        match self {
            Operator::Add => "+",
            Operator::Subtract => "-",
            Operator::Multiply => "*",
            Operator::Divide => "/",
            Operator::Modulus => "%",
            Operator::Root => "root",
            Operator::Exponent => "^",
            Operator::And => "and",
            Operator::Or => "or",
            Operator::GreaterThan => ">",
            Operator::GreaterThanOrEqual => ">=",
            Operator::LessThan => "<",
            Operator::LessThanOrEqual => "<=",
            Operator::Equality => "is",
            Operator::NotEqual => "is not",
            Operator::Not => "not",
            Operator::Range => "..",
        }
    }
}
