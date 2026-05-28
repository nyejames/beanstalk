use super::*;

#[test]
fn test_ordinal_suffixes() {
    assert_eq!(ordinal_suffix(1), "st");
    assert_eq!(ordinal_suffix(2), "nd");
    assert_eq!(ordinal_suffix(3), "rd");
    assert_eq!(ordinal_suffix(4), "th");
    assert_eq!(ordinal_suffix(11), "th");
    assert_eq!(ordinal_suffix(12), "th");
    assert_eq!(ordinal_suffix(13), "th");
    assert_eq!(ordinal_suffix(21), "st");
    assert_eq!(ordinal_suffix(22), "nd");
    assert_eq!(ordinal_suffix(23), "rd");
}

#[test]
fn test_month_name() {
    assert_eq!(month_name(1), "January");
    assert_eq!(month_name(5), "May");
    assert_eq!(month_name(12), "December");
}

#[test]
fn test_format_day_with_ordinal() {
    assert_eq!(format_day_with_ordinal(1), "1st");
    assert_eq!(format_day_with_ordinal(11), "11th");
    assert_eq!(format_day_with_ordinal(21), "21st");
}

#[test]
fn test_timestamp_month_key() {
    let ts = BenchmarkTimestamp {
        year: 2026,
        month: 5,
        day: 10,
        hour: 15,
        minute: 21,
    };
    assert_eq!(ts.month_key(), "2026-05");
}

#[test]
fn test_timestamp_format_run_header() {
    let ts = BenchmarkTimestamp {
        year: 2026,
        month: 5,
        day: 10,
        hour: 15,
        minute: 21,
    };
    assert_eq!(ts.format_run_header(), "May 10th - 15:21");
}

#[test]
fn test_timestamp_format_month_heading() {
    let ts = BenchmarkTimestamp {
        year: 2026,
        month: 5,
        day: 10,
        hour: 15,
        minute: 21,
    };
    assert_eq!(ts.format_month_heading(), "May 2026");
}

#[test]
fn test_leap_year_detection() {
    assert!(is_leap_year(2020));
    assert!(!is_leap_year(2021));
    assert!(!is_leap_year(1900));
    assert!(is_leap_year(2000));
}

#[test]
fn test_days_in_month() {
    assert_eq!(days_in_month(2021, 1), 31);
    assert_eq!(days_in_month(2021, 2), 28);
    assert_eq!(days_in_month(2020, 2), 29);
    assert_eq!(days_in_month(2021, 4), 30);
}
