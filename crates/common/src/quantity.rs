//! Quantity contract: engine-declared physical quantities and their two
//! display-system renderings (spec §5).
//!
//! Values crossing an engine boundary for a quantity-bearing field are in
//! that quantity's **SI display unit**; applications convert for display
//! and convert back on input using only the descriptor. Engines never
//! format, and applications never hardcode a conversion — so a quantity
//! this layer has never heard of (a rainfall intensity, an infiltration
//! rate) costs an application nothing to support.

use serde::Serialize;

/// Descriptor of one physical quantity in an engine's catalog (spec §5).
///
/// Quantity keys are engine-scoped: two engines may both declare a `flow`
/// quantity without their descriptors agreeing, because no value ever
/// crosses between engines.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuantityDescriptor {
    /// Stable quantity identifier, opaque to this layer; referenced by
    /// attribute schemas (spec §4.4) and result variables (spec §6).
    pub key: &'static str,
    /// Unit text in the SI display system (e.g. "m", "L/s", "mm/hr").
    pub si_label: &'static str,
    /// Unit text in the US-customary display system (e.g. "ft", "gpm").
    pub us_label: &'static str,
    /// Scale of the affine SI→US display conversion:
    /// `us = si * si_to_us_scale + si_to_us_offset`.
    pub si_to_us_scale: f64,
    /// Offset of the affine SI→US display conversion. Zero for all but
    /// temperature-like quantities.
    pub si_to_us_offset: f64,
    /// Suggested display precision in the SI system. Advisory.
    pub si_decimals: u8,
    /// Suggested display precision in the US system. Advisory.
    pub us_decimals: u8,
}

impl QuantityDescriptor {
    /// Convert a value from the SI display unit to the US display unit.
    pub fn si_to_us(&self, si: f64) -> f64 {
        si * self.si_to_us_scale + self.si_to_us_offset
    }

    /// Convert a value from the US display unit back to the SI display
    /// unit — the exact inverse of [`Self::si_to_us`].
    pub fn us_to_si(&self, us: f64) -> f64 {
        (us - self.si_to_us_offset) / self.si_to_us_scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMP: QuantityDescriptor = QuantityDescriptor {
        key: "temperature",
        si_label: "°C",
        us_label: "°F",
        si_to_us_scale: 1.8,
        si_to_us_offset: 32.0,
        si_decimals: 1,
        us_decimals: 1,
    };

    #[test]
    fn affine_conversion_round_trips() {
        assert_eq!(TEMP.si_to_us(100.0), 212.0);
        assert_eq!(TEMP.us_to_si(212.0), 100.0);
        assert_eq!(TEMP.us_to_si(TEMP.si_to_us(37.5)), 37.5);
    }
}
