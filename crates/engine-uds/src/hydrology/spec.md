# Urban Drainage — Hydrology Specification

This document holds §3–§4 of the urban drainage specification: the surface
water balance and control measures, and the subsurface, snow, and
sewer-inflow processes.

The relations here are predominantly **empirical fits and established
constitutive models** — Tier 1 territory under §1.4's triage, adopted with
their fitted constants intact. Where a constant embeds a unit system it is
identified as such, per §1.7, and evaluated through the stated conversion.
The predecessor already integrates this compartment under error control; the
§1.5 standard is inherited here rather than imposed.

---

## 3. Surface Water Balance

### 3.1 Forcing

**Precipitation** enters through gages (§2.4), as intensity, volume, or
cumulative volume on a fixed recording interval, from supplied series or
external records; user-supplied data are start-of-interval values, and the
supported external formats carrying end-of-interval stamps are shifted on
read. Gridded rainfall is represented by one gage per cell or by area
weighting.

An external record whose text the caller supplied at load (format:
interoperability §14.12) is realised as the equivalent series — the
station's readings, unit-converted, become the gage's record, and
everything downstream (form, interval, catch factor, snow split) treats
the gage exactly as if that series had been written in the model. A gage
naming a file the caller did not supply refuses the load with the file
named: absent rain data is a missing input, never a dry record.

**Temperature**, needed by snowmelt and Hargreaves evaporation, comes from a
series (linearly interpolated) or from daily maximum/minimum climate records
converted to instantaneous values by sinusoidal interpolation: the minimum
placed at sunrise and the maximum three hours before sunset, both from the
solar declination

$$\delta = 0.40928\cos\!\big(0.017202\,(172 - d)\big), \qquad
\omega_h = 3.8197\arccos(-\tan\delta\tan\varphi),$$

with $d$ the day of year and $\varphi$ the site latitude (the `[SNOWMELT]`
declaration when present, else the predecessor's default of 40°N — never
the equator, whose day-length is seasonless), half-sine arcs
fitted between successive extremes in three branches over the day, the
overnight limb spanning from the previous day's maximum so days join
continuously. Saturation vapour pressure follows from the same temperature
by the fitted exponential the rain-melt relation of §4.2 consumes.

A model declaring neither source runs at a constant **21.1 °C (70 °F)**, the
predecessor's default. The value is operative rather than decorative: a
declared snow pack melts against it, as it does there. Air temperature is a
property of the run and not of the snow model, so it is served whenever it
is asked for — a model carrying a temperature record but no snowmelt
declaration still reports that record, and the results file's air-temperature
series (§14.9) is never absent.

**Evaporation** applies to ponded water, subsurface moisture, channels,
storage units, and control measures, from one of five sources: a constant;
monthly averages; a supplied series — deliberately a *step* function,
holding each entry's rate until the next timestamp, where every other series
interpolates; daily climate values scaled by monthly pan coefficients; or
the **Hargreaves** relation

$$E = 0.0023\,\frac{R_a}{\lambda}\,T_r^{1/2}(T_a + 17.8)
\quad \text{mm/day},$$

with $T_a$ and $T_r$ the 7-day running average temperature and daily range
(°C), $\lambda = 2.50 - 0.002361\,T_a$ the latent heat, and $R_a$ the
extraterrestrial radiation from latitude and day of year — an empirical fit
whose constants embed its stated units. A dry-only switch suppresses
land-surface evaporation during rainfall, leaving channel, storage, and
subsurface evaporation untouched. With the switch off, surface evaporation
is a term of the §3.2 balance like any other and draws on the ponded depth
as that depth evolves — including the rain arriving over the step.

> **CORRESPONDENCE:** the predecessor caps a sub-area's surface
> evaporation at the depth ponded when the step *began*, computing the
> limit before it adds the step's precipitation, so rain falling in a step
> cannot evaporate within it however long the step is. That is a property
> of its discretisation rather than of a wetted surface, and this engine
> does not carry it: evaporation competes with infiltration and outflow
> for the same evolving depth. Measured on the predecessor's `user3`, a
> constant 3 mm/day with the dry-only switch off: 0.676 acre-feet here
> against 0.570 there, 19 % more. With the switch on the two agree to 3 %,
> which is where the difference lives entirely.
>
> *Source: `subcatch.c:963–964`, `surfMoisture = subarea->depth / tStep` then `surfEvap = MIN(surfMoisture, evap)`, both before `subarea->inflow += precip` at `:970`.* **Wind speed** (monthly averages — default zero — or daily
climate values) enters only the rain-melt relation. Days missing from a
climate record inherit the most recent recorded values.

