//! Shared frontend test utilities.
//!
//! WHAT: provides low-churn helpers reused across frontend subsystem tests.
//! WHY: path-resolution setup and source location construction are identical in several suites
//!      and should stay consistent.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::analysis::borrow_checker::{BorrowCheckReport, check_borrows};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind, SourceLocation};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::{Ast, AstBuildContext};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::test_support::register_test_external_function;
use crate::compiler_frontend::headers::parse_file_headers::{HeaderParseOptions, parse_headers};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::module_dependencies::resolve_module_dependencies;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, TokenizeMode};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

pub(crate) fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(
        cwd.clone(),
        cwd,
        &[],
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

fn parse_single_file_ast_result(source: &str) -> Result<(Ast, StringTable), CompilerError> {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let external_package_registry = ExternalPackageRegistry::new();
    let file_path = InternedPath::from_single_str("#page.bst", &mut string_table);
    let file_tokens = tokenize(
        source,
        &file_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )?;

    let mut warnings = Vec::new();
    let headers = parse_headers(
        vec![file_tokens],
        &external_package_registry,
        &mut warnings,
        &std::path::PathBuf::from("#page.bst"),
        HeaderParseOptions {
            entry_file_id: None,
            project_path_resolver: Some(test_project_path_resolver()),
            path_format_config: PathStringFormatConfig::default(),
            style_directives: style_directives.clone(),
        },
        &mut string_table,
    )
    .map_err(|mut errors| errors.remove(0))?;

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .map_err(|mut errors| errors.remove(0))?;

    let entry_path = InternedPath::from_single_str("#page.bst", &mut string_table);
    let ast = Ast::new(
        sorted.headers,
        sorted.top_level_const_fragments,
        sorted.module_symbols,
        AstBuildContext {
            external_package_registry: &external_package_registry,
            style_directives: &style_directives,
            string_table: &mut string_table,
            entry_dir: entry_path,
            build_profile: FrontendBuildProfile::Dev,
            project_path_resolver: Some(test_project_path_resolver()),
            path_format_config: PathStringFormatConfig::default(),
        },
    )
    .map_err(|mut messages| messages.errors.remove(0))?;

    Ok((ast, string_table))
}

pub(crate) fn parse_single_file_ast(source: &str) -> (Ast, StringTable) {
    parse_single_file_ast_result(source).expect("source should parse into AST")
}

pub(crate) fn parse_single_file_ast_error(source: &str) -> CompilerError {
    match parse_single_file_ast_result(source) {
        Ok(_) => panic!("source should fail during frontend parsing"),
        Err(error) => error,
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
        value: Expression::new(ExpressionKind::NoValue, location, data_type, value_mode),
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

pub(crate) fn fresh_returns(result_types: Vec<DataType>) -> Vec<ReturnSlot> {
    result_types
        .into_iter()
        .map(FunctionReturn::Value)
        .map(ReturnSlot::success)
        .collect()
}

pub(crate) fn runtime_function_call_node(
    name: InternedPath,
    result_types: Vec<DataType>,
    location: SourceLocation,
) -> AstNode {
    node(
        NodeKind::FunctionCall {
            name,
            args: vec![],
            result_types,
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
    location: SourceLocation,
) -> Expression {
    Expression::reference(name, data_type, location, ValueMode::ImmutableReference)
}

pub(crate) fn assignment_target(
    name: InternedPath,
    data_type: DataType,
    location: SourceLocation,
) -> AstNode {
    node(
        NodeKind::Rvalue(Expression::reference(
            name,
            data_type,
            location.clone(),
            ValueMode::MutableReference,
        )),
        location,
    )
}
pub(crate) fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    Ast {
        nodes,
        module_constants: vec![],
        doc_fragments: vec![],
        entry_path,
        const_top_level_fragments: vec![],
        rendered_path_usages: vec![],
        warnings: vec![],
        choice_definitions: vec![],
    }
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
    HirBuilder::new(string_table, PathStringFormatConfig::default())
        .build_hir_module(ast)
        .expect("HIR lowering should succeed")
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
                crate::compiler_frontend::external_packages::ExternalAbiType::I32,
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
) -> Result<BorrowCheckReport, CompilerError> {
    check_borrows(module, external_package_registry, string_table)
}
