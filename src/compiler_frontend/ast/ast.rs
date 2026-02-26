use crate::backends::function_registry::HostRegistry;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::statements::structs::create_struct_definition;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::headers::parse_file_headers::{
    Header, HeaderKind, TopLevelTemplateItem, TopLevelTemplateKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use crate::projects::settings::{self, IMPLICIT_START_FUNC_NAME, TOP_LEVEL_TEMPLATE_NAME};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

static CONTROL_FLOW_SCOPE_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct ModuleExport {
    pub id: StringId,
    pub signature: FunctionSignature,
}

#[derive(Clone, Debug)]
pub enum AstStartTemplateItem {
    ConstString {
        value: StringId,
        location: TextLocation,
    },
    RuntimeStringFunction {
        function: InternedPath,
        location: TextLocation,
    },
}

pub struct Ast {
    pub nodes: Vec<AstNode>,

    // The path to the original entry point file
    pub entry_path: InternedPath,

    // Exported out of the final compiled wasm module
    // Functions must use explicit 'export' syntax Token::Export to be exported
    // The only exception is the Main function, which is the start function of the entry point file
    pub external_exports: Vec<ModuleExport>,
    pub start_template_items: Vec<AstStartTemplateItem>,
    pub warnings: Vec<CompilerWarning>,
}

impl Ast {
    pub fn new(
        sorted_headers: Vec<Header>,
        top_level_template_items: Vec<TopLevelTemplateItem>,
        host_registry: &HostRegistry,
        string_table: &mut StringTable,
        entry_dir: InternedPath,
    ) -> Result<Ast, CompilerMessages> {
        // Each file will be combined into a single AST.
        let mut ast: Vec<AstNode> =
            Vec::with_capacity(sorted_headers.len() * settings::TOKEN_TO_NODE_RATIO);
        let external_exports: Vec<ModuleExport> = Vec::new();
        let mut warnings: Vec<CompilerWarning> = Vec::new();
        let mut const_templates_by_path: FxHashMap<InternedPath, StringId> = FxHashMap::default();

        // Collect module-level declarations first so all function/start bodies can resolve symbols.
        let mut declarations: Vec<Declaration> = Vec::new();
        for header in &sorted_headers {
            match &header.kind {
                HeaderKind::Function { signature } => declarations.push(Declaration {
                    id: header.tokens.src_path.to_owned(),
                    value: Expression::new(
                        ExpressionKind::None,
                        header.name_location.to_owned(),
                        DataType::Function(Box::new(None), signature.to_owned()),
                        Ownership::ImmutableReference,
                    ),
                }),
                HeaderKind::Struct => {
                    let mut struct_tokens = header.tokens.to_owned();
                    let fields = match create_struct_definition(&mut struct_tokens, string_table) {
                        Ok(fields) => fields,
                        Err(e) => {
                            return Err(CompilerMessages {
                                errors: vec![e],
                                warnings,
                            });
                        }
                    };

                    declarations.push(Declaration {
                        id: header.tokens.src_path.to_owned(),
                        value: Expression::new(
                            ExpressionKind::None,
                            header.name_location.to_owned(),
                            DataType::Struct(fields, Ownership::MutableOwned),
                            Ownership::ImmutableReference,
                        ),
                    });
                }
                HeaderKind::StartFunction => {
                    let start_name = header
                        .tokens
                        .src_path
                        .join_str(IMPLICIT_START_FUNC_NAME, string_table);
                    declarations.push(Declaration {
                        id: start_name,
                        value: Expression::new(
                            ExpressionKind::None,
                            header.name_location.to_owned(),
                            DataType::Function(
                                Box::new(None),
                                FunctionSignature {
                                    parameters: vec![],
                                    returns: vec![DataType::StringSlice],
                                },
                            ),
                            Ownership::ImmutableReference,
                        ),
                    });
                }
                _ => {}
            }
        }

        for mut header in sorted_headers {
            match header.kind {
                HeaderKind::Function { signature } => {
                    let mut function_declarations = declarations.to_owned();
                    function_declarations.extend(signature.parameters.to_owned());

                    // Function parameters should be available in the function body scope
                    let context = ScopeContext::new(
                        ContextKind::Function,
                        header.tokens.src_path.to_owned(),
                        &function_declarations,
                        host_registry.clone(),
                        signature.returns.clone(),
                    );

                    let mut token_stream = header.tokens;

                    let body = match function_body_to_ast(
                        &mut token_stream,
                        context.to_owned(),
                        &mut warnings,
                        string_table,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            return Err(CompilerMessages {
                                errors: vec![e],
                                warnings,
                            });
                        }
                    };

                    // Make the name from the header path.
                    // AST symbol IDs are stored as full InternedPath values and are unique
                    // module-wide, not only within a local scope.
                    ast.push(AstNode {
                        kind: NodeKind::Function(
                            token_stream.src_path,
                            signature.to_owned(),
                            body.to_owned(),
                        ),
                        location: header.name_location,
                        scope: context.scope.clone(), // Preserve the full path in the scope field
                    });
                }

                HeaderKind::StartFunction => {
                    let context = ScopeContext::new(
                        ContextKind::Module,
                        header.tokens.src_path.to_owned(),
                        &declarations,
                        host_registry.clone(),
                        vec![],
                    );

                    let mut token_stream = header.tokens;

                    let mut body = match function_body_to_ast(
                        &mut token_stream,
                        context.to_owned(),
                        &mut warnings,
                        string_table,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            return Err(CompilerMessages {
                                errors: vec![e],
                                warnings,
                            });
                        }
                    };

                    // Add the automatic return statement for the start function
                    let empty_string = string_table.get_or_intern(String::new());
                    body.push(AstNode {
                        kind: NodeKind::Return(vec![Expression::string_slice(
                            empty_string,
                            token_stream.current_location(),
                            Ownership::ImmutableOwned,
                        )]),
                        location: token_stream.current_location(),
                        scope: context.scope.clone(),
                    });

                    // Create an implicit "start" function that can be called by other modules
                    let full_name = token_stream
                        .src_path
                        .join_str(IMPLICIT_START_FUNC_NAME, string_table);

                    let main_signature = FunctionSignature {
                        parameters: vec![],
                        returns: vec![DataType::StringSlice],
                    };

                    ast.push(AstNode {
                        kind: NodeKind::Function(full_name, main_signature, body),
                        location: header.name_location,
                        scope: context.scope.clone(),
                    });
                }

                HeaderKind::Struct => {
                    let fields = match create_struct_definition(&mut header.tokens, string_table) {
                        Ok(f) => f,
                        Err(e) => {
                            return Err(CompilerMessages {
                                errors: vec![e],
                                warnings,
                            });
                        }
                    };

                    ast.push(AstNode {
                        kind: NodeKind::StructDefinition(header.tokens.src_path.to_owned(), fields), // Use the simple name for identifier
                        location: header.name_location,
                        scope: header.tokens.src_path, // Preserve the full path in the scope field
                    });
                }

                HeaderKind::Constant => {
                    // TODO: Implement constant handling
                    todo!()
                }

                HeaderKind::Choice => {
                    // TODO: Implement choice handling
                    todo!()
                }

                HeaderKind::ConstTemplate { .. } => {
                    let mut template_tokens = header.tokens;
                    let context = ScopeContext::new(
                        ContextKind::Constant,
                        template_tokens.src_path.to_owned(),
                        &declarations,
                        host_registry.clone(),
                        vec![],
                    );

                    let template =
                        match Template::new(&mut template_tokens, &context, None, string_table) {
                            Ok(template) => template,
                            Err(error) => {
                                return Err(CompilerMessages {
                                    errors: vec![error],
                                    warnings,
                                });
                            }
                        };

                    if !matches!(
                        template.kind,
                        crate::compiler_frontend::ast::templates::template::TemplateType::String
                    ) {
                        return Err(CompilerMessages {
                            errors: vec![CompilerError::new_rule_error(
                                "Top-level const templates must be fully foldable at compile time.",
                                template.location.to_error_location(string_table),
                            )],
                            warnings,
                        });
                    }

                    let html = match template.fold(&None, string_table) {
                        Ok(value) => value,
                        Err(error) => {
                            return Err(CompilerMessages {
                                errors: vec![error],
                                warnings,
                            });
                        }
                    };

                    const_templates_by_path.insert(template_tokens.src_path, html);
                }
            }

            // TODO: create a function definition for these exported headers
            if header.exported {}
        }

        let start_template_items = synthesize_start_template_items(
            &mut ast,
            &entry_dir,
            &top_level_template_items,
            &const_templates_by_path,
            string_table,
        )
        .map_err(|error| CompilerMessages {
            errors: vec![error],
            warnings: warnings.clone(),
        })?;

        Ok(Ast {
            nodes: ast,
            entry_path: entry_dir,
            external_exports,
            start_template_items,
            warnings,
        })
    }
}

