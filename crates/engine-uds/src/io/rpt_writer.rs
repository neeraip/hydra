//! Text-report writer (§14.9).
//!
//! The report reproduces the predecessor's *layout*, not merely its
//! content: block order, titles and their asterisk rules, column
//! headings and unit rows, dashed table rules, field widths and decimal
//! places. It is read by people diffing this engine against the
//! predecessor and by tools that parse the predecessor's reports, and
//! both fail on a report carrying the right numbers in a different
//! shape.
//!
//! Every column here is drawn from the §11.2 statistics catalogue. A
//! column with no statistic behind it cannot be printed, which is why
//! §11.2 enumerates rather than gestures.
//!
//! Four content differences from the predecessor are inherent and
//! carried openly (§14.9): the flow-classification table's
//! adjusted/actual length ratio is identically 1 (§6.5 retired the
//! transform, the column stays for layout); the pumping table's
//! off-curve columns are both live for every pump type (§11.2); the
//! step statistics report rejections and degraded-accuracy tallies in
//! place of steady-state-skip time (§10.3); and the banner names this
//! engine, never the predecessor's.

use std::io::{self, Write};

use crate::hydraulics::routing::{step_bands, LinkStats, RoutingReport, VertexStats};
use crate::hydrology::runoff::ParcelTotals;
use crate::io::options::{Date, InfiltrationModel};
use crate::model::{LinkKind, Network, VertexKind};

/// Width of the identifier column in the node and link tables.
const ID_W: usize = 21;
/// Width of the identifier column in the load tables, which carry no
/// kind column beside it and so pack one narrower.
const LOAD_ID_W: usize = 20;
/// Width of the kind column beside it.
const KIND_W: usize = 9;

/// The report's fixed volume conversions per unit system.
struct Rv {
    us: bool,
    /// m³/s per file flow unit.
    flow: f64,
}

impl Rv {
    /// m³ to acre-feet | hectare-metres.
    fn big(&self, v: f64) -> f64 {
        if self.us {
            v / 1_233.481_837_547_52
        } else {
            v * 1.0e-4
        }
    }
    /// m³ to 10⁶ gallons | 10⁶ litres.
    fn mgal(&self, v: f64) -> f64 {
        if self.us {
            v * 2.641_720_523_581e-4
        } else {
            v * 1.0e-3
        }
    }
    /// m³ to 1000 ft³ | 1000 m³, the storage table's unit.
    fn kcuft(&self, v: f64) -> f64 {
        if self.us {
            v / 28.316_846_592
        } else {
            v * 1.0e-3
        }
    }
    /// A flow (m³/s) to the user's flow unit.
    fn q(&self, v: f64) -> f64 {
        v / self.flow
    }
    /// A velocity (m/s) to ft/s | m/s.
    fn vel(&self, v: f64) -> f64 {
        if self.us {
            v / 0.3048
        } else {
            v
        }
    }
    /// A length (m) to feet | metres.
    fn len(&self, v: f64) -> f64 {
        if self.us {
            v / 0.3048
        } else {
            v
        }
    }
    /// A depth (m) to inches | millimetres.
    fn depth(&self, v: f64) -> f64 {
        if self.us {
            v / 0.0254
        } else {
            v * 1000.0
        }
    }

    /// A pollutant load in internal mass to the predecessor's load units
    /// (§14.9): pounds | kilograms, or log₁₀ of the count (zero at zero)
    /// for count-type constituents.
    fn load(&self, units: crate::model::ConcentrationUnits, v: f64) -> f64 {
        use crate::model::ConcentrationUnits as Cu;
        match units {
            Cu::MgPerL => v / if self.us { 453.592_37 } else { 1000.0 },
            Cu::UgPerL => v / if self.us { 453_592.37 } else { 1.0e6 },
            Cu::CountPerL => {
                let count = v * 1.0e3;
                if count > 0.0 {
                    count.log10()
                } else {
                    0.0
                }
            }
        }
    }

    /// The load column's unit word (§14.9).
    fn load_word(&self, units: crate::model::ConcentrationUnits) -> &'static str {
        use crate::model::ConcentrationUnits as Cu;
        match units {
            Cu::CountPerL => "LogN",
            _ if self.us => "lbs",
            _ => "kg",
        }
    }

    /// The volume word for the big column.
    fn big_word(&self) -> &'static str {
        if self.us {
            "acre-feet"
        } else {
            "hectare-m"
        }
    }
    /// The volume word for the second continuity column.
    fn mgal_word(&self) -> &'static str {
        if self.us {
            "10^6 gal"
        } else {
            "10^6 ltr"
        }
    }
    /// The depth word for the runoff continuity's second column.
    fn depth_word(&self) -> &'static str {
        if self.us {
            "inches"
        } else {
            "mm"
        }
    }
    fn len_word(&self) -> &'static str {
        if self.us {
            "Feet"
        } else {
            "Meters"
        }
    }
    fn depth_unit(&self) -> &'static str {
        if self.us {
            "in"
        } else {
            "mm"
        }
    }
    fn vel_word(&self) -> &'static str {
        if self.us {
            "ft/sec"
        } else {
            "m/sec"
        }
    }
    fn kcuft_word(&self) -> &'static str {
        if self.us {
            "1000 ft³"
        } else {
            "1000 m³"
        }
    }
}

/// A constituent's concentration units by id; the default unit if the id
/// is unknown (it never is — the ledger keys come from the model).
fn constituent_units(inp: &ReportInputs<'_>, id: &str) -> crate::model::ConcentrationUnits {
    inp.net
        .constituents
        .iter()
        .find(|c| c.id == id)
        .map(|c| c.units)
        .unwrap_or(crate::model::ConcentrationUnits::MgPerL)
}

// ── Formatting primitives ───────────────────────────────────────────────

/// The predecessor's block heading: an asterisk rule the width of the
/// title, the title, and the rule again, after a two-line gap.
fn heading(w: &mut impl Write, title: &str) -> io::Result<()> {
    heading_ruled(w, title, title.len())
}

/// A block heading whose rule is not the title's width. Exactly one
/// block needs this: the predecessor rules "Link Flow Summary" with
/// twenty asterisks against a seventeen-character title. Parity means
/// reproducing that, not correcting it.
fn heading_ruled(w: &mut impl Write, title: &str, bar_w: usize) -> io::Result<()> {
    let bar = "*".repeat(bar_w);
    writeln!(w, "  \n  \n  {bar}\n  {title}\n  {bar}")
}

