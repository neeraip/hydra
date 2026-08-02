//! Snow (§4.2): water-equivalent packs over the three-way surface split,
//! melted by Anderson's two-regime model — an empirical fit whose
//! constants embed US units, identified and evaluated in its native form
//! with SI conversion at the boundary — with cold content, free-water
//! routing, areal depletion, and plowing.

use crate::model::{SnowSurface, Snowpack};

/// Exact feet-to-metres.
const FT: f64 = 0.3048;

/// Per-step climate inputs the snow model reads (§3.1, §4.2).
#[derive(Debug, Clone, Copy)]
pub struct SnowClimate {
    /// Air temperature (°C).
    pub ta: f64,
    /// Wind speed (m/s).
    pub wind: f64,
    /// Rain/snow dividing temperature (°C).
    pub snow_temp: f64,
    /// Antecedent temperature index weight (6-hour basis).
    pub ati_weight: f64,
    /// Negative melt ratio.
    pub rnm: f64,
    /// Site elevation (m), for the atmospheric-pressure fit.
    pub elevation: f64,
    /// The seasonal sweep `sin(0.0172615·(day − 81))`.
    pub season: f64,
    /// Areal depletion curves (impervious, pervious); `None` = full cover.
    pub adc_impervious: Option<[f64; 10]>,
    pub adc_pervious: Option<[f64; 10]>,
}

impl SnowClimate {
    fn ta_f(&self) -> f64 {
        self.ta * 9.0 / 5.0 + 32.0
    }

    /// Saturation vapour pressure (in Hg) from the fitted exponential.
    fn ea(&self) -> f64 {
        8.1175e6 * (-7701.544 / (self.ta_f() + 405.0265)).exp()
    }

    /// The psychrometric factor from the elevation pressure fit, which is
    /// bypassed at or below sea level (a domain guard, §4.2).
    fn gamma(&self) -> f64 {
        let z = self.elevation / FT / 1000.0;
        let pa = if z <= 0.0 {
            29.9
        } else {
            29.9 - 1.02 * z + 0.0032 * z.powf(2.4)
        };
        0.000359 * pa
    }
}

/// One snow surface's parameters and live state, SI.
struct SurfaceState {
    dh_min: f64,
    dh_max: f64,
    t_base: f64,
    fw_frac: f64,
    /// Depth at full areal cover (m); `None` = always fully covered.
    si: Option<f64>,
    // State.
    wsnow: f64,
    fw: f64,
    coldc: f64,
    ati: f64,
    awe: f64,
    sba: f64,
    sbws: f64,
    imelt: f64,
}

impl SurfaceState {
    fn build(p: &SnowSurface, plowable: bool) -> SurfaceState {
        SurfaceState {
            dh_min: p.dh_min,
            dh_max: p.dh_max,
            t_base: p.t_base,
            fw_frac: p.fw_frac,
            si: if plowable { None } else { p.full_cover_depth },
            wsnow: p.init_depth,
            fw: p.init_free_water,
            coldc: 0.0,
            ati: 0.0,
            awe: 1.0,
            sba: 0.0,
            sbws: 0.0,
            imelt: 0.0,
        }
    }

    /// The seasonal degree-day coefficient (m/s per °C).
    fn dhm(&self, season: f64) -> f64 {
        0.5 * (self.dh_max * (1.0 + season) + self.dh_min * (1.0 - season))
    }
}

/// A parcel's snow pack over its three surfaces.
///
/// `has_cover` reports whether any surface currently holds snow, the
/// §8.2 snow-only accumulation gate.
pub struct SnowPack {
    /// [plowable, impervious, pervious], with each surface's fraction of
    /// the parcel area.
    surfaces: [Option<SurfaceState>; 3],
    f_area: [f64; 3],
    /// Plowing: trigger depth, five fractions, receiving parcel.
    removal: Option<(f64, [f64; 5], Option<usize>)>,
    /// Snow plowed to another parcel this step (m³, pervious target).
    pub transfer_out: Vec<(usize, f64)>,
    /// Snow water removed from the system by ploughing (m³), §11.1.
    pub exported: f64,
}

