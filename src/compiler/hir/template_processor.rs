//! Template Processor for HIR Generation
//!
//! This module handles the transformation of Beanstalk templates from AST to HIR.
//! Templates in Beanstalk can be either:
//! - Compile-time templates: Fully resolved at AST stage, become string literals in HIR
//! - Runtime templates: Require runtime evaluation, become function calls in HIR
//!
//! The TemplateProcessor is responsible for:
//! - Converting compile-time templates to HIR string literals
//! - Transforming runtime templates into HIR function calls
//! - Handling template variables and control flow
//! - Preserving template ID information for runtime access
//!
//! Feature: hir-builder, Property 8: Template Processing Correctness
//! Validates: Requirements 10.1, 10.2, 10.3, 10.4, 10.5, 10.6

use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::build_hir::HirBuilderContext;
use crate::compiler::hir::nodes::{HirExpr, HirExprKind, HirKind, HirNode, HirPlace, HirStmt};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::parsers::statements::template::TemplateType;
use crate::compiler::string_interning::InternedString;
use crate::return_compiler_error;

/// Processes Beanstalk templates during HIR generation.
///
/// The TemplateProcessor handles both compile-time and runtime templates,
/// converting them to appropriate HIR representations. Compile-time templates
/// become string literals, while runtime templates become function calls.
pub struct TemplateProcessor;

impl TemplateProcessor {
    /// Creates a new TemplateProcessor
    pub fn new() -> Self {
        TemplateProcessor
    }

    /// Processes a template expression and returns the appropriate HIR representation.
    ///
    /// This is the main entry point for template processing. It determines whether
    /// the template is compile-time or runtime and delegates to the appropriate handler.
    ///
    /// # Arguments
    /// * `template` - The template to process
    /// * `ctx` - The HIR builder context
    ///
    /// # Returns
    /// A tuple of (setup nodes, result expression) where setup nodes are any
    /// statements needed before the expression can be used.
    pub fn process_template(
        &mut self,
        template: &Template,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        match template.kind {
            TemplateType::String => {
                // Compile-time template - already folded to a string
                self.process_compile_time_template(template, ctx)
            }
            TemplateType::StringFunction => {
                // Runtime template - needs to become a function call
                self.process_runtime_template(template, ctx)
            }
            TemplateType::Slot => {
                // Slots are handled during template parsing, not HIR generation
                return_compiler_error!(
                    "Template slots should be resolved during AST stage, not HIR generation"
                )
            }
            TemplateType::Comment => {
                // Comments produce no output
                Ok((
                    Vec::new(),
                    HirExpr {
                        kind: HirExprKind::StringLiteral(ctx.string_table.intern("")),
                        location: template.location.clone(),
                    },
                ))
            }
        }
    }

    /// Processes a compile-time template that has been fully resolved to a string.
    ///
    /// Compile-time templates are those that can be completely evaluated at compile time.
    /// They contain only literal values and no runtime expressions. These templates
    /// are folded into string literals during AST construction.
    ///
    /// # Requirements
    /// - 10.1: WHEN processing compile-time templates THEN the system SHALL convert
    ///   them to HIR string literals since they were resolved at AST stage
    pub fn process_compile_time_template(
        &mut self,
        template: &Template,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        // For compile-time templates, we need to fold the content into a single string
        // The template should already be foldable at this point
        let inherited_style = template
            .style
            .child_default
            .as_ref()
            .map(|style| *style.clone());

        let folded_string = template.fold(&inherited_style, ctx.string_table)?;

        let expr = HirExpr {
            kind: HirExprKind::StringLiteral(folded_string),
            location: template.location.clone(),
        };

        Ok((Vec::new(), expr))
    }