/// A table's dashed rule, `n` dashes after the two-space margin.
fn rule(w: &mut impl Write, n: usize) -> io::Result<()> {
    writeln!(w, "  {}", "-".repeat(n))
}

/// A continuity row: label, dot leader filling to column 28, then the
/// value columns 14 wide at three decimals.
fn line(w: &mut impl Write, label: &str, values: &[f64]) -> io::Result<()> {
    let dots = ".".repeat(25usize.saturating_sub(label.len()));
    write!(w, "  {label} {dots}")?;
    for v in values {
        write!(w, "{v:>14.3}")?;
    }
    writeln!(w)
}

/// The continuity block's three heading lines: the asterisk rule
/// carrying the column titles, the block name carrying the unit words,
/// and the rule carrying the dashes.
fn continuity_head(
    w: &mut impl Write,
    title: &str,
    columns: &[&str],
    units: &[&str],
    dashes: &[usize],
) -> io::Result<()> {
    // The asterisk rule is a fixed width in the predecessor, not the
    // title's — every continuity block's rule is the same length, and
    // shorter titles are padded out to it.
    const BAR_W: usize = 26;
    let bar = "*".repeat(BAR_W);
    // The rules and the title carry the column text with them, so the
    // three lines are built in parallel rather than written as a
    // heading followed by a header row.
    let mut top = format!("  {bar}");
    let mut mid = format!("  {title:<BAR_W$}");
    let mut bot = format!("  {bar}");
    for ((c, u), d) in columns.iter().zip(units).zip(dashes) {
        top.push_str(&format!("{c:>14}"));
        mid.push_str(&format!("{u:>14}"));
        bot.push_str(&format!("{:>14}", "-".repeat(*d)));
    }
    writeln!(w, "  \n  \n{top}\n{mid}\n{bot}")
}

/// An elapsed instant as the predecessor's `days hr:min`, 13 columns.
fn elapsed(t: f64) -> String {
    let total_min = (t / 60.0).floor().max(0.0);
    let days = (total_min / 1440.0).floor();
    let rem = total_min - days * 1440.0;
    let h = (rem / 60.0).floor();
    let m = rem - h * 60.0;
    format!(
        "{days:>6}  {h:02}:{m:02}",
        days = days as u64,
        h = h as u64,
        m = m as u64
    )
}

/// A duration as `hh:mm:ss`, the options block's step format.
fn hhmmss(sec: f64) -> String {
    let s = sec.max(0.0).round() as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// A date and time as `mm/dd/yyyy hh:mm:ss`.
fn stamp(d: Date, sec: f64) -> String {
    format!("{:02}/{:02}/{:04} {}", d.month, d.day, d.year, hhmmss(sec))
}

/// The calendar date `sec` seconds after midnight on `d`, as
/// `mm/dd/yyyy`. Days roll forward through the month lengths, leap
/// years included, so a control action late in a long run is dated
/// where it happened.
fn datestamp(d: Date, sec: f64) -> String {
    let (mut y, mut m, mut day) = (d.year, d.month, d.day + (sec / 86400.0) as u32);
    loop {
        let len = match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
            _ => 28,
        };
        if day <= len {
            break;
        }
        day -= len;
        m += 1;
        if m > 12 {
            m = 1;
            y += 1;
        }
    }
    format!("{m:02}/{day:02}/{y:04}")
}

/// The predecessor's `%g`-style volume field: three significant digits,
/// no trailing zeros, plain `0` at zero.
fn g3(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let mag = v.abs().log10().floor() as i32;
    if !(-5..6).contains(&mag) {
        return format!("{v:.3e}");
    }
    let decimals = (2 - mag).max(0) as usize;
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// A vertex's kind word.
fn vertex_kind(v: &crate::model::Vertex) -> &'static str {
    match v.kind {
        VertexKind::Junction { .. } => "JUNCTION",
        VertexKind::Outfall { .. } => "OUTFALL",
        VertexKind::Storage { .. } => "STORAGE",
        VertexKind::Divider { .. } => "DIVIDER",
    }
}

/// A link's kind word.
fn link_kind(l: &crate::model::Link) -> &'static str {
    match l.kind {
        LinkKind::Channel { .. } => "CONDUIT",
        LinkKind::Pump { .. } => "PUMP",
        LinkKind::Orifice { .. } => "ORIFICE",
        LinkKind::Weir { .. } => "WEIR",
        LinkKind::Outlet { .. } => "OUTLET",
    }
}

/// The inputs the report draws on, gathered by the session.
pub struct ReportInputs<'a> {
    pub net: &'a Network,
    /// The §11.1 surface ledger parts, when a surface exists:
    /// (precipitation, run-on, evaporation, infiltration, runoff,
    /// ploughed snow, initial storage, final storage, error %).
    pub surface: Option<[f64; 9]>,
    /// The §11.1 subsurface parts: (infiltration, evapotranspiration,
    /// deep percolation, lateral flow, initial, final, error %).
    pub subsurface: Option<[f64; 7]>,
    /// Flow-routing parts: (sanitary, wet-weather, subsurface, sewer,
    /// external, outflow, flooding, evaporation, exfiltration, initial,
    /// final, error %).
    pub flow: [f64; 12],
    /// Per-constituent quality parts: (id, the five §11.2 admitted
    /// loads by origin, discharged, flooded, exfiltrated, reacted,
    /// initial stored, final stored, error %).
    pub quality: Vec<(String, [f64; 12])>,
    /// Per-constituent §11.1 surface-loading parts: (id, initial
    /// buildup, buildup, deposition, swept, infiltrated, BMP removed,
    /// washed off, remaining, error %).
    pub loading: Vec<(String, [f64; 9])>,
    /// Control actions: (elapsed s, link, setting, rule).
    pub actions: &'a [(f64, String, f64, String)],
    /// The whole-run numerical-performance statistics (§11.2).
    pub performance: &'a RoutingReport,
    /// §11.2 per-vertex and per-link statistics.
    pub vertex_stats: &'a [VertexStats],
    pub link_stats: &'a [LinkStats],
    /// Per-parcel §11.2 totals, parallel to the model's parcels.
    pub parcel_totals: Vec<ParcelTotals>,
    /// Per-parcel delivered washoff `[parcel][constituent]` (U).
    pub washoff_by_parcel: Option<Vec<Vec<f64>>>,
    /// Per-outfall discharged mass `[constituent][vertex]`.
    pub outfall_loads: Option<Vec<Vec<f64>>>,
    /// Per-link transported mass `[constituent][link]`.
    pub link_loads: Option<Vec<Vec<f64>>>,
    /// The top worst-error vertices: (id, accepted-step count).
    pub worst: Vec<(String, u64)>,
}