#[derive(Clone)]
struct RuntimeTemplateCandidate {
    declaration: Declaration,
    location: TextLocation,
    scope: InternedPath,
    preceding_statements: Vec<AstNode>,
}

fn synthesize_start_template_items(
    ast_nodes: &mut Vec<AstNode>,
    entry_dir: &InternedPath,
    top_level_template_items: &[TopLevelTemplateItem],
    const_templates_by_path: &FxHashMap<InternedPath, StringId>,
    string_table: &mut StringTable,
) -> Result<Vec<AstStartTemplateItem>, CompilerError> {
    let entry_start_function_name = entry_dir.join_str(IMPLICIT_START_FUNC_NAME, string_table);
    let Some(entry_start_index) = ast_nodes.iter().position(|node| {
        matches!(
            &node.kind,
            NodeKind::Function(name, _, _) if *name == entry_start_function_name
        )
    }) else {
        return Err(CompilerError::compiler_error(format!(
            "Failed to find entry start function '{}' while synthesizing start fragments.",
            entry_start_function_name.to_string(string_table)
        )));
    };

    let (runtime_candidates, entry_scope) =
        extract_runtime_template_candidates(ast_nodes, entry_start_index, string_table)?;

    let mut ordered_items = top_level_template_items.to_owned();
    ordered_items.sort_by_key(|item| item.file_order);

    let mut next_fragment_index = 0usize;
    let mut ordered_fragment_sources: Vec<OrderedFragmentSource> =
        Vec::with_capacity(ordered_items.len() + runtime_candidates.len());

    for template_item in ordered_items {
        if let TopLevelTemplateKind::ConstTemplate { header_path } = template_item.kind {
            let Some(value) = const_templates_by_path.get(&header_path).copied() else {
                return Err(CompilerError::compiler_error(format!(
                    "Missing const template value for '{}'",
                    header_path.to_string(string_table)
                )));
            };

            ordered_fragment_sources.push(OrderedFragmentSource::Const {
                value,
                location: template_item.location,
            });
        }
    }

    // Runtime fragment ordering comes from extracted top-level template declarations.
    // This avoids treating template expressions inside assignments/calls as start fragments.
    for candidate in runtime_candidates {
        ordered_fragment_sources.push(OrderedFragmentSource::Runtime(candidate));
    }

    ordered_fragment_sources.sort_by(compare_fragment_locations);

    let mut start_template_items = Vec::with_capacity(ordered_fragment_sources.len());
    for source in ordered_fragment_sources {
        match source {
            OrderedFragmentSource::Const { value, location } => {
                start_template_items.push(AstStartTemplateItem::ConstString { value, location });
            }

            OrderedFragmentSource::Runtime(candidate) => {
                let ExpressionKind::Template(template) = &candidate.declaration.value.kind else {
                    return Err(CompilerError::compiler_error(
                        "Top-level runtime template candidate was not parsed as a template expression.",
                    ));
                };

                if matches!(
                    template.kind,
                    crate::compiler_frontend::ast::templates::template::TemplateType::String
                ) {
                    let folded = template.fold(&None, string_table)?;
                    start_template_items.push(AstStartTemplateItem::ConstString {
                        value: folded,
                        location: candidate.location,
                    });
                    continue;
                }

                let fragment_name =
                    entry_dir.join_str(&format!("__bst_frag_{next_fragment_index}"), string_table);
                next_fragment_index += 1;

                let mut fragment_body = build_runtime_fragment_body(&candidate, string_table)?;
                fragment_body.push(AstNode {
                    kind: NodeKind::Return(vec![candidate.declaration.value.clone()]),
                    location: candidate.location.clone(),
                    scope: candidate.scope.clone(),
                });

                ast_nodes.push(AstNode {
                    kind: NodeKind::Function(
                        fragment_name.clone(),
                        FunctionSignature {
                            parameters: vec![],
                            returns: vec![DataType::StringSlice],
                        },
                        fragment_body,
                    ),
                    location: candidate.location.clone(),
                    scope: entry_scope.clone(),
                });

                start_template_items.push(AstStartTemplateItem::RuntimeStringFunction {
                    function: fragment_name,
                    location: candidate.location,
                });
            }
        }
    }

    Ok(start_template_items)
}

