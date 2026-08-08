//! Static SVG chart generator (spec §4 "Charts"), shared by the html and
//! pdf renderers. Documents are light-surface artifacts — no dark
//! variant. Marks follow fixed specs: bars ≤ 24u thick with 4u rounded
//! data-ends and square baselines, 2u lines with round caps, hairline
//! solid gridlines, one axis, value labels on bar caps, line series
//! direct-labeled at their ends when they fit plus a legend for ≥ 2
//! series. Series colors come from a fixed categorical order validated
//! for color-vision deficiency against the white document surface; text
//! always wears ink colors, never series colors.

use std::fmt::Write as _;

use hydra_common::{Chart, ChartData, LineSeries};

use super::human_number;

/// Fixed categorical series order (colorblind-validated on white; the
/// order is the safety mechanism — never cycle or re-sort it).
const SERIES: [&str; 8] = [
    "#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#4a3aa7", "#e34948",
];
const INK_SECONDARY: &str = "#52514e";
const INK_MUTED: &str = "#898781";
const GRID: &str = "#e1e0d9";
const BASELINE: &str = "#c3c2b7";
const FONT: &str = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif";

const WIDTH: f64 = 640.0;
const PLOT_HEIGHT: f64 = 220.0;
const MARGIN_LEFT: f64 = 52.0;
/// Room for the series names a line chart writes past its last point.
const MARGIN_RIGHT_LINE: f64 = 88.0;
/// A bar chart labels its bars from above, so it needs only enough room to
/// keep the last bar off the edge. Reserving the line chart's margin here
/// left a wide dead strip on every distribution chart in the report.
const MARGIN_RIGHT_BAR: f64 = 16.0;
const MARGIN_TOP: f64 = 30.0;
const MARGIN_BOTTOM: f64 = 46.0;

fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn axis_title(label: &str, unit: Option<&str>) -> String {
    match unit {
        Some(unit) => format!("{label} ({unit})"),
        None => label.to_string(),
    }
}

/// "Nice" ascending tick values covering `[0 or min, max]`.
fn ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    // NaN or degenerate ranges get a unit span.
    if max <= min || !max.is_finite() || !min.is_finite() {
        return vec![min, min + 1.0];
    }
    let span = max - min;
    let raw_step = span / target as f64;
    let magnitude = 10f64.powf(raw_step.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|s| span / s <= target as f64)
        .unwrap_or(magnitude * 10.0);
    let start = (min / step).floor() * step;
    let mut out = Vec::new();
    let mut tick = start;
    while tick <= max + step * 1e-9 {
        out.push(tick);
        tick += step;
    }
    out
}

/// Render a chart to a standalone inline-able `<svg>` element.
pub(crate) fn chart_svg(chart: &Chart) -> String {
    match &chart.data {
        ChartData::Bar { categories, values } => bar_svg(chart, categories, values),
        ChartData::Line { series } => line_svg(chart, series),
    }
}

// ── Bar (single series) ───────────────────────────────────────────────────────

fn bar_svg(chart: &Chart, categories: &[String], values: &[f64]) -> String {
    let height = MARGIN_TOP + PLOT_HEIGHT + MARGIN_BOTTOM;
    let plot_w = WIDTH - MARGIN_LEFT - MARGIN_RIGHT_BAR;
    let y_max = values.iter().copied().fold(0.0f64, f64::max);
    let y_ticks = ticks(0.0, y_max.max(1.0), 4);
    let y_top = y_ticks.last().copied().unwrap_or(1.0);
    let y = |v: f64| MARGIN_TOP + PLOT_HEIGHT - (v / y_top) * PLOT_HEIGHT;

    let mut s = svg_open(height);
    grid_and_y_axis(&mut s, &y_ticks, y, MARGIN_RIGHT_BAR);

    let n = categories.len().max(1);
    let band = plot_w / n as f64;
    let thickness = (band * 0.68).min(24.0);
    for (i, (category, &value)) in categories.iter().zip(values).enumerate() {
        let cx = MARGIN_LEFT + band * (i as f64 + 0.5);
        let x0 = cx - thickness / 2.0;
        let top = y(value.max(0.0));
        let bottom = y(0.0);
        let h = (bottom - top).max(0.0);
        // Rounded 4u data-end, square baseline.
        let r = 4.0f64.min(h).min(thickness / 2.0);
        let _ = writeln!(
            s,
            r#"<path d="M{x0:.1} {bottom:.1} V{:.1} Q{x0:.1} {top:.1} {:.1} {top:.1} H{:.1} Q{:.1} {top:.1} {:.1} {:.1} V{bottom:.1} Z" fill="{}"/>"#,
            top + r,
            x0 + r,
            x0 + thickness - r,
            x0 + thickness,
            x0 + thickness,
            top + r,
            SERIES[0],
        );
        // Value on the cap (relief for sub-contrast hues; exact values).
        let _ = writeln!(
            s,
            r#"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-family="{FONT}" font-size="11" fill="{INK_SECONDARY}">{}</text>"#,
            top - 5.0,
            esc(&human_number(value)),
        );
        // Category label.
        let _ = writeln!(
            s,
            r#"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-family="{FONT}" font-size="10" fill="{INK_MUTED}">{}</text>"#,
            MARGIN_TOP + PLOT_HEIGHT + 16.0,
            esc(category),
        );
    }

    axis_titles(&mut s, chart, height, MARGIN_RIGHT_BAR);
    s.push_str("</svg>\n");
    s
}