/// Write the §14.9 text report.
pub fn write_rpt(inp: &ReportInputs, w: &mut impl Write) -> io::Result<()> {
    let opt = &inp.net.options;
    let rv = Rv {
        us: opt.flow_units.is_us(),
        flow: match opt.flow_units {
            crate::io::options::FlowUnits::Cfs => 0.028_316_846_592,
            crate::io::options::FlowUnits::Gpm => 6.309_019_64e-5,
            crate::io::options::FlowUnits::Mgd => 0.043_812_636_4,
            crate::io::options::FlowUnits::Cms => 1.0,
            crate::io::options::FlowUnits::Lps => 1.0e-3,
            crate::io::options::FlowUnits::Mld => 1.0 / 86.4,
        },
    };

    write_banner(inp, w)?;
    write_options(inp, &rv, w)?;
    write_control_actions(inp, w)?;
    write_continuity(inp, &rv, w)?;
    write_diagnostics(inp, w)?;
    write_step_summary(inp, w)?;
    write_summary_tables(inp, &rv, w)?;
    writeln!(w, "\n  Analysis complete.")?;
    Ok(())
}

// ── Banner and options ──────────────────────────────────────────────────

fn write_banner(inp: &ReportInputs, w: &mut impl Write) -> io::Result<()> {
    // §14.9: the banner names this engine. A report is evidence of what
    // produced it, and a reader who cannot tell the two apart cannot use
    // it as evidence.
    writeln!(
        w,
        "\n  HYDRA URBAN DRAINAGE ENGINE - VERSION {}",
        env!("CARGO_PKG_VERSION")
    )?;
    writeln!(w, "  {}", "-".repeat(58))?;
    writeln!(w)?;
    for t in &inp.net.title {
        writeln!(w, "  {t}")?;
    }
    Ok(())
}

fn write_options(inp: &ReportInputs, rv: &Rv, w: &mut impl Write) -> io::Result<()> {
    let opt = &inp.net.options;
    let yes = |b: bool| if b { "YES" } else { "NO" };
    let runoff = !opt.ignore_rainfall && !inp.net.parcels.is_empty();
    heading(w, "Analysis Options")?;
    writeln!(
        w,
        "  Flow Units ............... {}",
        format!("{:?}", opt.flow_units).to_uppercase()
    )?;
    writeln!(w, "  Process Models:")?;
    writeln!(w, "    Rainfall/Runoff ........ {}", yes(runoff))?;
    writeln!(
        w,
        "    RDII ................... {}",
        yes(!opt.ignore_rdii && !inp.net.rdii.is_empty())
    )?;
    writeln!(
        w,
        "    Snowmelt ............... {}",
        yes(!opt.ignore_snowmelt && !inp.net.snowpacks.is_empty())
    )?;
    writeln!(
        w,
        "    Groundwater ............ {}",
        yes(!opt.ignore_groundwater && !inp.net.aquifers.is_empty())
    )?;
    writeln!(
        w,
        "    Flow Routing ........... {}",
        yes(!opt.ignore_routing)
    )?;
    writeln!(w, "    Ponding Allowed ........ {}", yes(opt.allow_ponding))?;
    writeln!(
        w,
        "    Water Quality .......... {}",
        yes(!opt.ignore_quality && !inp.net.constituents.is_empty())
    )?;
    if runoff {
        writeln!(
            w,
            "  Infiltration Method ...... {}",
            match opt.infiltration {
                InfiltrationModel::Horton => "HORTON",
                InfiltrationModel::ModifiedHorton => "MODIFIED_HORTON",
                InfiltrationModel::GreenAmpt => "GREEN_AMPT",
                InfiltrationModel::ModifiedGreenAmpt => "MODIFIED_GREEN_AMPT",
                InfiltrationModel::CurveNumber => "CURVE_NUMBER",
            }
        )?;
    }
    // §6.1: this engine has one solver. A file requesting a reduced form
    // was substituted at import with a notice, so the report states what
    // actually ran.
    writeln!(w, "  Flow Routing Method ...... DYNWAVE")?;
    // §6.2: pressurised flow rides a Preissmann slot, which is the
    // predecessor's SLOT option rather than its EXTRAN one.
    writeln!(w, "  Surcharge Method ......... SLOT")?;
    writeln!(
        w,
        "  Starting Date ............ {}",
        stamp(opt.start_date, opt.start_time)
    )?;
    writeln!(
        w,
        "  Ending Date .............. {}",
        stamp(opt.end_date, opt.end_time)
    )?;
    writeln!(w, "  Antecedent Dry Days ...... {:.1}", opt.dry_days)?;
    writeln!(
        w,
        "  Report Time Step ......... {}",
        hhmmss(opt.report_step)
    )?;
    if runoff {
        writeln!(w, "  Wet Time Step ............ {}", hhmmss(opt.wet_step))?;
        writeln!(w, "  Dry Time Step ............ {}", hhmmss(opt.dry_step))?;
    }
    writeln!(
        w,
        "  Routing Time Step ........ {:.2} sec",
        opt.routing_step
    )?;
    writeln!(
        w,
        "  Variable Time Step ....... {}",
        yes(opt.courant_factor > 0.0)
    )?;
    writeln!(w, "  Maximum Trials ........... {}", opt.max_trials)?;
    writeln!(w, "  Number of Threads ........ {}", opt.threads.max(1))?;
    writeln!(
        w,
        "  Head Tolerance ........... {:.6} {}",
        rv.len(opt.head_tol),
        if rv.us { "ft" } else { "m" }
    )?;
    Ok(())
}

// ── Continuity blocks ───────────────────────────────────────────────────

