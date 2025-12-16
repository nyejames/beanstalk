//! HIR builder
//!
//! Converts AST into a structured HIR representation suitable for borrow checking.
//!
//! Key responsibilities:
//! - Linearize expressions into statements operating on places
//! - Eliminate nested expressions by introducing temporary locals
//! - Convert borrow intent (not ownership outcome)
//! - Preserve structured control flow for CFG analysis
//! - Maintain a place-based memory model

use crate::compiler::compiler_messages::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::{
    BinOp, HirExpr, HirExprKind, HirKind, HirMatchArm, HirNode, HirNodeId,
};
use crate::compiler::hir::place::Place;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::compiler::parsers::statements::branching::MatchArm;
use crate::compiler::string_interning::{InternedString, StringTable};
use crate::return_compiler_error;
use std::collections::HashMap;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;

/// Build a HIR module from the AST
///
/// The HIR builder performs linearization of expressions and creates
/// a place-based representation suitable for borrow checking analysis.
pub struct HirBuilder<'a> {
    /// Current module scope for name resolution
    current_scope: InternedPath,

    /// Sequential ID generator for HIR nodes (used by the borrow checker for CFG)
    next_node_id: usize,

    /// Track local variable bindings and their types
    local_bindings: HashMap<InternedString, DataType>,

    /// Counter for generating unique temporary variable names
    temp_counter: usize,

    /// Counter for generating unique runtime template function names
    #[allow(dead_code)]
    template_counter: usize,

    /// Accumulated errors and warnings during lowering
    messages: CompilerMessages,

    /// String interning table
    string_table: &'a mut StringTable,
}

impl<'a> HirBuilder<'a> {
    pub fn new(scope: InternedPath, string_table: &'a mut StringTable) -> Self {
        Self {
            current_scope: scope,
            next_node_id: 0,
            local_bindings: HashMap::new(),
            temp_counter: 0,
            template_counter: 0,
            messages: CompilerMessages::new(),
            string_table,
        }
    }

    fn next_id(&mut self) -> HirNodeId {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }

    /// Generate a unique temporary variable name
    fn next_temp(&mut self) -> InternedString {
        let name = format!("_temp_{}", self.temp_counter);
        self.temp_counter += 1;
        self.string_table.intern(&name)
    }

    /// Main entry point: lower the entire AST to HIR
    pub fn lower_ast(
        ast: Vec<AstNode>,
        scope: InternedPath,
        string_table: &'a mut StringTable,
    ) -> Result<Vec<HirNode>, CompilerMessages> {
        let mut builder = Self::new(scope, string_table);

        let mut hir_nodes = Vec::new();
        for node in ast {
            match builder.lower_node(node) {
                Ok(mut node_hir) => hir_nodes.append(&mut node_hir),
                Err(e) => builder.messages.errors.push(e),
            }
        }

        if !builder.messages.errors.is_empty() {
            return Err(CompilerMessages {
                errors: builder.messages.errors,
                warnings: builder.messages.warnings,
            });
        }

        Ok(hir_nodes)
    }

    /// Helper: create a literal expression assignment to a temporary place
    fn create_literal_assignment(
        &mut self,
        expr_kind: HirExprKind,
        data_type: DataType,
        location: TextLocation,
    ) -> (Vec<HirNode>, Place) {
        let temp = self.next_temp();
        let temp_place = Place::local(temp);
        let literal_expr = HirExpr {
            kind: expr_kind,
            data_type,
            location: location.clone(),
        };
        let assign_node = self.create_assign_node_with_expr(
            temp_place.clone(),
            literal_expr,
            location,
            self.current_scope.clone(),
        );
        (vec![assign_node], temp_place)
    }

