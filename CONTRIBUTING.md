# Contributing to this project
This project is at a very early stage.

If you are interested in compilers/programming languages or the goals of this language 
and want to contribute or make suggestions, please get in touch or open a discussion on GitHub.

Any suggestions or questions about the future / design of this language are welcome.

See `docs/Beanstalk Language Overview.md` and `docs/Beanstalk Compiler Development Guide.md` for more in depth details.

## Best starting points for contribution
- Discussion and review of the technical design is welcome
- Backend is in very active development, so a review of the lifetime inference / borrow checker and IR code would be very helpful
- Build system for embedded Rust projects and other build system tooling needs work as a priority

## Commands:
**Basic Compiler test files**

cargo run --features "detailed_timers" build tests/cases/test.bst

**Basic Compiler test file build - running all test cases (including fails)**

cargo run --features "detailed_timers" tests