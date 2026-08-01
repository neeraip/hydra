//! Keyword matching and the section vocabulary (§14.3).
//!
//! One rule performs every keyword lookup: the first table entry that is a
//! case-insensitive **prefix** of the token matches. Trailing characters are
//! ignored; truncations are rejected. Table order is load-bearing wherever
//! one entry prefixes another, and the orderings below are normative:
//! `[INLET_USAGE` precedes `[INLET`, so the longer name wins.
//!
//! The accept-set is the predecessor's exactly. The *warning* layer is this
//! engine's own (§14.3): a token matched by prefix rather than by canonical
//! spelling is accepted and reported, so a typo the predecessor would
//! swallow is visible. Canonical spellings — not the table prefixes, which
//! are themselves abbreviations (`[JUNC`) — are the equality standard.

/// First-prefix-wins keyword lookup over `table`, case-insensitively.
///
/// Returns the index of the first entry that is a prefix of `token`. The
/// comparison runs to the end of the *entry*, so `DYNWAVEXYZ` matches
/// `DYNWAVE` while the truncation `DYN` matches nothing.
pub fn match_keyword(table: &[&str], token: &str) -> Option<usize> {
    table.iter().position(|entry| is_prefix_ci(entry, token))
}

fn is_prefix_ci(entry: &str, token: &str) -> bool {
    token.len() >= entry.len()
        && entry
            .chars()
            .zip(token.chars())
            .all(|(e, t)| e.eq_ignore_ascii_case(&t))
}

/// An input-file section (§14.5). Variants follow the predecessor's
/// vocabulary; the nine display-metadata sections carry no engine semantics
/// and are preserved verbatim for writers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Title,
    Options,
    Files,
    RainGages,
    Temperature,
    Evaporation,
    Subcatchments,
    Subareas,
    Infiltration,
    Aquifers,
    Groundwater,
    Snowpacks,
    Junctions,
    Outfalls,
    Storage,
    Dividers,
    Conduits,
    Pumps,
    Orifices,
    Weirs,
    Outlets,
    XSections,
    Transects,
    Losses,
    Controls,
    Pollutants,
    LandUses,
    Buildup,
    Washoff,
    Coverages,
    Dwf,
    Inflows,
    Patterns,
    Rdii,
    Hydrographs,
    Loadings,
    Treatment,
    Curves,
    TimeSeries,
    Report,
    Coordinates,
    Vertices,
    Polygons,
    Labels,
    Symbols,
    Backdrop,
    Tags,
    Profiles,
    Map,
    LidControls,
    LidUsage,
    Gwf,
    Adjustments,
    Events,
    Streets,
    InletUsage,
    Inlets,
}

impl Section {
    /// Whether this section is display metadata: parsed for well-formedness
    /// only and preserved verbatim for writers (§14.5).
    pub fn is_display_metadata(self) -> bool {
        matches!(
            self,
            Section::Map
                | Section::Coordinates
                | Section::Vertices
                | Section::Polygons
                | Section::Symbols
                | Section::Labels
                | Section::Backdrop
                | Section::Tags
                | Section::Profiles
        )
    }
}

