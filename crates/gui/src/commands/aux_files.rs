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
/// label naming its role, and whether the import has its bytes in hand.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SidecarRef {
    pub file: String,
    pub label: String,
    pub carried: bool,
}

/// The trailing file name of a reference as a model wrote it — models
/// carry paths from the machine they were authored on, and every match
/// here is by this tail.
pub(crate) fn aux_basename(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// External files a drainage model references, as `(name as written,
/// role label)` pairs, deduplicated in reference order.
pub(crate) fn uds_sidecar_refs(network: &hydra::uds::model::Network) -> Vec<(String, String)> {
    use hydra::uds::model::{FileMode, GageSource, TemperatureSource};
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |file: &str, role: &str| {
        if !out.iter().any(|(f, _)| f == file) {
            out.push((file.to_string(), format!("{role} \"{file}\"")));
        }
    };
    for gage in &network.gages {
        if let GageSource::File { file, .. } = &gage.source {
            push(file, "rain file");
        }
    }
    if let Some(TemperatureSource::File { name, .. }) = &network.climate.temperature {
        push(name, "climate file");
    }
    let iface = &network.interface_files;
    for (slot, role) in [
        (&iface.rainfall, "rainfall interface file"),
        (&iface.runoff, "runoff interface file"),
        (&iface.rdii, "RDII interface file"),
    ] {
        if let Some((FileMode::Use, name)) = slot {
            push(name, role);
        }
    }
    if let Some(name) = &iface.hotstart_use {
        push(name, "hotstart file");
    }
    if let Some(name) = &iface.inflows {
        push(name, "routing inflows file");
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
        .map(|(file, label)| {
            let base = aux_basename(&file).to_ascii_lowercase();
            SidecarRef {
                carried: gathered_names
                    .iter()
                    .any(|n| n.to_ascii_lowercase() == base),
                file,
                label,
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
        crate::meta::bundle::atomic_write(&dir.join(name), bytes).map_err(|e| e.to_string())?;
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
        assert_eq!(aux_files[0].0, "RAIN.DAT");
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