fn write_continuity(inp: &ReportInputs, rv: &Rv, w: &mut impl Write) -> io::Result<()> {
    let total_area: f64 = inp.net.parcels.iter().map(|p| p.area).sum();
    let dep = |v: f64| {
        if total_area > 0.0 {
            rv.depth(v / total_area)
        } else {
            0.0
        }
    };

    if let Some(s) = inp.surface {
        let [rain, runon, evap, infil, runoff, plowed, init, fin, err] = s;
        continuity_head(
            w,
            "Runoff Quantity Continuity",
            &["Volume", "Depth"],
            &[rv.big_word(), rv.depth_word()],
            &[9, 7],
        )?;
        if init > 0.0 {
            line(w, "Initial LID Storage", &[rv.big(init), dep(init)])?;
        }
        line(w, "Total Precipitation", &[rv.big(rain), dep(rain)])?;
        if runon > 0.0 {
            line(w, "Upstream Runon", &[rv.big(runon), dep(runon)])?;
        }
        line(w, "Evaporation Loss", &[rv.big(evap), dep(evap)])?;
        line(w, "Infiltration Loss", &[rv.big(infil), dep(infil)])?;
        line(w, "Surface Runoff", &[rv.big(runoff), dep(runoff)])?;
        if plowed > 0.0 {
            line(w, "Snow Removed", &[rv.big(plowed), dep(plowed)])?;
        }
        line(w, "Final Storage", &[rv.big(fin), dep(fin)])?;
        line(w, "Continuity Error (%)", &[err])?;
    }

    // Runoff quality continuity — the §11.1 surface-loading ledger, one
    // column per constituent.
    if !inp.loading.is_empty() {
        let names: Vec<&str> = inp.loading.iter().map(|(id, _)| id.as_str()).collect();
        let units: Vec<&str> = inp
            .loading
            .iter()
            .map(|(id, _)| rv.load_word(constituent_units(inp, id)))
            .collect();
        continuity_head(
            w,
            "Runoff Quality Continuity",
            &names,
            &units,
            &vec![10; names.len()],
        )?;
        let col = |k: usize| -> Vec<f64> {
            inp.loading
                .iter()
                .map(|(id, v)| rv.load(constituent_units(inp, id), v[k]))
                .collect()
        };
        line(w, "Initial Buildup", &col(0))?;
        line(w, "Surface Buildup", &col(1))?;
        line(w, "Wet Deposition", &col(2))?;
        line(w, "Sweeping Removal", &col(3))?;
        line(w, "Infiltration Loss", &col(4))?;
        line(w, "BMP Removal", &col(5))?;
        line(w, "Surface Runoff", &col(6))?;
        line(w, "Remaining Buildup", &col(7))?;
        let errs: Vec<f64> = inp.loading.iter().map(|(_, v)| v[8]).collect();
        line(w, "Continuity Error (%)", &errs)?;
    }

    if let Some(g) = inp.subsurface {
        let [infil, evap, perc, lateral, init, fin, err] = g;
        continuity_head(
            w,
            "Groundwater Continuity",
            &["Volume", "Depth"],
            &[rv.big_word(), rv.depth_word()],
            &[9, 7],
        )?;
        line(w, "Initial Storage", &[rv.big(init), dep(init)])?;
        line(w, "Infiltration", &[rv.big(infil), dep(infil)])?;
        line(w, "Evapotranspiration", &[rv.big(evap), dep(evap)])?;
        line(w, "Deep Percolation", &[rv.big(perc), dep(perc)])?;
        line(w, "Groundwater Flow", &[rv.big(lateral), dep(lateral)])?;
        line(w, "Final Storage", &[rv.big(fin), dep(fin)])?;
        line(w, "Continuity Error (%)", &[err])?;
    }

    {
        let [dwf, wet, gw, rdii, ext, out, flood, evap, exfil, init, fin, err] = inp.flow;
        continuity_head(
            w,
            "Flow Routing Continuity",
            &["Volume", "Volume"],
            &[rv.big_word(), rv.mgal_word()],
            &[9, 9],
        )?;
        let row = |v: f64| [rv.big(v), rv.mgal(v)];
        line(w, "Dry Weather Inflow", &row(dwf))?;
        line(w, "Wet Weather Inflow", &row(wet))?;
        line(w, "Groundwater Inflow", &row(gw))?;
        line(w, "RDII Inflow", &row(rdii))?;
        line(w, "External Inflow", &row(ext))?;
        line(w, "External Outflow", &row(out))?;
        line(w, "Flooding Loss", &row(flood))?;
        line(w, "Evaporation Loss", &row(evap))?;
        line(w, "Exfiltration Loss", &row(exfil))?;
        line(w, "Initial Stored Volume", &row(init))?;
        line(w, "Final Stored Volume", &row(fin))?;
        line(w, "Continuity Error (%)", &[err])?;
    }

    if !inp.quality.is_empty() {
        let names: Vec<&str> = inp.quality.iter().map(|(id, _)| id.as_str()).collect();
        let units: Vec<&str> = inp
            .quality
            .iter()
            .map(|(id, _)| rv.load_word(constituent_units(inp, id)))
            .collect();
        continuity_head(
            w,
            "Quality Routing Continuity",
            &names,
            &units,
            &vec![10; names.len()],
        )?;
        let col = |k: usize| -> Vec<f64> {
            inp.quality
                .iter()
                .map(|(id, v)| rv.load(constituent_units(inp, id), v[k]))
                .collect()
        };
        // The admitted load is split by origin (§11.2) into the same
        // five the volumetric ledger uses, so the two blocks partition
        // the same question.
        line(w, "Dry Weather Inflow", &col(0))?;
        line(w, "Wet Weather Inflow", &col(1))?;
        line(w, "Groundwater Inflow", &col(2))?;
        line(w, "RDII Inflow", &col(3))?;
        line(w, "External Inflow", &col(4))?;
        line(w, "External Outflow", &col(5))?;
        line(w, "Flooding Loss", &col(6))?;
        line(w, "Exfiltration Loss", &col(7))?;
        line(w, "Mass Reacted", &col(8))?;
        line(w, "Initial Stored Mass", &col(9))?;
        line(w, "Final Stored Mass", &col(10))?;
        let errs: Vec<f64> = inp.quality.iter().map(|(_, v)| v[11]).collect();
        line(w, "Continuity Error (%)", &errs)?;
    }
    Ok(())
}

fn write_control_actions(inp: &ReportInputs, w: &mut impl Write) -> io::Result<()> {
    if inp.actions.is_empty() {
        return Ok(());
    }
    let opt = &inp.net.options;
    heading(w, "Control Actions Taken")?;
    for (t, link, value, rule_id) in inp.actions {
        writeln!(
            w,
            "   {}: {} Link {link} setting changed to {value:>6.2} by Control {rule_id}",
            datestamp(opt.start_date, opt.start_time + t),
            hhmmss((opt.start_time + t) % 86400.0)
        )?;
    }
    Ok(())
}

