//! §14.15: the overland-mesh input sections.
//!
//! Everything here binds the successor format's `[2D_*]` grammar to the
//! §15.2 model: field orders, defaults, the index-or-tag addressing rule,
//! the one-way SI header, and display-unit conversion. Semantics live in
//! §15; this module never interprets the mesh, only reads it.

use super::keywords::{match_keyword, Section};
use super::lex::FiniteParse;
use super::survey::{Diagnostic, DiagnosticKind, TokenLine};
use crate::overland::{
    BoundaryCondition, BoundaryRow, CellClosure, ConveyanceRow, CouplingRow, FaceReconstruction,
    InfiltrationRow, InitVelocityRow, MeshCell, MeshVertex, OverlandMesh, OverlandOptions,
    RainfallMode, SeriesOrValue,
};

fn err(line: usize, kind: DiagnosticKind) -> Diagnostic {
    Diagnostic { line, kind }
}

fn bad(line: usize, token: &str) -> Diagnostic {
    Diagnostic {
        line,
        kind: DiagnosticKind::BadValue {
            token: token.to_string(),
        },
    }
}

/// The §14.15 retired-key vocabulary: the predecessor's implicit-solver
/// options, warned and ignored so files of any vintage open, plus the
/// two keys its retired flux machinery left behind.
const RETIRED_KEYS: &[&str] = &[
    "MIN_TIMESTEP",
    "REL_TOLERANCE",
    "ABS_TOLERANCE",
    "MAX_CVODE_STEPS",
    "MAX_KRYLOV_DIM",
    "LINEAR_SOLVER",
    "PRECONDITIONER",
    "JACOBIAN",
    "ATOL_AREA_REF",
    "COUPLING_INTERVAL",
    "COUPLING_WINDOW",
    "ACTIVE_SET",
    "ACTIVE_SET_HALO",
    "MOMENTUM",
    "LIMITER_EPSILON",
    "FLUX_DH_EPS",
];

/// Scan raw model text for the §14.15 mesh-units header. The header can
/// only assert SI — a US declaration is ignored, as the predecessor's is
/// — and any `;; UNITS:` line in the text counts, the last one winning
/// by virtue of overwriting nothing (asserting SI twice is asserting SI).
pub(crate) fn units_header_si(text: &str) -> bool {
    let mut si = false;
    for line in text.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix(";;") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(value) = rest
            .strip_prefix("UNITS:")
            .or_else(|| rest.strip_prefix("units:"))
            .or_else(|| rest.strip_prefix("Units:"))
        else {
            continue;
        };
        let v = value.trim().to_ascii_uppercase();
        if v.starts_with("SI") || v == "M" || v.starts_with("METRE") || v.starts_with("METER") {
            si = true;
        }
    }
    si
}

