//! Text-report writer (§14.9): the predecessor's structure — banner,
//! title, options summary, the §11 continuity balances in its block
//! formats, the control-actions log, and the numerical-performance
//! block, which reports rejections and degraded-accuracy tallies in
//! place of steady-state-skip time (§10.3). The per-object summary
//! tables follow with the §11.2 statistics catalogue.

use std::io::{self, Write};

use crate::model::Network;

/// The report's fixed volume conversions per unit system.
struct Rv {
    us: bool,
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
    /// A depth (m) to inches | millimetres.
    fn depth(&self, v: f64) -> f64 {
        if self.us {
            v / 0.0254
        } else {
            v * 1000.0
        }
    }
}

fn line(w: &mut impl Write, label: &str, a: f64, b: f64) -> io::Result<()> {
    let dots = ".".repeat(26usize.saturating_sub(label.len() + 3));
    writeln!(w, "  {label} {dots}{a:>14.3}{b:>14.3}")
}

fn err_line(w: &mut impl Write, e: f64) -> io::Result<()> {
    writeln!(w, "  Continuity Error (%) .....{e:>14.3}")
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
    /// reacted, final storage, stored, error %).
    pub quality: Vec<(String, [f64; 6])>,
    /// Control actions: (elapsed s, link, setting, rule).
    pub actions: &'a [(f64, String, f64, String)],
    /// Numerical performance: (accepted steps, rejected trials,
    /// degraded-accuracy steps, average step s).
    pub performance: (u64, u64, usize, f64),
}

/// Write the §14.9 text report.
pub fn write_rpt(inp: &ReportInputs, w: &mut impl Write) -> io::Result<()> {
    let rv = Rv {
        us: inp.net.options.flow_units.is_us(),
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
        let [admitted, discharged, reacted, flushed, stored, err] = *q;
        writeln!(w, "\n  **************************          Mass")?;
        writeln!(w, "  Quality Routing Continuity : {id}")?;
        writeln!(w, "  **************************     ---------")?;
        writeln!(w, "  Total Inflow Load ........{admitted:>14.3}")?;
        writeln!(w, "  External Outflow Load ....{discharged:>14.3}")?;
        writeln!(w, "  Reacted Mass .............{reacted:>14.3}")?;
        writeln!(w, "  Final Storage Flushes ....{flushed:>14.3}")?;
        writeln!(w, "  Stored Mass ..............{stored:>14.3}")?;
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
    writeln!(w, "\n  Analysis complete.")?;
    Ok(())
}
