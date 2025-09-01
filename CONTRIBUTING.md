# Contributing to this project
This project is at a very early stage.

If you are interested in compilers/programming languages or the goals of this language,
or want to contribute soon - please get in touch.

You can contact me at: Nyejamesmusic@gmail.com

Any suggestions or questions about the future / design of this language are welcome. If you think you can help out or are interested in implementing fresh, ambitious codegen or borrow checker strategies then get involved!

## Commands:
*Basic Compiler test file build*
cargo run build tests/cases/test.bst

*Full info from the cli command*
cargo run --features "show_char_stream,show_tokens,verbose_ast_logging,verbose_eval_logging,verbose_codegen_logging" -- build tests/cases/test.bst

*The stuff that is actually useful to see for AST generation*
cargo run --features "verbose_ast_logging,verbose_eval_logging,verbose_codegen_logging" -- build tests/cases/test.bst

*After AST info*
cargo run --features "verbose_eval_logging,verbose_codegen_logging" -- build tests/cases/test.bst