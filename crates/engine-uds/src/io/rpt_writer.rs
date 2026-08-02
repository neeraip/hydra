//! Text-report writer (§14.9): the predecessor's structure — banner,
//! title, options summary, the §11 continuity balances in its block
//! formats, the control-actions log, and the numerical-performance
//! block, which reports rejections and degraded-accuracy tallies in
//! place of steady-state-skip time (§10.3). The per-object summary
//! tables follow with the §11.2 statistics catalogue.

use std::io::{self, Write};

use crate::hydraulics::routing::{LinkStats, VertexStats};
use crate::hydrology::runoff::ParcelTotals;
use crate::model::{LinkKind, Network, VertexKind};

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

fn line(w: &mut impl Write, label: &str, a: f64, b: f64) -> io::Result<()> {
    let dots = ".".repeat(28usize.saturating_sub(label.len() + 4));
    writeln!(w, "  {label} {dots}{a:>14.3}{b:>14.3}")
}

fn err_line(w: &mut impl Write, e: f64) -> io::Result<()> {
    writeln!(w, "  Continuity Error (%) ......{e:>14.3}")
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
    /// external, outflow, flooding, evaporation + seepage losses,
    /// initial, final, error %).
    pub flow: [f64; 11],
    /// Per-constituent quality parts: (id, admitted, discharged,
    /// flooded, reacted, seepage, final storage, stored, error %).
    pub quality: Vec<(String, [f64; 8])>,
    /// Control actions: (elapsed s, link, setting, rule).
    pub actions: &'a [(f64, String, f64, String)],
    /// Numerical performance: (accepted steps, rejected trials,
    /// degraded-accuracy steps, average step s).
    pub performance: (u64, u64, usize, f64),
    /// §11.2 per-vertex and per-link statistics.
    pub vertex_stats: &'a [VertexStats],
    pub link_stats: &'a [LinkStats],
    /// Per-parcel §11.2 totals, parallel to the model's parcels.
    pub parcel_totals: Vec<ParcelTotals>,
    /// Per-parcel delivered washoff `[parcel][constituent]` (U).
    pub washoff_by_parcel: Option<Vec<Vec<f64>>>,
    /// Per-outfall discharged mass `[constituent][vertex]`.
    pub outfall_loads: Option<Vec<Vec<f64>>>,
    /// The top worst-error vertices: (id, accepted-step count).
    pub worst: Vec<(String, u64)>,
}