#[derive(Clone)]
enum OrderedFragmentSource {
    Const {
        value: StringId,
        location: TextLocation,
    },
    Runtime(RuntimeTemplateCandidate),
}

fn compare_fragment_locations(
    lhs: &OrderedFragmentSource,
    rhs: &OrderedFragmentSource,
) -> std::cmp::Ordering {
    let lhs_location = match lhs {
        OrderedFragmentSource::Const { location, .. } => location,
        OrderedFragmentSource::Runtime(candidate) => &candidate.location,
    };
    let rhs_location = match rhs {
        OrderedFragmentSource::Const { location, .. } => location,
        OrderedFragmentSource::Runtime(candidate) => &candidate.location,
    };

    lhs_location
        .start_pos
        .line_number
        .cmp(&rhs_location.start_pos.line_number)
        .then(
            lhs_location
                .start_pos
                .char_column
                .cmp(&rhs_location.start_pos.char_column),
        )
}

fn extract_runtime_template_candidates(
    ast_nodes: &mut [AstNode],
    entry_start_index: usize,
    string_table: &StringTable,
) -> Result<(Vec<RuntimeTemplateCandidate>, InternedPath), CompilerError> {
    let Some(entry_start_node) = ast_nodes.get_mut(entry_start_index) else {
        return Err(CompilerError::compiler_error(
            "Entry start function index is out of bounds while extracting runtime templates.",
        ));
    };

    let entry_scope = entry_start_node.scope.clone();
    let NodeKind::Function(_, _, body) = &mut entry_start_node.kind else {
        return Err(CompilerError::compiler_error(
            "Entry start function node is not a function while extracting runtime templates.",
        ));
    };

    let original_body = body.clone();
    let mut runtime_candidates = Vec::new();
    let mut filtered_body = Vec::with_capacity(original_body.len());

    for (index, node) in original_body.iter().enumerate() {
        if let Some(declaration) = as_top_level_template_declaration(node, string_table) {
            runtime_candidates.push(RuntimeTemplateCandidate {
                declaration: declaration.clone(),
                location: node.location.clone(),
                scope: node.scope.clone(),
                preceding_statements: original_body[..index]
                    .iter()
                    .filter(|statement| {
                        as_top_level_template_declaration(statement, string_table).is_none()
                    })
                    .cloned()
                    .collect(),
            });
            continue;
        }

        filtered_body.push(node.clone());
    }

    *body = filtered_body;
    Ok((runtime_candidates, entry_scope))
}

