//! Shared HTML builder test support.
//!
//! WHAT: centralises the small HIR/module fixtures and artifact helpers used across the
//!       HTML builder tests.
//! WHY: the refactor split tests by module responsibility, so common scaffolding should
//!      live in one place instead of being redefined in every test file.

use crate::build_system::build::{FileKind, Module, OutputFile};
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirBlock, HirExpression, HirExpressionKind, HirFunction,
    HirFunctionOrigin, HirModule, HirNodeId, HirRegion, HirStatement, HirStatementKind,
    HirTerminator, RegionId, ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::{CompileTimePathBase, CompileTimePathKind};
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::path::PathBuf;
use std::time::SystemTime;

/// Build a unique temp-directory path for tests that need a small on-disk project layout.
pub(crate) fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_html_builder_{prefix}_{unique}"))
}

/// Create the smallest valid HIR module with one entry start function.
pub(crate) fn create_test_hir_module() -> HirModule {
    let mut module = HirModule::new();
    let mut type_context = TypeContext::default();
    let unit_type = type_context.insert(HirType {
        kind: HirTypeKind::Unit,
    });

    module.type_context = type_context;
    module.regions = vec![HirRegion::lexical(RegionId(0), None)];
    module.blocks = vec![HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(HirExpression {
            id: crate::compiler_frontend::hir::hir_nodes::HirValueId(0),
            kind: HirExpressionKind::TupleConstruct { elements: vec![] },
            ty: unit_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        }),
    }];
    module.functions = vec![HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: unit_type,
        return_aliases: vec![],
    }];
    module.start_function = FunctionId(0);
    module
        .function_origins
        .insert(FunctionId(0), HirFunctionOrigin::EntryStart);

    module
}

/// Wrap the base HIR fixture in the build-system `Module` shape used by the HTML builder.
///
/// WHAT: binds the test module's names into the caller-owned shared string table.
/// WHY: HTML builder tests now need the same one-table diagnostic model as production builds.
pub(crate) fn create_test_module(entry_point: PathBuf, string_table: &mut StringTable) -> Module {
    let mut hir_module = create_test_hir_module();
    hir_module.side_table.bind_function_name(
        FunctionId(0),
        InternedPath::from_single_str("start_entry", string_table),
    );

    Module {
        entry_point,
        hir: hir_module,
        borrow_analysis: BorrowCheckReport::default(),
        warnings: vec![],
    }
}

pub(crate) fn interned_path(string_table: &mut StringTable, components: &[&str]) -> InternedPath {
    let mut path = InternedPath::new();
    for component in components {
        path.push_str(component, string_table);
    }
    path
}

pub(crate) fn rendered_path_usage(
    string_table: &mut StringTable,
    source_path_components: &[&str],
    public_path_components: &[&str],
    filesystem_path: PathBuf,
    base: CompileTimePathBase,
    kind: CompileTimePathKind,
    source_file_scope_components: &[&str],
    line_number: i32,
) -> RenderedPathUsage {
    let scope = interned_path(string_table, source_file_scope_components);
    RenderedPathUsage {
        source_path: interned_path(string_table, source_path_components),
        filesystem_path,
        public_path: interned_path(string_table, public_path_components),
        base,
        kind,
        source_file_scope: scope.clone(),
        render_location: SourceLocation::new(
            scope,
            crate::compiler_frontend::tokenizer::tokens::CharPosition {
                line_number,
                char_column: 1,
            },
            crate::compiler_frontend::tokenizer::tokens::CharPosition {
                line_number,
                char_column: 10,
            },
        ),
    }
}

/// Add a callable zero-argument unit-returning function to the test module.
pub(crate) fn add_callable_function(
    module: &mut Module,
    function_id: FunctionId,
    name: &str,
    string_table: &mut StringTable,
) {
    let unit_type = module.hir.functions[0].return_type;
    let block_id = BlockId(module.hir.blocks.len() as u32);
    let value_id =
        crate::compiler_frontend::hir::hir_nodes::HirValueId(module.hir.blocks.len() as u32 + 10);

    module.hir.blocks.push(HirBlock {
        id: block_id,
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(HirExpression {
            id: value_id,
            kind: HirExpressionKind::TupleConstruct { elements: vec![] },
            ty: unit_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        }),
    });
    module.hir.functions.push(HirFunction {
        id: function_id,
        entry: block_id,
        params: vec![],
        return_type: unit_type,
        return_aliases: vec![],
    });
    module
        .hir
        .function_origins
        .insert(function_id, HirFunctionOrigin::Normal);
    module.hir.side_table.bind_function_name(
        function_id,
        InternedPath::from_single_str(name, string_table),
    );
}

