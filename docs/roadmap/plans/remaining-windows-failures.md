thread 'compiler_frontend::compiler_messages::display_messages::display_messages_tests::normalize_display_path_strips_windows_extended_prefix' panicked at src\compiler_frontend\compiler_messages\tests\display_messages_tests.rs:182:5:
assertion `left == right` failed
  left: "C:workspace\\main.bst"
 right: "C:\\workspace\\main.bst"

---- projects::dev_server::error_page::tests::compiler_error_page_links_to_project_relative_resolved_source_path stdout ----
EXPECTED! file:///C:/Users/NyeJames/AppData/Local/Temp/beanstalk_relative_path_16844_1776959955658470500_115/src/docs/guide.bst

thread 'projects::dev_server::error_page::tests::compiler_error_page_links_to_project_relative_resolved_source_path' panicked at src\projects\dev_server\tests\error_page_tests.rs:102:5:
assertion failed: page.contains(&format!("href=\"{expected_href}\""))    


failures:
    build_system::build::tests::build_import_tests::build_project_keeps_one_shared_string_table_for_multi_module_diagnostics
    compiler_frontend::compiler_messages::display_messages::display_messages_tests::normalize_display_path_strips_windows_extended_prefix
    projects::dev_server::error_page::tests::compiler_error_page_links_to_project_relative_resolved_source_path