/// The section table: `(prefix, canonical, section)`, in the predecessor's
/// order. The prefix is what matching runs against; the canonical spelling
/// is the equality standard the §14.3 warning is keyed on.
///
/// Order is normative. In this table exactly one pair is prefix-related —
/// `[INLET_USAGE` and `[INLET` — and the longer is listed first, so it wins.
pub const SECTIONS: &[(&str, &str, Section)] = &[
    ("[TITLE", "[TITLE]", Section::Title),
    ("[OPTION", "[OPTIONS]", Section::Options),
    ("[FILE", "[FILES]", Section::Files),
    ("[RAINGAGE", "[RAINGAGES]", Section::RainGages),
    ("[TEMPERATURE", "[TEMPERATURE]", Section::Temperature),
    ("[EVAP", "[EVAPORATION]", Section::Evaporation),
    ("[SUBCATCHMENT", "[SUBCATCHMENTS]", Section::Subcatchments),
    ("[SUBAREA", "[SUBAREAS]", Section::Subareas),
    ("[INFIL", "[INFILTRATION]", Section::Infiltration),
    ("[AQUIFER", "[AQUIFERS]", Section::Aquifers),
    ("[GROUNDWATER", "[GROUNDWATER]", Section::Groundwater),
    ("[SNOWPACK", "[SNOWPACKS]", Section::Snowpacks),
    ("[JUNC", "[JUNCTIONS]", Section::Junctions),
    ("[OUTFALL", "[OUTFALLS]", Section::Outfalls),
    ("[STORAGE", "[STORAGE]", Section::Storage),
    ("[DIVIDER", "[DIVIDERS]", Section::Dividers),
    ("[CONDUIT", "[CONDUITS]", Section::Conduits),
    ("[PUMP", "[PUMPS]", Section::Pumps),
    ("[ORIFICE", "[ORIFICES]", Section::Orifices),
    ("[WEIR", "[WEIRS]", Section::Weirs),
    ("[OUTLET", "[OUTLETS]", Section::Outlets),
    ("[XSECT", "[XSECTIONS]", Section::XSections),
    ("[TRANSECT", "[TRANSECTS]", Section::Transects),
    ("[LOSS", "[LOSSES]", Section::Losses),
    ("[CONTROL", "[CONTROLS]", Section::Controls),
    ("[POLLUT", "[POLLUTANTS]", Section::Pollutants),
    ("[LANDUSE", "[LANDUSES]", Section::LandUses),
    ("[BUILDUP", "[BUILDUP]", Section::Buildup),
    ("[WASHOFF", "[WASHOFF]", Section::Washoff),
    ("[COVERAGE", "[COVERAGES]", Section::Coverages),
    ("[INFLOW", "[INFLOWS]", Section::Inflows),
    ("[DWF", "[DWF]", Section::Dwf),
    ("[PATTERN", "[PATTERNS]", Section::Patterns),
    ("[RDII", "[RDII]", Section::Rdii),
    ("[HYDROGRAPH", "[HYDROGRAPHS]", Section::Hydrographs),
    ("[LOADING", "[LOADINGS]", Section::Loadings),
    ("[TREATMENT", "[TREATMENT]", Section::Treatment),
    ("[CURVE", "[CURVES]", Section::Curves),
    ("[TIMESERIES", "[TIMESERIES]", Section::TimeSeries),
    ("[REPORT", "[REPORT]", Section::Report),
    ("[COORDINATE", "[COORDINATES]", Section::Coordinates),
    ("[VERTICES", "[VERTICES]", Section::Vertices),
    ("[POLYGON", "[POLYGONS]", Section::Polygons),
    ("[LABEL", "[LABELS]", Section::Labels),
    ("[SYMBOL", "[SYMBOLS]", Section::Symbols),
    ("[BACKDROP", "[BACKDROP]", Section::Backdrop),
    ("[TAG", "[TAGS]", Section::Tags),
    ("[PROFILE", "[PROFILES]", Section::Profiles),
    ("[MAP", "[MAP]", Section::Map),
    ("[LID_CONTROL", "[LID_CONTROLS]", Section::LidControls),
    ("[LID_USAGE", "[LID_USAGE]", Section::LidUsage),
    ("[GWF", "[GWF]", Section::Gwf),
    ("[ADJUSTMENT", "[ADJUSTMENTS]", Section::Adjustments),
    ("[EVENT", "[EVENTS]", Section::Events),
    ("[STREET", "[STREETS]", Section::Streets),
    ("[INLET_USAGE", "[INLET_USAGE]", Section::InletUsage),
    ("[INLET", "[INLETS]", Section::Inlets),
];

