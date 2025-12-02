//! Expression lowering from AST to HIR
//!
//! This module handles the conversion of AST expressions to HIR expressions,
//! including RPN to expression tree conversion and operator mapping.

use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::{BinOp, HirExpr, HirExprKind};
use crate::compiler::hir::place::Place;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::string_interning::StringTable;
use crate::return_compiler_error;

/// Lower an expression to HIR
pub(crate) fn lower_expr(
    expr: Expression,
    string_table: &mut StringTable,
) -> Result<HirExpr, CompilerError> {
    let location = expr.location.clone();
    let data_type = expr.data_type.clone();

    let kind = match expr.kind {
        // === Literals (already folded at AST stage) ===
        ExpressionKind::Int(n) => HirExprKind::Int(n),
        ExpressionKind::Float(f) => HirExprKind::Float(f),
        ExpressionKind::Bool(b) => HirExprKind::Bool(b),
        ExpressionKind::StringSlice(s) => HirExprKind::StringLiteral(s),

        // === Variable References ===
        ExpressionKind::Reference(name) => {
            // Default: immutable load
            let place = Place::Local(name);
            HirExprKind::Load(place)
        }

        // === Runtime Expressions (RPN from AST) ===
        ExpressionKind::Runtime(rpn_nodes) => {
            // Convert RPN sequence to expression tree
            lower_rpn_to_expr(rpn_nodes, string_table)?
        }

        // === Function Calls ===
        ExpressionKind::FunctionCall(name, args) => {
            let args = args
                .into_iter()
                .map(|e| lower_expr(e, string_table))
                .collect::<Result<Vec<_>, _>>()?;

            HirExprKind::Call { target: name, args }
        }

        // === Templates ===
        ExpressionKind::Template(template) => {
            // If the template can be folded, it's already a string
            // If not, create runtime template call
            lower_template(*template)?
        }

        // === Collections ===
        ExpressionKind::Collection(items) => {
            // Recursively lower each element expression
            let lowered_items = items
                .into_iter()
                .map(|e| lower_expr(e, string_table))
                .collect::<Result<Vec<_>, _>>()?;

            HirExprKind::Collection(lowered_items)
        }

        // === Struct Construction ===
        ExpressionKind::StructInstance(fields) => {
            // Lower each field value expression
            let lowered_fields = fields
                .into_iter()
                .map(|arg| {
                    let value = lower_expr(arg.value, string_table)?;
                    Ok((arg.id, value))
                })
                .collect::<Result<Vec<_>, _>>()?;

            HirExprKind::StructConstruct {
                type_name: string_table.intern(""), // TODO: Get from context
                fields: lowered_fields,
            }
        }

        // === Range ===
        ExpressionKind::Range(start, end) => {
            // Lower both start and end expressions
            let start = Box::new(lower_expr(*start, string_table)?);
            let end = Box::new(lower_expr(*end, string_table)?);

            HirExprKind::Range { start, end }
        }

        _ => {
            return_compiler_error!(
                "Unsupported expression kind in HIR lowering: {:?}",
                expr.kind; {
                    CompilationStage => "HIR Generation"
                }
            )
        }
    };

    Ok(HirExpr {
        kind,
        data_type,
        location,
    })
}

/// Lower expression as candidate move (for mutable assignments)
pub(crate) fn lower_expr_as_candidate_move(
    expr: Expression,
    string_table: &mut StringTable,
) -> Result<HirExpr, CompilerError> {
    let location = expr.location.clone();
    let data_type = expr.data_type.clone();

    // If the expression is a simple reference, mark as candidate move
    if let ExpressionKind::Reference(name) = expr.kind {
        let place = Place::Local(name);
        return Ok(HirExpr {
            kind: HirExprKind::CandidateMove(place),
            data_type,
            location,
        });
    }

    // Otherwise, it's a mutable borrow of the expression result
    lower_expr(expr, string_table)
}

/// Convert RPN sequence to expression tree
///
/// The AST stage has already performed type checking and constant folding,
/// so we use the type information from the AST expressions directly.
/// The result type is determined by the operator and operand types.
pub(crate) fn lower_rpn_to_expr(
    rpn: Vec<AstNode>,
    string_table: &mut StringTable,
) -> Result<HirExprKind, CompilerError> {
    let mut stack: Vec<HirExpr> = Vec::new();

    for node in rpn {
        match node.kind {
            // Push operands onto stack
            NodeKind::Expression(expr) => {
                stack.push(lower_expr(expr, string_table)?);
            }

            // Pop operands, apply operator, push result
            NodeKind::Operator(op) => {
                let right: HirExpr = match stack.pop() {
                    Some(right) => right,
                    None => {
                        return_compiler_error!("RPN stack underflow (right operand)"; {
                            CompilationStage => "HIR Generation"
                        })
                    }
                };

                let left: HirExpr = match stack.pop() {
                    Some(left) => left,
                    None => {
                        return_compiler_error!("RPN stack underflow (left operand)"; {
                            CompilationStage => "HIR Generation"
                        })
                    }
                };

                let bin_op = convert_operator(op)?;

                // Determine result type based on operator
                // Comparison and logical operators always return Bool
                // Arithmetic operators use the left operand's type
                // (AST stage has already done type checking and promotion)
                let result_type = match bin_op {
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                    | BinOp::And | BinOp::Or => DataType::Bool,
                    _ => left.data_type.clone(),
                };

                stack.push(HirExpr {
                    kind: HirExprKind::BinOp {
                        left: Box::new(left),
                        op: bin_op,
                        right: Box::new(right),
                    },
                    data_type: result_type,
                    location: node.location,
                });
            }

            _ => {
                return_compiler_error!(
                    "Unexpected node in RPN sequence: {:?}",
                    node.kind; {
                        CompilationStage => "HIR Generation"
                    }
                )
            }
        }
    }

    // Should have exactly one expression left
    if stack.len() != 1 {
        return_compiler_error!(
            "Invalid RPN sequence: stack size = {}",
            stack.len(); {
                CompilationStage => "HIR Generation"
            }
        )
    }

    Ok(stack.pop().unwrap().kind)
}

