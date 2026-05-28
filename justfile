set windows-shell := ["powershell", "-NoLogo", "-NoProfile", "-Command"]

validate:
    @echo "clippy"
    cargo clippy --quiet --all-targets --all-features -- -D warnings
    
    @echo "unit tests"
    cargo test --quiet -- --format terse

    @echo "integration tests"
    cargo run --quiet -- tests

    @echo "docs build"
    cargo run --quiet -- check docs

    @echo "benchmark check"
    cargo run --package xtask --bin xtask -- bench-check

ship:
    cargo fmt
    just validate
    just bench

release version:
    just validate
    git tag -a v{{version}} -m "Beanstalk v{{version}}"
    git push origin v{{version}}

bench:
    cargo run --package xtask --bin xtask -- bench

bench-frontend:
    cargo run --package xtask --bin xtask -- bench-frontend

bench-check:
    cargo run --package xtask --bin xtask -- bench-check

bench-frontend-check:
    cargo run --package xtask --bin xtask -- bench-frontend-check
