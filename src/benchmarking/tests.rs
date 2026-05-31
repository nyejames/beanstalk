use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::benchmarking::frontend::{
    FrontendBenchmarkBuildProfile, FrontendBenchmarkOptions, run_frontend_benchmark,
};

// The in-memory benchmark collector uses a global static. Parallel tests that
// each start/stop a collection scope would race. Serialize benchmarking tests
// to keep the harness simple without adding a test-dependency crate.
static BENCHMARK_TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn frontend_benchmark_runs_for_simple_file() {
    let _guard = BENCHMARK_TEST_MUTEX.lock().expect("test mutex should lock");

    let temp_dir = tempfile::tempdir().expect("should create temp dir");
    let file_path = temp_dir.path().join("test.bst");

    {
        let mut file = std::fs::File::create(&file_path).expect("should create file");
        file.write_all(b"x = 1\n").expect("should write to file");
    }

    let options = FrontendBenchmarkOptions {
        entry_path: file_path,
        build_profile: FrontendBenchmarkBuildProfile::Dev,
    };

    let report = run_frontend_benchmark(options).expect("benchmark should succeed");

    assert!(report.total_ms > 0.0, "total time should be positive");

    // Stage timings may be empty when detailed_timers is not enabled.
    // When it is enabled, we expect at least the canonical stages.
    #[cfg(feature = "detailed_timers")]
    assert!(
        !report.stages.is_empty(),
        "stage timings should be collected when detailed_timers is enabled"
    );

    #[cfg(feature = "detailed_timers")]
    assert!(
        !report.counters.is_empty(),
        "counters should be collected when detailed_timers is enabled"
    );
}

#[test]
fn frontend_benchmark_fails_for_missing_file() {
    let _guard = BENCHMARK_TEST_MUTEX.lock().expect("test mutex should lock");

    let options = FrontendBenchmarkOptions {
        entry_path: PathBuf::from("/definitely/does/not/exist.bst"),
        build_profile: FrontendBenchmarkBuildProfile::Dev,
    };

    let result = run_frontend_benchmark(options);
    assert!(result.is_err(), "benchmark should fail for missing file");
}

#[test]
fn frontend_benchmark_fails_for_invalid_syntax() {
    let _guard = BENCHMARK_TEST_MUTEX.lock().expect("test mutex should lock");

    let temp_dir = tempfile::tempdir().expect("should create temp dir");
    let file_path = temp_dir.path().join("bad.bst");

    {
        let mut file = std::fs::File::create(&file_path).expect("should create file");
        file.write_all(b"!!! invalid syntax !!!\n")
            .expect("should write to file");
    }

    let options = FrontendBenchmarkOptions {
        entry_path: file_path,
        build_profile: FrontendBenchmarkBuildProfile::Dev,
    };

    let result = run_frontend_benchmark(options);
    assert!(result.is_err(), "benchmark should fail for invalid syntax");
}
