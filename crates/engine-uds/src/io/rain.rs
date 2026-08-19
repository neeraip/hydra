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

// ── Archival station records (§14.12.1) ─────────────────────────────────────

/// The archival layouts this engine reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    /// Fixed-field tape: 12-byte groups from column 30.
    Tape,
    /// Space-delimited DSI export, with or without a station name.
    Space { named: bool },
    /// Comma-delimited DSI export.
    Comma,
    /// Environment-Canada, one line per day in fixed seven-character
    /// groups. `wide` is the quarter-hourly layout, `short_year` the
    /// hourly one whose year field is three digits.
    Canada { wide: bool, short_year: bool },
}

impl Layout {
    /// Where a line's readings begin, and how wide each one is.
    fn groups(self) -> (usize, usize) {
        match self {
            Layout::Tape => (30, 12),
            Layout::Space { named: false } => (28, 16),
            Layout::Space { named: true } => (59, 16),
            Layout::Comma => (28, 16),
            Layout::Canada { short_year, .. } => (if short_year { 17 } else { 18 }, 7),
        }
    }

    /// How many readings a line of this layout carries, where that is
    /// fixed by the layout rather than by the line's length.
    fn per_line(self) -> Option<usize> {
        match self {
            Layout::Canada { wide: true, .. } => Some(96),
            Layout::Canada { wide: false, .. } => Some(24),
            _ => None,
        }
    }

    /// The quantity code a line must carry to be rainfall in this layout.
    fn rainfall_element(self) -> Option<u32> {
        match self {
            Layout::Canada { wide: true, .. } => Some(159),
            Layout::Canada { wide: false, .. } => Some(123),
            _ => None,
        }
    }
}

/// The recording interval an element code declares (§14.12.1).
fn nws_interval(element: &str) -> Option<f64> {
    match element {
        "HPCP" => Some(3600.0),
        "QPCP" | "QGAG" => Some(900.0),
        _ => None,
    }
}

/// Recognise the layout and interval from a file's opening lines.
///
/// The first five lines are examined, as the predecessor does: an export
/// may carry header lines before its first record, and a layout that
/// cannot be recognised in five is not one of these.
fn recognise(text: &str) -> Option<(Layout, f64, String)> {
    for line in text.lines().take(5) {
        // Tape carries a three-character record type before the station.
        if line.len() > 30 {
            let station = line.get(3..9).unwrap_or("").trim();
            let element = line.get(11..15).unwrap_or("");
            if !station.is_empty()
                && station.bytes().all(|b| b.is_ascii_digit())
                && line
                    .get(9..11)
                    .is_some_and(|d| d.bytes().all(|b| b.is_ascii_digit()))
            {
                if let Some(interval) = nws_interval(element) {
                    return Some((Layout::Tape, interval, station.to_string()));
                }
            }
        }
        // Environment-Canada: station, year, month, day, quantity, all
        // fixed-width, and a line long enough to hold its own readings.
        for (wide, short_year, year_width, groups) in [
            (true, false, 4, 96),
            (false, false, 4, 24),
            (false, true, 3, 24),
        ] {
            let head = 7 + year_width + 2 + 2 + 3;
            if line.len() < head + groups * 7 {
                continue;
            }
            let station = &line[..7];
            let element = &line[head - 3..head];
            let layout = Layout::Canada { wide, short_year };
            if station.bytes().all(|b| b.is_ascii_digit())
                && line[7..head].bytes().all(|b| b.is_ascii_digit())
                && element.trim().parse::<u32>().ok() == layout.rainfall_element()
            {
                let interval = if wide { 900.0 } else { 3600.0 };
                return Some((layout, interval, station.trim().to_string()));
            }
        }

        // Space and comma exports both open with the station, then the
        // division and element, separated by their own delimiter.
        let head = line.get(..18).unwrap_or("");
        let comma = head.contains(',');
        let fields: Vec<&str> = if comma {
            head.split(',').collect()
        } else {
            head.split_whitespace().collect()
        };
        if fields.len() >= 3 {
            let station = fields[0].trim();
            if !station.is_empty() && station.bytes().all(|b| b.is_ascii_digit()) {
                if let Some(interval) = nws_interval(fields[2].trim()) {
                    let layout = if comma {
                        Layout::Comma
                    } else {
                        Layout::Space { named: false }
                    };
                    return Some((layout, interval, station.to_string()));
                }
            }
        }
    }
    None
}