impl SnowPack {
    /// Stored snow water volume over the parcel (m³), §11.1: water
    /// equivalent plus free water per surface share.
    pub fn stored_volume(&self, parcel_area: f64) -> f64 {
        self.surfaces
            .iter()
            .zip(self.f_area)
            .flat_map(|(sf, f)| sf.as_ref().map(|s| (s.wsnow + s.fw) * f * parcel_area))
            .sum()
    }

    /// Mean snow water equivalent over the present surfaces (m), the
    /// §14.9 snow-depth record.
    pub fn mean_depth(&self) -> f64 {
        let depths: Vec<f64> = self.surfaces.iter().flatten().map(|sf| sf.wsnow).collect();
        if depths.is_empty() {
            0.0
        } else {
            depths.iter().sum::<f64>() / depths.len() as f64
        }
    }

    /// Whether any surface holds snow water equivalent (§8.2).
    pub fn has_cover(&self) -> bool {
        self.surfaces.iter().flatten().any(|sf| sf.wsnow > 1.0e-6)
    }

    /// Build from the pack parameters and the parcel's impervious split.
    pub fn build(pack: &Snowpack, frac_imperv: f64) -> SnowPack {
        let plow_frac = pack.plow_fraction.clamp(0.0, 1.0);
        let f_area = [
            frac_imperv * plow_frac,
            frac_imperv * (1.0 - plow_frac),
            1.0 - frac_imperv,
        ];
        SnowPack {
            surfaces: [
                pack.plowable.as_ref().map(|p| SurfaceState::build(p, true)),
                pack.impervious
                    .as_ref()
                    .map(|p| SurfaceState::build(p, false)),
                pack.pervious
                    .as_ref()
                    .map(|p| SurfaceState::build(p, false)),
            ],
            f_area,
            removal: pack
                .removal
                .as_ref()
                .map(|r| (r.trigger_depth, r.fractions, r.to_parcel)),
            transfer_out: Vec::new(),
            exported: 0.0,
        }
    }

    /// Total water-equivalent volume held (m over the parcel).
    pub fn stored_depth(&self) -> f64 {
        self.surfaces
            .iter()
            .zip(&self.f_area)
            .map(|(s, f)| s.as_ref().map_or(0.0, |s| (s.wsnow + s.fw) * f))
            .sum()
    }

    /// Add snowfall and plow (§4.2). `snowfall` is the catch-scaled rate
    /// (m/s); `parcel_area` the parcel's plan area (m²).
    pub fn plow(&mut self, snowfall: f64, dt: f64, parcel_area: f64) {
        self.transfer_out.clear();
        for s in self.surfaces.iter_mut().flatten() {
            s.wsnow += snowfall * dt;
            s.imelt = 0.0;
        }
        let Some((trigger, fracs, to_parcel)) = self.removal else {
            return;
        };
        let f_plow = self.f_area[0];
        if f_plow <= 0.0 {
            return;
        }
        let Some(plow) = self.surfaces[0].as_mut() else {
            return;
        };
        if plow.wsnow < trigger {
            return;
        }
        let exc = plow.wsnow;
        let mut total = fracs[0]; // out of system
        self.exported += fracs[0] * exc * f_plow * parcel_area;
        // Onto the other impervious surface.
        if self.f_area[1] > 0.0 {
            if let Some(s) = self.surfaces[1].as_mut() {
                s.wsnow += fracs[1] * exc * f_plow / self.f_area[1];
            }
            total += fracs[1];
        }
        // Onto the pervious surface.
        if self.f_area[2] > 0.0 {
            if let Some(s) = self.surfaces[2].as_mut() {
                s.wsnow += fracs[2] * exc * f_plow / self.f_area[2];
            }
            total += fracs[2];
        }
        // Immediate melt.
        if let Some(plow) = self.surfaces[0].as_mut() {
            plow.imelt = fracs[3] * exc / dt;
        }
        total += fracs[3];
        // To another parcel's pervious surface (as a volume; the surface
        // model spreads it there).
        if fracs[4] > 0.0 {
            if let Some(p) = to_parcel {
                self.transfer_out
                    .push((p, fracs[4] * exc * f_plow * parcel_area));
                total += fracs[4];
            }
        }
        if let Some(plow) = self.surfaces[0].as_mut() {
            plow.wsnow = exc * (1.0 - total.min(1.0));
        }
    }

