# Contributing to this project
This file is just being used for some development notes for now.

The project isn't stable enough to warrant pull requests for the most part,
but if you are interested in compilers/programming languages or the goals of this language,
or want to contribute soon - please get in touch.

You can contact me at: Nyejamesmusic@gmail.com

Any suggestions or questions about the future / design of this language are welcome!

## Commands:
*Basic Compiler test file build*
cargo run build tests/cases/test.bs

*Full info from the cli command*
cargo run --features "show_char_stream,show_tokens,verbose_ast_logging,verbose_eval_logging,verbose_codegen_logging" -- build tests/cases/test.bs

*The stuff that is actually useful to see for AST generation*
cargo run --features "verbose_ast_logging,verbose_eval_logging,verbose_codegen_logging" -- build tests/cases/test.bs

*After AST info*
cargo run --features "verbose_eval_logging,verbose_codegen_logging" -- build tests/cases/test.bs