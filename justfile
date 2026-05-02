set windows-shell := ["powershell", "-NoLogo", "-NoProfile", "-Command"]

validate:
    @echo "format"
    cargo fmt --check

    @echo "clippy"
    cargo clippy --quiet --all-targets --all-features -- -D warnings
    
    @echo "unit tests"
    cargo test --quiet -- --format terse

    @echo "integration tests"
    cargo run --quiet -- tests

    @echo "docs build"
    cargo run --quiet -- check docs

    @echo "speed test"
    cargo run --package xtask --bin xtask -- bench-quick

ship:
    cargo fmt
    just validate

release version:
    just validate
    git tag -a v{{version}} -m "Beanstalk v{{version}}"
    git push origin v{{version}}

bench:
    cargo run --package xtask --bin xtask -- bench

bench-quick:
    cargo run --package xtask --bin xtask -- bench-quick

bench-ci:
    cargo run --package xtask --bin xtask -- bench-ci

bench-clean:
    rm -rf benchmarks/results
