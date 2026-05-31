use super::*;

#[test]
fn parse_bench_mode() {
    assert_eq!(BenchmarkMode::parse("bench"), Some(BenchmarkMode::Bench));
}

#[test]
fn parse_bench_check_mode() {
    assert_eq!(
        BenchmarkMode::parse("bench-check"),
        Some(BenchmarkMode::BenchCheck)
    );
}

#[test]
fn parse_bench_report_mode() {
    assert_eq!(
        BenchmarkMode::parse("bench-report"),
        Some(BenchmarkMode::BenchReport)
    );
}

#[test]
fn parse_bench_frontend_mode() {
    assert_eq!(
        BenchmarkMode::parse("bench-frontend"),
        Some(BenchmarkMode::BenchFrontend)
    );
}

#[test]
fn parse_bench_frontend_check_mode() {
    assert_eq!(
        BenchmarkMode::parse("bench-frontend-check"),
        Some(BenchmarkMode::BenchFrontendCheck)
    );
}

#[test]
fn parse_invalid_mode_returns_none() {
    assert_eq!(BenchmarkMode::parse("invalid"), None);
    assert_eq!(BenchmarkMode::parse(""), None);
    assert_eq!(BenchmarkMode::parse("bench-"), None);
    assert_eq!(BenchmarkMode::parse("bench-check-extra"), None);
}