**Monthly adjustments** modify the forcing itself: additive offsets to
temperature (a temperature *difference*, converted as one) and potential
evaporation, and multiplicative factors on gage rainfall (applied during
sewer-inflow preprocessing too) and on saturated hydraulic conductivity.
A conductivity factor entered as zero or negative means "no adjustment" and
is replaced by 1 — reproduced as file semantics, and warned, since the
silent reading of zero as one is a trap the predecessor never reports.
Per-parcel monthly patterns may further scale the pervious sub-area's
depression storage and roughness; impervious sub-areas are never adjusted.

### 3.2 Overland Flow

Each parcel sub-area is a **nonlinear reservoir**: an idealised plane of
area $A$, width $W$, slope $S$, and Manning roughness $n$ holding ponded
depth $d$ with depression storage $d_s$,

$$\frac{\mathrm{d}d}{\mathrm{d}t} = i - e - f - \alpha\,(d - d_s)^{5/3},
\qquad \alpha = \frac{W\sqrt{S}}{A\,n},$$

with $i$ the rain or melt input, $e$ surface evaporation, $f$ infiltration
(pervious sub-areas only), outflow zero while $d \le d_s$, and the $5/3$
exponent from the wide-channel assumption that hydraulic radius equals
$d - d_s$. In SI, $\alpha$ carries no unit constant — the predecessor's 1.49
is the US-customary Manning factor, identified per §2.11.

**Example.** A sub-area of $A = 10^4$ m², $W = 100$ m, $S = 0.01$, and
$n = 0.1$ has $\alpha = 100\sqrt{0.01}/(10^4 \times 0.1) = 0.01$; at
1 cm of depth above depression storage the outflow term is
$0.01 \times 0.01^{5/3} = 4.642\times10^{-6}$ m/s, a drawdown of
16.71 mm/h.

Each of the three sub-areas integrates its own copy — impervious sub-areas
share a prorated $\alpha$ over their combined area — under the
**error-controlled embedded-pair integrator** of §3.5, the filling phase up
to $d_s$ handled analytically before integration engages. Parcel runoff is
the area-weighted sum. Run-on from upstream parcels and outfall returns
spreads over the non-measure area only, one step delayed per hop, applied
like additional rainfall on the receiver — **a parcel is never its own
upstream**: one naming itself as its outlet sends its runoff out of the
parcel system rather than back onto its own surface; a fraction of impervious runoff
may re-route onto the pervious sub-area or (exclusively) the reverse; and
$n = 0$ bypasses routing entirely — ponded water above depression storage
converts to runoff each step — permitting runoff-coefficient emulation.
The width parameter is the calibration handle and carries the predecessor's
meaning exactly.

The self-outlet rule needs stating because the delay makes the alternative
look convergent rather than obviously circular. Each hop costs a step, so
a parcel feeding itself sees its runoff return as run-on, become runoff
again, and sum like a geometric series whose ratio is set by whatever the
surface loses per pass. On a parcel that is wholly impervious and wholly
covered by a control measure, the losses per pass are nearly nothing and
the series barely converges: measured on the predecessor's own porous
pavement test, four inches of rain reported as four hundred and
forty-eight, in a run whose continuity still closed to a quarter of a
percent because the phantom run-on and the phantom runoff balanced each
other exactly. A mass balance cannot see water that is conserved while it
circulates.

> **CORRESPONDENCE:** the predecessor excludes self-routing in both places
> it would matter — it adds run-on to another parcel only where the target
> is not the source, and it counts a self-routing parcel's outflow as
> system runoff rather than as a transfer. This engine follows it exactly.
>
> *Source: `subcatch.c:540–543`, guarded `k != j`; `:742`, whose outflow test passes a self-routing parcel through.*

### 3.3 Infiltration

Computed on the pervious sub-area by one of five relations, selected per
parcel. All share two conventions: the applied rate is
$f = \min(f_p,\ i + d/\Delta t)$ — capacity or availability including ponded
water, the Curve Number relation folding run-on into ponded depth only —
and every relation carries a dry-weather **recovery** model, so continuous
multi-storm simulation is meaningful. The monthly conductivity pattern
scales $f_0$, $f_\infty$, $K_s$ (and the Green–Ampt upper-zone depth by its
square root); a separate monthly recovery pattern scales every regeneration
coefficient.

