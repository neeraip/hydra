//! Civil-calendar arithmetic for the simulation clock: days from the civil
//! epoch, the inverse, and weekdays — Howard Hinnant's algorithms, exact
//! over the proleptic Gregorian calendar.

use crate::io::options::Date;

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

#[cfg(test)]
mod tests {
    use super::*;

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