    /// Lower a single AST node to HIR
    ///
    /// This method linearizes expressions by introducing temporary variables
    /// and converts all operations to work on places rather than nested expressions.
    pub(crate) fn lower_node(&mut self, node: AstNode) -> Result<Vec<HirNode>, CompilerError> {
        match node.kind {
            // === Variable Declaration ===
            NodeKind::VariableDeclaration(arg) => {
                self.local_bindings
                    .insert(arg.id, arg.value.data_type.clone());

                let place = Place::local(arg.id);
                let (mut nodes, value_place) = self.lower_expr_to_place(arg.value)?;

                nodes.push(self.create_assign_node(place, value_place, node.location, node.scope));
                Ok(nodes)
            }

            // Mutating an existing variable or field on that variable.
            // This reference is already enforced to be mutable by the parser
            NodeKind::Assignment {
                target,
                value: value_ast,
            } => {
                // Convert the target AST node to a proper Place
                let target_place = self.lower_ast_node_to_place(*target)?;

                let (value_nodes, value_place) = self.lower_expr_to_place(value_ast)?;
                let mut nodes = value_nodes;

                // For type inference, we'll use the root variable's type
                let target_type = match &target_place.root {
                    crate::compiler::hir::place::PlaceRoot::Local(name) => self
                        .local_bindings
                        .get(name)
                        .cloned()
                        .unwrap_or(DataType::Inferred),
                    _ => DataType::Inferred,
                };

                // Create a candidate move
                let value_expr = HirExpr {
                    kind: HirExprKind::CandidateMove(value_place),
                    data_type: target_type,
                    location: node.location.clone(),
                };

                let assignment_node = self.create_assign_node_with_expr(
                    target_place,
                    value_expr,
                    node.location,
                    node.scope,
                );
                nodes.push(assignment_node);

                Ok(nodes)
            }

            // Control Flow
            NodeKind::If(cond, then_block, else_block) => {
                let (cond_nodes, cond_place) = self.lower_expr_to_place(cond)?;
                let then_block = self.lower_block(then_block)?;
                let else_block = else_block.map(|b| self.lower_block(b)).transpose()?;

                let mut nodes = cond_nodes;
                nodes.push(HirNode {
                    kind: HirKind::If {
                        condition: cond_place,
                        then_block,
                        else_block,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            NodeKind::Match(subject, arms, default) => {
                let (subject_nodes, subject_place) = self.lower_expr_to_place(subject)?;
                let arms = arms
                    .into_iter()
                    .map(|arm| self.lower_match_arm(arm))
                    .collect::<Result<Vec<_>, _>>()?;
                let default = default.map(|b| self.lower_block(b)).transpose()?;

                let mut nodes = subject_nodes;
                nodes.push(HirNode {
                    kind: HirKind::Match {
                        scrutinee: subject_place,
                        arms,
                        default,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            NodeKind::ForLoop(item_arg, collection, body) => {
                let (collection_nodes, collection_place) = self.lower_expr_to_place(collection)?;
                let binding = Some((item_arg.id, item_arg.value.data_type));
                let body = self.lower_block(body)?;

                let mut nodes = collection_nodes;
                nodes.push(HirNode {
                    kind: HirKind::Loop {
                        binding,
                        iterator: collection_place,
                        body,
                        index_binding: None, // TODO: handle index binding
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            // === Function Calls ===
            NodeKind::FunctionCall(name, args, returns, _location) => {
                let mut nodes = Vec::new();
                let mut arg_places = Vec::new();

                // Lower all arguments to places
                for arg in args {
                    let (arg_nodes, arg_place) = self.lower_expr_to_place(arg)?;
                    nodes.extend(arg_nodes);
                    arg_places.push(arg_place);
                }

                // Create return places
                let return_places: Vec<Place> = returns
                    .iter()
                    .enumerate()
                    .map(|(i, _)| {
                        let temp_name = format!("_ret_{}", i);
                        Place::local(self.string_table.intern(&temp_name))
                    })
                    .collect();

                nodes.push(HirNode {
                    kind: HirKind::Call {
                        target: name,
                        args: arg_places,
                        returns: return_places,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            NodeKind::HostFunctionCall(name, args, returns, module, import, _location) => {
                let mut nodes = Vec::new();
                let mut arg_places = Vec::new();

                // Lower all arguments to places
                for arg in args {
                    let (arg_nodes, arg_place) = self.lower_expr_to_place(arg)?;
                    nodes.extend(arg_nodes);
                    arg_places.push(arg_place);
                }

                // Create return places
                let return_places: Vec<Place> = returns
                    .iter()
                    .enumerate()
                    .map(|(i, _)| {
                        let temp_name = format!("_ret_{}", i);
                        Place::local(self.string_table.intern(&temp_name))
                    })
                    .collect();

                nodes.push(HirNode {
                    kind: HirKind::HostCall {
                        target: name,
                        module,
                        import,
                        args: arg_places,
                        returns: return_places,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            // === Returns ===
            NodeKind::Return(exprs) => {
                let mut nodes = Vec::new();
                let mut return_places = Vec::new();

                // Lower all return expressions to places
                for expr in exprs {
                    let (expr_nodes, expr_place) = self.lower_expr_to_place(expr)?;
                    nodes.extend(expr_nodes);
                    return_places.push(expr_place);
                }

                nodes.push(HirNode {
                    kind: HirKind::Return(return_places),
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            // === Function Definitions ===
            NodeKind::Function(name, signature, body) => {
                let body = self.lower_block(body)?;

                Ok(vec![HirNode {
                    kind: HirKind::FunctionDef {
                        name,
                        signature,
                        body,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                }])
            }

            // === Struct Definitions ===
            NodeKind::StructDefinition(name, fields) => Ok(vec![HirNode {
                kind: HirKind::StructDef { name, fields },
                location: node.location,
                scope: node.scope,
                id: self.next_id(),
            }]),

            // === Field Access ===
            NodeKind::FieldAccess { base, field, .. } => {
                // Field access as a statement (not assignment target)
                let base_place = self.lower_ast_node_to_place(*base)?;
                let field_place = base_place.field(field);

                // Create an expression statement that loads from the field
                Ok(vec![HirNode {
                    kind: HirKind::ExprStmt(field_place),
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                }])
            }

            // === Expression as Statement ===
            NodeKind::Rvalue(expr) => {
                let (expr_nodes, expr_place) = self.lower_expr_to_place(expr)?;
                let mut nodes = expr_nodes;
                nodes.push(HirNode {
                    kind: HirKind::ExprStmt(expr_place),
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            // === While Loops ===
            NodeKind::WhileLoop(condition, body) => {
                let (cond_nodes, cond_place) = self.lower_expr_to_place(condition)?;
                let body = self.lower_block(body)?;

                let mut nodes = cond_nodes;
                nodes.push(HirNode {
                    kind: HirKind::Loop {
                        binding: None,        // While loops don't have iterator bindings
                        iterator: cond_place, // Use condition as an iterator (will be checked each iteration)
                        body,
                        index_binding: None,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            // === Method Calls ===
            NodeKind::MethodCall {
                base,
                method,
                args,
                signature,
            } => {
                let mut nodes = Vec::new();
                let mut arg_places = Vec::new();

                // Lower the base object to a place
                let (base_nodes, base_place) = self.lower_expr_to_place(base.get_expr()?)?;
                nodes.extend(base_nodes);
                arg_places.push(base_place); // Base object is first argument

                // Lower all method arguments to places
                for arg in args {
                    let (arg_nodes, arg_place) = self.lower_expr_to_place(arg.get_expr()?)?;
                    nodes.extend(arg_nodes);
                    arg_places.push(arg_place);
                }

                // Create return places
                let return_places: Vec<Place> = signature
                    .returns
                    .iter()
                    .enumerate()
                    .map(|(i, _)| {
                        let temp_name = format!("_ret_{}", i);
                        Place::local(self.string_table.intern(&temp_name))
                    })
                    .collect();

                nodes.push(HirNode {
                    kind: HirKind::Call {
                        target: method,
                        args: arg_places,
                        returns: return_places,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            // === Print (deprecated but still supported) ===
            NodeKind::Print(expr) => {
                // Convert print to io() host function call
                let (expr_nodes, expr_place) = self.lower_expr_to_place(expr)?;
                let mut nodes = expr_nodes;

                let io_name = self.string_table.intern("io");
                let module_name = self.string_table.intern("beanstalk_io");
                let import_name = self.string_table.intern("print");

                nodes.push(HirNode {
                    kind: HirKind::HostCall {
                        target: io_name,
                        module: module_name,
                        import: import_name,
                        args: vec![expr_place],
                        returns: vec![], // Print doesn't return anything
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            // === Imports and Includes ===
            NodeKind::Import(_path) => {
                // Imports are handled at the module level, not in HIR
                // They don't generate HIR nodes themselves
                Ok(vec![])
            }

            NodeKind::Include(_name, _path) => {
                // Includes are handled at the module level, not in HIR
                // They don't generate HIR nodes themselves
                Ok(vec![])
            }

            // === Configuration ===
            NodeKind::Config(_settings) => {
                // Config nodes are handled at the module level
                // They don't generate HIR nodes themselves
                Ok(vec![])
            }

            // === Warnings ===
            NodeKind::Warning(_message) => {
                // Warning nodes don't generate HIR - they're handled by the compiler messages system
                Ok(vec![])
            }

            // === Template and Layout Nodes ===
            NodeKind::ParentTemplate(_expr) => {
                // Templates are deferred to later compilation stages
                // For now, treat as no-op
                Ok(vec![])
            }

            NodeKind::Slot => {
                // Template slots are deferred to later compilation stages
                Ok(vec![])
            }

            // === Code Blocks (JS/CSS) ===
            NodeKind::JS(_code) => {
                // JavaScript code blocks are handled by the build system
                // They don't generate HIR nodes
                Ok(vec![])
            }

            NodeKind::Css(_code) => {
                // CSS code blocks are handled by the build system
                // They don't generate HIR nodes
                Ok(vec![])
            }

            // === Formatting Nodes ===
            NodeKind::Empty => {
                // Empty nodes don't generate HIR
                Ok(vec![])
            }

            NodeKind::Newline => {
                // Newline nodes are formatting only
                Ok(vec![])
            }

            NodeKind::Spaces(_count) => {
                // Space nodes are formatting only
                Ok(vec![])
            }

            // === Operators (should only appear in RPN context) ===
            NodeKind::Operator(_op) => {
                return_compiler_error!(
                    "Operator node found outside of RPN expression context"; {
                        CompilationStage => "HIR Generation",
                        PrimarySuggestion => "Operators should only appear within Runtime expressions"
                    }
                )
            }
        }
    }

    /// Helper: lower a block of nodes
    fn lower_block(&mut self, nodes: Vec<AstNode>) -> Result<Vec<HirNode>, CompilerError> {
        let mut hir_nodes = Vec::new();
        for node in nodes {
            let mut node_hir = self.lower_node(node)?;
            hir_nodes.append(&mut node_hir);
        }
        Ok(hir_nodes)
    }

    /// Lower an expression to a place, introducing temporaries as needed
    ///
    /// Returns a list of HIR nodes that compute the expression and the final place
    /// where the result is stored. This linearizes nested expressions.
    fn lower_expr_to_place(
        &mut self,
        expr: Expression,
    ) -> Result<(Vec<HirNode>, Place), CompilerError> {
        match expr.kind {
            // === Literals ===
            ExpressionKind::Int(n) => {
                let (nodes, place) = self.create_literal_assignment(
                    HirExprKind::Int(n),
                    expr.data_type,
                    expr.location,
                );
                Ok((nodes, place))
            }

            ExpressionKind::Float(f) => {
                let (nodes, place) = self.create_literal_assignment(
                    HirExprKind::Float(f),
                    expr.data_type,
                    expr.location,
                );
                Ok((nodes, place))
            }

            ExpressionKind::Bool(b) => {
                let (nodes, place) = self.create_literal_assignment(
                    HirExprKind::Bool(b),
                    expr.data_type,
                    expr.location,
                );
                Ok((nodes, place))
            }

            ExpressionKind::StringSlice(s) => {
                let (nodes, place) = self.create_literal_assignment(
                    HirExprKind::StringLiteral(s),
                    expr.data_type,
                    expr.location,
                );
                Ok((nodes, place))
            }

            ExpressionKind::Char(c) => {
                let (nodes, place) = self.create_literal_assignment(
                    HirExprKind::Char(c),
                    expr.data_type,
                    expr.location,
                );
                Ok((nodes, place))
            }

            // === Variable References ===
            ExpressionKind::Reference(name) => {
                // Direct reference to existing place
                Ok((vec![], Place::local(name)))
            }

            // === Runtime Expressions (RPN) ===
            ExpressionKind::Runtime(rpn_nodes) => {
                self.lower_rpn_to_place(rpn_nodes, expr.data_type, expr.location)
            }

            // === Collections ===
            ExpressionKind::Collection(elements) => {
                let mut nodes = Vec::new();
                let mut element_places = Vec::new();

                // Lower all elements to places
                for element in elements {
                    let (element_nodes, element_place) = self.lower_expr_to_place(element)?;
                    nodes.extend(element_nodes);
                    element_places.push(element_place);
                }

                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let collection_expr = HirExpr {
                    kind: HirExprKind::Collection(element_places),
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    collection_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                nodes.push(assign_node);
                Ok((nodes, temp_place))
            }

            // === Struct Construction ===
            ExpressionKind::StructInstance(fields) => {
                let mut nodes = Vec::new();
                let mut field_places = Vec::new();

                // Lower all field values to places
                for field in fields {
                    let (field_nodes, field_place) = self.lower_expr_to_place(field.value)?;
                    nodes.extend(field_nodes);
                    field_places.push((field.id, field_place));
                }

                let temp = self.next_temp();
                let temp_place = Place::local(temp);

                // Extract type name from the data type
                // For now, we'll use a generic name since DataType::Struct doesn't contain the type name
                let type_name = self.string_table.intern("StructInstance");

                let struct_expr = HirExpr {
                    kind: HirExprKind::StructConstruct {
                        type_name,
                        fields: field_places,
                    },
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    struct_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                nodes.push(assign_node);
                Ok((nodes, temp_place))
            }

            // === Range Expressions ===
            ExpressionKind::Range(start_expr, end_expr) => {
                let mut nodes = Vec::new();

                // Lower start and end expressions to places
                let (start_nodes, start_place) = self.lower_expr_to_place(*start_expr)?;
                nodes.extend(start_nodes);

                let (end_nodes, end_place) = self.lower_expr_to_place(*end_expr)?;
                nodes.extend(end_nodes);

                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let range_expr = HirExpr {
                    kind: HirExprKind::Range {
                        start: start_place,
                        end: end_place,
                    },
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    range_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                nodes.push(assign_node);
                Ok((nodes, temp_place))
            }

            // === Function Call Expressions ===
            ExpressionKind::FunctionCall(name, args) => {
                let mut nodes = Vec::new();
                let mut arg_places = Vec::new();

                // Lower all arguments to places
                for arg in args {
                    let (arg_nodes, arg_place) = self.lower_expr_to_place(arg)?;
                    nodes.extend(arg_nodes);
                    arg_places.push(arg_place);
                }

                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let call_expr = HirExpr {
                    kind: HirExprKind::Call {
                        target: name,
                        args: arg_places,
                    },
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    call_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                nodes.push(assign_node);
                Ok((nodes, temp_place))
            }

            // === None Expression ===
            ExpressionKind::None => {
                // None expressions don't produce a value, so we create a placeholder
                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let none_expr = HirExpr {
                    kind: HirExprKind::Load(temp_place.clone()), // Load from self as placeholder
                    data_type: DataType::None,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    none_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                Ok((vec![assign_node], temp_place))
            }

            // === Function Expressions ===
            ExpressionKind::Function(signature, body) => {
                // Function expressions become function definitions
                // Generate a unique name for the anonymous function
                let anonymous_function_name = format!("_anon_func_{}", self.temp_counter);
                self.temp_counter += 1;
                let anonymous_function_name_interned =
                    self.string_table.intern(&anonymous_function_name);

                // Lower the function body
                let function_body_hir = self.lower_block(body)?;

                // Create a function definition node
                let function_definition_node = HirNode {
                    kind: HirKind::FunctionDef {
                        name: anonymous_function_name_interned,
                        signature,
                        body: function_body_hir,
                    },
                    location: expr.location.clone(),
                    scope: self.current_scope.clone(),
                    id: self.next_id(),
                };

                // Create a place that references this function
                let temp_place_name = self.next_temp();
                let temp_place = Place::local(temp_place_name);
                let function_reference_expr = HirExpr {
                    kind: HirExprKind::Load(Place::local(anonymous_function_name_interned)),
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assignment_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    function_reference_expr,
                    expr.location,
                    self.current_scope.clone(),
                );

                Ok((vec![function_definition_node, assignment_node], temp_place))
            }

            // === Template Expressions ===
            ExpressionKind::Template(_template) => {
                // Templates become runtime template calls or string literals
                // For now, we'll treat them as string literals since template processing
                // is typically done at compile time
                let temp = self.next_temp();
                let temp_place = Place::local(temp);

                // Create a placeholder string literal for the template
                let template_string = self.string_table.intern("template_placeholder");
                let template_expr = HirExpr {
                    kind: HirExprKind::StringLiteral(template_string),
                    data_type: DataType::String,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    template_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                Ok((vec![assign_node], temp_place))
            }

            // === Struct Definition Expressions ===
            ExpressionKind::StructDefinition(_fields) => {
                // Struct definitions as expressions are not typical in HIR
                // We'll treat this as an error for now since struct definitions
                // should be handled at the statement level
                return_compiler_error!(
                    "Struct definitions as expressions are not supported in HIR"; {
                        CompilationStage => "HIR Generation",
                        PrimarySuggestion => "Move struct definition to statement level"
                    }
                )
            }
        }
    }

    /// Lower an RPN expression sequence to a place
    fn lower_rpn_to_place(
        &mut self,
        rpn: Vec<AstNode>,
        result_type: DataType,
        location: TextLocation,
    ) -> Result<(Vec<HirNode>, Place), CompilerError> {
        let mut nodes = Vec::new();
        let mut stack: Vec<Place> = Vec::new();

        for node in rpn {
            match node.kind {
                // Push operands (expressions) onto the stack
                NodeKind::Rvalue(expr) => {
                    let (expr_nodes, expr_place) = self.lower_expr_to_place(expr)?;
                    nodes.extend(expr_nodes);
                    stack.push(expr_place);
                }

                // Pop operands, apply operator, push the result
                NodeKind::Operator(op) => {
                    let right_operand = stack.pop().ok_or_else(|| {
                        use crate::compiler::compiler_messages::compiler_errors::{
                            ErrorLocation, ErrorType,
                        };
                        CompilerError::new(
                            "RPN stack underflow (right operand)",
                            ErrorLocation::default(),
                            ErrorType::Compiler,
                        )
                    })?;

                    let left_operand = stack.pop().ok_or_else(|| {
                        use crate::compiler::compiler_messages::compiler_errors::{
                            ErrorLocation, ErrorType,
                        };
                        CompilerError::new(
                            "RPN stack underflow (left operand)",
                            ErrorLocation::default(),
                            ErrorType::Compiler,
                        )
                    })?;

                    let binary_operator = self.convert_operator(op)?;
                    let result_temp_name = self.next_temp();
                    let result_temp_place = Place::local(result_temp_name);

                    // Determine the result type based on operator
                    let operation_result_type = match binary_operator {
                        BinOp::Eq
                        | BinOp::Ne
                        | BinOp::Lt
                        | BinOp::Le
                        | BinOp::Gt
                        | BinOp::Ge
                        | BinOp::And
                        | BinOp::Or => DataType::Bool,
                        _ => result_type.clone(),
                    };

                    let binary_operation_expr = HirExpr {
                        kind: HirExprKind::BinOp {
                            left: left_operand,
                            op: binary_operator,
                            right: right_operand,
                        },
                        data_type: operation_result_type,
                        location: node.location,
                    };

                    let assignment_node = self.create_assign_node_with_expr(
                        result_temp_place.clone(),
                        binary_operation_expr,
                        location.clone(),
                        self.current_scope.clone(),
                    );
                    nodes.push(assignment_node);
                    stack.push(result_temp_place);
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

        // Should have exactly one result
        if stack.len() != 1 {
            return_compiler_error!(
                "Invalid RPN sequence: stack size = {}",
                stack.len(); {
                    CompilationStage => "HIR Generation"
                }
            )
        }

        Ok((nodes, stack.pop().unwrap()))
    }

    /// Convert AST operator to HIR BinOp
    fn convert_operator(&self, ast_operator: Operator) -> Result<BinOp, CompilerError> {
        let hir_binary_operator = match ast_operator {
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
                return_compiler_error!(
                    "Unary operator 'Not' found in binary operation context"; {
                        CompilationStage => "HIR Generation"
                    }
                )
            }
            Operator::Range => {
                return_compiler_error!(
                    "Range operator should be handled as Range expression, not binary operation"; {
                        CompilationStage => "HIR Generation"
                    }
                )
            }
        };
        Ok(hir_binary_operator)
    }

    /// Create an assignment node from place to place
    fn create_assign_node(
        &mut self,
        target: Place,
        source: Place,
        location: TextLocation,
        scope: InternedPath,
    ) -> HirNode {
        let load_expr = HirExpr {
            kind: HirExprKind::Load(source),
            data_type: DataType::Inferred, // Type will be inferred
            location: location.clone(),
        };

        HirNode {
            kind: HirKind::Assign {
                place: target,
                value: load_expr,
            },
            location,
            scope,
            id: self.next_id(),
        }
    }

    /// Create an assignment node from an expression
    fn create_assign_node_with_expr(
        &mut self,
        target: Place,
        expr: HirExpr,
        location: TextLocation,
        scope: InternedPath,
    ) -> HirNode {
        HirNode {
            kind: HirKind::Assign {
                place: target,
                value: expr,
            },
            location,
            scope,
            id: self.next_id(),
        }
    }

    /// Helper: lower match arm
    fn lower_match_arm(&mut self, arm: MatchArm) -> Result<HirMatchArm, CompilerError> {
        // Lower the condition expression to create a pattern
        let pattern = self.lower_expr_to_pattern(arm.condition)?;

        // Lower the body recursively
        let body = self.lower_block(arm.body)?;

        Ok(HirMatchArm {
            pattern,
            guard: None, // Guards not yet supported in AST - will be added when parser supports them
            body,
        })
    }

    /// Helper: convert expression to HIR pattern
    fn lower_expr_to_pattern(
        &mut self,
        expr: Expression,
    ) -> Result<crate::compiler::hir::nodes::HirPattern, CompilerError> {
        use crate::compiler::hir::nodes::HirPattern;

        match expr.kind {
            // Literal patterns - these match exact values
            ExpressionKind::Int(n) => {
                let literal_expr = HirExpr {
                    kind: HirExprKind::Int(n),
                    data_type: expr.data_type,
                    location: expr.location,
                };
                Ok(HirPattern::Literal(literal_expr))
            }

            ExpressionKind::Float(f) => {
                let literal_expr = HirExpr {
                    kind: HirExprKind::Float(f),
                    data_type: expr.data_type,
                    location: expr.location,
                };
                Ok(HirPattern::Literal(literal_expr))
            }

            ExpressionKind::Bool(b) => {
                let literal_expr = HirExpr {
                    kind: HirExprKind::Bool(b),
                    data_type: expr.data_type,
                    location: expr.location,
                };
                Ok(HirPattern::Literal(literal_expr))
            }

            ExpressionKind::StringSlice(s) => {
                let literal_expr = HirExpr {
                    kind: HirExprKind::StringLiteral(s),
                    data_type: expr.data_type,
                    location: expr.location,
                };
                Ok(HirPattern::Literal(literal_expr))
            }

            ExpressionKind::Char(c) => {
                let literal_expr = HirExpr {
                    kind: HirExprKind::Char(c),
                    data_type: expr.data_type,
                    location: expr.location,
                };
                Ok(HirPattern::Literal(literal_expr))
            }

            // Range patterns - match values within a range
            ExpressionKind::Range(start_expr, end_expr) => {
                let start_hir = self.lower_expr_to_hir_expr(*start_expr)?;
                let end_hir = self.lower_expr_to_hir_expr(*end_expr)?;
                Ok(HirPattern::Range {
                    start: start_hir,
                    end: end_hir,
                })
            }

            // Variable references in patterns - these would be binding patterns
            // For now, treat as wildcard since binding patterns aren't fully implemented
            ExpressionKind::Reference(_) => Ok(HirPattern::Wildcard),

            // Complex expressions that can't be patterns
            ExpressionKind::Runtime(_)
            | ExpressionKind::Collection(_)
            | ExpressionKind::StructInstance(_)
            | ExpressionKind::FunctionCall(_, _) => {
                return_compiler_error!(
                    "Complex expressions cannot be used as match patterns: {:?}",
                    expr.kind; {
                        CompilationStage => "HIR Generation",
                        PrimarySuggestion => "Use literal values, ranges, or variables in match patterns"
                    }
                )
            }

            // Unsupported pattern types - treat as wildcard for now
            _ => Ok(HirPattern::Wildcard),
        }
    }

    /// Helper: convert expression to HIR expression (for patterns)
    fn lower_expr_to_hir_expr(&mut self, expr: Expression) -> Result<HirExpr, CompilerError> {
        match expr.kind {
            ExpressionKind::Int(n) => Ok(HirExpr {
                kind: HirExprKind::Int(n),
                data_type: expr.data_type,
                location: expr.location,
            }),

            ExpressionKind::Float(f) => Ok(HirExpr {
                kind: HirExprKind::Float(f),
                data_type: expr.data_type,
                location: expr.location,
            }),

            ExpressionKind::Bool(b) => Ok(HirExpr {
                kind: HirExprKind::Bool(b),
                data_type: expr.data_type,
                location: expr.location,
            }),

            ExpressionKind::StringSlice(s) => Ok(HirExpr {
                kind: HirExprKind::StringLiteral(s),
                data_type: expr.data_type,
                location: expr.location,
            }),

            ExpressionKind::Char(c) => Ok(HirExpr {
                kind: HirExprKind::Char(c),
                data_type: expr.data_type,
                location: expr.location,
            }),

            // Variable references in pattern contexts
            ExpressionKind::Reference(name) => {
                // For range bounds, we need to look up the variable's current value
                // This creates a Load operation to get the variable's place
                let place = Place::local(name);
                Ok(HirExpr {
                    kind: HirExprKind::Load(place),
                    data_type: expr.data_type,
                    location: expr.location,
                })
            }

            _ => {
                return_compiler_error!(
                    "Unsupported expression in pattern context: {:?}",
                    expr.kind; {
                        CompilationStage => "HIR Generation",
                        PrimarySuggestion => "Only literal values and variables can be used in pattern expressions"
                    }
                )
            }
        }
    }

    /// Helper: convert AST node to Place (for assignment targets)
    fn lower_ast_node_to_place(&mut self, node: AstNode) -> Result<Place, CompilerError> {
        match node.kind {
            NodeKind::Rvalue(expr) => Ok(self.lower_expr_to_place(expr)?.1),

            NodeKind::FieldAccess { base, field, .. } => {
                let base_place = self.lower_ast_node_to_place(*base)?;
                Ok(base_place.field(field))
            }

            // TODO: Add support for index access when it's implemented in the AST
            // For now, indexing might be handled through method calls like .get() and .set()
            _ => {
                return_compiler_error!(
                    "Invalid assignment target: {:?}",
                    node.kind; {
                        CompilationStage => "HIR Generation",
                        PrimarySuggestion => "Only variables and fields can be assigned to"
                    }
                )
            }
        }
    }
}
