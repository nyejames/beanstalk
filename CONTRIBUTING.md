# Contributing to this project
This project is at a very early stage with long-term goals.

If you are interested in compilers/programming languages or the goals of this language 
and want to contribute or make suggestions, please get in touch or open a discussion on GitHub.

Any questions about the future / design of this language are welcome.
Open a discussion on GitHub if you're curious.

## The Current Goal

To see the progress and current priority goals of the compiler and language, see `docs/roadmap/roadmap.md`.

See [the language overview](docs/language-overview.md) and
[the compiler overview](docs/compiler-design-overview.md) for more details about the language
itself.

New code contributions must follow the [codebase style guide](docs/codebase-style-guide.md).

## Testing

Run the compiler integration suite with `cargo run -- tests`.
Alternatively, run `just validate` to execute the full validation suite (clippy, unit tests, integration tests, docs build, and speed test). 
You must have `just` installed to run this.

New integration fixtures should use the canonical `tests/cases/<case>/input + expect.toml` layout.
An optional `tests/cases/manifest.toml` can define case ordering and tags during fixture migrations.

## Benchmarking

The project includes a Rust-only benchmark system for rough compiler performance sanity checks. The system executes the release `bean` binary against defined benchmark cases, records local timing data, and writes terse public summaries.

### Quick Start

```bash
just bench-check   # Run without recording: 1 warmup, 10 iterations (for validation/CI)
just bench         # Run and record: 1 warmup, 10 iterations (for comprehensive measurements)
```

`just bench-check` is safe for validation because it does not create or update benchmark history, system identity files, old benchmark archives, or tracked summary files. It still builds the release compiler, so Cargo may update `target/` as part of the normal build.

### How It Works

The benchmark system:

1. **Builds the compiler** with `cargo build --release --features detailed_timers`
2. **Parses benchmark cases** from `benchmarks/cases.txt` (group directives plus `<command> <arg1> <arg2>`)
3. **Executes benchmarks** with a warmup run followed by 10 measured iterations
4. **Records timing data** to local raw history (`benchmarks/local-data/`) and appends a concise monthly Markdown summary (`benchmarks/summaries/`)

### Benchmark Cases

The system benchmarks various compiler operations defined in `benchmarks/cases.txt`:
- **Core benchmarks**: `check` and `build` operations on `speed-test.bst`
- **Docs benchmarks**: `check docs`
- **Stress tests**: Template system, type system, constant folding, pattern matching, and collection operations
- **Module benchmarks**: A multi-file module/import/dependency graph fixture
- **Borrow benchmarks**: Valid borrow/exclusivity paths

Each stress test file exercises specific compiler subsystems to identify performance bottlenecks.

### Output Structure

Results are stored in two locations:

- **`benchmarks/local-data/`** (local-only, ignored): raw per-run JSONL history and system identity.
- **`benchmarks/summaries/`** (tracked): concise monthly Markdown summaries with per-run entries.

### Understanding Results

The monthly Markdown summary includes:
- **Initial benchmark**: first recorded run for your system this month
- **Latest benchmark**: most recent recorded run for your system this month
- **Change since initial**: shared-case movement between latest and initial
- **Group averages**: absolute averages for `all` cases and each benchmark group
- **Case spread latest**: spread across different benchmark cases, not timing uncertainty

Per-run entries show the change relative to the previous run on the same system:

```markdown
# macOS M1 (B7F2): May 10th - 15:21
**-4ms avg**; 5 faster, 0 slower; 8/8 cases
Avg: all ~41ms, core ~72ms, docs ~71ms, stress ~9ms
```

`no measurable change` means no overlapping case exceeded the rough per-case threshold. `mixed` means at least one case became meaningfully faster and at least one case became meaningfully slower. `case set changed` means added or removed cases affected comparability, so summary lines report how many cases overlapped.

### Adding New Benchmarks

Edit `benchmarks/cases.txt` to add new benchmark cases:

```
# Comment lines start with #
# group: core
check path/to/file.bst
build path/to/file.bst

# group: docs
check docs
```

The system automatically:
- Skips comment lines (starting with `#`) and empty lines
- Applies `# group: <name>` labels to following cases
- Handles multiple spaces as single separators
- Supports quoted arguments for paths with spaces

New benchmark cases should be valid end-to-end programs or projects. Do not add negative diagnostic tests here; those belong in the compiler integration suite.

Benchmark project fixtures should commit only source inputs. Generated `dev/` and `release/` output directories are ignored and must not be committed.

### Design Principles

- **Zero external dependencies**: Uses only Rust stdlib (no clap, serde, criterion)
- **Cross-platform**: Works on macOS, Linux, and Windows
- **Fail-fast**: Fail-fast with no partial benchmark writes
- **Isolated tooling**: Implemented in separate `xtask/` workspace crate

### Use Cases

- **Performance regression detection**: Compare results over time using recorded summaries
- **Optimization validation**: Verify that compiler changes improve targeted areas
- **CI integration**: Use `just bench-check` for fast automated performance checks
- **Bottleneck identification**: Stress tests reveal which subsystems need optimization

## New contributions
If you are thinking of contributing, start with something small that is easy to read and review and follow the style guide closely. Reliability and modularity are *TOP PRIORITY* in this codebase. 
90% of the time I use a simple subset of Rust that avoids complexity as the primary goal.

Only as things really solidify will that code get reviewed for performance and noisier syntax and more 'clever' patterns.

`cargo clippy`, `cargo test` and `cargo run tests` must be fully green before making a new commit (or run `just validate`).

A final commit must run `just ship` which validates, runs `cargo fmt` and writes a new benchmark log.

## Agents
Minimising redundant code and reading and validating EVERYTHING an agent produces is really important for maintaining a manageable and clean codebase. 

You usually have to end up removing or refactoring agent-generated code to reduce LOC and complexity, ask it to add more helpful, descriptive comments.

**Tests**

Agents should avoid updating or changing existing tests unless you understand exactly why a test might need to be updated and describe exactly how it should be updated.

Otherwise, the tests provide a useful baseline to prevent regressions and provide the agents with a useful way to make progress without breaking stuff.

There is an AGENTS.md file in the root directory that can be used as a baseline for improving LLM output when working with this codebase.
