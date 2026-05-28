//! Timestamp and date formatting for benchmark records
//!
//! This module provides UTC timestamp conversion and human-readable date
//! formatting without external dependencies.

use std::time::SystemTime;

/// A calendar timestamp for benchmark records
///
/// WHAT: Stores year, month, day, hour, minute in UTC
/// WHY: Provides a structured, dependency-free representation of benchmark run times
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkTimestamp {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

impl BenchmarkTimestamp {
    /// Create a timestamp from the current system time in UTC
    pub fn now() -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("System time before UNIX epoch");

        let total_seconds = now.as_secs();
        let seconds_today = (total_seconds % 86400) as u32;
        let mut days_since_epoch = (total_seconds / 86400) as i64;

        let mut year = 1970;
        loop {
            let year_days = if is_leap_year(year) { 366 } else { 365 };
            if days_since_epoch < year_days {
                break;
            }
            days_since_epoch -= year_days;
            year += 1;
        }

        let mut month = 1;
        loop {
            let month_days = days_in_month(year, month);
            if days_since_epoch < month_days as i64 {
                break;
            }
            days_since_epoch -= month_days as i64;
            month += 1;
        }

        let day = (days_since_epoch + 1) as u8;
        let hour = (seconds_today / 3600) as u8;
        let minute = ((seconds_today % 3600) / 60) as u8;

        Self {
            year,
            month,
            day,
            hour,
            minute,
        }
    }

    /// Format as "May 10th - 15:21"
    pub fn format_run_header(&self) -> String {
        format!(
            "{} {} - {:02}:{:02}",
            month_name(self.month),
            format_day_with_ordinal(self.day),
            self.hour,
            self.minute
        )
    }

    /// Format as "2026-05" for month-key lookups
    pub fn month_key(&self) -> String {
        format!("{:04}-{:02}", self.year, self.month)
    }

    /// Format as "May 2026" for summary headings
    pub fn format_month_heading(&self) -> String {
        format!("{} {}", month_name(self.month), self.year)
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: u8) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn month_name(month: u8) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
}

fn ordinal_suffix(day: u8) -> &'static str {
    match day {
        11..=13 => "th",
        _ => match day % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    }
}

fn format_day_with_ordinal(day: u8) -> String {
    format!("{}{}", day, ordinal_suffix(day))
}

#[cfg(test)]
mod tests;