/// Write the §14.9 text report.
pub fn write_rpt(inp: &ReportInputs, w: &mut impl Write) -> io::Result<()> {
    let rv = Rv {
        us: inp.net.options.flow_units.is_us(),
        flow: match inp.net.options.flow_units {
            crate::io::options::FlowUnits::Cfs => 0.028_316_846_592,
            crate::io::options::FlowUnits::Gpm => 6.309_019_64e-5,
            crate::io::options::FlowUnits::Mgd => 0.043_812_636_4,
            crate::io::options::FlowUnits::Cms => 1.0,
            crate::io::options::FlowUnits::Lps => 1.0e-3,
            crate::io::options::FlowUnits::Mld => 1.0 / 86.4,
        },
    };
    let total_area: f64 = inp.net.parcels.iter().map(|p| p.area).sum();

    // ── Banner and title ────────────────────────────────────────────────
    writeln!(
        w,
        "\n  Hydra water-infrastructure simulation — urban drainage engine"
    )?;
    writeln!(w, "  Predecessor-format analysis report (§14.9)\n")?;
    for t in &inp.net.title {
        writeln!(w, "  {t}")?;
    }

    // ── Analysis options ────────────────────────────────────────────────
    writeln!(
        w,
        "\n  *********************************************************"
    )?;
    writeln!(w, "  Analysis Options")?;
    writeln!(
        w,
        "  *********************************************************"
    )?;
    writeln!(
        w,
        "  Flow Units ............... {:?}",
        inp.net.options.flow_units
    )?;
    writeln!(w, "  Routing Method ........... Dynamic Wave")?;
    let d = inp.net.options.start_date;
    writeln!(
        w,
        "  Starting Date ............ {:02}/{:02}/{}",
        d.month, d.day, d.year
    )?;
    let d = inp.net.options.end_date;
    writeln!(
        w,
        "  Ending Date .............. {:02}/{:02}/{}",
        d.month, d.day, d.year
    )?;
    writeln!(
        w,
        "  Routing Time Step ........ {:.2} sec",
        inp.net.options.routing_step
    )?;
    writeln!(
        w,
        "  Report Time Step ......... {:.0} sec",
        inp.net.options.report_step
    )?;

    // ── Runoff quantity continuity ──────────────────────────────────────
    if let Some(s) = inp.surface {
        let [rain, runon, evap, infil, runoff, plowed, init, fin, err] = s;
        writeln!(
            w,
            "\n  **************************        Volume         Depth"
        )?;
        if rv.us {
            writeln!(
                w,
                "  Runoff Quantity Continuity     acre-feet        inches"
            )?;
        } else {
            writeln!(
                w,
                "  Runoff Quantity Continuity     hectare-m            mm"
            )?;
        }
        writeln!(
            w,
            "  **************************     ---------       -------"
        )?;
        let dep = |v: f64| {
            if total_area > 0.0 {
                rv.depth(v / total_area)
            } else {
                0.0
            }
        };
        if init > 0.0 {
            line(w, "Initial Storage", rv.big(init), dep(init))?;
        }
        line(w, "Total Precipitation", rv.big(rain), dep(rain))?;
        if runon > 0.0 {
            line(w, "Upstream Runon", rv.big(runon), dep(runon))?;
        }
        line(w, "Evaporation Loss", rv.big(evap), dep(evap))?;
        line(w, "Infiltration Loss", rv.big(infil), dep(infil))?;
        line(w, "Surface Runoff", rv.big(runoff), dep(runoff))?;
        if plowed > 0.0 {
            line(w, "Snow Removed", rv.big(plowed), dep(plowed))?;
        }
        line(w, "Final Storage", rv.big(fin), dep(fin))?;
        err_line(w, err)?;
    }

    // ── Groundwater continuity ──────────────────────────────────────────
    if let Some(g) = inp.subsurface {
        let [infil, evap, perc, lateral, init, fin, err] = g;
        writeln!(
            w,
            "\n  **************************        Volume         Depth"
        )?;
        if rv.us {
            writeln!(
                w,
                "  Groundwater Continuity         acre-feet        inches"
            )?;
        } else {
            writeln!(
                w,
                "  Groundwater Continuity         hectare-m            mm"
            )?;
        }
        writeln!(
            w,
            "  **************************     ---------       -------"
        )?;
        let dep = |v: f64| {
            if total_area > 0.0 {
                rv.depth(v / total_area)
            } else {
                0.0
            }
        };
        line(w, "Initial Storage", rv.big(init), dep(init))?;
        line(w, "Infiltration", rv.big(infil), dep(infil))?;
        line(w, "Evapotranspiration", rv.big(evap), dep(evap))?;
        line(w, "Deep Percolation", rv.big(perc), dep(perc))?;
        line(w, "Groundwater Flow", rv.big(lateral), dep(lateral))?;
        line(w, "Final Storage", rv.big(fin), dep(fin))?;
        err_line(w, err)?;
    }

    // ── Flow routing continuity ─────────────────────────────────────────
    {
        let [dwf, wet, gw, rdii, ext, out, flood, losses, init, fin, err] = inp.flow;
        writeln!(
            w,
            "\n  **************************        Volume        Volume"
        )?;
        if rv.us {
            writeln!(
                w,
                "  Flow Routing Continuity        acre-feet      10^6 gal"
            )?;
        } else {
            writeln!(
                w,
                "  Flow Routing Continuity        hectare-m      10^6 ltr"
            )?;
        }
        writeln!(
            w,
            "  **************************     ---------     ---------"
        )?;
        line(w, "Dry Weather Inflow", rv.big(dwf), rv.mgal(dwf))?;
        line(w, "Wet Weather Inflow", rv.big(wet), rv.mgal(wet))?;
        line(w, "Groundwater Inflow", rv.big(gw), rv.mgal(gw))?;
        line(w, "RDII Inflow", rv.big(rdii), rv.mgal(rdii))?;
        line(w, "External Inflow", rv.big(ext), rv.mgal(ext))?;
        line(w, "External Outflow", rv.big(out), rv.mgal(out))?;
        line(w, "Flooding Loss", rv.big(flood), rv.mgal(flood))?;
        line(w, "Evap/Exfil Losses", rv.big(losses), rv.mgal(losses))?;
        line(w, "Initial Stored Volume", rv.big(init), rv.mgal(init))?;
        line(w, "Final Stored Volume", rv.big(fin), rv.mgal(fin))?;
        err_line(w, err)?;
    }

    // ── Quality routing continuity ──────────────────────────────────────
    for (id, q) in &inp.quality {
        let cu = constituent_units(inp, id);
        let [admitted, discharged, flooded, reacted, seepage, flushed, stored, err] = *q;
        writeln!(w, "\n  **************************          Mass")?;
        writeln!(
            w,
            "  Quality Routing Continuity : {id}{:>10}",
            rv.load_word(cu)
        )?;
        writeln!(w, "  **************************     ---------")?;
        writeln!(
            w,
            "  Total Inflow Load ........{:>14.3}",
            rv.load(cu, admitted)
        )?;
        writeln!(
            w,
            "  External Outflow Load ....{:>14.3}",
            rv.load(cu, discharged)
        )?;
        writeln!(
            w,
            "  Flooding Loss ............{:>14.3}",
            rv.load(cu, flooded)
        )?;
        writeln!(
            w,
            "  Reacted Mass .............{:>14.3}",
            rv.load(cu, reacted)
        )?;
        writeln!(
            w,
            "  Seepage Loss .............{:>14.3}",
            rv.load(cu, seepage)
        )?;
        writeln!(
            w,
            "  Final Storage Flushes ....{:>14.3}",
            rv.load(cu, flushed)
        )?;
        writeln!(
            w,
            "  Stored Mass ..............{:>14.3}",
            rv.load(cu, stored)
        )?;
        err_line(w, err)?;
    }

    // ── Control actions ─────────────────────────────────────────────────
    if !inp.actions.is_empty() {
        writeln!(w, "\n  *******************")?;
        writeln!(w, "  Control Actions Taken")?;
        writeln!(w, "  *******************")?;
        for (t, link, value, rule) in inp.actions {
            let h = (t / 3600.0) as u64;
            let m = ((t % 3600.0) / 60.0) as u64;
            let sec = t % 60.0;
            writeln!(
                w,
                "  {h:02}:{m:02}:{sec:05.2}  Link {link} setting = {value:.2} by Control {rule}"
            )?;
        }
    }

    write_summary_tables(inp, &rv, w)?;

    // ── Numerical performance ───────────────────────────────────────────
    // Rejections and degraded-accuracy tallies stand in for the retired
    // steady-state skip (§10.3, §14.9).
    let (accepted, rejected, degraded, avg_dt) = inp.performance;
    writeln!(w, "\n  ********************************")?;
    writeln!(w, "  Numerical Performance")?;
    writeln!(w, "  ********************************")?;
    writeln!(w, "  Routing Steps Accepted ...{accepted:>14}")?;
    writeln!(w, "  Trials Rejected ..........{rejected:>14}")?;
    writeln!(w, "  Degraded-Accuracy Steps ..{degraded:>14}")?;
    writeln!(w, "  Average Time Step (sec) ..{avg_dt:>14.2}")?;
    if !inp.worst.is_empty() {
        writeln!(
            w,
            "\n  Most Frequent Governing Vertices (§6.5 error estimate)"
        )?;
        for (id, n) in &inp.worst {
            writeln!(w, "  {id:<20}{n:>10} steps")?;
        }
    }
    writeln!(w, "\n  Analysis complete.")?;
    Ok(())
}

