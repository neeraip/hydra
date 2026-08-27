//! The CLI's progress line, rendered for a page instead of a terminal.
//!
//! Copied from `crates/cli/src/main.rs`, where it is private and always has
//! been. Rendering it here rather than in the demo's JavaScript is on
//! purpose: it keeps the line identical to the one a terminal shows, and it
//! keeps the formatting where it can be tested — the tests below assert the
//! same strings the CLI's own tests do, so the two cannot drift apart
//! without one of them going red.
//!
//! The one thing that had to change is where wall time comes from. The CLI
//! reads an `Instant`; `wasm32-unknown-unknown` has no clock behind one, so
//! the caller passes elapsed seconds from `performance.now()` instead.

/// A simulated time as `h:mm:ss`, with hours unbounded — a 2540-hour run
/// reads `2540:00:00` rather than wrapping into days.
pub fn format_sim_clock(time_s: f64) -> String {
    let total_seconds = time_s.round().max(0.0) as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours}:{minutes:02}:{seconds:02}")
}

/// Elapsed wall time, seconds below a minute and `m ss` above it.
pub fn format_wall(s: f64) -> String {
    if s < 60.0 {
        format!("{s:.1}s")
    } else {
        let secs = s as u64;
        let m = secs / 60;
        let sec = secs % 60;
        format!("{m}m {sec:02}s")
    }
}

/// A `[████░░░░]` bar `width` cells wide.
pub fn render_bar(pct: u32, width: usize) -> String {
    let filled = ((pct as usize) * width / 100).min(width);
    let empty = width - filled;
    format!(
        "[{}{}]",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(empty)
    )
}

/// The line shown while a phase is running.
pub fn render_progress_line(phase: &str, simulated_s: f64, total_s: f64, wall_s: f64) -> String {
    // A zero-duration run is complete by definition — a steady-state model
    // has one step and no span to be part-way through.
    let pct = if total_s > 0.0 {
        ((100.0 * simulated_s / total_s).clamp(0.0, 100.0)) as u32
    } else {
        100
    };
    let bar = render_bar(pct, 20);
    let sim_str = format!(
        "{} / {}",
        format_sim_clock(simulated_s),
        format_sim_clock(total_s.max(0.0))
    );
    format!(
        "  {phase:<14} {bar} {pct:>3}%   {sim_str:<21}   {}",
        format_wall(wall_s)
    )
}

/// The line a finished phase leaves behind, replacing its progress line.
pub fn render_done_line(phase: &str, sim_s: f64, wall_s: f64) -> String {
    format!(
        "  \u{2713} {phase:<14} {}   {}",
        format_sim_clock(sim_s),
        format_wall(wall_s)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Mirrors of the CLI's own tests ────────────────────────────────────
    //
    // Same names, same expectations. If the CLI's rendering changes, these
    // are what fail.

    #[test]
    fn sim_clock_format_zero() {
        assert_eq!(format_sim_clock(0.0), "0:00:00");
    }

    #[test]
    fn sim_clock_format_whole_hours() {
        assert_eq!(format_sim_clock(2540.0 * 3600.0), "2540:00:00");
    }

    #[test]
    fn sim_clock_format_mixed_time() {
        assert_eq!(format_sim_clock(3661.0), "1:01:01");
    }

    #[test]
    fn render_progress_line_includes_percent_and_time_range() {
        let line = render_progress_line("Hydraulics", 1800.0, 7200.0, 0.0);
        assert!(line.contains("25%"), "missing percent: {line}");
        assert!(
            line.contains("0:30:00 / 2:00:00"),
            "missing sim clock: {line}"
        );
    }

    #[test]
    fn render_progress_line_zero_duration_reports_complete() {
        let line = render_progress_line("Hydraulics", 0.0, 0.0, 0.0);
        assert!(line.contains("100%"), "missing 100%: {line}");
        assert!(
            line.contains("0:00:00 / 0:00:00"),
            "missing sim clock: {line}"
        );
    }

    // ── What a page adds ──────────────────────────────────────────────────

    /// The page redraws the line in place, so its width must not depend on
    /// how far along the run is — a bar that grows would shift everything
    /// after it on every frame.
    #[test]
    fn the_line_is_the_same_width_at_every_percentage() {
        let widths: Vec<usize> = (0..=100)
            .step_by(5)
            .map(|pct| {
                let t = f64::from(pct) * 36.0;
                render_progress_line("Hydraulics", t, 3600.0, 1.0)
                    .chars()
                    .count()
            })
            .collect();
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "progress line changes width as it fills: {widths:?}"
        );
    }

    /// Every phase name has to fit the column: one that overflowed would
    /// push the bar out of alignment for that phase only.
    ///
    /// Both engines report "Simulation" today, so the live vocabulary is one
    /// name. The renderer is given the names it once had as well, because the
    /// padding it applies is what the test is about and a column that only
    /// ever sees one name proves nothing about it.
    #[test]
    fn every_phase_name_fits_its_column() {
        let lines: Vec<String> = ["Simulation", "Hydraulics", "Water quality"]
            .iter()
            .map(|p| render_progress_line(p, 0.0, 3600.0, 0.0))
            .collect();
        let widths: Vec<usize> = lines.iter().map(|l| l.chars().count()).collect();
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "a phase name overflows its column: {lines:?}"
        );
    }

    #[test]
    fn a_full_bar_is_all_filled_and_an_empty_one_is_not() {
        assert_eq!(render_bar(100, 4), "[████]");
        assert_eq!(render_bar(0, 4), "[░░░░]");
        assert_eq!(render_bar(50, 4), "[██░░]");
    }

    /// Percentages above 100 come from a step that overshoots the reporting
    /// horizon; the bar clamps rather than growing past its own width.
    #[test]
    fn an_overshooting_run_clamps_at_full() {
        let line = render_progress_line("Hydraulics", 7200.0, 3600.0, 0.0);
        assert!(line.contains("100%"), "{line}");
        assert!(!line.contains("101"), "{line}");
    }

    #[test]
    fn wall_time_switches_to_minutes_at_a_minute() {
        assert_eq!(format_wall(59.94), "59.9s");
        assert_eq!(format_wall(60.0), "1m 00s");
        assert_eq!(format_wall(3661.0), "61m 01s");
    }
}
