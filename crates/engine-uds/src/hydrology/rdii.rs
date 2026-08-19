//! Sewer inflow (§4.3): rainfall-dependent inflow and infiltration as a
//! rainfall convolution at designated vertices — triangular unit
//! hydrographs, up to three per group with all parameters varying by the
//! month the rainfall *fell*, each carrying its initial-abstraction
//! account.

use crate::model::{Network, UhResponse};

/// Flows below the predecessor's 1e-4 ft³/s are zeroed (§4.3).
const RDII_TOL: f64 = 2.832e-6;

/// A rainfall pulse in flight through a kernel.
struct Pulse {
    /// Elapsed time since the pulse fell (s).
    age: f64,
    /// Effective depth after abstraction and the R fraction (m).
    depth: f64,
    /// Kernel parameters from the fall month.
    t_peak: f64,
    t_base: f64,
}

/// One triangle's live state: its abstraction account and pulses.
struct Triangle {
    /// Remaining abstraction storage used (m); rainfall refills it before
    /// reaching the kernel, dry weather drains it.
    ia_used: f64,
    pulses: Vec<Pulse>,
}

/// One `[RDII]` assignment's convolution state.
pub struct RdiiState {
    /// The receiving vertex.
    pub vertex: usize,
    group: usize,
    area: f64,
    triangles: [Triangle; 3],
    /// Current inflow (m³/s).
    pub flow: f64,
}

impl RdiiState {
    /// Build states for every `[RDII]` assignment.
    pub fn build_all(net: &Network) -> Vec<RdiiState> {
        net.rdii
            .iter()
            .map(|r| {
                let mk = |slot: usize| {
                    // Initial depletion from the January entry, as the
                    // run-start condition.
                    let ia_used = net.unit_hydrographs[r.group].months[0][slot]
                        .as_ref()
                        .map_or(0.0, |u| u.ia_init);
                    Triangle {
                        ia_used,
                        pulses: Vec::new(),
                    }
                };
                RdiiState {
                    vertex: r.vertex,
                    group: r.group,
                    area: r.area,
                    triangles: [mk(0), mk(1), mk(2)],
                    flow: 0.0,
                }
            })
            .collect()
    }

    /// The gage driving this state's group.
    pub fn gage(&self, net: &Network) -> Option<usize> {
        net.unit_hydrographs[self.group].gage
    }

    /// Advance one hydrology step: `rain` is the group's gage rainfall
    /// rate (m/s), `month` the current 1–12 calendar month. Returns the
    /// updated inflow (m³/s).
    pub fn step(&mut self, net: &Network, rain: f64, month: u32, dt: f64) -> f64 {
        let group = &net.unit_hydrographs[self.group];
        let m = (month - 1) as usize;
        // §4.3: the processing grid is no coarser than the shortest
        // kernel limb, so a short triangle is never sampled past itself.
        let min_limb = group.months[m]
            .iter()
            .flatten()
            .flat_map(|u| {
                let base = u.t_peak * (1.0 + u.k);
                [u.t_peak, base - u.t_peak]
            })
            .filter(|l| *l > 0.0)
            .fold(f64::MAX, f64::min);
        let subs = if min_limb < dt && min_limb > 0.0 {
            ((dt / min_limb).ceil() as usize).clamp(1, 64)
        } else {
            1
        };
        let sub_dt = dt / subs as f64;
        let mut q_avg = 0.0;
        for step_i in 0..subs {
            // Rainfall enters once, on the first sub-step, at its full
            // step depth.
            let sub_rain = if step_i == 0 { rain * dt / sub_dt } else { 0.0 };
            q_avg += self.substep(net, sub_rain, m, sub_dt) * sub_dt;
        }
        self.flow = q_avg / dt;
        if self.flow < RDII_TOL {
            self.flow = 0.0;
        }
        self.flow
    }