fn write_diagnostics(inp: &ReportInputs, w: &mut impl Write) -> io::Result<()> {
    // The §6.5 error estimate names a governing vertex on every accepted
    // step; the ones named most often are the elements the step size is
    // actually being chosen for.
    heading(w, "Time-Step Critical Elements")?;
    let total: u64 = inp.performance.accepted.max(1);
    if inp.worst.is_empty() {
        writeln!(w, "  None")?;
    } else {
        for (id, n) in &inp.worst {
            writeln!(w, "  Node {id} ({:.2}%)", 100.0 * *n as f64 / total as f64)?;
        }
    }

    heading(w, "Highest Flow Instability Indexes")?;
    // The index is the turn count as a percentage of the link's own
    // accepted steps, so a long run and a short one are comparable.
    // Below the threshold a link is stable, and the predecessor says so
    // in a sentence rather than printing an empty list.
    const STABLE_BELOW: u64 = 10;
    let mut unstable: Vec<(&str, u64)> = inp
        .net
        .links
        .iter()
        .zip(inp.link_stats)
        .filter(|(_, st)| st.steps > 0)
        .map(|(l, st)| (l.id.as_str(), 100 * st.instability_count / st.steps))
        .filter(|(_, index)| *index >= STABLE_BELOW)
        .collect();
    unstable.sort_by_key(|x| std::cmp::Reverse(x.1));
    unstable.truncate(5);
    if unstable.is_empty() {
        writeln!(w, "  All links are stable.")?;
    } else {
        for (id, n) in unstable {
            writeln!(w, "  Link {id} ({n})")?;
        }
    }

    heading(w, "Most Frequent Nonconverging Nodes")?;
    if inp.performance.nonconverged == 0 || inp.worst.is_empty() {
        writeln!(w, "  Convergence obtained at all time steps.")?;
    } else {
        for (id, n) in &inp.worst {
            writeln!(w, "  Node {id} ({n})")?;
        }
    }
    Ok(())
}

fn write_step_summary(inp: &ReportInputs, w: &mut impl Write) -> io::Result<()> {
    let p = inp.performance;
    let accepted = p.accepted.max(1);
    heading(w, "Routing Time Step Summary")?;
    writeln!(w, "  Minimum Time Step           : {:>8.2} sec", p.dt_min)?;
    writeln!(
        w,
        "  Average Time Step           : {:>8.2} sec",
        p.elapsed / accepted as f64
    )?;
    writeln!(w, "  Maximum Time Step           : {:>8.2} sec", p.dt_max)?;
    writeln!(
        w,
        "  Average Iterations per Step : {:>8.2}",
        p.iterations as f64 / accepted as f64
    )?;
    writeln!(
        w,
        "  % of Steps Not Converging   : {:>8.2}",
        100.0 * p.nonconverged as f64 / accepted as f64
    )?;
    // §10.3 retired the steady-state skip, so its row is absent rather
    // than a zero; the rejection and degraded tallies stand in (§14.9).
    writeln!(w, "  Trials Rejected             : {:>8}", p.rejected)?;
    writeln!(w, "  Degraded-Accuracy Steps     : {:>8}", p.degraded.len())?;
    writeln!(w, "  Time Step Frequencies       :")?;
    let edges = step_bands(inp.net.options.routing_step);
    for k in 0..5 {
        writeln!(
            w,
            "  {:>10.3} - {:>6.3} sec      : {:>8.2} %",
            edges[k],
            edges[k + 1],
            100.0 * p.dt_bands[k] as f64 / accepted as f64
        )?;
    }
    Ok(())
}

// ── Per-object summary tables ───────────────────────────────────────────
// The header rows below are literals rather than a shared width table
// because the predecessor's are: it formats each header line with its
// own printf widths, and successive lines of the same table do not
// always agree with each other or with the data row beneath them. Only
// the data rows are computed, from the widths measured off its output.