**Horton**: capacity decays exponentially from $f_0$ to $f_\infty$ with
coefficient $k_d$. The state is the **equivalent time** $t_p$ on the curve —
advanced directly when infiltration ran at capacity or the curve has
flattened ($t_p \ge 16/k_d$), otherwise recovered by solving the cumulative
curve $F(t_p)$ for the time matching what actually infiltrated — and the capacity applied over a step is the **step average of
the cumulative curve**, floored at $f_\infty$, not the point rate. Dry steps
recover along an exponential drying curve, $k_r = 3.912/T_{dry}$ for a user
drying time in days, through the closed-form wetting/drying map. An optional
volume cap $F_{max}$ makes the surface impermeable beyond a total, wound
back along the recovery curve in dry weather.

**Modified Horton**: capacity declines with **cumulative excess
infiltration** $F_e$ (volume above $f_\infty$),
$f_p = \max(f_0 - k_d F_e,\ f_\infty)$ — better behaved under light rain —
fully explicit, with dry-weather decay $F_e \leftarrow F_e e^{-k_r\Delta t}$.
The optional $F_{max}$ cap is a **finite store above the steady drainage**:
the surface seals (zero infiltration) once $F_e$ reaches $F_{max}$, with
$F_e$ capped there, and the same dry-weather decay reopens it. The steady
$f_\infty$ share never counts against the cap — that water genuinely
drains away.

> **DEVIATION from SWMM:** the predecessor's cap line is inverted
> (`Fe = MAX(Fe, Fmax)` where a minimum is evidently meant), so any wet
> step under a configured cap instantly seals its surface. The cap's
> documented meaning is implemented; the defect is not adopted.

For both Horton forms, degenerate parameters ($f_0 = f_\infty$ or
$k_d = 0$) mean constant capacity $f_0$; but **$f_0 < f_\infty$ yields zero
infiltration for the whole run** — reproduced as file semantics, and flagged
by validation as an advisory, because the predecessor's silent zero is
indistinguishable in output from a deliberately impermeable surface.

**Green–Ampt** (Mein–Larson two-stage): parameters $K_s$, suction $\psi_s$,
initial deficit $\theta_d$, the current ponded depth added to the suction
throughout. All rain infiltrates until cumulative infiltration reaches
$F_s = K_s(\psi_s + d)\theta_d/(i_a - K_s)$; capacity thereafter is
$f_p = K_s\big(1 + (\psi_s + d)\theta_d/F\big)$, integrated over the step in
cumulative form by a bracketed solve (short steps with established $F$ using
the point rate), floored at $F + K_s\Delta t$ and capped by availability; a zero moisture
deficit bypasses the solve for infiltration at exactly $K_s$.
Recovery tracks the deficit of an upper zone of thickness
$L_u = 4\sqrt{K_s}$ — an empirical fit in inches with $K_s$ in in/hr,
identified and converted — regenerating at $k_r = \sqrt{K_s}/75$ per hour,
with a new event after a dry spell of $0.06/k_r$.

**Modified Green–Ampt**: identical except that low-intensity periods do not
reset the event state, so saturation arrives sooner under light rain. This
is the variation control-measure surfaces and storage seepage invoke.

**Curve Number**: the SCS relation differenced incrementally,
$S_{max} = 1000/CN - 10$ inches (the tabulated relation's own units,
identified), event totals updating $F = P - P^2/(P + S_e)$ with the applied
rate $\Delta F/\Delta t$, the previous rate held through rainless gaps so
ponded water keeps infiltrating; capacity recovers at $k_r S_{max}$ per hour
with $k_r = 1/(24\,T_{dry})$, new events after $0.06/k_r$ hours. $CN$ clamps
to $[10, 99]$ and the relation applies to the parcel as fully pervious,
tabulated urban curve numbers already lumping impervious cover.

### 3.4 Control Measures

A control measure is a layered moisture-accounting unit (§2.4): surface,
optional pavement, soil, and storage layers with an optional underdrain,
each layer's state a flux balance —