    /// One convolution sub-step (§4.3).
    fn substep(&mut self, net: &Network, rain: f64, m: usize, dt: f64) -> f64 {
        let group = &net.unit_hydrographs[self.group];
        let mut q_total = 0.0;
        for (slot, tri) in self.triangles.iter_mut().enumerate() {
            let params: Option<&UhResponse> = group.months[m][slot].as_ref();

            // Initial abstraction: absorb rainfall before convolution,
            // recover in dry weather (§4.3).
            let mut effective = rain;
            if let Some(u) = params {
                if rain > 0.0 {
                    let room = (u.ia_max - tri.ia_used).max(0.0);
                    let absorbed = (rain * dt).min(room);
                    tri.ia_used += absorbed;
                    effective = rain - absorbed / dt;
                } else {
                    // Recovery rate is written per day.
                    tri.ia_used = (tri.ia_used - u.ia_recovery * dt / 86_400.0).max(0.0);
                }
            }

            // A new pulse joins with the fall month's kernel.
            if let Some(u) = params {
                if effective > 0.0 && u.r > 0.0 && u.t_peak > 0.0 {
                    tri.pulses.push(Pulse {
                        age: 0.0,
                        depth: effective * dt * u.r,
                        t_peak: u.t_peak,
                        t_base: u.t_peak * (1.0 + u.k),
                    });
                }
            }

            // Convolve: each pulse contributes its kernel ordinate at the
            // interval midpoint (§4.3).
            let mut q = 0.0;
            tri.pulses.retain_mut(|p| {
                let t_mid = p.age + 0.5 * dt;
                q += p.depth * triangle_ordinate(t_mid, p.t_peak, p.t_base);
                p.age += dt;
                p.age < p.t_base
            });
            q_total += q;
        }
        q_total * self.area
    }
}

/// The unit-area triangular kernel ordinate (1/s) at elapsed time `t`.
fn triangle_ordinate(t: f64, t_peak: f64, t_base: f64) -> f64 {
    if t <= 0.0 || t >= t_base {
        return 0.0;
    }
    let peak = 2.0 / t_base;
    if t <= t_peak {
        peak * t / t_peak
    } else {
        peak * (t_base - t) / (t_base - t_peak)
    }
}

// ── Checkpointing (§12.3) ────────────────────────────────────────────────────

impl RdiiState {
    /// Write one assignment's convolution memory (§12.3).
    ///
    /// The pulses are the state: each is rainfall still draining through
    /// a unit hydrograph, and a run resumed without them starts dry after
    /// a storm it has already had.
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_f, put_u};
        let RdiiState {
            // Parameters: which vertex, which unit-hydrograph group, and
            // the contributing area.
            vertex: _,
            group: _,
            area: _,
            triangles,
            flow,
        } = self;
        for tri in triangles {
            put_f(w, tri.ia_used)?;
            put_u(w, tri.pulses.len() as u64)?;
            for p in &tri.pulses {
                for v in [p.age, p.depth, p.t_peak, p.t_base] {
                    put_f(w, v)?;
                }
            }
        }
        put_f(w, *flow)
    }

    /// Read back what `checkpoint_put` wrote.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        for tri in &mut self.triangles {
            tri.ia_used = r.f()?;
            let n = r.u()? as usize;
            // A pulse is four values, so a declared count is a length
            // claim about the file and is checked before it sizes a
            // buffer.
            tri.pulses = Vec::new();
            tri.pulses.reserve(n.min(1024));
            for _ in 0..n {
                let age = r.f()?;
                let depth = r.f()?;
                let t_peak = r.f()?;
                let t_base = r.f()?;
                tri.pulses.push(Pulse {
                    age,
                    depth,
                    t_peak,
                    t_base,
                });
            }
        }
        self.flow = r.f()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::triangle_ordinate;

    #[test]
    fn the_kernel_has_unit_area() {
        // Numerically integrate a 2 h peak, K = 2 triangle.
        let (tp, tb) = (7200.0, 21_600.0);
        let n = 100_000;
        let dt = tb / n as f64;
        let area: f64 = (0..n)
            .map(|i| triangle_ordinate((i as f64 + 0.5) * dt, tp, tb) * dt)
            .sum();
        assert!((area - 1.0).abs() < 1e-6, "area {area}");
    }
}