/// Append a start-function call targeting an already-registered function name.
pub(crate) fn add_start_call(
    module: &mut Module,
    target_name: &str,
    statement_id: u32,
    string_table: &mut StringTable,
) {
    let target_path = InternedPath::from_single_str(target_name, string_table);
    let target_function_id = module
        .hir
        .functions
        .iter()
        .find_map(|function| {
            module
                .hir
                .side_table
                .function_name_path(function.id)
                .filter(|path| **path == target_path)
                .map(|_| function.id)
        })
        .expect("target function should exist in HIR side table");
    let start_block = module
        .hir
        .blocks
        .iter_mut()
        .find(|block| block.id == BlockId(0))
        .expect("start block should exist");
    start_block.statements.push(HirStatement {
        id: HirNodeId(statement_id),
        kind: HirStatementKind::Call {
            target: CallTarget::UserFunction(target_function_id),
            args: vec![],
            result: None,
        },
        location: SourceLocation::default(),
    });
}

/// Collect output paths so tests can assert artifact layout without repeating iterator plumbing.
pub(crate) fn collect_output_paths(output_files: &[OutputFile]) -> Vec<PathBuf> {
    output_files
        .iter()
        .map(|file| file.relative_output_path().to_path_buf())
        .collect()
}

/// Extract an emitted HTML artifact by relative path.
pub(crate) fn expect_html_output<'a>(
    output_files: &'a [OutputFile],
    relative_path: &str,
) -> &'a str {
    let expected_path = PathBuf::from(relative_path);
    output_files
        .iter()
        .find_map(|file| match file.file_kind() {
            FileKind::Html(content) if file.relative_output_path() == expected_path.as_path() => {
                Some(content.as_str())
            }
            _ => None,
        })
        .expect("expected HTML output artifact")
}

/// Extract an emitted JS artifact by relative path.
pub(crate) fn expect_js_output<'a>(output_files: &'a [OutputFile], relative_path: &str) -> &'a str {
    let expected_path = PathBuf::from(relative_path);
    output_files
        .iter()
        .find_map(|file| match file.file_kind() {
            FileKind::Js(content) if file.relative_output_path() == expected_path.as_path() => {
                Some(content.as_str())
            }
            _ => None,
        })
        .expect("expected JS output artifact")
}

/// Extract an emitted binary artifact by relative path.
pub(crate) fn expect_bytes_output<'a>(
    output_files: &'a [OutputFile],
    relative_path: &str,
) -> &'a [u8] {
    let expected_path = PathBuf::from(relative_path);
    output_files
        .iter()
        .find_map(|file| match file.file_kind() {
            FileKind::Bytes(bytes) if file.relative_output_path() == expected_path.as_path() => {
                Some(bytes.as_slice())
            }
            _ => None,
        })
        .expect("expected binary output artifact")
}

/// Assert the basic full-document shell contract shared by all HTML builder outputs.
pub(crate) fn assert_has_basic_shell(html: &str) {
    for required_fragment in ["<!DOCTYPE html>", "<head>", "<body", "</body>", "</html>"] {
        assert!(
            html.contains(required_fragment),
            "expected HTML output to contain '{required_fragment}'"
        );
    }
}

/// Assert that a fragment appears before the closing body tag.
pub(crate) fn assert_fragment_before_body_close(html: &str, fragment: &str) {
    let fragment_pos = html.find(fragment).expect("expected fragment to exist");
    let body_close = html.find("</body>").expect("expected </body> to exist");
    assert!(
        fragment_pos < body_close,
        "expected '{fragment}' to appear before </body>"
    );
}