$$\phi_1\frac{\mathrm{d} d_1}{\mathrm{d}t} = i + q_0 - e_1 - f_1 - q_1,
\qquad
D_2\frac{\mathrm{d}\theta_2}{\mathrm{d}t} = f_1 - e_2 - f_2,
\qquad
\phi_3\frac{\mathrm{d} d_3}{\mathrm{d}t} = f_2 - e_3 - f_3 - q_3,$$

with the eight unit types the predecessor defines as configurations of this
template, adopted exactly: bio-retention (the full triple); rain gardens
(no storage layer, an unconditional equal-flux rule binding percolation to
exfiltration); green roofs (storage becomes a drainage mat, sealed —
exfiltration zero, drainage by Manning flow on the surface slope, and a mat
with no roughness passing percolation straight through rather than sealing);
infiltration trenches (no soil layer, one end-limited surface-to-storage
flux); permeable pavement (a clog-reduced permeability intake in place of
Green–Ampt, optional soil layer, and a **water-bearing pavement course**:
the layer stores water in its voids — thickness × void fraction ×
pervious paver fraction — with the same permeability limiting both its
intake and its percolation, so when the layer beneath is the bottleneck
water backs up into the pavement before the surface ponds, and a course
above a bottleneck buffers a storm instead of shedding it; its
evapotranspiration sits between the surface's and the soil's in the
top-down cascade, and the underdrain's stacked head passes through a
full pavement to reach the ponded surface); rain barrels (pure storage, sealed, no
evaporation; a barrel is an empty vessel, so its storage layer's void
ratio is read but not applied — stored volume is stored depth, and a
barrel holding $h_0$ of head over a drain $q = C h^{1/2}$ drains dry in
exactly $2\sqrt{h_0}/C$, not that time scaled by a void fraction the
vessel does not have; intake limited by freeboard *plus* concurrent drain outflow,
and a drain held shut until continuously dry weather has outlasted the
configured delay — dryness judged by the parcel's rainfall rate falling
below the 0.001 in/hr minimum-runoff threshold, never by the unit's total
inflow, so the receding Manning tail captured from tributary area cannot
hold the drain shut indefinitely; a **zero** delay never latches the drain,
which then discharges during rain; the drain-delay clock starts at the
configured delay, so a run beginning dry opens the drain only after that
much dry time; and a **covered** storage layer excludes direct rainfall
from the barrel's intake and nothing else — cover is the predecessor's
rain-barrel-only flag, never an exfiltration or evaporation seal);
rooftop disconnection (a lone surface layer whose gutter-capacity drain
pre-empts overflow — the drain line's coefficient is the gutter's
capacity, a plain rate in the file's rain-rate unit with the exponent
ignored, and a zero or absent coefficient is a gutter with no capacity,
everything shed going onward as surface outflow; a roof ponds on its
full plan area, so the surface line's vegetation fraction is read but
not applied — a template's 25 % vegetation would otherwise turn a 6 in
storage depth into 4.5 in of held water on a surface that has no
vegetation); and vegetative swales (trapezoidal depth-varying
geometry, balance written on volume, widths floored at 0.1524 m with the
side slope recomputed to keep the section consistent).

**Constitutive fluxes** echo §3.3 with measure-specific parameters:
surface outflow above the berm is Manning flow at the §3.2 α — the
unit's width over its area included, a widthless unit spilling its
excess directly. The ponded surface stores water in its voids: depth
advances by net flux over the void fraction (one minus the vegetation
volume fraction), so the free surface rises through the vegetation and
a vegetated berm overtops after berm × void of water — the only
advance under which stored volume, depth × void, conserves what
flowed; surface-to-soil intake is modified Green–Ampt (with the stated pavement and
swale exceptions, including the swale's dependence on the parent parcel's
own infiltration model); soil percolation is the exponential
$K_{2S}e^{-k_{slope}(\phi_2 - \theta_2)}$, zero below field capacity, on the
layer's own conductivity slope; exfiltration is the native soil's saturated
conductivity, capped by aquifer storability where §4.1 is modelled;
evapotranspiration cascades top-down with the predecessor's suppression
rules; the underdrain is the power relation $q_3 = C_{3D} h_3^{\eta_{3D}}$
with its head regimes — the head is the storage layer's water depth, and
only once storage is full does it stack upward: first the saturated-excess
fraction of the soil layer, $(\theta_2 - \theta_{FC})/(\phi_2 -
\theta_{FC})$ of its thickness, then, only when the soil is fully
saturated, the ponded surface depth — plus hysteretic open/close
thresholds on that same stacked head and an optional multiplier curve.
Its coefficients are unit-dependent per §14.6, the multiplier curve read
against the offset-relative head in the file's rain-depth unit.

