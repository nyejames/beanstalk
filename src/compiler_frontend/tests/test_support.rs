//! Shared frontend test utilities.
//!
//! WHAT: provides low-churn helpers reused across frontend subsystem tests.
//! WHY: path-resolution setup and source location construction are identical in several suites
//!      and should stay consistent.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::analysis::borrow_checker::{
    BorrowCheckError, BorrowCheckReport, check_borrows,
};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind, SourceLocation};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnChannel, ReturnSlot,
};
use crate::compiler_frontend::ast::{Ast, AstBuildContext, AstBuildInput};
use crate::compiler_frontend::compiler_errors::{CompilerError, compiler_error_to_diagnostic};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::datatypes::{DataType, TypeId};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::test_support::register_test_external_function;
use crate::compiler_frontend::headers::parse_file_headers::{
    HeaderParseOptions, parse_headers, prepare_file_from_tokens,
};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::module_dependencies::resolve_module_dependencies;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, TokenizeMode};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use crate::projects::settings::{DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS, IMPLICIT_START_FUNC_NAME};

pub(crate) fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(
        cwd.clone(),
        cwd,
        &crate::libraries::SourceLibraryRegistry::default(),
    )
    .expect("test path resolver should be valid")
}

/// Creates a single-line `SourceLocation` at the given line number for use in test fixtures.
///
/// WHAT: produces a deterministic source location with an arbitrary column span.
/// WHY: many test suites construct locations for the same reason; one canonical helper prevents
///      each suite from defining its own with slightly different shapes.
pub(crate) fn test_source_location(line: i32) -> SourceLocation {
    SourceLocation {
        scope: InternedPath::new(),
        start_pos: CharPosition {
            line_number: line,
            char_column: 0,
        },
        end_pos: CharPosition {
            line_number: line,
            char_column: 120,
        },
    }
}

fn parse_single_file_ast_result(
    source: &str,
) -> Result<(Ast, StringTable), Box<CompilerDiagnostic>> {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let external_package_registry = ExternalPackageRegistry::new();
    let file_path = std::path::PathBuf::from("#page.bst");

    let options = HeaderParseOptions {
        entry_file_id: None,
        project_path_resolver: Some(test_project_path_resolver()),
    };

    let interned_path = InternedPath::from_path_buf(&file_path, &mut string_table);
    let file_tokens = tokenize(
        source,
        &interned_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
        None,
    )
    .map_err(Box::new)?;

    let output = prepare_file_from_tokens(
        file_tokens,
        &file_path,
        &options,
        &external_package_registry,
        &mut string_table,
        0,
        0,
    )
    .map_err(|error| error.diagnostic)?;

    let headers = parse_headers(
        vec![output],
        &external_package_registry,
        &ExternalImportResolutionTable::default(),
        options.project_path_resolver.as_ref(),
        &mut string_table,
    )
    .map_err(|bag| {
        Box::new(
            bag.into_diagnostics()
                .into_iter()
                .next()
                .unwrap_or_else(|| {
                    compiler_error_to_diagnostic(&CompilerError::compiler_error(
                        "unknown header parsing error",
                    ))
                }),
        )
    })?;

    let sorted = resolve_module_dependencies(headers, &mut string_table).map_err(|bag| {
        Box::new(
            bag.into_diagnostics()
                .into_iter()
                .next()
                .unwrap_or_else(|| {
                    compiler_error_to_diagnostic(&CompilerError::compiler_error(
                        "unknown dependency sorting error",
                    ))
                }),
        )
    })?;

    let entry_path = InternedPath::from_single_str("#page.bst", &mut string_table);
    let ast = Ast::new(
        AstBuildInput {
            headers: sorted.headers,
            module_symbols: sorted.module_symbols,
            import_environment: sorted.import_environment,
            top_level_const_fragments: sorted.top_level_const_fragments,
        },
        AstBuildContext {
            external_package_registry: &external_package_registry,
            style_directives: &style_directives,
            string_table: &mut string_table,
            entry_dir: entry_path,
            build_profile: FrontendBuildProfile::Dev,
            project_path_resolver: Some(test_project_path_resolver()),
            path_format_config: PathStringFormatConfig::default(),
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        },
    )
    .map_err(|messages| {
        // Prefer typed diagnostics over legacy errors.
        if let Some(diagnostic) = messages.first_error() {
            Box::new(diagnostic.clone())
        } else {
            panic!("frontend parsing failed without an error diagnostic")
        }
    })?;

    Ok((ast, string_table))
}

pub(crate) fn parse_single_file_ast(source: &str) -> (Ast, StringTable) {
    parse_single_file_ast_result(source).expect("source should parse into AST")
}

pub(crate) fn parse_single_file_ast_diagnostic(source: &str) -> CompilerDiagnostic {
    match parse_single_file_ast_result(source) {
        Ok(_) => panic!("source should fail during frontend parsing"),
        Err(diagnostic) => *diagnostic,
    }
}

pub(crate) fn function_node_by_name<'a>(
    ast: &'a Ast,
    string_table: &StringTable,
    name: &str,
) -> &'a AstNode {
    ast.nodes
        .iter()
        .find(|node| match &node.kind {
            NodeKind::Function(path, ..) => path.name_str(string_table) == Some(name),
            _ => false,
        })
        .unwrap_or_else(|| panic!("expected function '{name}' in AST"))
}

pub(crate) fn function_signature_by_name<'a>(
    ast: &'a Ast,
    string_table: &StringTable,
    name: &str,
) -> &'a FunctionSignature {
    let node = function_node_by_name(ast, string_table, name);
    match &node.kind {
        NodeKind::Function(_, signature, _) => signature,
        _ => unreachable!("function lookup should only return function nodes"),
    }
}

