//! Reading the host's wall clock.
//!
//! The engine stamps two wall-clock times per run — when the analysis began
//! and when it ended — which the report prints and the binary results
//! prolog records. Nothing in the simulation depends on them; they are
//! provenance, so a reader can tell which run a results file came from.
//!
//! # Why this is not just `SystemTime::now()`
//!
//! On `wasm32-unknown-unknown` there is no clock behind `SystemTime`, and
//! `now()` does not fail — it panics, with "time not implemented on this
//! platform". A panic compiled to wasm becomes an `unreachable` trap, so
//! the first step of the first run in a browser aborted the module rather
//! than returning an error. Everything else in the engine already ran there
//! unchanged; this one call was the whole of it.
//!
//! So the clock is read through here instead. On every native target that
//! is `SystemTime::now()` verbatim. On wasm it goes through `chrono`, which
//! this crate already depends on and which reads JavaScript's `Date` when
//! built with `wasmbind` (see this crate's `Cargo.toml`) — no new
//! dependency, and no `#[cfg]` anywhere but here.
//!
//! The return type is `std::time::SystemTime` on every target on purpose.
//! It appears in `WritableSimulation::analysis_times`, which is public API,
//! and a signature that changed shape per target would make the published
//! surface a moving thing.

use std::time::SystemTime;

/// The current wall-clock time.
// The one place `SystemTime::now()` may be called: this crate's
// `clippy.toml` disallows it everywhere else, so a future call goes through
// here rather than trapping in a browser.
#[allow(clippy::disallowed_methods)]
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn now() -> SystemTime {
    SystemTime::now()
}

/// The current wall-clock time, read from JavaScript's `Date`.
///
/// Falls back to the epoch for a timestamp that cannot be represented —
/// only reachable if the host clock is set before 1970, and a provenance
/// stamp is not worth aborting a run over.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub(crate) fn now() -> SystemTime {
    let millis = chrono::Utc::now().timestamp_millis();
    match u64::try_from(millis) {
        Ok(ms) => SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(ms),
        Err(_) => SystemTime::UNIX_EPOCH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stamps are compared against each other (`begun <= ended`) and
    /// formatted into reports, so the clock has to be real and monotone
    /// enough for both, whichever target this is.
    #[test]
    fn the_clock_reads_a_plausible_present() {
        let t = now();
        let since_epoch = t
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("the wall clock should be at or after the epoch");
        // 2020-01-01, chosen only to catch a clock stuck at zero.
        assert!(
            since_epoch.as_secs() > 1_577_836_800,
            "wall clock reads {since_epoch:?} after the epoch"
        );
    }

    #[test]
    fn two_readings_do_not_go_backwards() {
        let first = now();
        let second = now();
        assert!(second >= first);
    }
}