> **CORRESPONDENCE:** the predecessor counts the surface layer's stored
> volume as depth × void yet advances the depth by the raw flux, so its
> vegetation displaces bookkeeping volume and no water: a vegetated
> berm holds its full height of water and sheds nothing a bare one
> would not. The two definitions cannot both hold, and the transient
> imbalance escapes its continuity check only because the pond later
> drains down the column. This engine keeps the volume definition, so
> on a vegetated surface over a bottlenecked column it sheds water the
> predecessor stores in space its own vegetation occupies — the
> standard porous-pavement fixture differs by exactly this, and zeroing
> the vegetation makes the engines agree to every printed digit.
>
> *Source: `lidproc.c` `pavementFluxRates` — `SurfaceVolume =
> surfaceDepth * voidFrac` beside `f[SURF]` undivided by the void; the
> other templates repeat the pair.*

**The limiter cascade is normative.** Each flux is clipped to what its
source supplies before the layer beneath is asked what it accepts —
percolation by drainable water, exfiltration by delivery plus store, drain
by standing volume, percolation re-capped by storage freeboard plus
outflow, intake last by soil voids plus soil outflow — and the
saturated-saturated case collapses to the equal-flux rule. This ordering is
part of the model: it is what stops a saturated cell draining faster than
its media pass water.

**Advance.** Layer states advance by the limiter-cascade balance over each
hydrology step — an update that is mass-conserving by construction, every
flux clipped to the volume actually present, with the rate-sampling error
governed by the wet-step bound of §10.1. The swale, whose geometry varies
with depth, advances by the iterated trapezoidal method on its **stored
volume** — equally weighted start- and end-of-step rates, both evaluated
under the step's own forcing, iterated to a 1 mm depth tolerance with at
most twenty passes, the final pass accepted as-is if the tolerance is
still unmet. The booked fluxes are the same equally weighted averages the
volume advance uses, so the unit's balance closes identically at any step
— booking one instant's rates against an averaged advance leaks the
half-difference every step, which is a ledger error that grows with the
step, not a discretisation error that shrinks with it. A volume clamped
at empty scales the drawing fluxes to the water actually present, and at
the berm the surplus of the averaged net inflow spills onward. This is a
deliberate, recorded exception
to blanket §3.5 integration: the cascade's clipped fluxes are discontinuous in state, where
the embedded-pair integrator presumes smoothness, and the balance form is
exact for the rates given.

**Deployment** is per unit: a percentage of the parcel's impervious-area
and (separately) pervious-area runoff, validated to sum to at most 100 %; a
combined footprint within 0.1 % of the parcel area snapped equal to it; and
run-on from upstream reaching units only when the footprint equals the
whole parcel — the snap is what makes that gate reachable. Direct rainfall
always lands on the unit (the covered rain barrel excepted). Surface overflow joins parcel runoff, exfiltration
joins infiltration, and drain flow routes separately — to the parcel's
outlet by default, to another parcel one hydrology step delayed, to a vertex
interpolated per routing step. A unit flagged **return-to-pervious**
instead sends its overflow and unrouted drain flow back onto the parcel's
pervious sub-area, one hydrology step delayed like run-on — a second
infiltration opportunity, the predecessor's semantics for the flag.

> **CORRESPONDENCE:** the predecessor's return-to-pervious accounting can
> *increase* a parcel's reported runoff volume by half over its
> measure-free twin under identical rain — water counted again as it
> recirculates. Adding a passive measure cannot create runoff; this
> engine's loop conserves, and its totals differ from the predecessor's
> on returned-flow models accordingly. Initial saturation pre-fills soil and storage
>
> *Source: `subcatch.c:69` (`VlidReturn`) and `:586`.*
and shrinks the Green–Ampt deficit accordingly. Gravel and pavement layers
may clog on cumulative treated volume: the file's clogging factor scales
the layer's own void depth — thickness × void fraction, further × the
pervious paver fraction for pavement — into a treatable depth, and
conductivity falls linearly to zero as cumulative unit inflow approaches
it. Pavement permeability may regenerate on a fixed-day cycle by a stated
degree, the regeneration discounting the pavement's treated-volume account;
the storage layer's account never regenerates. Outflow concentrations
are volume-based per the predecessor: load reduction is runoff reduction,
with optional per-constituent percent removals on drain loads only.