    /// Receive plowed snow onto the pervious surface (m³ over the parcel
    /// area m²).
    pub fn receive(&mut self, volume: f64, parcel_area: f64) {
        if self.f_area[2] > 0.0 && parcel_area > 0.0 {
            if let Some(s) = self.surfaces[2].as_mut() {
                s.wsnow += volume / parcel_area / self.f_area[2];
            }
        }
    }

    /// Melt the packs and return net precipitation (m/s) per runoff
    /// surface: (impervious, pervious), plus the remaining depth (m).
    pub fn melt(
        &mut self,
        rainfall: f64,
        snowfall: f64,
        dt: f64,
        cl: &SnowClimate,
    ) -> (f64, f64, f64) {
        let rmelt = rain_melt(rainfall, cl);
        let mut net = [0.0_f64; 3];
        let mut depth = 0.0;
        for (i, slot) in self.surfaces.iter_mut().enumerate() {
            let Some(s) = slot else {
                net[i] = rainfall;
                continue;
            };
            let asc;
            let mut smelt;
            // A pack below a thousandth of an inch flushes (§4.2).
            if s.wsnow <= 0.001 / 12.0 * FT {
                asc = 0.0;
                smelt = 0.0;
                s.imelt += (s.wsnow + s.fw) / dt;
                s.wsnow = 0.0;
                s.fw = 0.0;
                s.coldc = 0.0;
            } else {
                let adc = match i {
                    1 => cl.adc_impervious,
                    2 => cl.adc_pervious,
                    _ => None,
                };
                asc = areal_depletion(s, adc, snowfall, dt);
                smelt = melt_surface(s, rmelt, asc, snowfall, dt, cl);
                smelt = route_melt(s, smelt, asc, rainfall, dt);
            }
            net[i] = smelt + s.imelt + rainfall * (1.0 - asc);
            depth += s.wsnow * self.f_area[i];
        }
        // Combine the two impervious surfaces area-weighted.
        let f_imp = self.f_area[0] + self.f_area[1];
        let imperv = if f_imp > 0.0 {
            (net[0] * self.f_area[0] + net[1] * self.f_area[1]) / f_imp
        } else {
            0.0
        };
        (imperv, net[2], depth)
    }
}

/// Anderson's saturated rain-melt energy budget, evaluated in its native
/// units (in/hr, °F, mph, in Hg) and converted at the boundary (§4.2).
fn rain_melt(rainfall: f64, cl: &SnowClimate) -> f64 {
    let rain_inhr = rainfall / FT * 43_200.0;
    if rain_inhr <= 0.02 {
        return 0.0;
    }
    let uadj = 0.006 * (cl.wind / 0.447_04);
    let t1 = cl.ta_f() - 32.0;
    let t2 = 7.5 * cl.gamma() * uadj;
    let t3 = 8.5 * uadj * (cl.ea() - 0.18);
    let smelt_inhr = t1 * (0.001_167 + t2 + 0.007 * rain_inhr) + t3;
    smelt_inhr / 43_200.0 * FT
}