#[allow(clippy::too_many_lines)]
fn write_summary_tables(inp: &ReportInputs, rv: &Rv, w: &mut impl Write) -> io::Result<()> {
    let hr = |sec: f64| sec / 3600.0;
    let fu = format!("{:?}", inp.net.options.flow_units).to_uppercase();
    let lw = rv.len_word();

    // ── Subcatchment runoff summary ─────────────────────────────────────
    if !inp.net.parcels.is_empty() {
        heading(w, "Subcatchment Runoff Summary")?;
        writeln!(w, "  ")?;
        rule(w, 126)?;
        writeln!(
            w,
            "                            Total      Total      Total      Total     Imperv       Perv      Total       Total     Peak  Runoff"
        )?;
        writeln!(
            w,
            "                           Precip      Runon       Evap      Infil     Runoff     Runoff     Runoff      Runoff   Runoff   Coeff"
        )?;
        let d = rv.depth_unit();
        writeln!(
            w,
            "  {:<20}{d:>11}{d:>11}{d:>11}{d:>11}{d:>11}{d:>11}{d:>11}{:>12}{fu:>9}",
            "Subcatchment",
            rv.mgal_word()
        )?;
        rule(w, 126)?;
        for (p, t) in inp.net.parcels.iter().zip(&inp.parcel_totals) {
            let area = p.area.max(1e-12);
            let supply = t.precip + t.runon;
            let coeff = if supply > 0.0 { t.runoff / supply } else { 0.0 };
            writeln!(
                w,
                "  {:<20}{:>11.2}{:>11.2}{:>11.2}{:>11.2}{:>11.2}{:>11.2}{:>11.2}{:>12.2}{:>9.2}{:>8.3}",
                p.id,
                rv.depth(t.precip / area),
                rv.depth(t.runon / area),
                rv.depth(t.evap / area),
                rv.depth(t.infil / area),
                rv.depth(t.imperv_runoff / area),
                rv.depth(t.perv_runoff / area),
                rv.depth(t.runoff / area),
                rv.mgal(t.runoff),
                rv.q(t.peak_runoff),
                coeff
            )?;
        }
    }

    // ── Subcatchment washoff summary ────────────────────────────────────
    if let Some(loads) = &inp.washoff_by_parcel {
        if !inp.net.constituents.is_empty() {
            heading(w, "Subcatchment Washoff Summary")?;
            writeln!(w, "  ")?;
            let width = LOAD_ID_W + 14 * inp.net.constituents.len();
            rule(w, width)?;
            write!(w, "  {:<LOAD_ID_W$}", "")?;
            for c in &inp.net.constituents {
                write!(w, "{:>14}", c.id)?;
            }
            writeln!(w)?;
            write!(w, "  {:<LOAD_ID_W$}", "Subcatchment")?;
            for c in &inp.net.constituents {
                write!(w, "{:>14}", rv.load_word(c.units))?;
            }
            writeln!(w)?;
            rule(w, width)?;
            let mut totals = vec![0.0; inp.net.constituents.len()];
            for (p, row) in inp.net.parcels.iter().zip(loads) {
                write!(w, "  {:<LOAD_ID_W$}", p.id)?;
                for (ci, (c, v)) in inp.net.constituents.iter().zip(row).enumerate() {
                    totals[ci] += *v;
                    write!(w, "{:>14.3}", rv.load(c.units, *v))?;
                }
                writeln!(w)?;
            }
            rule(w, width)?;
            write!(w, "  {:<LOAD_ID_W$}", "System")?;
            for (c, v) in inp.net.constituents.iter().zip(&totals) {
                write!(w, "{:>14.3}", rv.load(c.units, *v))?;
            }
            writeln!(w)?;
        }
    }

    // ── Node depth summary ──────────────────────────────────────────────
    heading(w, "Node Depth Summary")?;
    writeln!(w, "  ")?;
    rule(w, 81)?;
    writeln!(
        w,
        "                                 Average  Maximum  Maximum  Time of Max    Reported"
    )?;
    writeln!(
        w,
        "                                   Depth    Depth      HGL   Occurrence   Max Depth"
    )?;
    writeln!(
        w,
        "  {:<ID_W$}{:<KIND_W$}{lw:>8}{lw:>9}{lw:>9}{:>13}{lw:>12}",
        "Node", "Type", "days hr:min"
    )?;
    rule(w, 81)?;
    for (v, st) in inp.net.vertices.iter().zip(inp.vertex_stats) {
        let mean = if st.obs_time > 0.0 {
            st.depth_sum / st.obs_time
        } else {
            0.0
        };
        writeln!(
            w,
            "  {:<ID_W$}{:<KIND_W$}{:>8.2}{:>9.2}{:>9.2}{:>13}{:>12.2}",
            v.id,
            vertex_kind(v),
            rv.len(mean),
            rv.len(st.max_depth),
            rv.len(v.invert + st.max_depth),
            elapsed(st.t_max_depth),
            rv.len(st.reported_max_depth)
        )?;
    }

    // ── Node inflow summary ─────────────────────────────────────────────
    heading(w, "Node Inflow Summary")?;
    writeln!(w, "  ")?;
    rule(w, 97)?;
    writeln!(
        w,
        "                                  Maximum  Maximum                  Lateral       Total        Flow"
    )?;
    writeln!(
        w,
        "                                  Lateral    Total  Time of Max      Inflow      Inflow     Balance"
    )?;
    writeln!(
        w,
        "                                   Inflow   Inflow   Occurrence      Volume      Volume       Error"
    )?;
    writeln!(
        w,
        "  {:<ID_W$}{:<KIND_W$}{fu:>9}{fu:>9}{:>13}{:>12}{:>12}{:>12}",
        "Node",
        "Type",
        "days hr:min",
        rv.mgal_word(),
        rv.mgal_word(),
        "Percent"
    )?;
    rule(w, 97)?;
    for (v, st) in inp.net.vertices.iter().zip(inp.vertex_stats) {
        // The §11.1 error statistic applied to this vertex alone: what
        // came in against what left and what stayed.
        let inflow = st.total_inflow_volume + st.initial_volume;
        let outflow = st.outflow_volume + st.final_volume;
        let err = if inflow > 1e-9 {
            100.0 * (1.0 - outflow / inflow)
        } else {
            0.0
        };
        writeln!(
            w,
            "  {:<ID_W$}{:<KIND_W$}{:>9.2}{:>9.2}{:>13}{:>12}{:>12}{:>12.3}",
            v.id,
            vertex_kind(v),
            rv.q(st.max_lat_inflow),
            rv.q(st.max_total_inflow),
            elapsed(st.t_max_total_inflow),
            g3(rv.mgal(st.lat_inflow_volume)),
            g3(rv.mgal(st.total_inflow_volume)),
            err
        )?;
    }

    // ── Node surcharge summary ──────────────────────────────────────────
    heading(w, "Node Surcharge Summary")?;
    writeln!(w, "  ")?;
    let surcharged: Vec<_> = inp
        .net
        .vertices
        .iter()
        .zip(inp.vertex_stats)
        .filter(|(_, st)| st.surcharge_time > 0.0)
        .collect();
    if surcharged.is_empty() {
        writeln!(w, "  No nodes were surcharged.")?;
    } else {
        writeln!(
            w,
            "  Surcharging occurs when water rises above the top of the highest conduit."
        )?;
        rule(w, 69)?;
        writeln!(
            w,
            "                                               Max. Height   Min. Depth"
        )?;
        writeln!(
            w,
            "                                   Hours       Above Crown    Below Rim"
        )?;
        writeln!(
            w,
            "  {:<ID_W$}{:<KIND_W$}{:>11}{lw:>15}{lw:>13}",
            "Node", "Type", "Surcharged"
        )?;
        rule(w, 69)?;
        for (v, st) in surcharged {
            writeln!(
                w,
                "  {:<ID_W$}{:<KIND_W$}{:>11.2}{:>15.3}{:>13.3}",
                v.id,
                vertex_kind(v),
                hr(st.surcharge_time),
                rv.len(st.max_crown_height),
                rv.len(st.min_rim_depth)
            )?;
        }
    }

    // ── Node flooding summary ───────────────────────────────────────────
    heading(w, "Node Flooding Summary")?;
    writeln!(w, "  ")?;
    let flooded: Vec<_> = inp
        .net
        .vertices
        .iter()
        .zip(inp.vertex_stats)
        .filter(|(_, st)| st.flood_time > 0.0)
        .collect();
    if flooded.is_empty() {
        writeln!(w, "  No nodes were flooded.")?;
    } else {
        writeln!(
            w,
            "  Flooding refers to all water that overflows a node, whether it ponds or not."
        )?;
        rule(w, 74)?;
        writeln!(
            w,
            "                                                             Total   Maximum"
        )?;
        writeln!(
            w,
            "                                 Maximum   Time of Max       Flood    Ponded"
        )?;
        writeln!(
            w,
            "                        Hours       Rate    Occurrence      Volume    Volume"
        )?;
        writeln!(
            w,
            "  {:<ID_W$}{:>7}{fu:>10}{:>14}{:>12}{:>10}",
            "Node",
            "Flooded",
            "days hr:min",
            rv.mgal_word(),
            rv.kcuft_word()
        )?;
        rule(w, 74)?;
        for (v, st) in flooded {
            writeln!(
                w,
                "  {:<ID_W$}{:>7.2}{:>10.2}{:>14}{:>12.3}{:>10.3}",
                v.id,
                hr(st.flood_time),
                rv.q(st.max_flood),
                elapsed(st.t_max_flood),
                rv.mgal(st.flood_volume),
                rv.kcuft(st.max_ponded_volume)
            )?;
        }
    }

    // ── Storage volume summary ──────────────────────────────────────────
    let storages: Vec<_> = inp
        .net
        .vertices
        .iter()
        .zip(inp.vertex_stats)
        .filter(|(_, st)| st.full_volume > 0.0)
        .collect();
    if !storages.is_empty() {
        heading(w, "Storage Volume Summary")?;
        writeln!(w, "  ")?;
        rule(w, 96)?;
        writeln!(
            w,
            "                         Average    Avg   Evap  Exfil     Maximum    Max    Time of Max    Maximum"
        )?;
        writeln!(
            w,
            "                          Volume   Pcnt   Pcnt   Pcnt      Volume   Pcnt     Occurrence    Outflow"
        )?;
        writeln!(
            w,
            "  {:<22}{:>8}{:>7}{:>7}{:>7}{:>12}{:>7}{:>15}{fu:>11}",
            "Storage Unit",
            rv.kcuft_word(),
            "Full",
            "Loss",
            "Loss",
            rv.kcuft_word(),
            "Full",
            "days hr:min"
        )?;
        rule(w, 96)?;
        for (v, st) in storages {
            let mean = if st.obs_time > 0.0 {
                st.volume_sum / st.obs_time
            } else {
                0.0
            };
            // The losses are a share of everything that passed through
            // the unit, which is the only bounded denominator available:
            // a mean-volume ratio is unbounded on a unit that fills and
            // empties.
            let through = (st.total_inflow_volume + st.initial_volume).max(1e-12);
            writeln!(
                w,
                "  {:<22}{:>8.3}{:>7.1}{:>7.1}{:>7.1}{:>12.3}{:>7.1}{:>15}{:>11.2}",
                v.id,
                rv.kcuft(mean),
                100.0 * mean / st.full_volume,
                100.0 * st.evap_loss_volume / through,
                100.0 * st.exfil_loss_volume / through,
                rv.kcuft(st.max_volume),
                100.0 * st.max_volume / st.full_volume,
                elapsed(st.t_max_volume),
                rv.q(st.max_outflow)
            )?;
        }
    }

    // ── Outfall loading summary ─────────────────────────────────────────
    let outfalls: Vec<usize> = inp
        .net
        .vertices
        .iter()
        .enumerate()
        .filter(|(_, v)| matches!(v.kind, VertexKind::Outfall { .. }))
        .map(|(i, _)| i)
        .collect();
    if !outfalls.is_empty() {
        heading(w, "Outfall Loading Summary")?;
        writeln!(w, "  ")?;
        let width = 59 + 14 * inp.net.constituents.len();
        rule(w, width)?;
        write!(
            w,
            "                         Flow       Avg       Max       Total"
        )?;
        for _ in &inp.net.constituents {
            write!(w, "{:>14}", "Total")?;
        }
        writeln!(w)?;
        write!(
            w,
            "                         Freq      Flow      Flow      Volume"
        )?;
        for c in &inp.net.constituents {
            write!(w, "{:>14}", c.id)?;
        }
        writeln!(w)?;
        write!(
            w,
            "  {:<22}{:>5}{fu:>10}{fu:>10}{:>12}",
            "Outfall Node",
            "Pcnt",
            rv.mgal_word()
        )?;
        for c in &inp.net.constituents {
            write!(w, "{:>14}", rv.load_word(c.units))?;
        }
        writeln!(w)?;
        rule(w, width)?;
        let (mut sum_time, mut sum_obs, mut sum_vol, mut peak) = (0.0, 0.0, 0.0, 0.0_f64);
        let mut totals = vec![0.0; inp.net.constituents.len()];
        for &vi in &outfalls {
            let st = &inp.vertex_stats[vi];
            let freq = if st.obs_time > 0.0 {
                100.0 * st.out_time / st.obs_time
            } else {
                0.0
            };
            let avg = if st.out_time > 0.0 {
                st.out_volume / st.out_time
            } else {
                0.0
            };
            sum_time += st.out_time;
            sum_obs += st.obs_time;
            sum_vol += st.out_volume;
            peak = peak.max(st.out_peak);
            write!(
                w,
                "  {:<22}{:>5.2}{:>10.2}{:>10.2}{:>12.3}",
                inp.net.vertices[vi].id,
                freq,
                rv.q(avg),
                rv.q(st.out_peak),
                rv.mgal(st.out_volume)
            )?;
            if let Some(loads) = &inp.outfall_loads {
                for (ci, (c, row)) in inp.net.constituents.iter().zip(loads).enumerate() {
                    totals[ci] += row[vi];
                    write!(w, "{:>14.3}", rv.load(c.units, row[vi]))?;
                }
            }
            writeln!(w)?;
        }
        rule(w, width)?;
        let sys_freq = if sum_obs > 0.0 {
            100.0 * sum_time / sum_obs
        } else {
            0.0
        };
        let sys_avg = if sum_time > 0.0 {
            sum_vol / sum_time * outfalls.len() as f64
        } else {
            0.0
        };
        write!(
            w,
            "  {:<22}{:>5.2}{:>10.2}{:>10.2}{:>12.3}",
            "System",
            sys_freq,
            rv.q(sys_avg),
            rv.q(peak),
            rv.mgal(sum_vol)
        )?;
        for (c, v) in inp.net.constituents.iter().zip(&totals) {
            write!(w, "{:>14.3}", rv.load(c.units, *v))?;
        }
        writeln!(w)?;
    }

    // ── Link flow summary ───────────────────────────────────────────────
    heading_ruled(w, "Link Flow Summary", 20)?;
    writeln!(w, "  ")?;
    rule(w, 77)?;
    writeln!(
        w,
        "                                 Maximum  Time of Max   Maximum    Max/    Max/"
    )?;
    writeln!(
        w,
        "                                  |Flow|   Occurrence   |Veloc|    Full    Full"
    )?;
    writeln!(
        w,
        "  {:<ID_W$}{:<KIND_W$}{fu:>8}{:>13}{:>10}{:>8}{:>8}",
        "Link",
        "Type",
        "days hr:min",
        rv.vel_word(),
        "Flow",
        "Depth"
    )?;
    rule(w, 77)?;
    for (l, st) in inp.net.links.iter().zip(inp.link_stats) {
        write!(
            w,
            "  {:<ID_W$}{:<KIND_W$}{:>8.2}{:>13}",
            l.id,
            link_kind(l),
            rv.q(st.max_flow),
            elapsed(st.t_max_flow)
        )?;
        if st.full_flow > 0.0 && st.full_depth > 0.0 {
            writeln!(
                w,
                "{:>10.2}{:>8.2}{:>8.2}",
                rv.vel(st.max_velocity),
                st.max_flow / st.full_flow,
                st.max_depth / st.full_depth
            )?;
        } else {
            // Pumps and regulators have no section, so no velocity and
            // nothing to be full of; the columns stay blank rather than
            // printing zeros that read as measurements.
            writeln!(w)?;
        }
    }

    // ── Flow classification summary ─────────────────────────────────────
    let conduits: Vec<_> = inp
        .net
        .links
        .iter()
        .zip(inp.link_stats)
        .filter(|(l, _)| matches!(l.kind, LinkKind::Channel { .. }))
        .collect();
    if !conduits.is_empty() {
        heading(w, "Flow Classification Summary")?;
        writeln!(w, "  ")?;
        rule(w, 85)?;
        writeln!(
            w,
            "                      Adjusted    ---------- Fraction of Time in Flow Class ----------"
        )?;
        writeln!(
            w,
            "                       /Actual         Up    Down  Sub   Sup   Up    Down  Norm  Inlet"
        )?;
        writeln!(
            w,
            "  Conduit               Length    Dry  Dry   Dry   Crit  Crit  Crit  Crit  Ltd   Ctrl"
        )?;
        rule(w, 85)?;
        for (l, st) in conduits {
            let obs = st.obs_time.max(1e-12);
            // §6.5 retired the length transform, so the ratio is
            // identically 1; the column stays for layout (§14.9).
            write!(
                w,
                "  {:<22}{:>6.2}{:>7.2}",
                l.id,
                1.0,
                st.class_time[0] / obs
            )?;
            for k in 1..7 {
                write!(w, "{:>6.2}", st.class_time[k] / obs)?;
            }
            writeln!(
                w,
                "{:>6.2}{:>6.2}",
                st.norm_limited_time / obs,
                st.inlet_control_time / obs
            )?;
        }
    }

    // ── Conduit surcharge summary ───────────────────────────────────────
    heading(w, "Conduit Surcharge Summary")?;
    writeln!(w, "  ")?;
    let surcharged: Vec<_> = inp
        .net
        .links
        .iter()
        .zip(inp.link_stats)
        .filter(|(l, st)| {
            matches!(l.kind, LinkKind::Channel { .. })
                && (st.full_both_time > 0.0
                    || st.full_up_time > 0.0
                    || st.full_down_time > 0.0
                    || st.above_normal_time > 0.0
                    || st.capacity_limited_time > 0.0)
        })
        .collect();
    if surcharged.is_empty() {
        writeln!(w, "  No conduits were surcharged.")?;
    } else {
        rule(w, 76)?;
        writeln!(
            w,
            "                                                           Hours        Hours"
        )?;
        writeln!(
            w,
            "                         --------- Hours Full --------   Above Full   Capacity"
        )?;
        writeln!(
            w,
            "  Conduit                Both Ends  Upstream  Dnstream   Normal Flow   Limited"
        )?;
        rule(w, 76)?;
        for (l, st) in surcharged {
            writeln!(
                w,
                "  {:<23}{:>9.2}{:>10.2}{:>10.2}{:>10.2}{:>12.2}",
                l.id,
                hr(st.full_both_time),
                hr(st.full_both_time + st.full_up_time),
                hr(st.full_both_time + st.full_down_time),
                hr(st.above_normal_time),
                hr(st.capacity_limited_time)
            )?;
        }
    }

    // ── Pumping summary ─────────────────────────────────────────────────
    let pumps: Vec<_> = inp
        .net
        .links
        .iter()
        .zip(inp.link_stats)
        .filter(|(l, _)| matches!(l.kind, LinkKind::Pump { .. }))
        .collect();
    if !pumps.is_empty() {
        heading(w, "Pumping Summary")?;
        writeln!(w, "  ")?;
        rule(w, 105)?;
        writeln!(
            w,
            "                                                  Min       Avg       Max     Total     Power    % Time Off"
        )?;
        writeln!(
            w,
            "                        Percent   Number of      Flow      Flow      Flow    Volume     Usage    Pump Curve"
        )?;
        writeln!(
            w,
            "  {:<ID_W$}{:>8}{:>12}{fu:>10}{fu:>10}{fu:>10}{:>10}{:>10}{:>7}{:>7}",
            "Pump",
            "Utilized",
            "Start-Ups",
            rv.mgal_word(),
            "Kw-hr",
            "Low",
            "High"
        )?;
        rule(w, 105)?;
        for (l, st) in pumps {
            let min_q = if st.min_flow == f64::MAX {
                0.0
            } else {
                st.min_flow
            };
            let avg = if st.on_time > 0.0 {
                st.volume / st.on_time
            } else {
                0.0
            };
            let obs = st.obs_time.max(1e-12);
            writeln!(
                w,
                "  {:<ID_W$}{:>8.2}{:>12}{:>10.2}{:>10.2}{:>10.2}{:>10.3}{:>10.2}{:>7.1}{:>7.1}",
                l.id,
                100.0 * st.on_time / obs,
                st.startups,
                rv.q(min_q),
                rv.q(avg),
                rv.q(st.max_pump_flow),
                rv.mgal(st.volume),
                st.energy_kwh,
                100.0 * st.off_low_time / obs,
                100.0 * st.off_high_time / obs
            )?;
        }
    }

    // ── Link pollutant load summary ─────────────────────────────────────
    if let Some(loads) = &inp.link_loads {
        if !inp.net.constituents.is_empty() {
            heading(w, "Link Pollutant Load Summary")?;
            writeln!(w, "  ")?;
            let width = LOAD_ID_W + 14 * inp.net.constituents.len();
            rule(w, width)?;
            write!(w, "  {:<LOAD_ID_W$}", "")?;
            for c in &inp.net.constituents {
                write!(w, "{:>14}", c.id)?;
            }
            writeln!(w)?;
            write!(w, "  {:<LOAD_ID_W$}", "Link")?;
            for c in &inp.net.constituents {
                write!(w, "{:>14}", rv.load_word(c.units))?;
            }
            writeln!(w)?;
            rule(w, width)?;
            for (li, l) in inp.net.links.iter().enumerate() {
                write!(w, "  {:<LOAD_ID_W$}", l.id)?;
                for (c, row) in inp.net.constituents.iter().zip(loads) {
                    write!(w, "{:>14.3}", rv.load(c.units, row[li]))?;
                }
                writeln!(w)?;
            }
        }
    }
    Ok(())
}