### 3.5 Integration

Sub-area ponded depths (§3.2) and the aquifer pair (§4.1) integrate under
an **adaptive embedded-pair integrator** with per-step local error control:
step rescaling by the standard $0.9\,\varepsilon^{-1/4}$ (rejection) and
$0.9\,\varepsilon^{-1/5}$ (acceptance) rule, growth and shrink clamped to
$[0.1, 5]\times$, and an error tolerance of $10^{-5}$ m on each integrated
state. A step that cannot meet tolerance at the integrator's floor proceeds
with the degraded-accuracy warning of §1.5.

## 4. Subsurface, Snow, and Sewer Inflow

### 4.1 Groundwater

Each parcel may sit on a two-zone aquifer: an unsaturated zone of uniform
moisture $\theta$ over a saturated zone of depth $d_L$, with
$d_U = E_G - E_B - d_L$ for ground and bottom elevations. Six per-area
fluxes connect the zones: surface infiltration $f_I$ (the §3.3 result,
pervious-scaled, capped by storability), upper-zone evapotranspiration
$f_{EU}$, percolation $f_U$, lower-zone evapotranspiration $f_{EL}$, deep
percolation $f_L$, and lateral discharge $f_G$.

The state pair advances as

$$\frac{\mathrm{d}\theta}{\mathrm{d}t} = \frac{f_I - f_{EU} - f_U}{E_G - E_B - d_L},
\qquad
\frac{\mathrm{d} d_L}{\mathrm{d}t} = \frac{f_U - f_{EL} - f_L - f_G}{\phi - \theta},$$

under §3.5's integrator, $\theta$ clamped to $[\theta_{WP}, \phi)$, $d_L$ to
$[0, E_G - E_B)$, the water table jumping to the surface when $\theta$
reaches porosity. These are the forms the predecessor *implements*; its
manual states two other, mutually inconsistent variants, and per §1.3 the
source is authoritative.

**Constitutive relations**, adopted with provenance: percolation
$f_U = K_s\,e^{-(\phi - \theta)HCO}\big(1 + 2\psi_{TS}(\theta -
\theta_{FC})/d_U\big)$, zero below field capacity, capped by drainable
volume — the suction-gradient factor the manual omits and the source
applies. Evapotranspiration draws surface → upper → lower in priority, the
upper share a user fraction of potential (pervious-prorated, optionally
monthly-patterned), the lower share complementary to the *patterned* upper
fraction, scaled by water-table reach into the cutoff depth, with no
subsurface draw during steps with surface infiltration and none below the
wilting point. Deep percolation is the linear reservoir
$f_L = DP\,d_L/(E_G - E_B)$.

**Lateral discharge** is the configurable power relation

$$f_G = A1\,(d_L - h^*)^{B1} - A2\,(h_{SW} - h^*)^{B2} + A3\,d_L\,h_{SW}$$

with $h_{SW}$ the receiving vertex's stage (fixed or live) and $h^*$ a
threshold defaulting to the vertex invert; the whole relation returns zero
when $d_L \le h^*$ — surface water cannot recharge a depleted aquifer
through the interaction terms — and negative $f_G$ (bank storage) is
admitted when the interaction term is unused. The flux is bounded by
aquifer storage, unsaturated-zone acceptance, and vertex supply. Custom
expressions asymmetrically **replace** deep percolation but **add to**
lateral discharge, and evaluate per §14.6's expression rule in the file's
unit system. Their vocabulary is eleven names: `HGW` water-table height
and `HSW` surface-water height above the aquifer bottom, `HCB` the
threshold height $h^*$, `HGS` the total aquifer depth (all in the file's
length unit); `KS` saturated and `K` current unsaturated conductivity,
`FI` surface infiltration, and `FU` upper-zone percolation (rain-rate
unit); `THETA` moisture and `PHI` porosity (dimensionless); and `A` the
parcel area (land-area unit). The deep-percolation result reads in the
rain-rate unit; the lateral result in the lateral-coefficient basis of
§14.6 (ft³/s per acre, m³/s per hectare). Coefficients are unit-dependent
per §14.6. Infiltrate arrives at the vertex clean unless a constant
concentration is assigned.

