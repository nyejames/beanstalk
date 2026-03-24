use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;

pub(crate) fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(cwd.clone(), cwd, &[]).expect("test path resolver should be valid")
}

mod hir_expression_lowering_tests;
mod hir_function_origin_tests;
mod hir_statement_lowering_tests;
mod hir_validation_tests;