fn as_top_level_template_declaration<'a>(
    node: &'a AstNode,
    string_table: &StringTable,
) -> Option<&'a Declaration> {
    let NodeKind::VariableDeclaration(declaration) = &node.kind else {
        return None;
    };

    let is_template_name = declaration
        .id
        .name_str(string_table)
        .is_some_and(|name| name == TOP_LEVEL_TEMPLATE_NAME);

    if !is_template_name {
        return None;
    }

    if !matches!(declaration.value.kind, ExpressionKind::Template(_)) {
        return None;
    }

    Some(declaration)
}

fn build_runtime_fragment_body(
    candidate: &RuntimeTemplateCandidate,
    string_table: &StringTable,
) -> Result<Vec<AstNode>, CompilerError> {
    let mut required_symbols = FxHashSet::default();
    collect_references_from_expression(&candidate.declaration.value, &mut required_symbols);

    let declaration_lookup = candidate
        .preceding_statements
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            let NodeKind::VariableDeclaration(declaration) = &node.kind else {
                return None;
            };

            Some((declaration.id.clone(), (index, declaration)))
        })
        .collect::<FxHashMap<_, _>>();

    let mut included_declarations = FxHashSet::default();
    let mut visiting = FxHashSet::default();
    for symbol in required_symbols {
        include_declaration_dependencies(
            &symbol,
            &declaration_lookup,
            &mut included_declarations,
            &mut visiting,
        )?;
    }

    let included_symbols = included_declarations
        .iter()
        .filter_map(|index| {
            let NodeKind::VariableDeclaration(declaration) =
                &candidate.preceding_statements.get(*index)?.kind
            else {
                return None;
            };
            Some(declaration.id.clone())
        })
        .collect::<FxHashSet<_>>();

    for statement in &candidate.preceding_statements {
        let NodeKind::Assignment { target, .. } = &statement.kind else {
            continue;
        };

        let mut assignment_targets = FxHashSet::default();
        collect_references_from_ast_node(target, &mut assignment_targets);

        if assignment_targets
            .into_iter()
            .any(|symbol| included_symbols.contains(&symbol))
        {
            return Err(CompilerError::new_rule_error(
                "Runtime start-fragment captures currently do not support mutable reassignments before template evaluation.",
                statement.location.to_error_location(string_table),
            ));
        }
    }

    let mut ordered_indices = included_declarations.into_iter().collect::<Vec<_>>();
    ordered_indices.sort_unstable();

    let mut fragment_body = Vec::with_capacity(ordered_indices.len());
    for index in ordered_indices {
        let Some(statement) = candidate.preceding_statements.get(index).cloned() else {
            return Err(CompilerError::compiler_error(
                "Fragment dependency index was out of bounds.",
            ));
        };
        fragment_body.push(statement);
    }

    Ok(fragment_body)
}

