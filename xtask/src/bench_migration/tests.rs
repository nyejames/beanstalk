use std::fs;

use super::migrate_old_results;

#[test]
fn test_migration_moves_results() {
    let temp_dir = std::env::temp_dir().join("bench_migration_test_moves");
    let _ = fs::remove_dir_all(&temp_dir);

    let results_path = temp_dir.join("benchmarks/results");
    let old_benchmarks_dir = temp_dir.join("benchmarks/old-benchmarks");

    fs::create_dir_all(&results_path).unwrap();
    fs::write(results_path.join("old-report.txt"), "legacy data").unwrap();

    migrate_old_results(&results_path, &old_benchmarks_dir);

    assert!(!results_path.exists(), "old results path should be removed");

    let archives: Vec<_> = fs::read_dir(&old_benchmarks_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(
        archives.len(),
        1,
        "should create exactly one archive folder"
    );

    let archived_file = archives[0].path().join("old-report.txt");
    assert!(
        archived_file.exists(),
        "archived file should exist at {:?}",
        archived_file
    );
    assert_eq!(fs::read_to_string(archived_file).unwrap(), "legacy data");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_migration_no_op_when_missing() {
    let temp_dir = std::env::temp_dir().join("bench_migration_test_no_op");
    let _ = fs::remove_dir_all(&temp_dir);

    let results_path = temp_dir.join("benchmarks/results");
    let old_benchmarks_dir = temp_dir.join("benchmarks/old-benchmarks");

    migrate_old_results(&results_path, &old_benchmarks_dir);

    assert!(!old_benchmarks_dir.exists());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_migration_no_op_when_empty_results_dir() {
    let temp_dir = std::env::temp_dir().join("bench_migration_test_empty");
    let _ = fs::remove_dir_all(&temp_dir);

    let results_path = temp_dir.join("benchmarks/results");
    let old_benchmarks_dir = temp_dir.join("benchmarks/old-benchmarks");

    fs::create_dir_all(&results_path).unwrap();

    migrate_old_results(&results_path, &old_benchmarks_dir);

    assert!(!results_path.exists());
    assert!(old_benchmarks_dir.exists());

    let _ = fs::remove_dir_all(&temp_dir);
}
