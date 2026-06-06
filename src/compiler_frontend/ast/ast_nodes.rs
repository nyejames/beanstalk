//! Core AST node declarations and shared expression-ordering helpers.
//!
//! WHAT: defines statement/expression node shapes plus operator precedence metadata used by
//! expression evaluation.
//! WHY: parser output, type checking, and constant folding need one authoritative AST surface and
//! one precedence table so behavior stays deterministic across frontend stages.
//!
//! ## Diagnostic boundary
//!
//! `CompilerError` in this module means an internal compiler invariant or setup failure only.
//! Source-authored syntax, type, and rule failures are rejected earlier with `CompilerDiagnostic`.

use crate::compiler_frontend::ast::expressions::call_argument::{
    CallArgument, normalize_call_arguments,
};
use crate::compiler_frontend::ast::expressions::expression::{
    ConstRecordState, Expression, ExpressionKind, FallibleHandling, Operator,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::statements::match_patterns::MatchArm;
use crate::compiler_frontend::builtins::CollectionBuiltinOp;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{TypeId, builtin_type_ids};
use crate::compiler_frontend::datatypes::{DataType, diagnostic_type_spelling};
use crate::compiler_frontend::external_packages::ExternalFunctionId;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};
use crate::compiler_frontend::traits::ids::{TraitId, TraitRequirementId};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::return_compiler_error;

pub(crate) use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

#[derive(Debug, Clone)]
pub struct Declaration {
    pub id: InternedPath,
    pub value: Expression,
}

impl Declaration {
    pub(crate) fn is_unresolved_constant_placeholder(&self) -> bool {
        matches!(self.value.kind, ExpressionKind::NoValue)
            && matches!(self.value.diagnostic_type, DataType::Inferred)
    }

    /// Remap interned path and expression in this declaration.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.id.remap_string_ids(remap);
        self.value.remap_string_ids(remap);
    }
}

#[derive(Debug, Clone)]
pub enum MultiBindTargetKind {
    Declaration,
    Assignment,
}