/// Parse an archival station record into the form a rainfall interface
/// file holds: depths in inches over the interval the file declares
/// (§14.12.1).
///
/// The layout and interval are the file's own. A file in a layout this
/// engine does not read is refused rather than read at a guess.
pub fn parse_archive_file(
    text: &str,
) -> Result<(crate::io::iface::RainGageRecord, Vec<String>), String> {
    let Some((layout, interval, station)) = recognise(text) else {
        return Err(
            "not an archival station record this engine reads: no line in the \
             first five carries a station, division and a recording element of \
             HPCP, QPCP or QGAG (§14.12.1)"
                .into(),
        );
    };
    let (start, width) = layout.groups();
    let mut readings: Vec<(f64, f64)> = Vec::new();
    let mut notices = Vec::new();
    // The condition a bracket opened, which outlives the line that opened
    // it: a record that opens a missing period and never closes it leaves
    // everything after it missing, which is what the record says.
    let mut missing_period = false;
    let mut accum_start: Option<f64> = None;

    for (n, line) in text.lines().enumerate() {
        let bad = |what: &str| format!("archival record line {}: {what}", n + 1);
        let Some((date, day_seconds)) = archive_date(line, layout) else {
            continue;
        };
        // The Canadian layouts carry no clock: a group's instant is its
        // position on the line, so they are read by counting rather than
        // by parsing, and every other semantic is shared.
        if let Some(count) = layout.per_line() {
            for j in 0..count {
                let k = start + j * width;
                let Some(group) = line.get(k..k + width) else {
                    break;
                };
                let Ok(value) = group[..6].trim().parse::<i64>() else {
                    break;
                };
                // Tenths of a millimetre. Missing is −99999, which this
                // test already excludes along with every other reading of
                // nothing: the layout has no positive value that means
                // absent, so naming it separately would be a branch no
                // record can take.
                if value <= 0 {
                    continue;
                }
                let at = day_seconds + j as f64 * interval - interval;
                readings.push((archive_day(date, at), value as f64 / 10.0 / 25.4));
            }
            continue;
        }
        let mut k = start;
        while k + width <= line.len() {
            let group = &line[k..k + width];
            k += width;
            let Some((hour, minute, value, flag)) = archive_group(group, layout) else {
                break;
            };
            if hour >= 25 {
                break;
            }
            let condition = match flag {
                'a' | 'A' => Some(false),
                '{' | '[' => Some(true),
                '}' | ']' => Some(false),
                _ => None,
            };
            if let Some(open) = condition {
                if flag == '{' || flag == '[' {
                    missing_period = true;
                } else if !open {
                    missing_period = false;
                }
            }
            let absent = missing_period || flag == 'M' || value >= 9999;
            let at = day_seconds + 3600.0 * hour as f64 + 60.0 * minute as f64;
            match flag {
                'a' => accum_start = Some(at),
                'A' => {
                    let Some(from) = accum_start.take() else {
                        return Err(bad("an accumulation closes that never opened"));
                    };
                    let spans = ((at - from) / interval).round().max(0.0) as usize + 1;
                    if !absent {
                        let each = value as f64 / spans as f64 / 100.0;
                        for j in 0..spans {
                            let t = from + j as f64 * interval - interval;
                            readings.push((archive_day(date, t), each));
                        }
                        notices.push(format!(
                            "an accumulated total of {:.2} in was divided evenly over \
                             {spans} periods ending at line {}, because the record \
                             carries no measurement within them (§14.12.1)",
                            value as f64 / 100.0,
                            n + 1
                        ));
                    }
                }
                _ => {
                    // Missing is absent from the record, not dry, and a
                    // zero the predecessor drops is dropped here too.
                    if !absent && value > 0 {
                        let t = at - interval;
                        readings.push((archive_day(date, t), value as f64 / 100.0));
                    }
                }
            }
        }
    }
    if readings.is_empty() && notices.is_empty() {
        return Err(format!(
            "archival station record for {station:?} holds no rainfall at all"
        ));
    }
    Ok((
        crate::io::iface::RainGageRecord {
            station,
            interval,
            readings,
        },
        notices,
    ))
}