/// Areal depletion with Anderson's temporary-curve adjustment after fresh
/// snowfall on partial cover (§4.2).
fn areal_depletion(s: &mut SurfaceState, adc: Option<[f64; 10]>, snowfall: f64, dt: f64) -> f64 {
    let Some(si) = s.si else {
        return 1.0; // the plowable surface, always fully covered
    };
    if si <= 0.0 || s.wsnow >= si {
        s.awe = 1.0;
        return 1.0;
    }
    if s.wsnow == 0.0 {
        s.awe = 1.0;
        return 0.0;
    }
    let cover = |x: f64| areal_cover(adc, x);
    if snowfall > 0.0 {
        let awe = ((s.wsnow - snowfall * dt) / si).max(0.0);
        s.awe = awe;
        s.sba = cover(awe);
        s.sbws = (awe + 0.75 * snowfall * dt / si).min(1.0);
        1.0
    } else {
        let awesi = s.wsnow / si;
        if awesi < s.awe {
            s.awe = 1.0;
            cover(awesi)
        } else if awesi >= s.sbws {
            1.0
        } else {
            s.sba + (1.0 - s.sba) / (s.sbws - s.awe) * (awesi - s.awe)
        }
    }
}

/// The x-value on a ten-interval areal depletion curve.
fn areal_cover(adc: Option<[f64; 10]>, awesi: f64) -> f64 {
    let Some(t) = adc else {
        return 1.0;
    };
    if awesi <= 0.0 {
        return 0.0;
    }
    if awesi >= 0.9999 {
        return 1.0;
    }
    let m = (awesi * 10.0 + 0.000_01) as usize;
    let a1 = t[m];
    let a2 = if m >= 9 { 1.0 } else { t[m + 1] };
    a1 + (a2 - a1) / 0.1 * (awesi - 0.1 * m as f64)
}

/// The two-regime melt with cold-content accounting (§4.2).
fn melt_surface(
    s: &mut SurfaceState,
    rmelt: f64,
    asc: f64,
    snowfall: f64,
    dt: f64,
    cl: &SnowClimate,
) -> f64 {
    let mut smelt;
    if rmelt > 0.0 {
        smelt = rmelt;
    } else if cl.ta >= s.t_base {
        smelt = s.dhm(cl.season) * (cl.ta - s.t_base);
    } else {
        update_cold_content(s, asc, snowfall, dt, cl);
        return 0.0;
    }
    smelt *= asc;
    // Cold content must be paid before liquid leaves.
    let cc_factor = dt * cl.rnm * asc;
    if smelt * cc_factor > s.coldc {
        if cc_factor > 0.0 {
            smelt -= s.coldc / cc_factor;
        }
        s.coldc = 0.0;
    } else {
        s.coldc -= smelt * cc_factor;
        smelt = 0.0;
    }
    s.ati = s.t_base;
    smelt
}

/// Cold content under non-melting conditions (§4.2): the antecedent
/// temperature index snapping to air temperature during snowfall, the
/// heat-capacity cap at 0.007 water-equivalent per °F per foot of pack.
fn update_cold_content(s: &mut SurfaceState, asc: f64, snowfall: f64, dt: f64, cl: &SnowClimate) {
    let snowing = snowfall / FT * 43_200.0 > 0.02;
    if snowing {
        s.ati = cl.ta;
    } else {
        let tipm = 1.0 - (1.0 - cl.ati_weight).powf(dt / (6.0 * 3600.0));
        s.ati += tipm * (cl.ta - s.ati);
    }
    s.ati = s.ati.min(s.t_base);
    s.coldc += cl.rnm * s.dhm(cl.season) * (s.ati - cl.ta) * dt * asc;
    s.coldc = s.coldc.max(0.0);
    // 0.007 per °F per foot, in SI: per (5/9 °C) per 0.3048 m.
    let cc_max = s.wsnow * (0.007 / 12.0) * ((s.t_base - s.ati) * 9.0 / 5.0);
    s.coldc = s.coldc.min(cc_max.max(0.0));
}

/// Free-water routing (§4.2): melt debits the pack, joins the reservoir
/// with rain on the covered fraction, and only the excess over holding
/// capacity leaves.
fn route_melt(s: &mut SurfaceState, smelt: f64, asc: f64, rainfall: f64, dt: f64) -> f64 {
    let vmelt = (smelt * dt).min(s.wsnow);
    s.wsnow -= vmelt;
    s.fw += vmelt + rainfall * dt * asc;
    let excess = (s.fw - s.fw_frac * s.wsnow).max(0.0);
    s.fw -= excess;
    excess / dt
}
