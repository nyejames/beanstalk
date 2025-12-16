use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::builder::HirBuilder;
use crate::compiler::hir::nodes::{HirExpr, HirExprKind, HirKind, HirNode};
use crate::compiler::hir::place::Place;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::return_compiler_error;

/// Lower a single AST node to HIR
///
/// This method linearizes expressions by introducing temporary variables
/// and converts all operations to work on places rather than nested expressions.
impl <'a> HirBuilder<'a> {
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
                        .get(&name)
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
}