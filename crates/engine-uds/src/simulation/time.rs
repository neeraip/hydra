//! Civil-calendar arithmetic for the simulation clock: days from the civil
//! epoch, the inverse, and weekdays — Howard Hinnant's algorithms, exact
//! over the proleptic Gregorian calendar.

use crate::model::options::Date;

/// Days from 1970-01-01 for a civil date.
pub fn days_from_civil(d: Date) -> i64 {
    let y = i64::from(d.year) - i64::from(d.month <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(d.month);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(d.day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date for a day count from 1970-01-01.
pub fn civil_from_days(z: i64) -> Date {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    Date {
        year: (y + i64::from(month <= 2)) as i32,
        month,
        day,
    }
}

/// Weekday for a day count, Sunday = 0.
pub fn weekday(days: i64) -> u32 {
    (days + 4).rem_euclid(7) as u32
}

/// Seconds from the simulation start to the report start (§11.2).
///
/// Two things read this and they must agree: the router gates its
/// per-object statistics on it, and the report measures every printed
/// instant from it (§14.9). A report start before the run start is the run
/// start, so the answer is never negative.
pub fn report_start_offset(o: &crate::model::options::AnalysisOptions) -> f64 {
    let Some((d, s)) = o.report_start else {
        return 0.0;
    };
    let start = days_from_civil(o.start_date) as f64 * 86_400.0 + o.start_time;
    (days_from_civil(d) as f64 * 86_400.0 + s - start).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(
        start: (i32, u32, u32),
        start_s: f64,
        report: Option<((i32, u32, u32), f64)>,
    ) -> crate::model::options::AnalysisOptions {
        let d = |(year, month, day): (i32, u32, u32)| Date { year, month, day };
        crate::model::options::AnalysisOptions {
            start_date: d(start),
            start_time: start_s,
            report_start: report.map(|(rd, rs)| (d(rd), rs)),
            ..Default::default()
        }
    }

    #[test]
    fn the_report_origin_is_the_offset_from_the_run_start() {
        // No declaration: reporting begins with the run.
        assert_eq!(report_start_offset(&opts((2024, 6, 1), 0.0, None)), 0.0);
        // Later the same day, counted from the run's start time, not midnight.
        assert_eq!(
            report_start_offset(&opts(
                (2024, 6, 1),
                3600.0,
                Some(((2024, 6, 1), 4.0 * 3600.0))
            )),
            3.0 * 3600.0
        );
        // Across a date boundary.
        assert_eq!(
            report_start_offset(&opts((2024, 6, 1), 0.0, Some(((2024, 6, 3), 0.0)))),
            2.0 * 86_400.0
        );
    }

    #[test]
    fn a_report_start_before_the_run_start_is_the_run_start() {
        // The predecessor clamps it (`project.c:151`), and an unclamped
        // negative would make the statistics gate and the printed origin
        // disagree about where the run begins.
        assert_eq!(
            report_start_offset(&opts((2024, 6, 5), 0.0, Some(((2024, 6, 1), 0.0)))),
            0.0
        );
        assert_eq!(
            report_start_offset(&opts((2024, 6, 1), 7200.0, Some(((2024, 6, 1), 0.0)))),
            0.0
        );
    }

    #[test]
    fn civil_round_trips_and_weekdays_hold() {
        let d = Date {
            year: 2024,
            month: 2,
            day: 29,
        };
        let z = days_from_civil(d);
        assert_eq!(civil_from_days(z), d);
        // 1970-01-01 was a Thursday; 2024-02-29 a Thursday too.
        assert_eq!(
            weekday(days_from_civil(Date {
                year: 1970,
                month: 1,
                day: 1
            })),
            4
        );
        assert_eq!(weekday(z), 4);
    }
}