// ── Line (one or more series) ─────────────────────────────────────────────────

fn line_svg(chart: &Chart, series: &[LineSeries]) -> String {
    let legend_h = if series.len() >= 2 { 22.0 } else { 0.0 };
    let height = MARGIN_TOP + legend_h + PLOT_HEIGHT + MARGIN_BOTTOM;
    let plot_top = MARGIN_TOP + legend_h;
    let plot_w = WIDTH - MARGIN_LEFT - MARGIN_RIGHT_LINE;

    let xs: Vec<f64> = series
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p[0]))
        .collect();
    let ys: Vec<f64> = series
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p[1]))
        .collect();
    let (x_min, x_max) = min_max(&xs);
    let (y_min, y_max) = min_max(&ys);
    let y_ticks = ticks(y_min.min(0.0f64.max(y_min)), y_max, 4);
    let (y_lo, y_hi) = (
        y_ticks.first().copied().unwrap_or(0.0),
        y_ticks.last().copied().unwrap_or(1.0),
    );
    let x = |v: f64| {
        if x_max > x_min {
            MARGIN_LEFT + (v - x_min) / (x_max - x_min) * plot_w
        } else {
            MARGIN_LEFT + plot_w / 2.0
        }
    };
    let y = |v: f64| {
        if y_hi > y_lo {
            plot_top + PLOT_HEIGHT - (v - y_lo) / (y_hi - y_lo) * PLOT_HEIGHT
        } else {
            plot_top + PLOT_HEIGHT / 2.0
        }
    };

    let mut s = svg_open(height);

    // Legend (always for ≥ 2 series; never for one).
    if series.len() >= 2 {
        let mut lx = MARGIN_LEFT;
        for (i, sr) in series.iter().enumerate() {
            let color = SERIES[i % SERIES.len()];
            let _ = writeln!(
                s,
                r#"<rect x="{lx:.1}" y="{:.1}" width="10" height="10" rx="2" fill="{color}"/>"#,
                MARGIN_TOP - 9.0 + 6.0,
            );
            let _ = writeln!(
                s,
                r#"<text x="{:.1}" y="{:.1}" font-family="{FONT}" font-size="11" fill="{INK_SECONDARY}">{}</text>"#,
                lx + 14.0,
                MARGIN_TOP + 6.0,
                esc(&sr.name),
            );
            lx += 14.0 + 7.0 * (sr.name.chars().count() as f64) + 18.0;
        }
    }

    grid_and_y_axis(&mut s, &y_ticks, y, MARGIN_RIGHT_LINE);

    // X ticks.
    for tick in ticks(x_min, x_max, 5) {
        if tick < x_min - 1e-9 || tick > x_max + 1e-9 {
            continue;
        }
        let _ = writeln!(
            s,
            r#"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-family="{FONT}" font-size="10" fill="{INK_MUTED}">{}</text>"#,
            x(tick),
            plot_top + PLOT_HEIGHT + 16.0,
            esc(&human_number(tick)),
        );
    }

    // Series: 2u round-capped lines, end-dot with a 2u surface ring, and
    // a direct end-label when it does not collide with the previous one.
    let mut last_label_y = f64::NEG_INFINITY;
    let mut ends: Vec<(usize, f64)> = series
        .iter()
        .enumerate()
        .filter_map(|(i, sr)| sr.points.last().map(|p| (i, p[1])))
        .collect();
    ends.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let labelled: Vec<usize> = {
        let mut out = Vec::new();
        for (i, end_y) in &ends {
            let py = y(*end_y);
            if py - last_label_y >= 12.0 || last_label_y == f64::NEG_INFINITY {
                out.push(*i);
                last_label_y = py;
            }
        }
        out
    };
    for (i, sr) in series.iter().enumerate() {
        if sr.points.is_empty() {
            continue;
        }
        let color = SERIES[i % SERIES.len()];
        let path: Vec<String> = sr
            .points
            .iter()
            .enumerate()
            .map(|(j, p)| {
                format!(
                    "{}{:.1} {:.1}",
                    if j == 0 { "M" } else { "L" },
                    x(p[0]),
                    y(p[1])
                )
            })
            .collect();
        let _ = writeln!(
            s,
            r#"<path d="{}" fill="none" stroke="{color}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"#,
            path.join(" "),
        );
        let end = sr.points[sr.points.len() - 1];
        let _ = writeln!(
            s,
            r##"<circle cx="{:.1}" cy="{:.1}" r="4" fill="{color}" stroke="#ffffff" stroke-width="2"/>"##,
            x(end[0]),
            y(end[1]),
        );
        if labelled.contains(&i) {
            let _ = writeln!(
                s,
                r#"<text x="{:.1}" y="{:.1}" font-family="{FONT}" font-size="11" fill="{INK_SECONDARY}">{}</text>"#,
                x(end[0]) + 8.0,
                y(end[1]) + 4.0,
                esc(&sr.name),
            );
        }
    }

    axis_titles(&mut s, chart, height, MARGIN_RIGHT_LINE);
    s.push_str("</svg>\n");
    s
}

