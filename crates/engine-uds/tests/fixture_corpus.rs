//! Every fixture in `tests/fixtures/uds/` (workspace root) must parse
//! with no error diagnostics, and every cross-section the current §5
//! build stage supports must construct.
//!
//! Fixtures are deliberately tiny: each isolates one behaviour, distilled
//! from real networks, and is named for the behaviour it pins.

use std::path::PathBuf;

use hydra_engine_uds::hydraulics::section::{
    build_section, build_street_section, build_transect_section, BuildError,
};
use hydra_engine_uds::io::objects::parse_network;
use hydra_engine_uds::model::{XsectReferent, XsectShape};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/uds")
}

#[test]
fn every_fixture_parses_clean() {
    let mut names: Vec<PathBuf> = std::fs::read_dir(fixture_dir())
        .expect("fixtures dir")
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension().is_some_and(|x| x == "inp")).then_some(p)
        })
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no fixtures found");

    for path in names {
        let text = std::fs::read_to_string(&path).expect("read fixture");
        let (net, diags) = parse_network(&text);
        let errors: Vec<_> = diags.iter().filter(|d| d.kind.is_error()).collect();
        assert!(errors.is_empty(), "{} refused: {errors:?}", path.display());

        // Geometry: whatever the §5 build stage supports must construct;
        // staged families (§5.3–§5.6) are tolerated until they land.
        let len = if net.options.flow_units.is_us() {
            0.3048
        } else {
            1.0
        };
        for link in &net.links {
            let Some(xs) = &link.cross_section else {
                continue;
            };
            let built = match (xs.shape, xs.referent) {
                (XsectShape::Irregular, Some(XsectReferent::Transect(t))) => {
                    build_transect_section(&net.transects[t])
                }
                (XsectShape::Street, Some(XsectReferent::Street(st))) => {
                    build_street_section(&net.streets[st])
                }
                _ => {
                    let curve = match xs.referent {
                        Some(XsectReferent::Curve(c)) => Some(net.curves[c].points.as_slice()),
                        _ => None,
                    };
                    build_section(xs.shape, xs.geom_user, len, curve)
                }
            };
            match built {
                Ok(b) => {
                    let y = 0.5 * b.section.y_full();
                    assert!(b.section.area(y).is_finite());
                    assert!(b.section.hyd_radius(y).is_finite());
                }
                Err(BuildError::Unsupported(_)) => {
                    panic!(
                        "{} link {}: nothing should stage now",
                        path.display(),
                        link.id
                    )
                }
                Err(e) => panic!("{} link {}: {e:?}", path.display(), link.id),
            }
        }
    }
}
