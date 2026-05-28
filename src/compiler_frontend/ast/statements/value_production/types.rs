//! Core types for the value-production subsystem.
//!
//! WHAT: defines the shapes that represent produced values, active production targets,
//! and the results of branch-flow analysis.
//! WHY: these types cross parser boundaries (dispatcher, catch handler, future value-block
//! receivers) and need one canonical definition.

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::ast_nodes::MatchExhaustiveness;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::match_patterns::MatchArm;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::StringIdRemap;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// Values produced by a `then` statement inside a value-producing block.
///
/// WHAT: one or more expressions that are returned from the nearest active value-producing
/// block to its receiving site.
/// WHY: a statement-shaped marker is needed so `then` can see locals declared earlier in
/// the same body, and so HIR lowering can distinguish value production from ordinary
/// expression statements.
#[derive(Clone, Debug)]
pub struct ProducedValues {
    pub expressions: Vec<Expression>,
    pub location: SourceLocation,
}

/// Target that `then` statements inside a value-producing block should produce values for.
///
/// WHAT: carries the expected result types and source location of the receiving site that
/// activated the value production.
/// WHY: the parser needs this to validate arity and apply contextual coercion at the point
/// where `then` values are parsed, before HIR lowering allocates result locals.
#[derive(Clone, Debug)]
pub struct ActiveValueProductionTarget {
    pub result_type_ids: Vec<TypeId>,
    /// The receiver kind keeps diagnostics receiver-aware without scattering boolean flags.
    pub receiver_kind: ValueReceiverKind,
    /// When `result_type_ids` is empty but the receiver still expects a specific
    /// number of produced values (e.g. multi-bind with some inferred slots), this
    /// tells `parse_produced_values_typed` how many expressions to read after `then`.
    pub expected_arity: Option<usize>,
}

/// Classification of the site that receives produced values.
///
/// WHAT: identifies why a value-production target was activated.
/// WHY: future diagnostics and lowering may need to distinguish declarations from returns
/// from nested `then` sites; keeping the kind explicit avoids boolean flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ValueReceiverKind {
    Declaration,
    Assignment,
    Return,
    MultiBind,
    NestedThen,
    CatchHandler,
}

/// A value-producing control-flow block used as an expression at closed receiving sites.
///
/// WHAT: represents `if` and future `match` / `catch` shapes that produce values instead
/// of executing statements for side effects.
/// WHY: receiving sites need to distinguish value blocks from ordinary expressions so
/// they can validate arity, type, and completeness before HIR lowering.
#[derive(Clone, Debug)]
pub enum ValueBlock {
    If(ValueIfBlock),
    Match(ValueMatchBlock),
    Catch(ValueCatchBlock),
}

impl ValueBlock {
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            ValueBlock::If(value_if) => value_if.remap_string_ids(remap),
            ValueBlock::Match(value_match) => value_match.remap_string_ids(remap),
            ValueBlock::Catch(value_catch) => value_catch.remap_string_ids(remap),
        }
    }
}

/// Single `if` value-producing block.
///
/// WHAT: `if condition then a else b` or the colon/block equivalent.
/// WHY: carries both branches as statement bodies so `then` can see locals declared
/// earlier in the same branch.
#[derive(Clone, Debug)]
pub struct ValueIfBlock {
    pub condition: Expression,
    pub then_body: Vec<AstNode>,
    pub else_body: Vec<AstNode>,
    pub location: SourceLocation,
    /// Expected result types for each produced value slot.
    ///
    /// WHAT: one type per value produced by `then` in each branch.
    /// WHY: HIR lowering needs the individual slot types to allocate result locals,
    ///      and the AST expression type is derived from these (single type or tuple).
    pub result_type_ids: Vec<TypeId>,
}

impl ValueIfBlock {
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.condition.remap_string_ids(remap);
        for node in &mut self.then_body {
            node.remap_string_ids(remap);
        }
        for node in &mut self.else_body {
            node.remap_string_ids(remap);
        }
        self.location.remap_string_ids(remap);
    }
}

/// Full value-producing match block.
///
/// WHAT: `if value is:` used at a closed receiving site, with each reachable arm
/// producing values via `then` or terminating.
/// WHY: this reuses statement match parsing and HIR match CFG lowering while keeping
/// value-block result slots explicit for hidden result-local allocation.
#[derive(Clone, Debug)]
pub struct ValueMatchBlock {
    pub scrutinee: Expression,
    pub arms: Vec<MatchArm>,
    pub default: Option<Vec<AstNode>>,
    pub exhaustiveness: MatchExhaustiveness,
    pub location: SourceLocation,
    pub result_type_ids: Vec<TypeId>,
}

impl ValueMatchBlock {
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.scrutinee.remap_string_ids(remap);
        for arm in &mut self.arms {
            arm.remap_string_ids(remap);
        }
        if let Some(default_body) = &mut self.default {
            for node in default_body {
                node.remap_string_ids(remap);
            }
        }
        self.location.remap_string_ids(remap);
    }
}

/// Value-producing catch block.
///
/// WHAT: wraps a handled fallible expression whose catch handler body uses
/// `ThenValue` statements to produce the recovered success values.
/// WHY: catch recovery now shares the same value-block lowering target as `if`
/// and match blocks, instead of carrying catch-specific terminal fallback values.
#[derive(Clone, Debug)]
pub struct ValueCatchBlock {
    pub handled_value: Box<Expression>,
    pub location: SourceLocation,
    pub result_type_ids: Vec<TypeId>,
}

impl ValueCatchBlock {
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.handled_value.remap_string_ids(remap);
        self.location.remap_string_ids(remap);
    }
}

/// Result of analyzing a body's control flow for value production.
///
/// WHAT: tells a caller whether a sequence of AST nodes falls through, produces values,
/// or terminates on all reachable paths.
/// WHY: value-producing blocks require every path to either produce or terminate;
/// `FallsThrough` indicates a completeness error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchFlow {
    /// Body can reach the end without producing values or terminating.
    FallsThrough,
    /// Body contains at least one `then` on a reachable path.
    ProducesValue,
    /// Body guarantees termination (return, return!, panic) on all reachable paths.
    Terminates,
}
