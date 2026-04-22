running 1 test
test compiler_frontend::compiler_messages::display_messages::display_messages_tests::normalize_display_path_strips_windows_extended_prefix ... FAILED

failures:

---- compiler_frontend::compiler_messages::display_messages::display_messages_tests::normalize_display_path_strips_windows_extended_prefix stdout ----
Normalized path: "\\workspace\\main.bst"

thread 'compiler_frontend::compiler_messages::display_messages::display_messages_tests::normalize_display_path_strips_windows_extended_prefix' panicked at src\compiler_frontend\compiler_messages\tests\display_messages_tests.rs:183:5:
assertion `left == right` failed
  left: "\\workspace\\main.bst"
 right: "C:\\workspace\\main.bst"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    compiler_frontend::compiler_messages::display_messages::display_messages_tests::normalize_display_path_strips_windows_extended_prefix

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1097 filtered out; finished in 0.00s


Integration Tests:

Failures:
=====================================
  config_root_folders_absolute_path_rejected [html]
✗ UNEXPECTED SUCCESS
[expectation violation]
Expected ordered diagnostic message fragments were not found in any emitted error.

 (-_-)  🔥🔥🔥🔥 \code\beanstalk\tests\cases\config_root_folders_absolute_path_rejected\input\#config.bst 🔥🔥🔥🔥  <(^~^)/         
CONFIG FILE ISSUE-
Malformed config file, something doesn't make sense inside the project config)
Invalid '#root_folders' entry '/absolute/lib'. Root folders must be a single top-level folder name such as '@lib'.
Help: Use a single folder name like '@lib', not a nested path like '@lib/utils'

#root_folders = { "/absolute/lib" }
                  ^^^^^^^^^^^^^^^
-------------------------------------
  template_positional_named_mixed [html]
✗ FAIL
[normalized mismatch]
Golden output 'index.html' did not match after normalization.     
--- expected
+++ actual
-         __bs_read(bst___hir_tmp_N_lN).push("title= Mixed Example\npositional=P1\ndefault=\nBody Text");
+         __bs_read(bst___hir_tmp_N_lN).push("\r\ntitle= Mixed Example\r\npositional=P1\r\ndefault=\r\n    \r\n    Body Text\r");   
-------------------------------------
  template_positional_slot_children_body [html]
✗ FAIL
[normalized mismatch]
Golden output 'index.html' did not match after normalization.     
--- expected
+++ actual
-         __bs_read(bst___hir_tmp_N_lN).push("H:  First\n\nR:  Second\nR:  Third");
+         __bs_read(bst___hir_tmp_N_lN).push("\r\n    \r\n    H:  First\r\n    \r\n    R:  Second\r\n    R:  Third\r\r");
-------------------------------------


=====================================
Test Results Summary. Took: 7.6309596s

  Total tests:             382
  Successful compilations: 231
  Failed compilations:     2
  Expected failures:       148
  Unexpected successes:    1

  Incorrect results: 3 / 382

  Backend breakdown:
  ───────────────────────────────────
    html       total: 363  passed: 360  failed: 3
    html_wasm  total: 19  passed: 19

99.2 % of tests behaved as expected
=====================================