/// The §11.2 per-object summary tables, in the predecessor's grouping.
#[allow(clippy::too_many_lines)]
fn write_summary_tables(inp: &ReportInputs, rv: &Rv, w: &mut impl Write) -> io::Result<()> {
    let hr = |sec: f64| sec / 3600.0;
    // ── Subcatchment runoff summary ─────────────────────────────────────
    if !inp.net.parcels.is_empty() {
        writeln!(w, "\n  ***************************")?;
        writeln!(w, "  Subcatchment Runoff Summary")?;
        writeln!(w, "  ***************************")?;
        writeln!(
            w,
            "  {:<16}{:>12}{:>12}{:>12}{:>12}{:>12}{:>12}{:>8}",
            "Subcatchment", "Precip", "Runon", "Evap", "Infil", "Runoff-Vol", "Peak-Flow", "Coeff"
        )?;
        for (p, t) in inp.net.parcels.iter().zip(&inp.parcel_totals) {
            let supply = t.precip + t.runon;
            let coeff = if supply > 0.0 { t.runoff / supply } else { 0.0 };
            writeln!(
                w,
                "  {:<16}{:>12.3}{:>12.3}{:>12.3}{:>12.3}{:>12.3}{:>12.3}{:>8.3}",
                p.id,
                rv.big(t.precip),
                rv.big(t.runon),
                rv.big(t.evap),
                rv.big(t.infil),
                rv.big(t.runoff),
                rv.q(t.peak_runoff),
                coeff
            )?;
        }
    }
    // ── Subcatchment washoff summary ────────────────────────────────────
    if let Some(loads) = &inp.washoff_by_parcel {
        if !inp.net.constituents.is_empty() {
            writeln!(w, "\n  ****************************")?;
            writeln!(w, "  Subcatchment Washoff Summary")?;
            writeln!(w, "  ****************************")?;
            write!(w, "  {:<16}", "Subcatchment")?;
            for c in &inp.net.constituents {
                write!(w, "{:>14}", c.id)?;
            }
            writeln!(w)?;
            write!(w, "  {:<16}", "")?;
            for c in &inp.net.constituents {
                write!(w, "{:>14}", rv.load_word(c.units))?;
            }
            writeln!(w)?;
            for (p, row) in inp.net.parcels.iter().zip(loads) {
                write!(w, "  {:<16}", p.id)?;
                for (c, v) in inp.net.constituents.iter().zip(row) {
                    write!(w, "{:>14.3}", rv.load(c.units, *v))?;
                }
                writeln!(w)?;
            }
        }
    }
    // ── Node depth / surcharge / flooding summaries ─────────────────────
    writeln!(w, "\n  ******************")?;
    writeln!(w, "  Node Depth Summary")?;
    writeln!(w, "  ******************")?;
    writeln!(
        w,
        "  {:<16}{:>12}{:>12}{:>14}{:>14}",
        "Node", "Max-Depth", "Max-HGL", "Hr-of-Max", "Surch-Hrs"
    )?;
    for (v, st) in inp.net.vertices.iter().zip(inp.vertex_stats) {
        writeln!(
            w,
            "  {:<16}{:>12.3}{:>12.3}{:>14.2}{:>14.2}",
            v.id,
            st.max_depth / rvlen(rv),
            (v.invert + st.max_depth) / rvlen(rv),
            hr(st.t_max_depth),
            hr(st.surcharge_time)
        )?;
    }
    let flooded: Vec<_> = inp
        .net
        .vertices
        .iter()
        .zip(inp.vertex_stats)
        .filter(|(_, st)| st.flood_time > 0.0)
        .collect();
    if !flooded.is_empty() {
        writeln!(w, "\n  *********************")?;
        writeln!(w, "  Node Flooding Summary")?;
        writeln!(w, "  *********************")?;
        writeln!(
            w,
            "  {:<16}{:>12}{:>14}{:>14}",
            "Node", "Hrs-Flooded", "Max-Rate", "Total-Vol"
        )?;
        for (v, st) in flooded {
            writeln!(
                w,
                "  {:<16}{:>12.2}{:>14.3}{:>14.3}",
                v.id,
                hr(st.flood_time),
                rv.q(st.max_flood),
                rv.big(st.flood_volume)
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
        writeln!(w, "\n  ***********************")?;
        writeln!(w, "  Outfall Loading Summary")?;
        writeln!(w, "  ***********************")?;
        write!(
            w,
            "  {:<16}{:>10}{:>12}{:>14}",
            "Outfall", "Freq-%", "Max-Flow", "Total-Vol"
        )?;
        for c in &inp.net.constituents {
            write!(w, "{:>14}", c.id)?;
        }
        writeln!(w)?;
        if !inp.net.constituents.is_empty() {
            write!(w, "  {:<16}{:>10}{:>12}{:>14}", "", "", "", "")?;
            for c in &inp.net.constituents {
                write!(w, "{:>14}", rv.load_word(c.units))?;
            }
            writeln!(w)?;
        }
        for &vi in &outfalls {
            let st = &inp.vertex_stats[vi];
            let freq = if st.obs_time > 0.0 {
                100.0 * st.out_time / st.obs_time
            } else {
                0.0
            };
            write!(
                w,
                "  {:<16}{:>10.2}{:>12.3}{:>14.3}",
                inp.net.vertices[vi].id,
                freq,
                rv.q(st.out_peak),
                rv.big(st.out_volume)
            )?;
            if let Some(loads) = &inp.outfall_loads {
                for (c, row) in inp.net.constituents.iter().zip(loads) {
                    write!(w, "{:>14.3}", rv.load(c.units, row[vi]))?;
                }
            }
            writeln!(w)?;
        }
    }
    // ── Link flow summary ───────────────────────────────────────────────
    writeln!(w, "\n  *****************")?;
    writeln!(w, "  Link Flow Summary")?;
    writeln!(w, "  *****************")?;
    writeln!(
        w,
        "  {:<16}{:>12}{:>14}{:>12}{:>12}{:>12}",
        "Link", "Max-Flow", "Hr-of-Max", "Max-Veloc", "Max-Depth", "Full-Hrs"
    )?;
    for (l, st) in inp.net.links.iter().zip(inp.link_stats) {
        writeln!(
            w,
            "  {:<16}{:>12.3}{:>14.2}{:>12.2}{:>12.3}{:>12.2}",
            l.id,
            rv.q(st.max_flow),
            hr(st.t_max_flow),
            rv.vel(st.max_velocity),
            st.max_depth / rvlen(rv),
            hr(st.full_time)
        )?;
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
        writeln!(w, "\n  ***************")?;
        writeln!(w, "  Pumping Summary")?;
        writeln!(w, "  ***************")?;
        writeln!(
            w,
            "  {:<16}{:>10}{:>10}{:>10}{:>10}{:>12}{:>12}{:>12}{:>12}",
            "Pump",
            "Util-Hrs",
            "Startups",
            "Min-Flow",
            "Max-Flow",
            "Volume",
            "kW-hr",
            "OffLo-Hrs",
            "OffHi-Hrs"
        )?;
        for (l, st) in pumps {
            let min_q = if st.min_flow == f64::MAX {
                0.0
            } else {
                st.min_flow
            };
            writeln!(
                w,
                "  {:<16}{:>10.2}{:>10}{:>10.3}{:>10.3}{:>12.3}{:>12.2}{:>12.2}{:>12.2}",
                l.id,
                hr(st.on_time),
                st.startups,
                rv.q(min_q),
                rv.q(st.max_pump_flow),
                rv.big(st.volume),
                st.energy_kwh,
                hr(st.off_low_time),
                hr(st.off_high_time)
            )?;
        }
    }
    Ok(())
}

fn rvlen(rv: &Rv) -> f64 {
    if rv.us {
        0.3048
    } else {
        1.0
    }
}
