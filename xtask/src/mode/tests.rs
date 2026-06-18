use super::*;
use crate::profile::options::ProfileFilterMode;

// ----------------------------
//  Legacy parse (single argument)
// ----------------------------

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

// ----------------------------
//  parse_args: single-argument modes
// ----------------------------

fn unwrap_mode(result: ModeParseResult) -> BenchmarkMode {
    match result {
        ModeParseResult::Mode(mode) => mode,
        other => panic!("Expected Mode, got: {:?}", format!("{:?}", other)),
    }
}

fn unwrap_error(result: ModeParseResult) -> String {
    match result {
        ModeParseResult::Error(msg) => msg,
        other => panic!("Expected Error, got: {:?}", format!("{:?}", other)),
    }
}

impl std::fmt::Debug for ModeParseResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mode(mode) => write!(f, "Mode({:?})", mode),
            Self::ProfileHelp(_) => write!(f, "ProfileHelp(...)"),
            Self::Error(msg) => write!(f, "Error({:?})", msg),
        }
    }
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parse_args_bench() {
    assert_eq!(
        unwrap_mode(BenchmarkMode::parse_args(&args(&["bench"]))),
        BenchmarkMode::Bench
    );
}

#[test]
fn parse_args_bench_check() {
    assert_eq!(
        unwrap_mode(BenchmarkMode::parse_args(&args(&["bench-check"]))),
        BenchmarkMode::BenchCheck
    );
}

#[test]
fn parse_args_bench_report() {
    assert_eq!(
        unwrap_mode(BenchmarkMode::parse_args(&args(&["bench-report"]))),
        BenchmarkMode::BenchReport
    );
}

#[test]
fn parse_args_bench_frontend() {
    assert_eq!(
        unwrap_mode(BenchmarkMode::parse_args(&args(&["bench-frontend"]))),
        BenchmarkMode::BenchFrontend
    );
}

#[test]
fn parse_args_bench_frontend_check() {
    assert_eq!(
        unwrap_mode(BenchmarkMode::parse_args(&args(&["bench-frontend-check"]))),
        BenchmarkMode::BenchFrontendCheck
    );
}

#[test]
fn parse_args_single_mode_extra_args_rejected() {
    let error = unwrap_error(BenchmarkMode::parse_args(&args(&["bench", "extra"])));
    assert!(error.contains("does not accept additional arguments"));
}

#[test]
fn parse_args_empty_rejected() {
    let error = unwrap_error(BenchmarkMode::parse_args(&args(&[])));
    assert!(error.contains("No mode specified"));
}

#[test]
fn parse_args_unknown_mode_rejected() {
    let error = unwrap_error(BenchmarkMode::parse_args(&args(&["unknown-mode"])));
    assert!(error.contains("Unknown mode 'unknown-mode'"));
}

// ----------------------------
//  parse_args: bench-profile
// ----------------------------

#[test]
fn parse_args_bench_profile_default() {
    let mode = unwrap_mode(BenchmarkMode::parse_args(&args(&["bench-profile"])));
    match mode {
        BenchmarkMode::BenchProfile(options) => {
            assert_eq!(options.filter, ProfileFilterMode::Terse);
            assert_eq!(options.case_filter, None);
            assert_eq!(options.samply_rate_hz, None);
            assert!(!options.presymbolicate);
        }
        other => panic!("Expected BenchProfile, got {:?}", other),
    }
}

#[test]
fn parse_args_bench_profile_with_filter() {
    let mode = unwrap_mode(BenchmarkMode::parse_args(&args(&["bench-profile", "deep"])));
    match mode {
        BenchmarkMode::BenchProfile(options) => {
            assert_eq!(options.filter, ProfileFilterMode::Deep);
        }
        other => panic!("Expected BenchProfile, got {:?}", other),
    }
}

#[test]
fn parse_args_bench_profile_with_case() {
    let mode = unwrap_mode(BenchmarkMode::parse_args(&args(&[
        "bench-profile",
        "--case",
        "my_case",
    ])));
    match mode {
        BenchmarkMode::BenchProfile(options) => {
            assert_eq!(options.case_filter, Some("my_case".to_string()));
        }
        other => panic!("Expected BenchProfile, got {:?}", other),
    }
}

#[test]
fn parse_args_bench_profile_help() {
    let result = BenchmarkMode::parse_args(&args(&["bench-profile", "--help"]));
    match result {
        ModeParseResult::ProfileHelp(help) => {
            assert!(help.contains("Usage:"));
        }
        other => panic!("Expected ProfileHelp, got {:?}", format!("{:?}", other)),
    }
}

#[test]
fn parse_args_bench_profile_error() {
    let error = unwrap_error(BenchmarkMode::parse_args(&args(&[
        "bench-profile",
        "--unknown",
    ])));
    assert!(error.contains("Unknown argument"));
}