### 4.2 Snow

Snow state is water-equivalent depth per parcel over a three-way split —
pervious, **plowable** impervious (a user fraction, always fully covered),
and remaining impervious. Precipitation falls as snow at or below the
rain/snow temperature, gage snowfall scaled by the catch factor; the split
applies gage-wide, so a parcel without a snow pack receives catch-scaled
snowfall as immediate liquid.

**Melt** is Anderson's two-regime model, an empirical fit whose constants
embed US units (in/hr, °F, mph), identified and converted at evaluation.
During rain above 0.02 in/hr, the saturated radiation-free energy budget

$$SMELT = \big(0.001167 + 7.5\gamma U_A + 0.007\,i\big)(T_a - 32)
        + 8.5\,U_A(e_a - 0.18)$$

with the wind function, saturation vapour pressure of §3.1, and the
psychrometric factor $\gamma = 0.000359\,p_a$ from the site-elevation
atmospheric-pressure fit $p_a = 29.9 - 1.02z + 0.0032z^{2.4}$ in Hg, $z$ in
thousands of feet — an empirical fit in its native units, bypassed for
$z \le 0$, which takes the sea-level value directly: the fractional power is
undefined for negative $z$, a domain guard, not a claim that sea level is
special. Otherwise a degree-day law
$DHM\,(T_a - T_{base})$, the coefficient sweeping sinusoidally between the
user's 21 December minimum and 21 June maximum. De-icing is represented by
lowering a surface's base temperature.

**Cold content** must be paid before liquid leaves: an antecedent
temperature index snapping to air temperature during snowfall above
0.02 in/hr and otherwise relaxing by the user weight rescaled from its
6-hour basis to the step, the index capped at the surface's base melt
temperature; below base temperature the deficit accumulates,
capped by the pack's heat capacity (0.007 *inches* of water equivalent per
°F per *foot* of pack, so the ratio applied to a pack depth is 0.007/12 per
°F); melt debits the deficit before releasing. A **free-water
reservoir** of the pack's holding capacity must also fill before release —
rain on the covered fraction joins it, so rain-on-snow is delayed by the
pack — and an over-specified initial free water is clamped to capacity.
Partial cover on the unplowed surfaces follows the two watershed **areal
depletion curves** with Anderson's temporary-curve adjustment after fresh
snowfall on partial cover; melt and cold-content exchange scale by covered
fraction. When the plowable surface exceeds its trigger depth (defaulted
effectively off), its entire depth redistributes by five constant fractions
— other sub-areas, another parcel, out of system, immediate melt. A
transfer to a parcel that cannot hold snow — no pack, or no pervious
surface — leaves the system with the plowed export rather than vanishing.
A pack below $10^{-3}$ in flushes as immediate melt. The net per-surface
result replaces gage rainfall as input to §3.2 and §3.3; snow does not
alter infiltration or roughness.

The pack's volume basis is the **full parcel area**, control-measure
footprint included: the footprint receives the impervious surfaces' melt
output as its precipitation input, so its share of snowfall stores on the
same basis it later melts from — precipitation booked on arrival, melt
booked on release, the §11.1 surface ledger closing at every step.

### 4.3 Sewer Inflow (RDII)

Rainfall-dependent inflow and infiltration is a rainfall convolution at
designated vertices, independent of the surface machinery. The kernel is
the **triangular unit hydrograph** with volume fraction $R$, time to peak
$T$, and recession ratio $K$ — base $T_b = T(1+K)$, unit area, evaluated at
interval midpoints — summed over up to three triangles per group (rapid,
mixed, slow) with all parameters varying by calendar month, the month being
that in which the rainfall *fell*, not the response month. Each triangle
carries an **initial abstraction** account (capacity, initial depletion,
dry-weather recovery rate) absorbing rainfall before convolution. The
convolved per-area depth is scaled by a user sewershed area, which need not
correspond to any parcel.

Monthly $R$ triples must be non-negative and sum to at most 1, with the
predecessor's 1 % slack accepted. Sampling uses a processing grid no
coarser than the wet hydrology step or the shortest kernel limb; results
are held piecewise-constant during routing, with flows below
$2.832\times10^{-6}$ m³/s (the predecessor's $10^{-4}$ ft³/s) zeroed. Whether the convolution is precomputed or
evaluated on demand is implementation freedom; its semantics are as stated.