    /// Processes a runtime template that requires runtime evaluation.
    ///
    /// Runtime templates contain expressions that cannot be evaluated at compile time,
    /// such as variable references or function calls. These templates are transformed
    /// into HIR function calls that will construct the string at runtime.
    ///
    /// # Requirements
    /// - 10.2: WHEN handling runtime templates THEN the system SHALL generate HIR
    ///   function calls to template functions created during AST processing
    /// - 10.3: WHEN encountering template variables THEN the system SHALL create
    ///   proper HIR variable access instructions for template interpolation
    pub fn process_runtime_template(
        &mut self,
        template: &Template,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        let mut setup_nodes = Vec::new();

        // Generate a unique name for the template function
        let template_fn_name = self.generate_template_fn_name(template, ctx);

        // Collect all captures (variables referenced in the template)
        let captures = self.collect_template_captures(template, ctx)?;

        // Parse the template ID if present
        let template_id = self.parse_template_id(template, ctx);

        // Create the RuntimeTemplateCall node
        let node_id = ctx.allocate_node_id();
        let call_node = HirNode {
            kind: HirKind::Stmt(HirStmt::RuntimeTemplateCall {
                template_fn: template_fn_name,
                captures: captures.clone(),
                id: template_id,
            }),
            location: template.location.clone(),
            id: node_id,
        };

        setup_nodes.push(call_node);

        // The result is a heap-allocated string from the template call
        // We create a temporary variable to hold the result
        let result_var_name = ctx.metadata_mut().generate_temp_name();
        let result_var = ctx.string_table.intern(&result_var_name);

        // Create an assignment to capture the template result
        let assign_node_id = ctx.allocate_node_id();
        let assign_node = HirNode {
            kind: HirKind::Stmt(HirStmt::Assign {
                target: HirPlace::Var(result_var),
                value: HirExpr {
                    kind: HirExprKind::HeapString(template_fn_name),
                    location: template.location.clone(),
                },
                is_mutable: true,
            }),
            location: template.location.clone(),
            id: assign_node_id,
        };

        setup_nodes.push(assign_node);

        // Mark the result variable as potentially owned
        ctx.mark_potentially_owned(result_var);

        // Return the result expression
        let result_expr = HirExpr {
            kind: HirExprKind::Load(HirPlace::Var(result_var)),
            location: template.location.clone(),
        };

        Ok((setup_nodes, result_expr))
    }

    /// Generates a unique name for a template function.
    ///
    /// Template functions are generated for runtime templates and need unique names
    /// to avoid conflicts. The name includes the template's ID if present.
    pub fn generate_template_fn_name(
        &self,
        template: &Template,
        ctx: &mut HirBuilderContext,
    ) -> InternedString {
        let name = if template.id.is_empty() {
            let counter = ctx.metadata_mut().generate_temp_name();
            format!("__template_fn_{}", counter)
        } else {
            format!("__template_fn_{}", template.id)
        };
        ctx.string_table.intern(&name)
    }