fn include_declaration_dependencies(
    symbol: &InternedPath,
    declaration_lookup: &FxHashMap<InternedPath, (usize, &Declaration)>,
    included_declarations: &mut FxHashSet<usize>,
    visiting: &mut FxHashSet<InternedPath>,
) -> Result<(), CompilerError> {
    let Some((index, declaration)) = declaration_lookup.get(symbol) else {
        return Ok(());
    };

    if included_declarations.contains(index) {
        return Ok(());
    }

    if !visiting.insert(symbol.clone()) {
        return Err(CompilerError::compiler_error(
            "Cyclic declaration capture detected while synthesizing runtime fragment.",
        ));
    }

    let mut nested_symbols = FxHashSet::default();
    collect_references_from_expression(&declaration.value, &mut nested_symbols);
    for dependency in nested_symbols {
        if dependency != *symbol {
            include_declaration_dependencies(
                &dependency,
                declaration_lookup,
                included_declarations,
                visiting,
            )?;
        }
    }

    visiting.remove(symbol);
    included_declarations.insert(*index);
    Ok(())
}

fn collect_references_from_expression(
    expression: &Expression,
    references: &mut FxHashSet<InternedPath>,
) {
    match &expression.kind {
        ExpressionKind::Reference(name) => {
            references.insert(name.clone());
        }

        ExpressionKind::Runtime(nodes) => {
            for node in nodes {
                collect_references_from_ast_node(node, references);
            }
        }

        ExpressionKind::FunctionCall(_, args)
        | ExpressionKind::HostFunctionCall(_, args)
        | ExpressionKind::Collection(args) => {
            for argument in args {
                collect_references_from_expression(argument, references);
            }
        }

        ExpressionKind::Template(template) => {
            for value in template.content.flatten() {
                collect_references_from_expression(value, references);
            }
        }

        ExpressionKind::StructDefinition(arguments) | ExpressionKind::StructInstance(arguments) => {
            for argument in arguments {
                collect_references_from_expression(&argument.value, references);
            }
        }

        ExpressionKind::Range(lower, upper) => {
            collect_references_from_expression(lower, references);
            collect_references_from_expression(upper, references);
        }

        ExpressionKind::Function(_, body) => {
            for node in body {
                collect_references_from_ast_node(node, references);
            }
        }

        ExpressionKind::None
        | ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_) => {}
    }
}

