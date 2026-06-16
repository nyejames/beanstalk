# Contributing to this project
This project is at a very early stage with long-term high-difficulty goals.

If you are interested in compilers/programming languages or the goals of this language 
and want to contribute or make suggestions, please get in touch or open a discussion on GitHub.

Any questions about the future / design of this language are welcome.
Open a discussion on GitHub if you're curious.

This is still an undisclosed project (its open sourced but no one knows about it), so getting it touch prior to contributing is preferred at this stage over yeeting a random PR at the repo.

## Learning about this project

The [user facing docs](docs/src/docs/**) contain examples, and beginner-oriented explanations for the language and using the tooling.

To see the current plans and priority goals of the compiler and language, see the [roadmap](docs/roadmap/roadmap.md).

The [progress matrix](docs/src/docs/progress/#page.bst) tracks current support, partial support, clean rejection, experimental backend status and coverage status.

See [the language overview](docs/language-overview.md), 
[the compiler overview](docs/compiler-design-overview.md) and [the memory management strategy](docs/memory-management-design.md) for more technical details about the language, compiler and build system.

New code contributions must follow the [codebase style guide](docs/codebase-style-guide.md).

## Validation command guide

| Command | Use when | Writes tracked output? |
|---|---|---|
| `just validate` | Before submitting any compiler/docs change | No, except normal build artifacts |
| `cargo run -- tests` | Fast integration-suite iteration | No |
| `just bench-check` | Performance sanity check without recording history | No |
| `just bench` | Intentional benchmark recording after meaningful performance work | Yes, tracked summaries may change |
| `just bench-report` | Local investigation of benchmark history | No tracked output |
| `just profile-build` | Building a profiling binary after a benchmark report identifies a target | No tracked output |

---

## Testing

Run the compiler integration suite with `cargo run -- tests`.
Alternatively, run `just validate` to execute the full validation suite. 
You must have `just` installed to run this.

New integration fixtures should use the canonical `tests/cases/<case>/input + expect.toml` layout.
An optional `tests/cases/manifest.toml` can define case ordering and tags during fixture migrations.

## Benchmark contribution policy

Benchmarks are rough compiler-development evidence, not a correctness gate by themselves.

Use `just bench-check` for validation and PR confidence. Use `just bench` only when you intentionally want to record benchmark history or update tracked monthly summaries.

Do not add benchmark cases for negative diagnostics. Those belong in `tests/cases`. Benchmark cases should be valid programs or projects that exercise real compiler work.

When adding or changing benchmarks:

- keep source fixtures committed
- do not commit generated `dev/` or `release/` outputs
- prefer end-to-end compiler workloads over tiny microbenchmarks
- add focused stress cases only when they represent a real compiler subsystem
- use `just bench-report` to inspect local history before claiming a performance improvement
- do not turn timing noise into a CI blocker without a dedicated benchmark-infrastructure change

## Benchmarking

Since the benchmarking system isn't introduced or mentioned anywhere else, here is a quick summary of it:

The project includes a Rust-only (crappy and imprecise) benchmark system for rough compiler performance sanity checks. The system executes the release `bean` binary against defined benchmark cases, records local timing data, and writes terse public summaries tied to the local hardware that ran it.

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