// ── Shared pieces ─────────────────────────────────────────────────────────────

fn svg_open(height: f64) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {WIDTH} {height}" width="{WIDTH}" height="{height}" role="img">
"#
    )
}

fn grid_and_y_axis(s: &mut String, y_ticks: &[f64], y: impl Fn(f64) -> f64, margin_right: f64) {
    for (i, &tick) in y_ticks.iter().enumerate() {
        let py = y(tick);
        let stroke = if i == 0 { BASELINE } else { GRID };
        let _ = writeln!(
            s,
            r#"<line x1="{MARGIN_LEFT}" y1="{py:.1}" x2="{:.1}" y2="{py:.1}" stroke="{stroke}" stroke-width="1"/>"#,
            WIDTH - margin_right,
        );
        let _ = writeln!(
            s,
            r#"<text x="{:.1}" y="{:.1}" text-anchor="end" font-family="{FONT}" font-size="10" fill="{INK_MUTED}">{}</text>"#,
            MARGIN_LEFT - 8.0,
            py + 3.5,
            esc(&human_number(tick)),
        );
    }
}

fn axis_titles(s: &mut String, chart: &Chart, height: f64, margin_right: f64) {
    // Y title horizontal at top-left (no rotated text — friendlier to the
    // pdf SVG pipeline); X title centered below the category/tick labels.
    let _ = writeln!(
        s,
        r#"<text x="{:.1}" y="{:.1}" font-family="{FONT}" font-size="10" fill="{INK_MUTED}">{}</text>"#,
        6.0,
        14.0,
        esc(&axis_title(&chart.y_label, chart.y_unit.as_deref())),
    );
    let _ = writeln!(
        s,
        r#"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-family="{FONT}" font-size="10" fill="{INK_MUTED}">{}</text>"#,
        MARGIN_LEFT + (WIDTH - MARGIN_LEFT - margin_right) / 2.0,
        height - 10.0,
        esc(&axis_title(&chart.x_label, chart.x_unit.as_deref())),
    );
}

fn min_max(values: &[f64]) -> (f64, f64) {
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if min.is_finite() && max.is_finite() {
        (min, max)
    } else {
        (0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar_chart() -> Chart {
        Chart {
            x_label: "Minimum pressure".into(),
            x_unit: Some("m".into()),
            x_quantity: None,
            y_label: "Junctions".into(),
            y_unit: None,
            y_quantity: None,
            data: ChartData::Bar {
                categories: vec!["0 – 14".into(), "14 – 28".into()],
                values: vec![3.0, 7.0],
            },
        }
    }

    #[test]
    fn bar_svg_carries_marks_labels_and_escaped_text() {
        let mut chart = bar_chart();
        if let ChartData::Bar { categories, .. } = &mut chart.data {
            categories[0] = "<0 & up".into();
        }
        let svg = chart_svg(&chart);
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains(SERIES[0]), "single series uses slot 1");
        assert!(svg.contains("&lt;0 &amp; up"), "category text is escaped");
        assert!(svg.contains(">7<"), "value label on the cap");
        assert!(svg.contains("Minimum pressure (m)"));
        assert!(!svg.contains("<script"));
    }

    #[test]
    fn line_svg_legend_rule_follows_series_count() {
        let series_one = Chart {
            x_label: "Time".into(),
            x_unit: Some("h".into()),
            x_quantity: None,
            y_label: "Head".into(),
            y_unit: Some("m".into()),
            y_quantity: None,
            data: ChartData::Line {
                series: vec![LineSeries {
                    name: "T1".into(),
                    points: vec![[0.0, 10.0], [1.0, 12.0]],
                }],
            },
        };
        let one = chart_svg(&series_one);
        assert!(
            !one.contains("<rect"),
            "single series draws no legend swatch"
        );

        let mut two = series_one.clone();
        if let ChartData::Line { series } = &mut two.data {
            series.push(LineSeries {
                name: "T2".into(),
                points: vec![[0.0, 8.0], [1.0, 6.0]],
            });
        }
        let two_svg = chart_svg(&two);
        assert!(two_svg.contains("<rect"), "two series draw a legend");
        assert!(two_svg.contains(SERIES[1]), "second series wears slot 2");
        assert!(
            two_svg.contains(r##"stroke="#ffffff" stroke-width="2""##),
            "end dots ring in surface"
        );
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(chart_svg(&bar_chart()), chart_svg(&bar_chart()));
    }
}
