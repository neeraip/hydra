//! External rain records (§14.12): the predecessor's user-prepared station
//! format, parsed from text the caller read. One reading per line —
//! `station year month day hour minute value` — blank lines and `;`
//! comments ignored, stations interleaved freely, unlisted intervals dry.
//!
//! The archival formats (NWS and Environment-Canada layouts) are deferred
//! (§1): their files fail this parse and are refused with its reason.

use crate::io::options::Date;

/// One reading: a station's value for the recording interval starting at
/// the stamped minute (§3.1 of the hydrology specification).
#[derive(Debug, Clone, PartialEq)]
pub struct RainReading {
    /// Station identifier as written.
    pub station: String,
    /// Calendar date of the reading.
    pub date: Date,
    /// Seconds past that date's midnight.
    pub seconds: f64,
    /// The value, in the record's own declared unit, meaning whatever the
    /// gage's form declares (intensity, volume, cumulative).
    pub value: f64,
}

/// Parse a user-prepared rain record. A malformed line is an error naming
/// its line number, never skipped — silently dropping a typoed reading
/// would make a wet interval dry.
pub fn parse_rain_file(text: &str) -> Result<Vec<RainReading>, String> {
    let mut readings = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        let bad = |what: &str| format!("rain record line {}: {what}", i + 1);
        if t.len() != 7 {
            return Err(bad(&format!(
                "expected 7 fields (station year month day hour minute value), found {}",
                t.len()
            )));
        }
        let int = |field: &str, name: &str| -> Result<u32, String> {
            field
                .parse::<u32>()
                .map_err(|_| bad(&format!("{name} {field:?} is not a whole number")))
        };
        let year = t[1]
            .parse::<i32>()
            .map_err(|_| bad(&format!("year {:?} is not a whole number", t[1])))?;
        let month = int(t[2], "month")?;
        let day = int(t[3], "day")?;
        let hour = int(t[4], "hour")?;
        let minute = int(t[5], "minute")?;
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return Err(bad(&format!("{}-{month}-{day} is not a date", year)));
        }
        if hour > 23 || minute > 59 {
            return Err(bad(&format!("{hour}:{minute:02} is not a clock time")));
        }
        let value = t[6]
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| bad(&format!("value {:?} is not a number", t[6])))?;
        readings.push(RainReading {
            station: t[0].to_string(),
            date: Date { year, month, day },
            seconds: f64::from(hour) * 3600.0 + f64::from(minute) * 60.0,
            value,
        });
    }
    Ok(readings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_parse_with_stations_interleaved() {
        let text = "\
; gauges around the catchment
sta1 2012 6 29 0 1 0.5

sta2 2012 6 29 0 1 1.5
sta1 2012 6 29 0 2 0.25
";
        let readings = parse_rain_file(text).expect("parses");
        assert_eq!(readings.len(), 3);
        assert_eq!(readings[0].station, "sta1");
        assert_eq!(
            readings[0].date,
            Date {
                year: 2012,
                month: 6,
                day: 29
            }
        );
        assert_eq!(readings[0].seconds, 60.0);
        assert_eq!(readings[2].value, 0.25);
    }

    #[test]
    fn malformed_lines_error_with_their_line_number() {
        let err = parse_rain_file("sta1 2012 6 29 0 1\n").unwrap_err();
        assert!(err.contains("line 1"), "{err}");
        assert!(err.contains("7 fields"), "{err}");

        let err = parse_rain_file("\n\nsta1 2012 13 29 0 1 0.5\n").unwrap_err();
        assert!(err.contains("line 3"), "{err}");

        let err = parse_rain_file("sta1 2012 6 29 24 0 0.5\n").unwrap_err();
        assert!(err.contains("clock time"), "{err}");

        let err = parse_rain_file("sta1 2012 6 29 0 1 wet\n").unwrap_err();
        assert!(err.contains("not a number"), "{err}");
    }
}