    /// Collects all variable captures from a template.
    ///
    /// This method walks through the template content and identifies all variables
    /// that need to be captured for runtime evaluation.
    ///
    /// # Requirements
    /// - 10.3: WHEN encountering template variables THEN the system SHALL create
    ///   proper HIR variable access instructions for template interpolation
    pub fn collect_template_captures(
        &mut self,
        template: &Template,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirExpr>, CompilerError> {
        let mut captures = Vec::new();

        // Process all expressions in the template content
        for expr in template.content.flatten() {
            self.collect_captures_from_expression(expr, &mut captures, ctx)?;
        }

        Ok(captures)
    }

    /// Recursively collects captures from an expression.
    fn collect_captures_from_expression(
        &mut self,
        expr: &Expression,
        captures: &mut Vec<HirExpr>,
        ctx: &mut HirBuilderContext,
    ) -> Result<(), CompilerError> {
        match &expr.kind {
            ExpressionKind::Reference(var_name) => {
                // This is a variable reference - add it as a capture
                let hir_expr = HirExpr {
                    kind: HirExprKind::Load(HirPlace::Var(*var_name)),
                    location: expr.location.clone(),
                };
                captures.push(hir_expr);
            }
            ExpressionKind::Template(nested_template) => {
                // Recursively process nested templates
                let nested_captures = self.collect_template_captures(nested_template, ctx)?;
                captures.extend(nested_captures);
            }
            ExpressionKind::FunctionCall(_, args) => {
                // Process function call arguments
                for arg in args {
                    self.collect_captures_from_expression(arg, captures, ctx)?;
                }
            }
            ExpressionKind::Runtime(nodes) => {
                // Process runtime expressions
                for node in nodes {
                    if let Ok(node_expr) = node.get_expr() {
                        self.collect_captures_from_expression(&node_expr, captures, ctx)?;
                    }
                }
            }
            // Literal values don't need to be captured
            ExpressionKind::StringSlice(_)
            | ExpressionKind::Int(_)
            | ExpressionKind::Float(_)
            | ExpressionKind::Bool(_)
            | ExpressionKind::Char(_)
            | ExpressionKind::None => {}
            // Other expression types
            ExpressionKind::Collection(items) => {
                for item in items {
                    self.collect_captures_from_expression(item, captures, ctx)?;
                }
            }
            ExpressionKind::StructInstance(args) | ExpressionKind::StructDefinition(args) => {
                for arg in args {
                    self.collect_captures_from_expression(&arg.value, captures, ctx)?;
                }
            }
            ExpressionKind::Range(start, end) => {
                self.collect_captures_from_expression(start, captures, ctx)?;
                self.collect_captures_from_expression(end, captures, ctx)?;
            }
            ExpressionKind::Function(_, body) => {
                // Functions in templates are complex - for now, we don't capture from them
                // This could be extended in the future
                let _ = body;
            }
        }

        Ok(())
    }

    /// Parses the template ID if present.
    ///
    /// Template IDs are used for runtime access to templates, particularly
    /// in web/DOM contexts where templates may need to be referenced by ID.
    ///
    /// # Requirements
    /// - 10.5: WHEN handling template IDs THEN the system SHALL preserve template
    ///   ID information for runtime access
    pub fn parse_template_id(
        &self,
        template: &Template,
        ctx: &mut HirBuilderContext,
    ) -> Option<InternedString> {
        if template.id.is_empty() {
            None
        } else {
            Some(ctx.string_table.intern(&template.id))
        }
    }

    /// Creates a template function definition for a runtime template.
    ///
    /// This method generates an HIR function definition that will be called
    /// at runtime to construct the template string.
    ///
    /// # Requirements
    /// - 10.2: WHEN handling runtime templates THEN the system SHALL generate HIR
    ///   function calls to template functions created during AST processing
    pub fn create_template_function(
        &mut self,
        name: InternedString,
        captures: &[(InternedString, DataType)],
        template: &Template,
        ctx: &mut HirBuilderContext,
    ) -> Result<HirNode, CompilerError> {
        // Create a new block for the template function body
        let body_block_id = ctx.create_block();

        // Enter a new scope for the function
        ctx.enter_scope_with_block(
            crate::compiler::hir::build_hir::ScopeType::Function,
            body_block_id,
        );

        // Process the template content and add nodes to the body block
        let content_nodes = self.process_template_content(template, ctx)?;
        for node in content_nodes {
            ctx.add_node_to_block(body_block_id, node);
        }

        // Exit the function scope
        let _dropped_vars = ctx.exit_scope();

        // Create the TemplateFn node
        let node_id = ctx.allocate_node_id();
        let template_fn_node = HirNode {
            kind: HirKind::Stmt(HirStmt::TemplateFn {
                name,
                params: captures.to_vec(),
                body: body_block_id,
            }),
            location: template.location.clone(),
            id: node_id,
        };

        Ok(template_fn_node)
    }

    /// Processes the content of a template and generates HIR nodes.
    ///
    /// This method handles the various types of content that can appear in a template,
    /// including string literals, variable interpolations, and nested templates.
    ///
    /// # Requirements
    /// - 10.4: WHEN processing template control flow THEN the system SHALL handle
    ///   if and for constructs within templates appropriately
    fn process_template_content(
        &mut self,
        template: &Template,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        // Process each expression in the template content
        for expr in template.content.flatten() {
            let content_nodes = self.process_template_expression(expr, ctx)?;
            nodes.extend(content_nodes);
        }

        Ok(nodes)
    }

    /// Processes a single expression within a template.
    fn process_template_expression(
        &mut self,
        expr: &Expression,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        match &expr.kind {
            ExpressionKind::StringSlice(interned_string) => {
                // String literal - create an expression statement
                let node_id = ctx.allocate_node_id();
                let node = HirNode {
                    kind: HirKind::Stmt(HirStmt::ExprStmt(HirExpr {
                        kind: HirExprKind::StringLiteral(*interned_string),
                        location: expr.location.clone(),
                    })),
                    location: expr.location.clone(),
                    id: node_id,
                };
                nodes.push(node);
            }
            ExpressionKind::Reference(var_name) => {
                // Variable reference - load and convert to string
                let node_id = ctx.allocate_node_id();
                let node = HirNode {
                    kind: HirKind::Stmt(HirStmt::ExprStmt(HirExpr {
                        kind: HirExprKind::Load(HirPlace::Var(*var_name)),
                        location: expr.location.clone(),
                    })),
                    location: expr.location.clone(),
                    id: node_id,
                };
                nodes.push(node);
            }
            ExpressionKind::Template(nested_template) => {
                // Nested template - recursively process
                let (setup_nodes, _result_expr) = self.process_template(nested_template, ctx)?;
                nodes.extend(setup_nodes);
            }
            ExpressionKind::Int(value) => {
                // Integer literal - will be converted to string at runtime
                let node_id = ctx.allocate_node_id();
                let node = HirNode {
                    kind: HirKind::Stmt(HirStmt::ExprStmt(HirExpr {
                        kind: HirExprKind::Int(*value),
                        location: expr.location.clone(),
                    })),
                    location: expr.location.clone(),
                    id: node_id,
                };
                nodes.push(node);
            }
            ExpressionKind::Float(value) => {
                // Float literal - will be converted to string at runtime
                let node_id = ctx.allocate_node_id();
                let node = HirNode {
                    kind: HirKind::Stmt(HirStmt::ExprStmt(HirExpr {
                        kind: HirExprKind::Float(*value),
                        location: expr.location.clone(),
                    })),
                    location: expr.location.clone(),
                    id: node_id,
                };
                nodes.push(node);
            }
            ExpressionKind::Bool(value) => {
                // Bool literal - will be converted to string at runtime
                let node_id = ctx.allocate_node_id();
                let node = HirNode {
                    kind: HirKind::Stmt(HirStmt::ExprStmt(HirExpr {
                        kind: HirExprKind::Bool(*value),
                        location: expr.location.clone(),
                    })),
                    location: expr.location.clone(),
                    id: node_id,
                };
                nodes.push(node);
            }
            _ => {
                // Other expression types - create a generic expression statement
                // This handles function calls, runtime expressions, etc.
                let hir_expr = self.convert_expression_to_hir(expr, ctx)?;
                let node_id = ctx.allocate_node_id();
                let node = HirNode {
                    kind: HirKind::Stmt(HirStmt::ExprStmt(hir_expr)),
                    location: expr.location.clone(),
                    id: node_id,
                };
                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    /// Converts an AST expression to an HIR expression.
    ///
    /// This is a helper method for converting expressions that appear in templates
    /// to their HIR equivalents.
    fn convert_expression_to_hir(
        &mut self,
        expr: &Expression,
        ctx: &mut HirBuilderContext,
    ) -> Result<HirExpr, CompilerError> {
        let hir_kind = match &expr.kind {
            ExpressionKind::Int(value) => HirExprKind::Int(*value),
            ExpressionKind::Float(value) => HirExprKind::Float(*value),
            ExpressionKind::Bool(value) => HirExprKind::Bool(*value),
            ExpressionKind::Char(value) => HirExprKind::Char(*value),
            ExpressionKind::StringSlice(interned) => HirExprKind::StringLiteral(*interned),
            ExpressionKind::Reference(var_name) => HirExprKind::Load(HirPlace::Var(*var_name)),
            ExpressionKind::None => {
                // None becomes an empty string in template context
                HirExprKind::StringLiteral(ctx.string_table.intern(""))
            }
            ExpressionKind::FunctionCall(name, args) => {
                // Convert function call arguments
                let mut hir_args = Vec::new();
                for arg in args {
                    let hir_arg = self.convert_expression_to_hir(arg, ctx)?;
                    hir_args.push(hir_arg);
                }
                HirExprKind::Call {
                    target: *name,
                    args: hir_args,
                }
            }
            _ => {
                // For complex expressions, we create a placeholder
                // The expression linearizer should handle these
                return_compiler_error!(
                    "Complex expression in template not yet supported: {:?}",
                    expr.kind
                )
            }
        };

        Ok(HirExpr {
            kind: hir_kind,
            location: expr.location.clone(),
        })
    }

    /// Handles template control flow (if/for constructs within templates).
    ///
    /// # Requirements
    /// - 10.4: WHEN processing template control flow THEN the system SHALL handle
    ///   if and for constructs within templates appropriately
    pub fn handle_template_control_flow(
        &mut self,
        _template: &Template,
        _ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        // Template control flow is currently handled during AST construction
        // This method is a placeholder for future enhancements
        Ok(Vec::new())
    }

    /// Processes nested templates.
    ///
    /// # Requirements
    /// - 10.6: WHEN processing nested templates THEN the system SHALL ensure proper
    ///   HIR generation for complex template structures
    pub fn process_nested_template(
        &mut self,
        nested_template: &Template,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        // Nested templates are processed the same way as top-level templates
        self.process_template(nested_template, ctx)
    }
}

impl Default for TemplateProcessor {
    fn default() -> Self {
        Self::new()
    }
}