/// Helper: lower template (handle runtime templates)
fn lower_template(_template: Template) -> Result<HirExprKind, CompilerError> {
    // If the template has runtime interpolations, create a runtime template call
    // Otherwise, it should already be folded to a string literal at the AST stage

    // TODO: Implement based on your Template structure
    unimplemented!("Template lowering")
}

/// Convert AST operator to HIR BinOp
pub(crate) fn convert_operator(op: Operator) -> Result<BinOp, CompilerError> {
    let bin_op = match op {
        Operator::Add => BinOp::Add,
        Operator::Subtract => BinOp::Sub,
        Operator::Multiply => BinOp::Mul,
        Operator::Divide => BinOp::Div,
        Operator::Modulus => BinOp::Mod,
        Operator::Root => BinOp::Root,
        Operator::Exponent => BinOp::Exponent,
        Operator::And => BinOp::And,
        Operator::Or => BinOp::Or,
        Operator::GreaterThan => BinOp::Gt,
        Operator::GreaterThanOrEqual => BinOp::Ge,
        Operator::LessThan => BinOp::Lt,
        Operator::LessThanOrEqual => BinOp::Le,
        Operator::Equality => BinOp::Eq,
        Operator::Not => {
            // Not is a unary operator, should not appear in binary context
            return_compiler_error!(
                "Unary operator 'Not' found in binary operation context"; {
                    CompilationStage => "HIR Generation"
                }
            )
        }
        Operator::Range => {
            // Range is handled separately in expression lowering
            return_compiler_error!(
                "Range operator should be handled as Range expression, not binary operation"; {
                    CompilationStage => "HIR Generation"
                }
            )
        }
    };
    Ok(bin_op)
}

// === Place Construction Functions ===

/// Create a Place::Local for a variable reference
///
/// This is the simplest form of place construction, representing
/// a direct reference to a local variable by its interned name.
pub(crate) fn create_local_place(name: crate::compiler::string_interning::InternedString) -> Place {
    Place::Local(name)
}

/// Create a Place::Field for a field access
///
/// Constructs a nested place representing field access on a base place.
/// For example, `obj.field` becomes Place::Field { base: Place::Local("obj"), field: "field" }
pub(crate) fn create_field_place(
    base: Place,
    field: crate::compiler::string_interning::InternedString,
) -> Place {
    Place::Field {
        base: Box::new(base),
        field,
    }
}

/// Create a Place::Index for an index operation
///
/// Constructs a nested place representing index access on a base place.
/// For example, `arr[i]` becomes Place::Index { base: Place::Local("arr"), index: Load(Place::Local("i")) }
pub(crate) fn create_index_place(base: Place, index: HirExpr) -> Place {
    Place::Index {
        base: Box::new(base),
        index: Box::new(index),
    }
}

/// Create a Place::Global for a global variable reference
///
/// Represents a reference to a module-level constant or static variable.
pub(crate) fn create_global_place(
    name: crate::compiler::string_interning::InternedString,
) -> Place {
    Place::Global(name)
}

/// Build a nested place from a chain of accesses
///
/// This helper function constructs complex nested places like `obj.field[index].other_field`
/// by recursively building the place structure from the innermost to outermost access.
///
/// # Example
/// For `obj.field[0]`:
/// 1. Start with Place::Local("obj")
/// 2. Apply field access: Place::Field { base: Place::Local("obj"), field: "field" }
/// 3. Apply index: Place::Index { base: Place::Field {...}, index: Int(0) }
pub(crate) fn build_nested_place(
    base: Place,
    accesses: Vec<PlaceAccess>,
    string_table: &mut StringTable,
) -> Result<Place, CompilerError> {
    let mut current_place = base;

    for access in accesses {
        current_place = match access {
            PlaceAccess::Field(field_name) => create_field_place(current_place, field_name),
            PlaceAccess::Index(index_expr) => {
                let index_hir = lower_expr(index_expr, string_table)?;
                create_index_place(current_place, index_hir)
            }
        };
    }

    Ok(current_place)
}

/// Represents a single access operation in a place chain
///
/// Used by `build_nested_place` to construct complex nested places.
pub(crate) enum PlaceAccess {
    Field(crate::compiler::string_interning::InternedString),
    Index(Expression),
}