/// §14.15's external mesh file: re-parse the mesh from the model's own
/// 2D sections continued by the external file's, under the combined SI
/// header. The model text's non-2D lines are blanked rather than
/// removed, so inline diagnostics keep their line numbers; the inline
/// 2D sections were already parsed once by the ordinary pass, so their
/// warnings may repeat here — a cosmetic cost the combined numbering is
/// worth.
pub(crate) fn reparse_with_external(
    input: &str,
    external: &str,
    options: &super::options::AnalysisOptions,
    diags: &mut Vec<Diagnostic>,
) -> Option<OverlandMesh> {
    let inline_2d = two_d_lines_only(input);
    let s1 = super::survey::survey(&inline_2d);
    let s2 = super::survey::survey(external);
    let mut sections: Vec<(Section, Vec<TokenLine<'_>>)> = Vec::new();
    for (sec, lines) in s1.sections.iter().chain(s2.sections.iter()) {
        if is_two_d(*sec) && *sec != Section::TwoDMeshFile {
            sections.push((*sec, lines.clone()));
        }
    }
    let units_si = units_header_si(input) || units_header_si(external);
    let cv = super::objects::UnitConverter::new(options.flow_units, options.link_offsets);
    let mut mesh = parse_overland(
        &sections,
        units_si,
        cv.len,
        cv.flow,
        cv.suction,
        cv.conductivity,
        diags,
    )?;
    // The declaration survives for writers and refusals even though the
    // combined parse no longer sees the section.
    mesh.mesh_file = two_d_mesh_file_name(input);
    Some(mesh)
}

fn is_two_d(sec: Section) -> bool {
    matches!(
        sec,
        Section::TwoDOptions
            | Section::TwoDVertices
            | Section::TwoDTriangles
            | Section::TwoDInitialVelocity
            | Section::TwoDVertexNodeMap
            | Section::TwoDTriangleNodeMap
            | Section::TwoDBoundaryConditions
            | Section::TwoDEdgeConveyance
            | Section::TwoDMeshFile
    )
}

/// The model text with every line outside a 2D section blanked, so a
/// re-survey sees only the mesh and every diagnostic keeps its line.
fn two_d_lines_only(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut keep = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            keep = t.to_ascii_uppercase().starts_with("[2D_");
        }
        if keep {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn two_d_mesh_file_name(text: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t.to_ascii_uppercase().starts_with("[2D_MESH_FILE");
            continue;
        }
        if in_section {
            let mut tokens = t.split_whitespace();
            if tokens
                .next()
                .is_some_and(|w| w.eq_ignore_ascii_case("FILE"))
            {
                if let Some(name) = tokens.next() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// Parse every §14.15 section into the §15.2 mesh model. `None` when the
/// model carries no overland input at all. `len` is the display-length
/// factor (m per file length unit) and `flow` the display-flow factor
/// (m³/s per file flow unit); both are bypassed for lengths when the SI
/// header asserted the mesh is already metric.
pub(crate) fn parse_overland(
    sections: &[(Section, Vec<TokenLine<'_>>)],
    units_si: bool,
    len: f64,
    flow: f64,
    suction: f64,
    conductivity: f64,
    diags: &mut Vec<Diagnostic>,
) -> Option<OverlandMesh> {
    let present = sections.iter().any(|(sec, _)| {
        matches!(
            sec,
            Section::TwoDOptions
                | Section::TwoDVertices
                | Section::TwoDTriangles
                | Section::TwoDInfiltration
                | Section::TwoDInitialVelocity
                | Section::TwoDVertexNodeMap
                | Section::TwoDTriangleNodeMap
                | Section::TwoDBoundaryConditions
                | Section::TwoDEdgeConveyance
                | Section::TwoDMeshFile
        )
    });
    if !present {
        return None;
    }

    let vlen = if units_si { 1.0 } else { len };
    let mut mesh = OverlandMesh {
        units_si,
        ..Default::default()
    };

    // Geometry first: the addressed sections resolve indices and tags
    // against it, whatever order the file wrote its sections in.
    for (sec, lines) in sections {
        match sec {
            Section::TwoDOptions => parse_options(lines, &mut mesh.options, diags),
            Section::TwoDVertices => parse_vertices(lines, vlen, &mut mesh.verts, diags),
            Section::TwoDTriangles => parse_cells(lines, vlen, &mut mesh.cells, diags),
            Section::TwoDMeshFile => parse_mesh_file(lines, &mut mesh.mesh_file, diags),
            _ => {}
        }
    }
    for (sec, lines) in sections {
        match sec {
            Section::TwoDInitialVelocity => {
                parse_init_velocity(lines, &mut mesh.init_velocity, diags);
            }
            Section::TwoDVertexNodeMap => {
                let cd = mesh.options.coupling_cd;
                let rows = parse_couplings(lines, cd, vlen, diags);
                mesh.vertex_couplings.extend(rows);
            }
            Section::TwoDTriangleNodeMap => {
                let cd = mesh.options.coupling_cd;
                let rows = parse_couplings(lines, cd, vlen, diags);
                mesh.cell_couplings.extend(rows);
            }
            Section::TwoDBoundaryConditions => {
                parse_boundaries(lines, vlen, flow, &mut mesh.boundaries, diags);
            }
            Section::TwoDEdgeConveyance => {
                parse_conveyance(lines, &mut mesh.conveyance, diags);
            }
            Section::TwoDInfiltration => {
                // §14.15: the SI header governs this section like the
                // geometry — millimetres and millimetres per hour.
                let (suc, cond) = if units_si {
                    (1e-3, 1e-3 / 3600.0)
                } else {
                    (suction, conductivity)
                };
                parse_infiltration(lines, suc, cond, &mut mesh.infiltration, diags);
            }
            _ => {}
        }
    }
    Some(mesh)
}

fn parse_vertices(
    lines: &[TokenLine<'_>],
    vlen: f64,
    out: &mut Vec<MeshVertex>,
    diags: &mut Vec<Diagnostic>,
) {
    for line in lines {
        let t = &line.tokens;
        if t.len() < 3 {
            diags.push(err(line.line, DiagnosticKind::MissingItems));
            continue;
        }
        let (Ok(x), Ok(y), Ok(z)) = (t[0].finite_f64(), t[1].finite_f64(), t[2].finite_f64())
        else {
            diags.push(bad(line.line, t[0]));
            continue;
        };
        out.push(MeshVertex {
            x: x * vlen,
            y: y * vlen,
            z: z * vlen,
            tag: t.get(3).map(|s| s.to_string()),
        });
    }
}

fn parse_cells(
    lines: &[TokenLine<'_>],
    vlen: f64,
    out: &mut Vec<MeshCell>,
    diags: &mut Vec<Diagnostic>,
) {
    for line in lines {
        let t = &line.tokens;
        if t.len() < 4 {
            diags.push(err(line.line, DiagnosticKind::MissingItems));
            continue;
        }
        let (Ok(v0), Ok(v1), Ok(v2)) = (
            t[0].parse::<u32>(),
            t[1].parse::<u32>(),
            t[2].parse::<u32>(),
        ) else {
            diags.push(bad(line.line, t[0]));
            continue;
        };
        let Ok(n) = t[3].finite_f64() else {
            diags.push(bad(line.line, t[3]));
            continue;
        };
        // §14.15: the fifth column is a depth when numeric, a tag
        // otherwise; the sixth is a tag when the depth is present.
        let (h0, tag) = match t.get(4) {
            None => (0.0, None),
            Some(fifth) => match fifth.finite_f64() {
                Ok(d) => (d * vlen, t.get(5).map(|s| s.to_string())),
                Err(()) => (0.0, Some(fifth.to_string())),
            },
        };
        if h0 < 0.0 {
            diags.push(bad(line.line, t[4]));
            continue;
        }
        out.push(MeshCell {
            v: [v0, v1, v2],
            n,
            h0,
            tag,
        });
    }
}

fn parse_init_velocity(
    lines: &[TokenLine<'_>],
    out: &mut Vec<InitVelocityRow>,
    diags: &mut Vec<Diagnostic>,
) {
    for line in lines {
        let t = &line.tokens;
        if t.len() < 3 {
            diags.push(err(line.line, DiagnosticKind::MissingItems));
            continue;
        }
        // Range is a whole-mesh question — an external file may still
        // add cells — so it belongs to Topology::build, not here.
        let Ok(cell) = t[0].parse::<u32>() else {
            diags.push(bad(line.line, t[0]));
            continue;
        };
        let (Ok(u), Ok(v)) = (t[1].finite_f64(), t[2].finite_f64()) else {
            diags.push(bad(line.line, t[1]));
            continue;
        };
        out.push(InitVelocityRow { cell, u, v });
    }
}

fn parse_couplings(
    lines: &[TokenLine<'_>],
    default_cd: f64,
    vlen: f64,
    diags: &mut Vec<Diagnostic>,
) -> Vec<CouplingRow> {
    let mut rows = Vec::new();
    for line in lines {
        let t = &line.tokens;
        if t.len() < 2 {
            diags.push(err(line.line, DiagnosticKind::MissingItems));
            continue;
        }
        let cd = match t.get(2) {
            None => default_cd,
            Some(tok) => match tok.finite_f64() {
                Ok(v) => v,
                Err(()) => {
                    diags.push(bad(line.line, tok));
                    continue;
                }
            },
        };
        let (area, area_authored) = match t.get(3) {
            None => (1.0, false),
            Some(tok) => match tok.finite_f64() {
                Ok(v) => (v * vlen * vlen, true),
                Err(()) => {
                    diags.push(bad(line.line, tok));
                    continue;
                }
            },
        };
        rows.push(CouplingRow {
            address: t[0].to_string(),
            node: t[1].to_string(),
            cd,
            area,
            area_authored,
        });
    }
    rows
}

fn parse_boundaries(
    lines: &[TokenLine<'_>],
    vlen: f64,
    flow: f64,
    out: &mut Vec<BoundaryRow>,
    diags: &mut Vec<Diagnostic>,
) {
    const TYPES: &[&str] = &[
        "WALL",
        "NORMAL_FLOW",
        "SPECIFIED_STAGE",
        "TS_STAGE",
        "SPECIFIED_FLOW",
        "TS_FLOW",
        "RATING_CURVE",
    ];
    for line in lines {
        let t = &line.tokens;
        if t.len() < 3 {
            diags.push(err(line.line, DiagnosticKind::MissingItems));
            continue;
        }
        let (Ok(cell), Ok(edge)) = (t[0].parse::<u32>(), t[1].parse::<u8>()) else {
            diags.push(bad(line.line, t[0]));
            continue;
        };
        if edge > 2 {
            diags.push(bad(line.line, t[1]));
            continue;
        }
        let Some(kind) = match_keyword(TYPES, t[2]) else {
            diags.push(bad(line.line, t[2]));
            continue;
        };
        let p1 = t.get(3).copied().filter(|p| *p != "*");
        let series_or_value = |scale: f64| -> Option<SeriesOrValue> {
            let p = p1?;
            Some(match p.finite_f64() {
                Ok(v) => SeriesOrValue::Value(v * scale),
                Err(()) => SeriesOrValue::Series(p.to_string()),
            })
        };
        let condition = match kind {
            0 => BoundaryCondition::Wall,
            1 => {
                let Some(Ok(slope)) = p1.map(|p| p.finite_f64()) else {
                    diags.push(err(line.line, DiagnosticKind::MissingItems));
                    continue;
                };
                BoundaryCondition::NormalFlow { slope }
            }
            // SPECIFIED_STAGE and TS_STAGE are one condition: the
            // parameter's spelling decides constant against series.
            2 | 3 => match series_or_value(vlen) {
                Some(v) => BoundaryCondition::Stage(v),
                None => {
                    diags.push(err(line.line, DiagnosticKind::MissingItems));
                    continue;
                }
            },
            // Likewise SPECIFIED_FLOW and TS_FLOW. Per-metre discharge
            // converts by the flow factor alone: the metre is a metre in
            // every unit system (§14.15).
            4 | 5 => match series_or_value(flow) {
                Some(v) => BoundaryCondition::Flow(v),
                None => {
                    diags.push(err(line.line, DiagnosticKind::MissingItems));
                    continue;
                }
            },
            _ => {
                let Some(curve) = p1 else {
                    diags.push(err(line.line, DiagnosticKind::MissingItems));
                    continue;
                };
                BoundaryCondition::RatingCurve {
                    curve: curve.to_string(),
                }
            }
        };
        // Token 4 is the reserved PARAM_2 placeholder; the group can only
        // follow it (§14.15).
        let group = t.get(5).copied().filter(|g| *g != "*").map(str::to_string);
        out.push(BoundaryRow {
            cell,
            edge,
            condition,
            group,
        });
    }
}

fn parse_conveyance(
    lines: &[TokenLine<'_>],
    out: &mut Vec<ConveyanceRow>,
    diags: &mut Vec<Diagnostic>,
) {
    for line in lines {
        let t = &line.tokens;
        if t.len() < 3 {
            diags.push(err(line.line, DiagnosticKind::MissingItems));
            continue;
        }
        let (Ok(from), Ok(to)) = (t[0].parse::<u32>(), t[1].parse::<u32>()) else {
            diags.push(bad(line.line, t[0]));
            continue;
        };
        let Ok(factor) = t[2].finite_f64() else {
            diags.push(bad(line.line, t[2]));
            continue;
        };
        out.push(ConveyanceRow { from, to, factor });
    }
}

/// §14.15 `[2D_INFILTRATION]`: `INDEX_OR_TAG IL CL`, losses in the
/// model's rain conventions (mm and mm/h under SI, inches and in/h
/// under US), converted here to metres and metres per second. A later
/// row for the same cell replaces the earlier at resolution (§15.7).
fn parse_infiltration(
    lines: &[TokenLine<'_>],
    suction: f64,
    conductivity: f64,
    out: &mut Vec<InfiltrationRow>,
    diags: &mut Vec<Diagnostic>,
) {
    for line in lines {
        let t = &line.tokens;
        if t.len() < 3 {
            diags.push(err(line.line, DiagnosticKind::MissingItems));
            continue;
        }
        let Ok(il) = t[1].finite_f64() else {
            diags.push(bad(line.line, t[1]));
            continue;
        };
        let Ok(cl) = t[2].finite_f64() else {
            diags.push(bad(line.line, t[2]));
            continue;
        };
        out.push(InfiltrationRow {
            address: t[0].to_string(),
            il: il * suction,
            cl: cl * conductivity,
        });
    }
}

fn parse_mesh_file(lines: &[TokenLine<'_>], out: &mut Option<String>, diags: &mut Vec<Diagnostic>) {
    for line in lines {
        let t = &line.tokens;
        if t.len() < 2 || !t[0].eq_ignore_ascii_case("FILE") {
            diags.push(err(line.line, DiagnosticKind::MissingItems));
            continue;
        }
        // §14.15: only the first FILE line is honoured.
        if out.is_none() {
            *out = Some(t[1].to_string());
        }
    }
}

fn parse_options(lines: &[TokenLine<'_>], opts: &mut OverlandOptions, diags: &mut Vec<Diagnostic>) {
    for line in lines {
        let t = &line.tokens;
        if t.len() < 2 {
            diags.push(err(line.line, DiagnosticKind::MissingItems));
            continue;
        }
        let key = t[0].to_ascii_uppercase();
        let value = t[1];
        if RETIRED_KEYS.contains(&key.as_str()) {
            diags.push(err(
                line.line,
                DiagnosticKind::RetiredOverlandOption { key: key.clone() },
            ));
            continue;
        }
        let mut num = |lo: f64, hi: f64| -> Option<f64> {
            match value.finite_f64() {
                Ok(v) if (lo..=hi).contains(&v) => Some(v),
                _ => {
                    diags.push(bad(line.line, value));
                    None
                }
            }
        };
        let yes = |v: &str| matches!(v.to_ascii_uppercase().as_str(), "YES" | "ON" | "TRUE" | "1");
        match key.as_str() {
            "INTEGRATOR" => {
                if !value.eq_ignore_ascii_case("EXPLICIT") {
                    diags.push(err(
                        line.line,
                        DiagnosticKind::RetiredOverlandOption { key },
                    ));
                }
            }
            "CFL_NUMBER" => {
                if let Some(v) = num(f64::MIN_POSITIVE, 1.0) {
                    opts.cfl_number = v;
                }
            }
            "MAX_TIMESTEP" => {
                if let Some(v) = num(f64::MIN_POSITIVE, f64::MAX) {
                    opts.max_timestep = v;
                }
            }
            "THETA" => {
                if let Some(v) = num(f64::MIN_POSITIVE, 1.0) {
                    opts.theta = v;
                }
            }
            "FROUDE_MAX" => {
                if let Some(v) = num(f64::MIN_POSITIVE, f64::MAX) {
                    opts.froude_max = v;
                }
            }
            "LTS_TIERS" => match value.parse::<u32>() {
                Ok(v @ 1..=8) => opts.lts_tiers = v,
                _ => diags.push(bad(line.line, value)),
            },
            "H_MOVE" => {
                if let Some(v) = num(0.0, f64::MAX) {
                    opts.h_move = v;
                }
            }
            "DRY_DEPTH" => {
                if let Some(v) = num(f64::MIN_POSITIVE, f64::MAX) {
                    opts.dry_depth = v;
                }
            }
            "CELL_CLOSURE" => match value.to_ascii_uppercase().as_str() {
                "FLAT" => opts.cell_closure = CellClosure::Flat,
                "VFR" => opts.cell_closure = CellClosure::Vfr,
                _ => diags.push(bad(line.line, value)),
            },
            "FACE_RECONSTRUCTION" => match value.to_ascii_uppercase().as_str() {
                "MEAN" => opts.face_reconstruction = FaceReconstruction::Mean,
                "VFR_FACE" => opts.face_reconstruction = FaceReconstruction::VfrFace,
                _ => diags.push(bad(line.line, value)),
            },
            "VFR_MIN_WET_FRAC" => {
                if let Some(v) = num(f64::MIN_POSITIVE, 0.5) {
                    opts.vfr_min_wet_frac = v;
                }
            }
            "ADVECTION" => opts.advection = yes(value),
            "RAINFALL_MODE" => match value.to_ascii_uppercase().as_str() {
                "NATURAL_NEIGHBOUR" | "NATURAL_NEIGHBOR" => {
                    opts.rainfall_mode = RainfallMode::NaturalNeighbour;
                }
                "SYSTEM" => opts.rainfall_mode = RainfallMode::System,
                "NONE" => opts.rainfall_mode = RainfallMode::None,
                _ => diags.push(bad(line.line, value)),
            },
            "COUPLING_AREA" => match value.to_ascii_uppercase().as_str() {
                "AUTO" => opts.coupling_area_auto = true,
                "DEFAULT" => opts.coupling_area_auto = false,
                _ => diags.push(bad(line.line, value)),
            },
            "COUPLING_CD" => {
                if let Some(v) = num(f64::MIN_POSITIVE, f64::MAX) {
                    opts.coupling_cd = v;
                }
            }
            "COUPLING_SYNC" => {
                if let Some(v) = num(0.0, f64::MAX) {
                    opts.coupling_sync = v;
                }
            }
            "REPORT_2D" => opts.report_2d = yes(value),
            "OUTPUT_FILE" => opts.output_file = Some(value.to_string()),
            // §14.15: an unknown key warns and is ignored — the
            // predecessor's vocabulary churns, and a file is never
            // refused over an option.
            _ => diags.push(err(
                line.line,
                DiagnosticKind::UnknownOverlandOption { key },
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::objects::parse_network;

    fn model(extra: &str) -> String {
        format!(
            "[OPTIONS]\nFLOW_UNITS CMS\n\n{extra}\n[JUNCTIONS]\nJ1 10 4 0 0 0\n\
             [OUTFALLS]\nO1 9 FREE NO\n[CONDUITS]\nC1 J1 O1 100 0.013 0 0 0 0\n\
             [XSECTIONS]\nC1 CIRCULAR 1 0 0 0 1\n"
        )
    }

    const MESH: &str = "[2D_VERTICES]\n0 0 10.0\n1 0 10.2 VA\n1 1 10.4\n0 1 10.6\n\
                        [2D_TRIANGLES]\n0 1 2 0.02\n0 2 3 0.03 0.05 TB\n";

    /// §14.15 `[2D_INFILTRATION]`: losses author in the model's rain
    /// conventions and convert to SI — millimetres under CMS, inches
    /// under CFS — and export writes them back in millimetres.
    #[test]
    fn infiltration_rows_convert_per_the_rain_conventions() {
        let extra = format!("{MESH}[2D_INFILTRATION]\n0 20 10\nTB 5 2.5\n");
        let (net, diags) = parse_network(&model(&extra));
        assert!(diags.iter().all(|d| !d.kind.is_error()), "{diags:?}");
        let mesh = net.overland.expect("mesh present");
        assert_eq!(mesh.infiltration.len(), 2);
        // 20 mm and 10 mm/h, SI.
        assert!((mesh.infiltration[0].il - 0.020).abs() < 1e-12);
        assert!((mesh.infiltration[0].cl - 0.010 / 3600.0).abs() < 1e-15);
        assert_eq!(mesh.infiltration[1].address, "TB");

        // The same rows under US units: inches and inches per hour.
        let us = model(&extra).replace("FLOW_UNITS CMS", "FLOW_UNITS CFS");
        let (net, diags) = parse_network(&us);
        assert!(diags.iter().all(|d| !d.kind.is_error()), "{diags:?}");
        let mesh = net.overland.clone().expect("mesh present");
        assert!((mesh.infiltration[0].il - 20.0 * 0.0254).abs() < 1e-12);
        assert!((mesh.infiltration[0].cl - 10.0 * 0.0254 / 3600.0).abs() < 1e-15);

        // Export writes SI millimetres, and re-imports to the same
        // values under the SI header.
        let out = crate::io::inp_writer::write_inp(&net).expect("export");
        assert!(out.contains("[2D_INFILTRATION]"), "section exported");
        let (net2, diags) = parse_network(&out);
        assert!(diags.iter().all(|d| !d.kind.is_error()), "{diags:?}");
        let mesh2 = net2.overland.expect("mesh present");
        assert!((mesh2.infiltration[0].il - mesh.infiltration[0].il).abs() < 1e-12);
        assert!((mesh2.infiltration[0].cl - mesh.infiltration[0].cl).abs() < 1e-15);

        // A negative loss is a §15.2 refusal with the cell named.
        let bad = format!("{MESH}[2D_INFILTRATION]\n0 -5 0\n");
        let (net, _) = parse_network(&model(&bad));
        let errors = crate::overland::Topology::build(&net.overland.expect("mesh"))
            .expect_err("negative losses refuse");
        assert!(errors
            .iter()
            .any(|e| matches!(e, crate::overland::MeshError::BadInfiltration { .. })));
    }

    #[test]
    fn a_mesh_parses_with_indices_in_file_order() {
        let (net, diags) = parse_network(&model(MESH));
        assert!(diags.iter().all(|d| !d.kind.is_error()), "{diags:?}");
        let mesh = net.overland.expect("mesh present");
        assert_eq!(mesh.verts.len(), 4);
        assert_eq!(mesh.verts[1].tag.as_deref(), Some("VA"));
        assert_eq!(mesh.cells.len(), 2);
        assert_eq!(mesh.cells[1].v, [0, 2, 3]);
        assert_eq!(mesh.cells[1].h0, 0.05);
        assert_eq!(mesh.cells[1].tag.as_deref(), Some("TB"));
    }

    /// §14.15: the fifth triangle column is a depth when numeric and a
    /// tag otherwise.
    #[test]
    fn the_fifth_triangle_column_disambiguates_by_spelling() {
        let extra = "[2D_VERTICES]\n0 0 1\n1 0 1\n0 1 1\n\
                     [2D_TRIANGLES]\n0 1 2 0.02 LEGACY_TAG\n";
        let (net, _) = parse_network(&model(extra));
        let mesh = net.overland.expect("mesh");
        assert_eq!(mesh.cells[0].h0, 0.0);
        assert_eq!(mesh.cells[0].tag.as_deref(), Some("LEGACY_TAG"));
    }

    /// A US-unit model scales lengths and areas; the SI header bypasses
    /// exactly that scaling and cannot be revoked.
    #[test]
    fn display_units_scale_unless_the_header_asserts_si() {
        let us = "[OPTIONS]\nFLOW_UNITS CFS\n\n[2D_VERTICES]\n0 0 100\n3 0 100\n0 3 100\n\
                  [2D_TRIANGLES]\n0 1 2 0.02 0.5\n\
                  [2D_VERTEX_NODE_MAP]\n0 J1 0.6 2.0\n\
                  [JUNCTIONS]\nJ1 10 4 0 0 0\n";
        let (net, _) = parse_network(us);
        let mesh = net.overland.expect("mesh");
        let ft = 0.3048;
        assert!((mesh.verts[1].x - 3.0 * ft).abs() < 1e-12);
        assert!((mesh.cells[0].h0 - 0.5 * ft).abs() < 1e-12);
        assert!((mesh.vertex_couplings[0].area - 2.0 * ft * ft).abs() < 1e-12);

        let si = format!(";; UNITS: SI (m)\n{us}");
        let (net, _) = parse_network(&si);
        let mesh = net.overland.expect("mesh");
        assert_eq!(mesh.verts[1].x, 3.0);
        assert_eq!(mesh.cells[0].h0, 0.5);
        assert_eq!(mesh.vertex_couplings[0].area, 2.0);
        assert!(mesh.units_si);
    }

    /// §14.15 addressing is derivation: rows carry their authored
    /// address, and resolution — numeric index first, tag fallback —
    /// happens against the full mesh.
    #[test]
    fn couplings_carry_addresses_and_resolve_at_build() {
        let extra = format!(
            "{MESH}[2D_VERTEX_NODE_MAP]\nVA J1\n\
             [2D_TRIANGLE_NODE_MAP]\nTB J1 0.7\n1 O1\n"
        );
        let (net, diags) = parse_network(&model(&extra));
        assert!(diags.iter().all(|d| !d.kind.is_error()), "{diags:?}");
        let mesh = net.overland.expect("mesh");
        assert_eq!(mesh.vertex_couplings.len(), 1);
        assert_eq!(mesh.vertex_couplings[0].address, "VA");
        assert_eq!(mesh.resolve_vertex("VA"), Some(1), "tag VA is vertex 1");
        assert_eq!(mesh.vertex_couplings[0].cd, 0.65, "default coefficient");
        assert!(!mesh.vertex_couplings[0].area_authored);
        // Triangle rows accumulate; tag TB and the numeric row address
        // the same cell — both rows stand.
        assert_eq!(mesh.cell_couplings.len(), 2);
        assert_eq!(mesh.resolve_cell("TB"), Some(1));
        assert_eq!(mesh.cell_couplings[0].cd, 0.7);
        assert_eq!(mesh.resolve_cell(&mesh.cell_couplings[1].address), Some(1));
        // The full mesh validates: every address is known.
        assert!(crate::overland::Topology::build(&mesh).is_ok());
        // An address matching nothing is a named build defect.
        let mut broken = mesh.clone();
        broken.vertex_couplings[0].address = "NOWHERE".into();
        let errors = crate::overland::Topology::build(&broken).expect_err("bad address");
        assert!(errors
            .iter()
            .any(|e| matches!(e, crate::overland::MeshError::UnknownAddress { .. })));
    }

    #[test]
    fn boundary_rows_carry_their_five_conditions() {
        let extra = format!(
            "{MESH}[2D_BOUNDARY_CONDITIONS]\n\
             0 0 WALL\n\
             0 2 NORMAL_FLOW 0.01\n\
             1 0 SPECIFIED_STAGE 10.5\n\
             1 1 TS_STAGE TIDE * G1\n\
             0 1 SPECIFIED_FLOW 0.2\n\
             1 2 RATING_CURVE RC1\n"
        );
        let (net, diags) = parse_network(&model(&extra));
        assert!(diags.iter().all(|d| !d.kind.is_error()), "{diags:?}");
        let b = &net.overland.expect("mesh").boundaries;
        assert_eq!(b.len(), 6);
        assert_eq!(b[0].condition, BoundaryCondition::Wall);
        assert_eq!(
            b[1].condition,
            BoundaryCondition::NormalFlow { slope: 0.01 }
        );
        assert_eq!(
            b[2].condition,
            BoundaryCondition::Stage(SeriesOrValue::Value(10.5))
        );
        assert_eq!(
            b[3].condition,
            BoundaryCondition::Stage(SeriesOrValue::Series("TIDE".into()))
        );
        assert_eq!(b[3].group.as_deref(), Some("G1"));
        assert_eq!(
            b[4].condition,
            BoundaryCondition::Flow(SeriesOrValue::Value(0.2))
        );
        assert_eq!(
            b[5].condition,
            BoundaryCondition::RatingCurve {
                curve: "RC1".into()
            }
        );
    }

    /// Options apply; retired and unknown keys warn without refusing.
    #[test]
    fn options_apply_and_retired_keys_only_warn() {
        let extra = format!(
            "{MESH}[2D_OPTIONS]\n\
             CFL_NUMBER 0.5\nTHETA 1.0\nCELL_CLOSURE VFR\n\
             RAINFALL_MODE NONE\nCOUPLING_CD 0.8\nLTS_TIERS 1\n\
             MIN_TIMESTEP 0.01\nSOME_FUTURE_KEY 7\n"
        );
        let (net, diags) = parse_network(&model(&extra));
        let mesh = net.overland.expect("mesh");
        assert_eq!(mesh.options.cfl_number, 0.5);
        assert_eq!(mesh.options.theta, 1.0);
        assert_eq!(mesh.options.cell_closure, CellClosure::Vfr);
        assert_eq!(mesh.options.rainfall_mode, RainfallMode::None);
        assert_eq!(mesh.options.coupling_cd, 0.8);
        assert_eq!(mesh.options.lts_tiers, 1);
        let retired = diags
            .iter()
            .filter(|d| matches!(&d.kind, DiagnosticKind::RetiredOverlandOption { .. }))
            .count();
        let unknown = diags
            .iter()
            .filter(|d| matches!(&d.kind, DiagnosticKind::UnknownOverlandOption { .. }))
            .count();
        assert_eq!(retired, 1);
        assert_eq!(unknown, 1);
        assert!(diags.iter().all(|d| !d.kind.is_error()));
    }

    /// `COUPLING_CD` is honoured as the default for rows without their
    /// own coefficient — the correspondence deviation §14.15 records.
    #[test]
    fn the_global_coupling_cd_serves_rows_without_their_own() {
        let extra = format!("{MESH}[2D_OPTIONS]\nCOUPLING_CD 0.8\n[2D_VERTEX_NODE_MAP]\n0 J1\n");
        let (net, _) = parse_network(&model(&extra));
        let mesh = net.overland.expect("mesh");
        assert_eq!(mesh.vertex_couplings[0].cd, 0.8);
    }

    #[test]
    fn only_the_first_mesh_file_line_is_honoured() {
        let extra = "[2D_MESH_FILE]\nFILE terrain.2dm\nFILE other.2dm\n";
        let (net, _) = parse_network(&model(extra));
        let mesh = net.overland.expect("mesh");
        assert_eq!(mesh.mesh_file.as_deref(), Some("terrain.2dm"));
    }

    /// §14.15 export: a written mesh re-imports as the same mesh — the
    /// SI header makes the round trip unit-exact, and the always-written
    /// depth keeps a tagged cell's fifth column unambiguous.
    #[test]
    fn a_written_mesh_reimports_identically() {
        let extra = format!(
            "{MESH}[2D_OPTIONS]
CFL_NUMBER 0.5
CELL_CLOSURE VFR
COUPLING_CD 0.8
             [2D_INITIAL_VELOCITY]
0 0.1 -0.2
             [2D_VERTEX_NODE_MAP]
0 J1
             [2D_TRIANGLE_NODE_MAP]
TB O1 0.7 2.5
             [2D_BOUNDARY_CONDITIONS]
0 0 WALL
1 1 TS_STAGE TIDE * G1
             [2D_EDGE_CONVEYANCE]
0 2 0.3
"
        );
        let (net, _) = parse_network(&model(&extra));
        let text = crate::io::inp_writer::write_inp(&net).expect("writable");
        let (net2, diags) = parse_network(&text);
        assert!(diags.iter().all(|d| !d.kind.is_error()), "{diags:?}");
        let (a, b) = (net.overland.expect("mesh"), net2.overland.expect("mesh"));
        // The reimport asserts SI, whatever the original said.
        assert!(b.units_si);
        assert_eq!(a.verts, b.verts);
        assert_eq!(a.cells, b.cells);
        assert_eq!(a.init_velocity, b.init_velocity);
        assert_eq!(a.vertex_couplings, b.vertex_couplings);
        assert_eq!(a.cell_couplings, b.cell_couplings);
        assert_eq!(a.boundaries, b.boundaries);
        assert_eq!(a.conveyance, b.conveyance);
        assert_eq!(a.options, b.options);
    }

    /// §15.7: `[SYMBOLS]` feeds gauge positions exactly when a mesh is
    /// present, in the mesh's own unit handling.
    #[test]
    fn symbols_position_gauges_only_when_a_mesh_is_present() {
        let gage = "[RAINGAGES]\nG1 INTENSITY 1:00 1.0 TIMESERIES TS1\n\
                    [TIMESERIES]\nTS1 0:00 0\n[SYMBOLS]\nG1 300 400\n";
        let with_mesh = format!("{MESH}{gage}");
        let (net, _) = parse_network(&model(&with_mesh));
        assert_eq!(net.gages[0].position, Some((300.0, 400.0)));

        let (net, _) = parse_network(&model(gage));
        assert_eq!(net.gages[0].position, None, "no mesh, no exception");
    }

    #[test]
    fn a_model_without_mesh_sections_carries_no_mesh() {
        let (net, _) = parse_network(&model(""));
        assert!(net.overland.is_none());
    }
}