pub(crate) fn function_body_by_name<'a>(
    ast: &'a Ast,
    string_table: &StringTable,
    name: &str,
) -> &'a [AstNode] {
    let node = function_node_by_name(ast, string_table, name);
    match &node.kind {
        NodeKind::Function(_, _, body) => body,
        _ => unreachable!("function lookup should only return function nodes"),
    }
}

pub(crate) fn start_function_body<'a>(ast: &'a Ast, string_table: &StringTable) -> &'a [AstNode] {
    function_body_by_name(ast, string_table, IMPLICIT_START_FUNC_NAME)
}

pub(crate) fn test_location(line: i32) -> SourceLocation {
    test_source_location(line)
}

pub(crate) fn node(kind: NodeKind, location: SourceLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

pub(crate) fn make_test_variable(name: InternedPath, value: Expression) -> Declaration {
    Declaration { id: name, value }
}

pub(crate) fn param(
    name: InternedPath,
    data_type: DataType,
    id: TypeId,
    mutable: bool,
    location: SourceLocation,
) -> Declaration {
    let value_mode = if mutable {
        ValueMode::MutableOwned
    } else {
        ValueMode::ImmutableOwned
    };

    Declaration {
        id: name,
        value: Expression::new(ExpressionKind::NoValue, location, id, data_type, value_mode),
    }
}

pub(crate) fn function_node(
    name: InternedPath,
    signature: FunctionSignature,
    body: Vec<AstNode>,
    location: SourceLocation,
) -> AstNode {
    node(NodeKind::Function(name, signature, body), location)
}

pub(crate) fn fresh_success_returns(result_type_ids: Vec<TypeId>) -> Vec<ReturnSlot> {
    result_type_ids
        .into_iter()
        .map(|type_id| ReturnSlot {
            value: FunctionReturn::Value(DataType::Inferred),
            type_id: Some(type_id),
            channel: ReturnChannel::Success,
        })
        .collect()
}

pub(crate) fn runtime_function_call_node(
    name: InternedPath,
    result_type_ids: Vec<TypeId>,
    location: SourceLocation,
) -> AstNode {
    node(
        NodeKind::FunctionCall {
            name,
            args: vec![],
            result_type_ids,
            location: location.clone(),
        },
        location,
    )
}

pub(crate) fn runtime_operator_node(operator: Operator, location: SourceLocation) -> AstNode {
    node(NodeKind::Operator(operator), location)
}

pub(crate) fn symbol(name: &str, string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(name, string_table)
}
pub(crate) fn reference_expr(
    name: InternedPath,
    data_type: DataType,
    id: TypeId,
    location: SourceLocation,
) -> Expression {
    Expression::reference_with_type_id(
        name,
        data_type,
        id,
        location,
        ValueMode::ImmutableReference,
        crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState::RuntimeValue,
    )
}

pub(crate) fn assignment_target(
    name: InternedPath,
    data_type: DataType,
    id: TypeId,
    location: SourceLocation,
) -> AstNode {
    node(
        NodeKind::Rvalue(Expression::reference_with_type_id(
            name,
            data_type,
            id,
            location.clone(),
            ValueMode::MutableReference,
            crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState::RuntimeValue,
        )),
        location,
    )
}
pub(crate) fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    crate::compiler_frontend::hir::hir_builder::build_ast(nodes, entry_path)
}

pub(crate) fn entry_and_start(string_table: &mut StringTable) -> (InternedPath, InternedPath) {
    let entry_path = InternedPath::from_single_str("main.bst", string_table);
    let start_name = entry_path.join_str(IMPLICIT_START_FUNC_NAME, string_table);
    (entry_path, start_name)
}

pub(crate) fn lower_hir(
    ast: Ast,
    string_table: &mut StringTable,
) -> crate::compiler_frontend::hir::module::HirModule {
    let type_environment = ast.type_environment.clone();
    let (module, _) = HirBuilder::new(
        string_table,
        PathStringFormatConfig::default(),
        type_environment,
    )
    .build_hir_module(ast)
    .expect("HIR lowering should succeed");
    module
}

pub(crate) fn default_external_package_registry(
    _string_table: &mut StringTable,
) -> ExternalPackageRegistry {
    ExternalPackageRegistry::new()
}

pub(crate) fn register_external_function(
    registry: &mut ExternalPackageRegistry,
    name: &'static str,
    param_access: Vec<
        crate::compiler_frontend::external_packages::test_support::TestExternalAccessKind,
    >,
    return_alias: crate::compiler_frontend::external_packages::test_support::TestExternalReturnAlias,
    return_type: crate::compiler_frontend::external_packages::test_support::TestExternalAbiType,
) -> crate::compiler_frontend::external_packages::ExternalFunctionId {
    let parameters = param_access
        .into_iter()
        .map(|access_kind| {
            (
                crate::compiler_frontend::external_packages::ExternalSignatureType::Abi(
                    crate::compiler_frontend::external_packages::ExternalAbiType::I32,
                ),
                access_kind,
            )
        })
        .collect::<Vec<_>>();
    register_test_external_function(registry, name, parameters, return_alias, return_type)
        .expect("external function registration should succeed")
}

pub(crate) fn run_borrow_checker(
    module: &crate::compiler_frontend::hir::module::HirModule,
    external_package_registry: &ExternalPackageRegistry,
    string_table: &StringTable,
) -> Result<BorrowCheckReport, BorrowCheckError> {
    check_borrows(module, external_package_registry, string_table)
}
