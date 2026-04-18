//! Shared test utilities for the Beanstalk crate.
//!
//! WHAT: common helpers used across unit and integration tests.
//! WHY: avoids duplicating small utility functions in every test module.

use std::path::PathBuf;
use std::time::SystemTime;

/// Creates a unique temporary directory path for test isolation.
///
/// WHAT: joins `std::env::temp_dir()` with a prefix and a nanosecond timestamp.
/// WHY: prevents test collisions when multiple tests run concurrently or in sequence.
pub fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_{prefix}_{unique}"))
}