#[derive(Debug, Clone)]
pub struct MultiBindTarget {
    pub id: InternedPath,
    pub diagnostic_type: DataType,
    pub type_id: TypeId,
    pub value_mode: ValueMode,
    pub kind: MultiBindTargetKind,
    pub location: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct AstNode {
    pub kind: NodeKind,
    pub location: SourceLocation,
    pub scope: InternedPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeEndKind {
    Exclusive,
    Inclusive,
}

#[derive(Debug, Clone)]
pub struct LoopBindings {
    pub item: Option<Declaration>,
    pub index: Option<Declaration>,
}

#[derive(Debug, Clone)]
pub struct RangeLoopSpec {
    pub start: Expression,
    pub end: Expression,
    pub end_kind: RangeEndKind,
    pub step: Option<Expression>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchExhaustiveness {
    /// The match has an explicit `else =>` arm.
    HasDefault,
    /// The match has no explicit default, but AST proved it covers every choice variant.
    ExhaustiveChoice,
}

/// Text payload for a failed assertion message.
///
/// WHAT: carries the literal string data and source location for an assertion message.
/// WHY: assertion messages are compile-time text, not runtime expressions, so they are
///      stored as resolved string data rather than as an `Expression` node.
#[derive(Debug, Clone)]
pub struct AssertMessage {
    pub text: StringId,
    pub location: SourceLocation,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum NodeKind {
    // Control Flow
    Return(Vec<Expression>),                            // Return value,
    ReturnError(Expression),                            // return! value
    If(Expression, Vec<AstNode>, Option<Vec<AstNode>>), // Condition, If true, Else

    Match {
        scrutinee: Expression,
        arms: Vec<MatchArm>,
        default: Option<Vec<AstNode>>,
        exhaustiveness: MatchExhaustiveness,
    },

    ScopedBlock {
        body: Vec<AstNode>,
    },

    RangeLoop {
        bindings: LoopBindings,
        range: RangeLoopSpec,
        body: Vec<AstNode>,
    },
    CollectionLoop {
        bindings: LoopBindings,
        iterable: Expression,
        body: Vec<AstNode>,
    },
    WhileLoop(Expression, Vec<AstNode>), // Condition, Body,
    Break,
    Continue,

    /// Runtime assertion statement intrinsic.
    ///
    /// WHAT: `assert(condition)` and `assert(condition, "message")` are language-owned
    ///       statement surfaces for runtime invariant checking.
    /// WHY: keeping assert out of the ordinary function-call path prevents shadowing,
    ///      named arguments, mutable markers, and result handling that do not apply.
    Assert {
        condition: Expression,
        message: Option<AssertMessage>,
    },

    /// Value-production terminator for active value-producing blocks.
    ///
    /// WHAT: statement-shaped marker carrying one or more expressions that are
    /// returned from the nearest active value-producing block.
    /// WHY: `then` must be a statement so it can see locals declared earlier in
    /// the same body. The owning block parser (catch, future value `if`, match)
    /// consumes this node before HIR lowering.
    ThenValue(crate::compiler_frontend::ast::statements::value_production::ProducedValues),

    // Basics
    VariableDeclaration(Declaration), // Variable name, Value, Visibility,

    /// Accumulate a runtime string expression into the entry start() fragment list.
    ///
    /// WHAT: replaces the old `VariableDeclaration(#template, ...)` protocol used to
    /// mark top-level runtime templates for later extraction/synthesis passes.
    /// WHY: explicit intent avoids encoding the protocol through synthetic variable names,
    /// and removes the need for post-hoc fragment extraction from the start body.
    /// The HIR builder lowers this into a `PushRuntimeFragment` statement inside entry start().
    PushStartRuntimeFragment(Expression),

    // For simple field access: obj.field
    FieldAccess {
        base: Box<AstNode>,        // The expression being accessed
        field: StringId,           // The field name
        diagnostic_type: DataType, // Non-authoritative spelling for diagnostics
        type_id: TypeId,
        const_record_state: ConstRecordState,
        value_mode: ValueMode, // ValueMode of the resolved field
    },

    // For method calls: obj.method(args)
    MethodCall {
        receiver: Box<AstNode>,
        method_path: InternedPath,
        method: StringId,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        location: SourceLocation,
    },

    /// Dynamic trait receiver dispatch selected from the erased trait surface.
    ///
    /// WHAT: carries the trait requirement identity instead of a concrete method path.
    /// WHY: dynamic values expose only the trait surface; HIR/backend lowering dispatches through
    /// the wrapper's method table without repeating receiver lookup.
    DynamicTraitMethodCall {
        receiver: Box<AstNode>,
        trait_id: TraitId,
        requirement_id: TraitRequirementId,
        method: StringId,
        receiver_requires_mutable: bool,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        location: SourceLocation,
    },

    // For compiler-owned collection builtins: collection.get/set/push/remove/length(...)
    CollectionBuiltinCall {
        receiver: Box<AstNode>,
        op: CollectionBuiltinOp,
        receiver_requires_mutable: bool,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        location: SourceLocation,
    },

    FunctionCall {
        name: InternedPath,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        location: SourceLocation,
    },

    HandledFallibleFunctionCall {
        name: InternedPath,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        handling: FallibleHandling,
        location: SourceLocation,
    },

    HandledFallibleHostFunctionCall {
        name: ExternalFunctionId,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        error_type_id: TypeId,
        handling: FallibleHandling,
        location: SourceLocation,
    },

    // Host function call (functions provided by the runtime)
    HostFunctionCall {
        name: ExternalFunctionId,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        location: SourceLocation,
    },

    // example: new_struct_instance = MyStructDefinition(arg1, arg2)
    //          new_struct_instance(arg) -- Calls the main function of the struct
    StructDefinition(
        InternedPath,     // Full unique name path
        Vec<Declaration>, // Fields
    ),

    Function(InternedPath, FunctionSignature, Vec<AstNode>),

    // Mutation of existing mutable variables
    Assignment {
        target: Box<AstNode>, // Variable, FieldAccess, Deref, etc.
        value: Expression,
    },

    MultiBind {
        targets: Vec<MultiBindTarget>,
        value: Expression,
    },

    // An actual r-value
    // Currently used for function calls and struct accesses
    Rvalue(Expression),

    // Operators
    Operator(Operator),
}

impl AstNode {
    /// Returns the expression TypeId for expression-shaped nodes without rebuilding
    /// an owned `Expression`.
    ///
    /// WHAT: postfix/member parsing needs the receiver type at every chain step.
    /// WHY: using this lightweight query avoids cloning call arguments, field
    /// access trees, and diagnostic `DataType` payloads just to inspect type
    /// identity.
    pub fn expression_type_id(&self) -> Result<TypeId, CompilerError> {
        match &self.kind {
            NodeKind::VariableDeclaration(declaration) => Ok(declaration.value.type_id),
            NodeKind::Rvalue(expression) => Ok(expression.type_id),

            NodeKind::FunctionCall {
                result_type_ids, ..
            }
            | NodeKind::HostFunctionCall {
                result_type_ids, ..
            }
            | NodeKind::HandledFallibleHostFunctionCall {
                result_type_ids, ..
            }
            | NodeKind::HandledFallibleFunctionCall {
                result_type_ids, ..
            }
            | NodeKind::MethodCall {
                result_type_ids, ..
            }
            | NodeKind::DynamicTraitMethodCall {
                result_type_ids, ..
            }
            | NodeKind::CollectionBuiltinCall {
                result_type_ids, ..
            } => expression_type_id_for_call_result(result_type_ids),

            NodeKind::FieldAccess { type_id, .. } => Ok(*type_id),

            // Non-expression nodes — compiler invariant violation.
            // This path should never be reached from well-formed AST; it indicates a bug in
            // an earlier stage that allowed a non-expression node into expression context.
            _ => {
                return_compiler_error!(
                    "AST invariant: tried to get the type of a non-expression AST node: {:?}",
                    &self.kind
                );
            }
        }
    }

    /// Returns whether this expression-shaped node represents a const-record value.
    ///
    /// WHAT: receiver-call validation needs to distinguish an actual const-record
    /// value from an exported struct's nominal `TypeId`.
    /// WHY: exported structs share the same canonical type identity for runtime
    /// and const-record values; the const-record call restriction is value-level,
    /// so it must inspect the explicit `ConstRecordState` on the expression rather
    /// than branching on diagnostic-only `DataType` spelling.
    pub fn expression_is_const_record_value(&self) -> Result<bool, CompilerError> {
        match &self.kind {
            NodeKind::VariableDeclaration(declaration) => {
                Ok(declaration.value.is_const_record_value())
            }
            NodeKind::Rvalue(expression) => Ok(expression.is_const_record_value()),

            // Call results are never const-record values; const-record semantics apply
            // only to struct literal expressions, not to function return values.
            NodeKind::FunctionCall { .. }
            | NodeKind::HostFunctionCall { .. }
            | NodeKind::HandledFallibleHostFunctionCall { .. }
            | NodeKind::HandledFallibleFunctionCall { .. }
            | NodeKind::MethodCall { .. }
            | NodeKind::DynamicTraitMethodCall { .. }
            | NodeKind::CollectionBuiltinCall { .. } => Ok(false),

            NodeKind::FieldAccess {
                const_record_state, ..
            } => Ok(matches!(const_record_state, ConstRecordState::ConstRecord)),

            // Non-expression nodes — compiler invariant violation.
            // This path should never be reached from well-formed AST; it indicates a bug in
            // an earlier stage that allowed a non-expression node into expression context.
            _ => {
                return_compiler_error!(
                    "AST invariant: tried to inspect the const-record value state of a non-expression AST node: {:?}",
                    &self.kind
                );
            }
        }
    }

    pub fn get_expr(&self) -> Result<Expression, CompilerError> {
        self.get_expr_with_optional_type_environment(None)
    }

    pub fn get_expr_with_type_environment(
        &self,
        type_environment: &TypeEnvironment,
    ) -> Result<Expression, CompilerError> {
        self.get_expr_with_optional_type_environment(Some(type_environment))
    }

    fn get_expr_with_optional_type_environment(
        &self,
        type_environment: Option<&TypeEnvironment>,
    ) -> Result<Expression, CompilerError> {
        match &self.kind {
            // Declarations and rvalues
            NodeKind::VariableDeclaration(declaration) => Ok(declaration.value.to_owned()),
            NodeKind::Rvalue(expression) => Ok(expression.to_owned()),

            // Call variants
            NodeKind::FunctionCall {
                name,
                args: arguments,
                result_type_ids,
                location,
            } => {
                let type_id = expression_type_id_for_call_result(result_type_ids)?;
                Ok(Expression::new(
                    ExpressionKind::FunctionCall {
                        name: name.to_owned(),
                        args: normalize_call_arguments(arguments),
                        result_type_ids: result_type_ids.to_owned(),
                    },
                    location.to_owned(),
                    type_id,
                    call_result_diagnostic_type(result_type_ids, type_environment),
                    ValueMode::MutableOwned,
                ))
            }

            NodeKind::HostFunctionCall {
                name,
                args: arguments,
                result_type_ids,
                location,
            } => {
                let type_id = expression_type_id_for_call_result(result_type_ids)?;
                Ok(Expression::new(
                    ExpressionKind::HostFunctionCall {
                        id: *name,
                        args: normalize_call_arguments(arguments),
                        result_type_ids: result_type_ids.to_owned(),
                    },
                    location.to_owned(),
                    type_id,
                    call_result_diagnostic_type(result_type_ids, type_environment),
                    ValueMode::MutableOwned,
                ))
            }

            NodeKind::HandledFallibleFunctionCall {
                name,
                args: arguments,
                result_type_ids,
                handling,
                location,
            } => {
                let type_id = expression_type_id_for_call_result(result_type_ids)?;
                Ok(Expression::new(
                    ExpressionKind::HandledFallibleFunctionCall {
                        name: name.to_owned(),
                        args: normalize_call_arguments(arguments),
                        result_type_ids: result_type_ids.to_owned(),
                        handling: handling.to_owned(),
                    },
                    location.to_owned(),
                    type_id,
                    call_result_diagnostic_type(result_type_ids, type_environment),
                    ValueMode::MutableOwned,
                ))
            }

            NodeKind::HandledFallibleHostFunctionCall {
                name,
                args: arguments,
                result_type_ids,
                error_type_id,
                handling,
                location,
            } => {
                let type_id = expression_type_id_for_call_result(result_type_ids)?;
                Ok(Expression::new(
                    ExpressionKind::HandledFallibleHostFunctionCall {
                        id: *name,
                        args: normalize_call_arguments(arguments),
                        result_type_ids: result_type_ids.to_owned(),
                        error_type_id: *error_type_id,
                        handling: handling.to_owned(),
                    },
                    location.to_owned(),
                    type_id,
                    call_result_diagnostic_type(result_type_ids, type_environment),
                    ValueMode::MutableOwned,
                ))
            }

            // Field and method access
            NodeKind::FieldAccess {
                diagnostic_type,
                type_id,
                const_record_state,
                value_mode,
                ..
            } => {
                let mut expression = Expression::runtime_with_type_id(
                    vec![self.to_owned()],
                    diagnostic_type.to_owned(),
                    *type_id,
                    self.location.to_owned(),
                    value_mode.to_owned(),
                );
                expression.const_record_state = *const_record_state;
                Ok(expression)
            }
            NodeKind::MethodCall {
                result_type_ids,
                location,
                ..
            }
            | NodeKind::DynamicTraitMethodCall {
                result_type_ids,
                location,
                ..
            }
            | NodeKind::CollectionBuiltinCall {
                result_type_ids,
                location,
                ..
            } => {
                let type_id = expression_type_id_for_call_result(result_type_ids)?;
                Ok(Expression::runtime_with_type_id(
                    vec![self.to_owned()],
                    call_result_diagnostic_type(result_type_ids, type_environment),
                    type_id,
                    location.to_owned(),
                    ValueMode::MutableOwned,
                ))
            }
            // Non-expression nodes — compiler invariant violation.
            // This path should never be reached from well-formed AST; it indicates a bug in
            // an earlier stage that allowed a non-expression node into expression context.
            _ => {
                return_compiler_error!(
                    "AST invariant: tried to get the expression of a non-expression AST node: {:?}",
                    &self.kind
                );
            }
        }
    }

    pub fn get_precedence(&self) -> u32 {
        match &self.kind {
            NodeKind::Operator(operator) => match operator {
                // Special Operators with the highest precedence
                Operator::Range => 6,
                Operator::Not => 6,

                // Highest precedence: exponentiation
                Operator::Exponent => 5,

                // High precedence: multiplication, division, modulus
                Operator::Multiply => 4,
                Operator::Divide => 4,
                Operator::IntDivide => 4,
                Operator::Modulus => 4,

                // Medium precedence: addition, subtraction
                Operator::Add => 3,
                Operator::Subtract => 3,

                // Comparisons
                Operator::LessThan => 2,
                Operator::LessThanOrEqual => 2,
                Operator::GreaterThan => 2,
                Operator::GreaterThanOrEqual => 2,
                Operator::Equality => 2,
                Operator::NotEqual => 2,

                // Logical AND
                Operator::And => 1,

                // Logical OR
                Operator::Or => 0,
            },
            _ => 0,
        }
    }

    pub fn is_left_associative(&self) -> bool {
        !matches!(self.kind, NodeKind::Operator(Operator::Exponent))
    }
}

impl MultiBindTarget {
    /// Remap interned path, diagnostic type, and location.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.id.remap_string_ids(remap);
        self.diagnostic_type.remap_string_ids(remap);
        self.location.remap_string_ids(remap);
    }
}

impl AssertMessage {
    /// Remap the interned string ID in this assertion message.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.text = remap.get(self.text);
        self.location.remap_string_ids(remap);
    }
}

impl LoopBindings {
    /// Remap declaration names/expressions in loop bindings.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        if let Some(item) = &mut self.item {
            item.remap_string_ids(remap);
        }
        if let Some(index) = &mut self.index {
            index.remap_string_ids(remap);
        }
    }
}