fn collect_references_from_ast_node(node: &AstNode, references: &mut FxHashSet<InternedPath>) {
    match &node.kind {
        NodeKind::VariableDeclaration(declaration) => {
            collect_references_from_expression(&declaration.value, references);
        }

        NodeKind::Assignment { target, value } => {
            collect_references_from_ast_node(target, references);
            collect_references_from_expression(value, references);
        }

        NodeKind::FieldAccess { base, .. } => {
            collect_references_from_ast_node(base, references);
        }

        NodeKind::MethodCall { base, args, .. } => {
            collect_references_from_ast_node(base, references);
            for argument in args {
                collect_references_from_ast_node(argument, references);
            }
        }

        NodeKind::FunctionCall { args, .. } | NodeKind::HostFunctionCall { args, .. } => {
            for argument in args {
                collect_references_from_expression(argument, references);
            }
        }

        NodeKind::StructDefinition(_, fields) => {
            for field in fields {
                collect_references_from_expression(&field.value, references);
            }
        }

        NodeKind::Function(_, _, body) => {
            for statement in body {
                collect_references_from_ast_node(statement, references);
            }
        }

        NodeKind::Rvalue(expression)
        | NodeKind::Template(expression)
        | NodeKind::TopLevelTemplate(expression) => {
            collect_references_from_expression(expression, references);
        }

        NodeKind::Return(values) => {
            for value in values {
                collect_references_from_expression(value, references);
            }
        }

        NodeKind::If(condition, then_body, else_body) => {
            collect_references_from_expression(condition, references);
            for statement in then_body {
                collect_references_from_ast_node(statement, references);
            }
            if let Some(else_body) = else_body {
                for statement in else_body {
                    collect_references_from_ast_node(statement, references);
                }
            }
        }

        NodeKind::Match(scrutinee, arms, default) => {
            collect_references_from_expression(scrutinee, references);
            for arm in arms {
                collect_references_from_expression(&arm.condition, references);
                for statement in &arm.body {
                    collect_references_from_ast_node(statement, references);
                }
            }
            if let Some(default_body) = default {
                for statement in default_body {
                    collect_references_from_ast_node(statement, references);
                }
            }
        }

        NodeKind::ForLoop(binding, range, body) => {
            collect_references_from_expression(&binding.value, references);
            collect_references_from_expression(&range.start, references);
            collect_references_from_expression(&range.end, references);
            if let Some(step) = &range.step {
                collect_references_from_expression(step, references);
            }
            for statement in body {
                collect_references_from_ast_node(statement, references);
            }
        }

        NodeKind::WhileLoop(condition, body) => {
            collect_references_from_expression(condition, references);
            for statement in body {
                collect_references_from_ast_node(statement, references);
            }
        }

        NodeKind::Warning(_)
        | NodeKind::Config(_)
        | NodeKind::Break
        | NodeKind::Continue
        | NodeKind::Slot
        | NodeKind::Empty
        | NodeKind::Operator(_)
        | NodeKind::Newline
        | NodeKind::Spaces(_) => {}
    }
}

