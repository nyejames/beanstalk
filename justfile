set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

validate:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test
    cargo run -- tests
    cargo run -- check docs
    cargo run --release -- check speed-test.bst

ship:
    cargo fmt
    just validate

release version:
    just validate
    git tag -a v{{version}} -m "Beanstalk v{{version}}"
    git push origin v{{version}}
