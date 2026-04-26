# Beanstalk Roadmap
This is the main todo list for the language and compiler.

The current major goal is getting to a healthy alpha stage.
Each plan or PR that is needed will be linked here.

Use the language surface integration matrix as a reference for what is currently implemented: `docs/src/docs/progress/#page.bst`

---

# Plans / Notes / TODOS

- builtin `Error` enrichment beyond what is already required for the current compiler/runtime surface
- full tagged unions
- full pattern-matching design (capture patterns)
- full traits implementation
- Closures
- Hash Maps
- Compile time arbitary precision aritmetic + Decimals Type support
- Core Math library
- Optimised template folding
- Move to more specific explicit type declarations for numbers (I32, I64, F32, F64) - JS backend just makes all an F64 and accepts the precision loss, more for future Wasm backend.

## Wasm

Broader Wasm maturity beyond the current experimental path.

### Notes and limitations from previous investigations
- The WASM backend can't handle Choice/Union types yet (maps to Handle but produces i32/i64 mismatches). 
- Exponents (requires imported Math library)
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.

## Rust Interpreter
- Unimplmented - mostly just scaffolding
- primary goal: for basic CTFE as a release mode optimization step after HIR generation
- long term goal (noted here to not forget the idea): will work like MIRI to enable a special `checked: .. ;` blocks.
In beanstalk these would HAVE to be fully evaluated (would not be actually unsafe, just more heavily verified), but would run this additional advanced checking through those blocks specifically. Tradeoff of slower compile-times for using these special blocks, but gain more control. Rust interpreter means Beanstalk can do much more sophisticated analysis to prove the block is safe, allowing more flexible code patterns and faster runtime code.
