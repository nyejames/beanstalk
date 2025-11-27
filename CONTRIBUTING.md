# Contributing to this project
This project is at a very early stage.

If you are interested in compilers/programming languages or the goals of this language,
or want to contribute soon - please get in touch or open a discussion on Github.

Any suggestions or questions about the future / design of this language are welcome. If you think you can help out or are interested in implementing fresh, ambitious codegen or borrow checker strategies then get involved!

See `docs/Beanstalk Language Overview.md` and `docs/Beanstalk Compiler Development Guide.md` for more in depth details.

## Commands:
The two basic commands for running and building files are "run" and "build".

Run will JIT compile and execute the file, build will create the output files.

The timer features are useful for seeing the basic control flow, even when you're not benchmarking.

*Basic Compiler test files jit - just running the nice little test file*
cargo run --features "detailed_timers" run tests/cases/test.bst

*Basic Compiler test file build - running all test cases (including fails)*
cargo run --features "detailed_timers" run tests

## Compile Features for logging out info
- detailed_timers
- show_char_stream
- show_tokens
- verbose_ast_logging
- verbose_eval_logging
- verbose_ir_logging
- verbose_codegen_logging

## Examples of using logging with the default test file
*All possible info from the cli command*
cargo run --features "show_char_stream,show_tokens,verbose_ast_logging,verbose_eval_logging,verbose_ir_logging,verbose_codegen_logging, detailed_timers" -- run tests/cases/test.bst

*The stuff that is actually useful to see for AST debugging*
cargo run --features "verbose_ast_logging,verbose_eval_logging,verbose_ir_logging" -- run tests/cases/test.bst

*Useful for backend debugging*
cargo run --features "verbose_ir_logging,verbose_codegen_logging" -- run tests/cases/test.bst

*Dumb benchmarking*
run --color=always --features detailed_timers --profile release -- build tests/cases/test.bst