/// A line's calendar date and the seconds its midnight sits at, or `None`
/// when the line is a header rather than a record.
fn archive_date(line: &str, layout: Layout) -> Option<(Date, f64)> {
    let (y, m, d) = match layout {
        Layout::Tape => {
            let f = line.get(17..30)?;
            (
                f.get(..4)?.trim().parse().ok()?,
                f.get(4..6)?.trim().parse().ok()?,
                f.get(6..10)?.trim().parse().ok()?,
            )
        }
        Layout::Space { named } => {
            let at = if named { 49 } else { 18 };
            let f = line.get(at..at + 10)?;
            (
                f.get(..4)?.trim().parse().ok()?,
                f.get(5..7)?.trim().parse().ok()?,
                f.get(8..10)?.trim().parse().ok()?,
            )
        }
        Layout::Comma => {
            let f = line.get(18..28)?;
            (
                f.get(..4)?.trim().parse().ok()?,
                f.get(5..7)?.trim().parse().ok()?,
                f.get(8..10)?.trim().parse().ok()?,
            )
        }
        Layout::Canada { short_year, .. } => {
            let w = if short_year { 3 } else { 4 };
            let f = line.get(7..7 + w + 4)?;
            let year: i32 = f.get(..w)?.trim().parse().ok()?;
            (
                // §14.12.1: three digits are the year less 1900, which is
                // what the layout means and not what the predecessor
                // computes.
                if short_year { year + 1900 } else { year },
                f.get(w..w + 2)?.trim().parse().ok()?,
                f.get(w + 2..w + 4)?.trim().parse().ok()?,
            )
        }
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((
        Date {
            year: y,
            month: m,
            day: d,
        },
        0.0,
    ))
}

/// One reading group: its hour, minute, hundredths of an inch, and flag.
fn archive_group(group: &str, layout: Layout) -> Option<(u32, u32, i64, char)> {
    let (hour, minute, value, flag) = match layout {
        Layout::Tape => (
            group.get(..2)?,
            group.get(2..4)?,
            group.get(4..10)?,
            group.get(10..11)?,
        ),
        Layout::Space { .. } => (
            group.get(1..3)?,
            group.get(3..5)?,
            group.get(6..12)?,
            group.get(13..14)?,
        ),
        Layout::Comma => (
            group.get(1..3)?,
            group.get(3..5)?,
            group.get(6..12)?,
            group.get(13..14)?,
        ),
        // A Canadian group carries a value and a flag and no clock at
        // all: its instant is its position on the line.
        Layout::Canada { .. } => return None,
    };
    Some((
        hour.trim().parse().ok()?,
        minute.trim().parse().ok()?,
        value.trim().parse().ok()?,
        flag.chars().next().unwrap_or(' '),
    ))
}

/// A date and an offset in seconds as the decimal day these records are
/// stamped with. An offset past midnight, or before it, carries the date.
fn archive_day(date: Date, seconds: f64) -> f64 {
    const SWMM_EPOCH_DAYS: f64 = 25_569.0;
    SWMM_EPOCH_DAYS + crate::simulation::time::days_from_civil(date) as f64 + seconds / 86_400.0
}

#[cfg(test)]
mod archive_tests {
    use super::*;

    /// Every expectation here is what SWMM 5 itself made of the same file:
    /// each fixture was run through the reference implementation with
    /// `SAVE RAINFALL`, and these are the readings its interface file
    /// held. See `tests/fixtures/uds/archive/README.txt`.
    pub(super) fn fixture(name: &str) -> String {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/uds/archive")
            .join(name);
        std::fs::read_to_string(path).expect("fixture readable")
    }

    /// Readings as `(hour of 2020-01-01, inches)`, rounded past the noise
    /// the decimal-day encoding carries.
    pub(super) fn hours(rec: &crate::io::iface::RainGageRecord) -> Vec<(f64, f64)> {
        rec.readings
            .iter()
            .map(|(day, v)| {
                let hour = ((day - 43_831.0) * 24.0 * 1e6).round() / 1e6;
                (hour, (v * 1e6).round() / 1e6)
            })
            .collect()
    }

    #[test]
    fn a_space_delimited_record_reads_as_the_predecessor_reads_it() {
        let (rec, notices) = parse_archive_file(&fixture("nws_space.dat")).expect("parse");
        assert_eq!("123456", rec.station);
        assert_eq!(3600.0, rec.interval, "HPCP is hourly");
        // Stamped one interval before the instant the record marks: the
        // 01:00 reading is the hour that ended at 01:00.
        assert_eq!(vec![(0.0, 0.25), (1.0, 0.10)], hours(&rec));
        assert!(notices.is_empty(), "{notices:?}");
    }

    #[test]
    fn the_comma_and_tape_layouts_read_the_same_record() {
        let space = parse_archive_file(&fixture("nws_space.dat"))
            .expect("space")
            .0;
        for name in ["nws_comma.dat", "nws_tape.dat"] {
            let (rec, _) = parse_archive_file(&fixture(name)).expect(name);
            assert_eq!(space.station, rec.station, "{name}: station");
            assert_eq!(space.interval, rec.interval, "{name}: interval");
            assert_eq!(hours(&space), hours(&rec), "{name}: readings");
        }
    }

    /// A missing reading is absent, and so is a zero. Neither is a dry
    /// interval this parse invents.
    #[test]
    fn a_missing_reading_and_a_zero_are_both_absent() {
        let (rec, _) = parse_archive_file(&fixture("nws_space.dat")).expect("parse");
        assert_eq!(2, rec.readings.len(), "the record carries four readings");
        assert!(
            !hours(&rec).iter().any(|(h, _)| *h == 2.0 || *h == 3.0),
            "the flagged and zero readings must not appear: {:?}",
            hours(&rec)
        );
    }

    /// An accumulated total is divided evenly, and said so.
    #[test]
    fn an_accumulation_is_spread_and_reported() {
        let (rec, notices) = parse_archive_file(&fixture("nws_accum.dat")).expect("parse");
        assert_eq!(
            vec![
                (0.0, 0.15),
                (1.0, 0.15),
                (2.0, 0.15),
                (3.0, 0.15),
                (5.0, 0.05),
            ],
            hours(&rec),
            "0.60 in over four periods, then a measured 0.05 in"
        );
        // The uniformity is an artefact of the record, and a modeller who
        // is not told reads four identical hours as a measurement.
        assert_eq!(1, notices.len(), "{notices:?}");
        assert!(notices[0].contains("0.60 in"), "{}", notices[0]);
        assert!(notices[0].contains("4 periods"), "{}", notices[0]);
    }

    #[test]
    fn a_layout_this_engine_does_not_read_is_refused() {
        // The standard station format, which §14.12 serves by another path.
        let err = parse_archive_file("STA01  2020  1  1  0  0  0.10\n").unwrap_err();
        assert!(err.contains("not an archival station record"), "{err}");
        // An element code that declares no interval.
        let err = parse_archive_file("123456 21 ZZZZ  HI2020 01 01 0100     25    \n").unwrap_err();
        assert!(err.contains("HPCP"), "{err}");
    }

    #[test]
    fn a_quarter_hourly_element_reads_at_its_own_interval() {
        let (rec, _) =
            parse_archive_file("123456 21 QPCP  HI2020 01 01 0015     25    \n").expect("parse");
        assert_eq!(900.0, rec.interval, "QPCP is quarter-hourly");
        // Stamped a quarter hour before the instant the record marks.
        assert_eq!(vec![(0.0, 0.25)], hours(&rec));
    }
}

/// A supplied rain file, in whichever form it turned out to be.
///
/// A caller does not declare which: the layouts are recognised from the
/// file's own opening lines, as the predecessor recognises them, so a
/// modeller who swaps a station export for an archive changes nothing but
/// the file.
#[derive(Debug, Clone, PartialEq)]
pub enum RainRecords {
    /// The user-prepared station format (§14.12), read in the record's own
    /// declared unit and meaning whatever the gage's form declares.
    Station(Vec<RainReading>),
    /// An archival station record (§14.12.1), already normalised to depths
    /// in inches over the interval the file declares.
    Archive(crate::io::iface::RainGageRecord),
}

/// Parse a supplied rain file in whichever layout it is written in.
///
/// The archival layouts are tried first because they are recognised from
/// their own header, where the standard format is recognised only by a
/// line parsing successfully: an archival line would otherwise have to be
/// rejected by the standard parse, which reports its own reason and hides
/// the real one.
pub fn parse_any_rain_file(text: &str) -> Result<(RainRecords, Vec<String>), String> {
    match parse_archive_file(text) {
        Ok((record, notices)) => Ok((RainRecords::Archive(record), notices)),
        Err(archive_reason) => match parse_rain_file(text) {
            Ok(readings) => Ok((RainRecords::Station(readings), Vec::new())),
            // Both refused: the standard format's reason names a line and
            // is the more useful of the two, since a file meant to be an
            // archive usually fails recognition on its first line.
            Err(station_reason) => Err(format!(
                "{station_reason} (and it is {})",
                archive_reason
                    .strip_prefix("not an archival station record this engine reads: ")
                    .unwrap_or(&archive_reason)
            )),
        },
    }
}

#[cfg(test)]
mod canada_archive_tests {
    use super::archive_tests::{fixture, hours};
    use super::*;

    /// 25 tenths of a millimetre is 2.5 mm, which is 0.098425 inches. The
    /// reference implementation writes exactly that.
    const FIRST: f64 = 0.098425;
    const SECOND: f64 = 0.03937;

    #[test]
    fn an_hourly_canadian_record_reads_as_the_predecessor_reads_it() {
        let (rec, _) = parse_archive_file(&fixture("cmc_hly.dat")).expect("parse");
        assert_eq!("1234567", rec.station);
        assert_eq!(3600.0, rec.interval);
        let got = hours(&rec);
        assert_eq!(2, got.len(), "the missing reading must not appear: {got:?}");
        assert_eq!(0.0, got[0].0);
        assert_eq!(1.0, got[1].0);
        assert!((got[0].1 - FIRST).abs() < 1e-5, "{got:?}");
        assert!((got[1].1 - SECOND).abs() < 1e-5, "{got:?}");
    }

    #[test]
    fn a_quarter_hourly_canadian_record_reads_at_its_own_interval() {
        let (rec, _) = parse_archive_file(&fixture("cmc_fif.dat")).expect("parse");
        assert_eq!(900.0, rec.interval);
        let got = hours(&rec);
        assert_eq!(0.0, got[0].0);
        assert_eq!(0.25, got[1].0, "a quarter of an hour later");
    }

    /// The first group of a day is the interval that ended at its
    /// midnight, so it belongs to the day before.
    #[test]
    fn the_first_group_of_a_day_belongs_to_the_day_before() {
        let (rec, _) = parse_archive_file(&fixture("cmc_edge.dat")).expect("parse");
        assert_eq!(1, rec.readings.len());
        // 2020-01-02's first group lands at 2020-01-01 23:00, which is
        // hour 23 of the day the other fixtures use.
        assert_eq!(23.0, hours(&rec)[0].0, "{:?}", hours(&rec));
    }

    /// The three-digit year is the year less 1900, not the predecessor's
    /// arithmetic, which puts a 2020 record in 1120 and reads nothing.
    #[test]
    fn a_three_digit_year_is_read_as_the_layout_defines_it() {
        let (rec, _) = parse_archive_file(&fixture("aes_hly.dat")).expect("parse");
        // 43831.0 is 2020-01-01; the fixture's year field is 120.
        let day = rec.readings[0].0.floor();
        assert_eq!(43_831.0, day, "a year field of 120 is 2020, not 1120");
        assert!((rec.readings[0].1 - FIRST).abs() < 1e-5);
    }

    /// A line carrying some other quantity is skipped, not read as rain.
    #[test]
    fn a_line_that_is_not_rainfall_is_not_read() {
        // The quantity code is the three characters after the date, not
        // the first "123" on the line, which is the station.
        let rain = fixture("cmc_hly.dat");
        let temperature = format!("{}078{}", &rain[..15], &rain[18..]);
        assert!(
            temperature.starts_with("123456720200101078"),
            "{}",
            &temperature[..20]
        );
        let err = parse_archive_file(&temperature).unwrap_err();
        assert!(err.contains("not an archival station record"), "{err}");
    }
}