impl RangeLoopSpec {
    /// Remap expressions in range bounds and step.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.start.remap_string_ids(remap);
        self.end.remap_string_ids(remap);
        if let Some(step) = &mut self.step {
            step.remap_string_ids(remap);
        }
    }
}

impl AstNode {
    /// Remap scope, location, and kind for this AST node.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.scope.remap_string_ids(remap);
        self.location.remap_string_ids(remap);
        self.kind.remap_string_ids(remap);
    }
}

impl NodeKind {
    /// Remap all interned string IDs and paths in this node kind recursively.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            NodeKind::Return(expressions) => {
                for expression in expressions {
                    expression.remap_string_ids(remap);
                }
            }

            NodeKind::ReturnError(expression) => {
                expression.remap_string_ids(remap);
            }

            NodeKind::If(condition, if_true, if_false) => {
                condition.remap_string_ids(remap);
                for node in if_true {
                    node.remap_string_ids(remap);
                }
                if let Some(else_body) = if_false {
                    for node in else_body {
                        node.remap_string_ids(remap);
                    }
                }
            }

            NodeKind::Match {
                scrutinee,
                arms,
                default,
                ..
            } => {
                scrutinee.remap_string_ids(remap);
                for arm in arms {
                    arm.remap_string_ids(remap);
                }
                if let Some(default_body) = default {
                    for node in default_body {
                        node.remap_string_ids(remap);
                    }
                }
            }

            NodeKind::ScopedBlock { body } => {
                for node in body {
                    node.remap_string_ids(remap);
                }
            }

            NodeKind::RangeLoop {
                bindings,
                range,
                body,
            } => {
                bindings.remap_string_ids(remap);
                range.remap_string_ids(remap);
                for node in body {
                    node.remap_string_ids(remap);
                }
            }

            NodeKind::CollectionLoop {
                bindings,
                iterable,
                body,
            } => {
                bindings.remap_string_ids(remap);
                iterable.remap_string_ids(remap);
                for node in body {
                    node.remap_string_ids(remap);
                }
            }

            NodeKind::WhileLoop(condition, body) => {
                condition.remap_string_ids(remap);
                for node in body {
                    node.remap_string_ids(remap);
                }
            }

            NodeKind::Assert { condition, message } => {
                condition.remap_string_ids(remap);
                if let Some(message) = message {
                    message.remap_string_ids(remap);
                }
            }

            NodeKind::Break | NodeKind::Continue => {}

            NodeKind::ThenValue(produced_values) => {
                for expression in &mut produced_values.expressions {
                    expression.remap_string_ids(remap);
                }
            }

            NodeKind::VariableDeclaration(declaration) => {
                declaration.remap_string_ids(remap);
            }

            NodeKind::PushStartRuntimeFragment(expression) => {
                expression.remap_string_ids(remap);
            }

            NodeKind::FieldAccess {
                base,
                field,
                diagnostic_type,
                ..
            } => {
                base.remap_string_ids(remap);
                *field = remap.get(*field);
                diagnostic_type.remap_string_ids(remap);
            }

            NodeKind::MethodCall {
                receiver,
                method_path,
                method,
                args,
                location,
                ..
            } => {
                receiver.remap_string_ids(remap);
                method_path.remap_string_ids(remap);
                *method = remap.get(*method);
                for arg in args {
                    arg.remap_string_ids(remap);
                }
                location.remap_string_ids(remap);
            }

            NodeKind::DynamicTraitMethodCall {
                receiver,
                method,
                args,
                location,
                ..
            } => {
                receiver.remap_string_ids(remap);
                *method = remap.get(*method);
                for arg in args {
                    arg.remap_string_ids(remap);
                }
                location.remap_string_ids(remap);
            }

            NodeKind::CollectionBuiltinCall {
                receiver,
                args,
                location,
                ..
            } => {
                receiver.remap_string_ids(remap);
                for arg in args {
                    arg.remap_string_ids(remap);
                }
                location.remap_string_ids(remap);
            }

            NodeKind::FunctionCall {
                name,
                args,
                location,
                ..
            } => {
                name.remap_string_ids(remap);
                for arg in args {
                    arg.remap_string_ids(remap);
                }
                location.remap_string_ids(remap);
            }

            NodeKind::HandledFallibleFunctionCall {
                name,
                args,
                handling,
                location,
                ..
            } => {
                name.remap_string_ids(remap);
                for arg in args {
                    arg.remap_string_ids(remap);
                }
                handling.remap_string_ids(remap);
                location.remap_string_ids(remap);
            }

            NodeKind::HandledFallibleHostFunctionCall {
                args,
                handling,
                location,
                ..
            } => {
                for arg in args {
                    arg.remap_string_ids(remap);
                }
                handling.remap_string_ids(remap);
                location.remap_string_ids(remap);
            }

            NodeKind::HostFunctionCall { args, location, .. } => {
                for arg in args {
                    arg.remap_string_ids(remap);
                }
                location.remap_string_ids(remap);
            }

            NodeKind::StructDefinition(name, fields) => {
                name.remap_string_ids(remap);
                for field in fields {
                    field.remap_string_ids(remap);
                }
            }

            NodeKind::Function(name, signature, body) => {
                name.remap_string_ids(remap);
                signature.remap_string_ids(remap);
                for node in body {
                    node.remap_string_ids(remap);
                }
            }

            NodeKind::Assignment { target, value } => {
                target.remap_string_ids(remap);
                value.remap_string_ids(remap);
            }

            NodeKind::MultiBind { targets, value } => {
                for target in targets {
                    target.remap_string_ids(remap);
                }
                value.remap_string_ids(remap);
            }

            NodeKind::Rvalue(expression) => {
                expression.remap_string_ids(remap);
            }

            NodeKind::Operator(_) => {}
        }
    }
}