/// A section-header match: which section, and whether the token was the
/// canonical spelling (a non-canonical match is accepted and warned).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionMatch {
    /// The recognised section.
    pub section: Section,
    /// Whether the token was the canonical spelling.
    pub canonical: bool,
}

/// Recognise a section-header token (§14.5): first-prefix-wins over the
/// table, with canonicity judged against the canonical spelling.
pub fn match_section(token: &str) -> Option<SectionMatch> {
    SECTIONS
        .iter()
        .find(|(prefix, _, _)| is_prefix_ci(prefix, token))
        .map(|&(_, canonical, section)| SectionMatch {
            section,
            canonical: token.eq_ignore_ascii_case(canonical),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_characters_are_ignored_truncations_rejected() {
        let table = &["DYNWAVE"];
        assert_eq!(match_keyword(table, "DYNWAVE"), Some(0));
        assert_eq!(match_keyword(table, "DYNWAVEXYZ"), Some(0));
        assert_eq!(match_keyword(table, "dynwavexyz"), Some(0));
        assert_eq!(match_keyword(table, "DYN"), None);
    }

    #[test]
    fn first_prefix_wins_by_table_order() {
        // The [REPORT] pathology: NODE listed before NODESTATS makes the
        // NODESTATS branch unreachable in the predecessor (§14.3).
        let table = &["NODE", "NODESTATS"];
        assert_eq!(match_keyword(table, "NODESTATS"), Some(0));
    }

    #[test]
    fn every_canonical_spelling_matches_its_own_section() {
        for &(_, canonical, section) in SECTIONS {
            let m = match_section(canonical).expect(canonical);
            assert_eq!(m.section, section, "{canonical}");
            assert!(m.canonical, "{canonical} should be canonical");
        }
    }

    #[test]
    fn inlet_usage_wins_over_inlet_by_order() {
        assert_eq!(
            match_section("[INLET_USAGE]").unwrap().section,
            Section::InletUsage
        );
        assert_eq!(match_section("[INLETS]").unwrap().section, Section::Inlets);
        // The bare prefix is Inlets, not InletUsage — order decides.
        assert_eq!(match_section("[INLET]").unwrap().section, Section::Inlets);
    }

    #[test]
    fn abbreviated_headers_are_accepted_but_not_canonical() {
        // The table's own prefixes are abbreviations: [JUNC] is accepted —
        // the entry "[JUNC" prefixes it — but warned as non-canonical.
        let m = match_section("[JUNC]").unwrap();
        assert_eq!(m.section, Section::Junctions);
        assert!(!m.canonical);

        let m = match_section("[junctions]").unwrap();
        assert_eq!(m.section, Section::Junctions);
        assert!(m.canonical, "case difference alone is still canonical");
    }

    #[test]
    fn garbage_trailing_a_valid_header_is_accepted_but_warned() {
        let m = match_section("[JUNCTIONS-OLD]").unwrap();
        assert_eq!(m.section, Section::Junctions);
        assert!(!m.canonical);
    }

    #[test]
    fn an_unrecognised_header_matches_nothing() {
        assert_eq!(match_section("[NOSUCHSECTION]"), None);
    }

    #[test]
    fn the_display_metadata_set_is_exactly_nine() {
        let n = SECTIONS
            .iter()
            .filter(|(_, _, s)| s.is_display_metadata())
            .count();
        assert_eq!(n, 9);
    }

    #[test]
    fn the_table_order_matches_the_predecessors() {
        // Pin the one normative ordering plus the table length, so a
        // re-sort or an insertion cannot silently change matching.
        let iu = SECTIONS
            .iter()
            .position(|(p, _, _)| *p == "[INLET_USAGE")
            .unwrap();
        let i = SECTIONS
            .iter()
            .position(|(p, _, _)| *p == "[INLET")
            .unwrap();
        assert!(iu < i, "[INLET_USAGE must precede [INLET");
        assert_eq!(SECTIONS.len(), 57);
    }
}
