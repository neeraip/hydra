//! The auxiliary files a model names, supplied by the page rather than
//! read from a directory.
//!
//! A uds model can declare a climate record, a hotstart file, or a routing
//! inflows file by name. The CLI resolves each of those against the model
//! file's own directory, which is the convention the engine's predecessor
//! set. A browser has no directory to resolve against, so the analogue is
//! the *other files the user dropped*: whatever else came in alongside the
//! model is what a declared name can refer to.
//!
//! That makes name matching the whole of this module, and it is less
//! obvious than it looks — see [`AuxFiles::get`].
//!
//! (The file is `aux_files.rs` rather than the obvious `aux.rs` because
//! `AUX` is a reserved device name on Windows, where a checkout containing
//! one fails outright.)

/// The files supplied alongside the model, addressable by the names the
/// model declares.
#[derive(Debug, Default, Clone)]
pub struct AuxFiles {
    entries: Vec<(String, Vec<u8>)>,
}

impl AuxFiles {
    /// An empty set — nothing was supplied but the model.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a supplied file under the name it arrived with.
    pub fn insert(&mut self, name: impl Into<String>, bytes: Vec<u8>) {
        self.entries.push((name.into(), bytes));
    }

    /// Whether nothing was supplied.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The bytes for a name the model declared, or `None`.
    ///
    /// Two allowances, both of which a strict comparison would fail on real
    /// models:
    ///
    /// **The declared name may carry a path** (`data\climate.dat`,
    /// `../shared/rain.dat`) because it was written for a filesystem. Only
    /// its last segment can mean anything here, so only that is compared —
    /// and both separators count, since a model authored on Windows travels
    /// to a browser on any platform.
    ///
    /// **Case is ignored.** The declared name and the dropped file's name
    /// come from the same file on a case-insensitive filesystem, where
    /// `CLIMATE.DAT` and `climate.dat` were never two different files. Being
    /// strict here would reject models that work everywhere else, and the
    /// cost of being wrong is bounded: the user chose these files.
    pub fn get(&self, declared: &str) -> Option<&[u8]> {
        let wanted = basename(declared);
        self.entries
            .iter()
            .find(|(name, _)| basename(name).eq_ignore_ascii_case(wanted))
            .map(|(_, bytes)| bytes.as_slice())
    }

    /// The bytes for a declared name, as text.
    pub fn get_text(&self, declared: &str) -> Option<String> {
        self.get(declared)
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }
}

/// The last path segment of `name`, treating `/` and `\` alike.
fn basename(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> AuxFiles {
        let mut aux = AuxFiles::new();
        aux.insert("climate.dat", b"records".to_vec());
        aux
    }

    #[test]
    fn a_plain_name_matches() {
        assert_eq!(files().get("climate.dat"), Some(&b"records"[..]));
    }

    /// The declared name was written for a filesystem the browser cannot
    /// see, so its directory part cannot be part of the comparison.
    #[test]
    fn a_declared_path_matches_on_its_last_segment() {
        let aux = files();
        assert_eq!(aux.get("data/climate.dat"), Some(&b"records"[..]));
        assert_eq!(aux.get("..\\shared\\climate.dat"), Some(&b"records"[..]));
    }

    #[test]
    fn case_does_not_decide_it() {
        assert_eq!(files().get("CLIMATE.DAT"), Some(&b"records"[..]));
    }

    /// A dropped file may itself arrive with a path (a directory drop), so
    /// the allowance has to work from both sides, not just the declared one.
    #[test]
    fn a_supplied_path_matches_a_plain_declaration() {
        let mut aux = AuxFiles::new();
        aux.insert("site/climate.dat", b"records".to_vec());
        assert_eq!(aux.get("climate.dat"), Some(&b"records"[..]));
    }

    #[test]
    fn a_name_nobody_supplied_is_absent() {
        assert_eq!(files().get("rain.dat"), None);
        assert!(AuxFiles::new().is_empty());
    }

    /// Matching on the last segment alone means a name that merely *ends*
    /// with the wanted one must not match — `myclimate.dat` is a different
    /// file, and quietly feeding it to the engine would be worse than
    /// reporting the declared file as missing.
    #[test]
    fn a_longer_name_ending_in_the_wanted_one_does_not_match() {
        let mut aux = AuxFiles::new();
        aux.insert("myclimate.dat", b"wrong".to_vec());
        assert_eq!(aux.get("climate.dat"), None);
    }
}
