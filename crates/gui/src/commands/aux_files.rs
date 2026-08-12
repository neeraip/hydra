//! Auxiliary files a drainage model references by name — rain records,
//! climate files, interface files — shared between every import surface.
//!
//! The engine performs no file I/O (§12.1): whoever imports a model owns
//! finding these files and carrying their bytes. The archive import
//! matches them among the archive's entries; the single-file import looks
//! beside the picked model and lets the user locate what is not there;
//! `create_project` writes whatever was gathered into the bundle's
//! `base/aux/`, where the run queue reads it back.

/// One referenced auxiliary file: the name as the model wrote it, a human
/// label naming its role, whether the import has its bytes in hand, and
/// whether a run can actually consume them. An unsupported reference is
/// named — the user must know the model asks for it — but never promised:
/// carrying bytes nothing reads is not an import.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SidecarRef {
    pub file: String,
    pub label: String,
    pub carried: bool,
    pub supported: bool,
}

/// One reference as the survey finds it: name, role label, and whether
/// the run path can consume supplied bytes.
pub(crate) struct SidecarSource {
    pub file: String,
    pub label: String,
    pub supported: bool,
}

/// The trailing file name of a reference as a model wrote it — models
/// carry paths from the machine they were authored on, and every match
/// here is by this tail.
pub(crate) fn aux_basename(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// Where a reference's bytes live inside a bundle's `aux/`, or `None` when
/// the name cannot name a file there.
///
/// Trimming the directories is usually enough, because models carry paths
/// from the machine they were authored on and what is left is a plain file
/// name. What it does not leave is a guarantee, and a model can come from
/// anywhere — the archive import exists so a colleague can send you one.
/// `..` survives the trim whole, and on Windows so does `C:rain.dat`,
/// which carries a drive prefix: `Path::push` replaces the entire path
/// when handed one of those, so the join lands on another drive rather
/// than inside the bundle.
///
/// So the name is a plain file name or it is nothing. Refusing beats
/// repairing — a repaired name is still the one the model expects to be
/// honoured, and quietly reading or writing somewhere else answers a
/// question nobody asked. `:` is refused on every platform, not only the
/// one where it means a drive: a bundle is portable, and a name that
/// cannot work on Windows should not be carried into a bundle on Linux
/// and fail there later.
pub(crate) fn aux_file_path(dir: &std::path::Path, referenced: &str) -> Option<std::path::PathBuf> {
    let base = aux_basename(referenced);
    if base.is_empty() || base == "." || base == ".." || base.contains([':', '/', '\\']) {
        return None;
    }
    Some(dir.join(base))
}

/// External files a drainage model references, deduplicated in reference
/// order. `supported` says whether the run path consumes supplied bytes:
/// rain records, climate records, hotstart state, and routing inflows do;
/// the rainfall/runoff/RDII interface formats and external `[TIMESERIES]`
/// files are declared but not yet served (the engine refuses or reads
/// empty), and the import must say so rather than promise them.
pub(crate) fn uds_sidecar_refs(network: &hydra::uds::model::Network) -> Vec<SidecarSource> {
    use hydra::uds::model::{FileMode, GageSource, TemperatureSource, TimeSeriesSource};
    let mut out: Vec<SidecarSource> = Vec::new();
    let mut push = |file: &str, role: &str, supported: bool| {
        if !out.iter().any(|s| s.file == file) {
            out.push(SidecarSource {
                file: file.to_string(),
                label: format!("{role} \"{file}\""),
                supported,
            });
        }
    };
    for gage in &network.gages {
        if let GageSource::File { file, .. } = &gage.source {
            push(file, "rain file", true);
        }
    }
    if let Some(TemperatureSource::File { name, .. }) = &network.climate.temperature {
        push(name, "climate file", true);
    }
    let iface = &network.interface_files;
    if let Some(name) = &iface.hotstart_use {
        push(name, "hotstart file", true);
    }
    if let Some(name) = &iface.inflows {
        push(name, "routing inflows file", true);
    }
    for (slot, role) in [
        (&iface.rainfall, "rainfall interface file"),
        (&iface.runoff, "runoff interface file"),
        (&iface.rdii, "RDII interface file"),
    ] {
        if let Some((FileMode::Use, name)) = slot {
            push(name, role, false);
        }
    }
    for series in &network.timeseries {
        if let TimeSeriesSource::External { file } = &series.source {
            push(file, "data series file", false);
        }
    }
    out
}

/// The references as [`SidecarRef`]s, `carried` answered by whether the
/// gathered bytes hold a matching trailing name.
pub(crate) fn sidecar_status(
    network: &hydra::uds::model::Network,
    gathered_names: &[String],
) -> Vec<SidecarRef> {
    uds_sidecar_refs(network)
        .into_iter()
        .map(|source| {
            let base = aux_basename(&source.file).to_ascii_lowercase();
            SidecarRef {
                carried: gathered_names
                    .iter()
                    .any(|n| n.to_ascii_lowercase() == base),
                file: source.file,
                label: source.label,
                supported: source.supported,
            }
        })
        .collect()
}

/// Write gathered auxiliary files into a project bundle's `base/aux/`.
pub(crate) fn write_aux_files(
    app_data: &std::path::Path,
    project_id: &str,
    files: &[(String, Vec<u8>)],
) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }
    let dir = crate::meta::bundle::aux_dir(app_data, project_id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (name, bytes) in files {
        // A name that cannot sit in `aux/` is skipped rather than failing
        // the write, so the files that can still land. The run then
        // refuses naming what it could not find, which is the report the
        // user can act on.
        let Some(path) = aux_file_path(&dir, name) else {
            tracing::warn!(file = %name, "auxiliary file name is not a plain file name; not stored");
            continue;
        };
        crate::meta::bundle::atomic_write(&path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal drainage model with one file-sourced gage.
    const FILE_GAGE_INP: &str = "\
[OPTIONS]
FLOW_UNITS  CMS

[RAINGAGES]
rg1  VOLUME  0:01  1.0  FILE  \"forcing/rain.dat\"  sta1  MM

[JUNCTIONS]
J1  10  2

[OUTFALLS]
O1  9  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.0  0  0  0
";

    #[test]
    fn attach_covers_a_reference_and_refuses_a_stranger() {
        use super::super::network_dto::NetworkStateInner;
        let (network, _) = hydra::uds::io::objects::parse_network(FILE_GAGE_INP);
        let mut inner = NetworkStateInner::LoadedUds {
            dirty: false,
            raw_text: FILE_GAGE_INP.to_string(),
            network: std::sync::Arc::new(network),
            aux_files: Vec::new(),
            owner_project_id: None,
            owner_scenario_id: None,
        };

        // A file the model never mentions is refused, not silently held.
        let err =
            super::super::projects::attach_aux_bytes(&mut inner, "unrelated.dat", b"x".to_vec())
                .unwrap_err();
        assert!(err.contains("unrelated.dat"), "{err}");

        // The referenced name attaches (matched by trailing name, case
        // apart) and the status flips to carried.
        let status = super::super::projects::attach_aux_bytes(
            &mut inner,
            "RAIN.DAT",
            b"sta1 2020 1 1 0 0 1.0\n".to_vec(),
        )
        .expect("attaches");
        assert_eq!(status.len(), 1);
        assert!(status[0].carried);

        // create_project's read side sees the gathered bytes.
        let NetworkStateInner::LoadedUds { aux_files, .. } = &inner else {
            unreachable!()
        };
        assert_eq!(aux_files.len(), 1);
        // Stored under the name the *model* wrote, not the picked file's:
        // the run path reads `rain.dat`, and on a case-sensitive
        // filesystem `RAIN.DAT` would be a different file.
        assert_eq!(aux_files[0].0, "rain.dat");
    }

    /// A model names its own auxiliary files, and an imported model is
    /// someone else's file. So the names that would leave `aux/` are
    /// refused rather than trimmed into something plausible.
    #[test]
    fn a_reference_that_would_leave_the_aux_directory_is_refused() {
        let dir = std::path::Path::new("/app/projects/p1/base/aux");

        // The ordinary cases still resolve, directories and all.
        for (name, expect) in [
            ("rain.dat", "rain.dat"),
            ("forcing/rain.dat", "rain.dat"),
            (r"C:\models\run\rain.dat", "rain.dat"),
            ("../rain.dat", "rain.dat"),
        ] {
            assert_eq!(
                aux_file_path(dir, name),
                Some(dir.join(expect)),
                "{name} should resolve to {expect}"
            );
        }

        for name in [
            "",
            ".",
            "..",
            "forcing/..",
            // A drive prefix and no separator: `Path::push` replaces the
            // whole path on Windows, so this would land on drive C.
            "C:rain.dat",
            r"sub\..",
        ] {
            assert_eq!(aux_file_path(dir, name), None, "{name:?} should be refused");
        }
    }

    /// The refusal has to hold at the write, not only in the helper.
    #[test]
    fn a_refused_reference_writes_nothing_and_lets_the_rest_through() {
        let app_data = tempfile::tempdir().expect("app data");
        write_aux_files(
            app_data.path(),
            "p1",
            &[
                ("C:escape.dat".into(), b"no".to_vec()),
                ("rain.dat".into(), b"yes".to_vec()),
            ],
        )
        .expect("the good file still lands");

        let aux = app_data.path().join("projects/p1/base/aux");
        assert_eq!(
            std::fs::read(aux.join("rain.dat")).ok(),
            Some(b"yes".into())
        );
        let stored: Vec<String> = std::fs::read_dir(&aux)
            .expect("aux dir")
            .filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned()))
            .collect();
        assert_eq!(stored, ["rain.dat"], "only the plain name should be stored");
    }

    #[test]
    fn aux_files_land_in_the_bundles_aux_dir() {
        let app_data = tempfile::tempdir().expect("app data");
        write_aux_files(
            app_data.path(),
            "p1",
            &[("rain.dat".into(), b"sta1 2020 1 1 0 0 1.0\n".to_vec())],
        )
        .expect("writes");
        let written =
            std::fs::read(app_data.path().join("projects/p1/base/aux/rain.dat")).expect("readable");
        assert_eq!(written, b"sta1 2020 1 1 0 0 1.0\n");
    }
}
