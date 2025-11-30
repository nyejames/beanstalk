# Contributing to this project
This project is at a very early stage.

If you are interested in compilers/programming languages or the goals of this language 
or want to contribute soon, please get in touch or open a discussion on GitHub.

Any suggestions or questions about the future / design of this language are welcome. 
If you think you can help out or are interested in implementing fresh, ambitious codegen, build system or borrow checker strategies, then get involved!

See `docs/Beanstalk Language Overview.md` and `docs/Beanstalk Compiler Development Guide.md` for more in depth details.

## Commands:
The two basic commands for running and building files are "run" and "build".

Run will JIT compile and execute the file, build will create the output files.

The timer features are useful for seeing the basic control flow, even when you're not benchmarking.

**Basic Compiler test files jit - just running the nice little test file**

cargo run --features "detailed_timers" run tests/cases/test.bst

**Basic Compiler test file build - running all test cases (including fails)**

cargo run --features "detailed_timers" run tests

**Dumb benchmarking**

run --color=always --features detailed_timers --profile release -- build tests/cases/test.bst