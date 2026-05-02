# Contributing to this project
This project is at a very early stage with long-term goals.

If you are interested in compilers/programming languages or the goals of this language 
and want to contribute or make suggestions, please get in touch or open a discussion on GitHub.

Any questions about the future / design of this language are welcome.
Open a discussion on GitHub if you're curious.

## The Current Goal

To see the progress and current priority goals of the compiler and language, see `docs/roadmap/roadmap.md`.

See <a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Language%20Overview.md">the language overview</a> and <a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Compiler%20Design%20Overview.md">the compiler overview</a> for more details about the language itself.

New code contributions must follow the style guide: <a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Compiler%20Codebase%20Style%20Guide.md">Codebase Style Guide</a>

## Testing

Run the compiler integration suite with `cargo run -- tests`.
Alternatively, run `just validate` to execute the full validation suite (fmt check, clippy, unit tests, integration tests, docs build, and speed test). 
You must have `just` installed to run this.

New integration fixtures should use the canonical `tests/cases/<case>/input + expect.toml` layout.
An optional `tests/cases/manifest.toml` can define case ordering and tags during fixture migrations.

## Benchmarking

The project includes a Rust-only benchmark system for measuring compiler performance. The system executes the release `bean` binary against defined benchmark cases, records timing data, and generates reports.

### Quick Start

```bash
just bench-ci      # Fast: 0 warmup, 1 iteration (for CI/quick checks)
just bench-quick   # Medium: 1 warmup, 3 iterations (for local development)
just bench         # Full: 2 warmup, 10 iterations (for comprehensive measurements)
just bench-clean   # Remove all benchmark results
```

### How It Works

The benchmark system:

1. **Builds the compiler** with `cargo build --release --features detailed_timers`
2. **Parses benchmark cases** from `benchmarks/cases.txt` (simple text format: `<command> <arg1> <arg2>`)
3. **Executes benchmarks** with optional warmup runs followed by measured iterations
4. **Records timing data** as JSONL (machine-readable) and Markdown (human-readable)
5. **Preserves execution logs** (stdout/stderr) for each iteration

### Benchmark Cases

The system benchmarks various compiler operations defined in `benchmarks/cases.txt`:
- **Core benchmarks**: `check` and `build` operations on `speed-test.bst` and docs
- **Stress tests**: Template system, type system, constant folding, pattern matching, and collection operations

Each stress test file exercises specific compiler subsystems to identify performance bottlenecks.

### Output Structure

Results are stored in timestamped directories under `benchmarks/results/`:

```
benchmarks/results/
├── 2026-05-16_20-01-52/          # Timestamped run directory
│   ├── raw.jsonl                  # Per-iteration timing data (JSON Lines format)
│   ├── summary.md                 # Aggregated statistics table
│   └── logs/                      # Stdout/stderr for each iteration
│       ├── check_speed-test_bst_iter_1.log
│       └── ...
├── latest.jsonl -> ...            # Symlink to most recent raw data
└── latest-summary.md -> ...       # Symlink to most recent summary
```

### Understanding Results

The Markdown summary includes:
- **Mean**: Average duration across iterations
- **Median**: Middle value (robust to outliers)
- **Min/Max**: Fastest and slowest runs
- **StdDev**: Standard deviation (consistency measure)
- **Failures**: Count of failed executions

Example summary:
```markdown
| Case | Iterations | Mean (ms) | Median (ms) | Min (ms) | Max (ms) | StdDev (ms) | Failures |
|------|------------|-----------|-------------|----------|----------|-------------|----------|
| check_speed-test_bst | 3 | 127.80 | 130.66 | 120.51 | 132.25 | 5.20 | 0 |
```

### Adding New Benchmarks

Edit `benchmarks/cases.txt` to add new benchmark cases:

```
# Comment lines start with #
check path/to/file.bst
build path/to/file.bst
check docs
```

The system automatically:
- Skips comment lines (starting with `#`) and empty lines
- Handles multiple spaces as single separators
- Supports quoted arguments for paths with spaces

### Design Principles

- **Zero external dependencies**: Uses only Rust stdlib (no clap, serde, criterion)
- **Cross-platform**: Works on macOS, Linux, and Windows
- **Continue-on-failure**: Single benchmark failures don't abort the entire run
- **Isolated tooling**: Implemented in separate `xtask/` workspace crate

### Use Cases

- **Performance regression detection**: Compare results over time using JSONL data
- **Optimization validation**: Verify that compiler changes improve targeted areas
- **CI integration**: Use `just bench-ci` for fast automated performance checks
- **Bottleneck identification**: Stress tests reveal which subsystems need optimization

## New contributions
If you are thinking of contributing, start with something small that is easy to read and review and follow the style guide closely. Reliability and modularity are *TOP PRIORITY* in this codebase. 
90% of the time I use a simple subset of Rust that avoids complexity as the primary goal.

Only as things really solidify will that code get reviewed for performance and noisier syntax and more 'clever' patterns.

`cargo clippy`, `cargo test` and `cargo run tests` must be fully green before making a new commit (or run `just validate`).

## Agents
Minimising redundant code and reading and validating EVERYTHING an agent produces is really important for maintaining a manageable and clean codebase. 

You usually have to end up removing or refactoring agent-generated code to reduce LOC and complexity, ask it to add more helpful, descriptive comments.

**Tests**

Agents should avoid updating or changing existing tests unless you understand exactly why a test might need to be updated and describe exactly how it should be updated.

Otherwise, the tests provide a useful baseline to prevent regressions and provide the agents with a useful way to make progress without breaking stuff.

There is an AGENTS.md file in the root directory that can be used as a baseline for improving LLM output when working with this codebase.