fn expression_type_id_for_call_result(result_type_ids: &[TypeId]) -> Result<TypeId, CompilerError> {
    match result_type_ids {
        [] => Ok(builtin_type_ids::NONE),
        [single] => Ok(*single),
        multiple => Err(CompilerError::compiler_error(format!(
            "AST invariant: tried to convert a {}-result call node into a single expression. Multi-result calls must be handled before expression conversion.",
            multiple.len()
        ))),
    }
}

fn call_result_diagnostic_fallback() -> DataType {
    // Exact call-result spelling requires a TypeEnvironment, which is not available when a
    // statement-shaped call node is reconstructed as an expression. This is display-only fallback;
    // semantic decisions and user diagnostics must use the canonical TypeId path.
    DataType::Inferred
}

fn call_result_diagnostic_type(
    result_type_ids: &[TypeId],
    type_environment: Option<&TypeEnvironment>,
) -> DataType {
    let Some(type_environment) = type_environment else {
        return call_result_diagnostic_fallback();
    };

    match result_type_ids {
        [] => DataType::None,
        [single] => diagnostic_type_spelling(*single, type_environment),
        multiple => DataType::Returns(
            multiple
                .iter()
                .map(|type_id| diagnostic_type_spelling(*type_id, type_environment))
                .collect(),
        ),
    }
}
