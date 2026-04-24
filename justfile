set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

validate:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo run -- tests
    cargo run --features "detailed_timers" -- docs
    cargo run --release --features "detailed_timers" -- speed-test.bst

ship:
    cargo fmt
    just validate