#[derive(Clone)]
pub struct ScopeContext {
    pub kind: ContextKind,
    pub scope: InternedPath,
    pub declarations: Vec<Declaration>,
    pub returns: Vec<DataType>,
    pub host_registry: HostRegistry,
    pub loop_depth: usize,
}
#[derive(PartialEq, Clone)]
pub enum ContextKind {
    Module, // The top-level scope of each file in the module
    Expression,
    Constant, // An expression that is enforced to be evaluated at compile time and can't contain non-constant references
    Function,
    Condition, // For loops and if statements
    Loop,
    Branch,
    Template,
}

impl ScopeContext {
    pub fn new(
        kind: ContextKind,
        scope: InternedPath,
        declarations: &[Declaration],
        host_registry: HostRegistry,
        returns: Vec<DataType>,
    ) -> ScopeContext {
        ScopeContext {
            kind,
            scope,
            declarations: declarations.to_owned(),
            returns,
            host_registry,
            loop_depth: 0,
        }
    }

    pub fn new_child_control_flow(
        &self,
        kind: ContextKind,
        string_table: &mut StringTable,
    ) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = kind;
        if matches!(new_context.kind, ContextKind::Loop) {
            new_context.loop_depth += 1;
        }

        let scope_id = CONTROL_FLOW_SCOPE_COUNTER.fetch_add(1, Ordering::Relaxed);
        new_context.scope = self
            .scope
            .join_str(&format!("__scope_{}", scope_id), string_table);

        new_context
    }

    pub fn new_child_function(
        &self,
        id: StringId,
        signature: FunctionSignature,
        string_table: &mut StringTable,
    ) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Function;
        new_context.returns = signature.returns.to_owned();

        // Create a new scope path by joining the current scope with the function name
        new_context.scope = self.scope.append(id);
        new_context.loop_depth = 0;

        new_context.declarations = self.declarations.to_owned();
        new_context.declarations.extend(signature.parameters);

        new_context
    }

    pub fn new_child_expression(&self, returns: Vec<DataType>) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Expression;
        new_context.returns = returns;
        new_context
    }

    // Can also be a cheeky struct or enum or something
    pub fn new_constant(scope: InternedPath) -> ScopeContext {
        ScopeContext {
            kind: ContextKind::Constant,
            scope,
            declarations: Vec::new(),
            returns: Vec::new(),
            host_registry: HostRegistry::default(),
            loop_depth: 0,
        }
    }

    pub fn add_var(&mut self, arg: Declaration) {
        self.declarations.push(arg);
    }

    pub fn is_inside_loop(&self) -> bool {
        self.loop_depth > 0
    }
}

/// A new AstContext for scenes
///
/// Usage:
/// name (for the scope), args (declarations it can access)
#[macro_export]
macro_rules! new_template_context {
    ($context:expr) => {
        &ScopeContext {
            kind: ContextKind::Template,
            scope: $context.scope.clone(),
            declarations: $context.declarations.to_owned(),
            returns: vec![],
            host_registry: $context.host_registry.clone(),
            loop_depth: $context.loop_depth,
        }
    };
}

/// New Config AstContext
///
/// name (for scope), args (declarations it can reference)
#[macro_export]
macro_rules! new_config_context {
    ($name:expr, $args:expr, $registry:expr, $string_table:expr) => {{
        let mut scope = InternedPath::new();
        scope.push_str($name, $string_table);
        ScopeContext {
            kind: ContextKind::Template,
            scope,
            declarations: $args,
            returns: vec![],
            host_registry: $registry,
            loop_depth: 0,
        }
    }};
}

/// New Condition AstContext
///
/// name (for scope), args (declarations it can reference)
#[macro_export]
macro_rules! new_condition_context {
    ($name:expr, $args:expr, $registry:expr, $string_table:expr) => {{
        let mut scope = InternedPath::new();
        scope.push_str($name, $string_table);
        ScopeContext {
            kind: ContextKind::Condition,
            scope,
            declarations: $args,
            returns: vec![], //Empty because conditions are always booleans
            host_registry: $registry,
            loop_depth: 0,
        }
    }};
}
