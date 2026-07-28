//! Bounded worker pool for `basemap:` tile requests.
//!
//! The webview issues scheme requests on a thread that must never block, so
//! each request is handed off to a worker. Handing it to a *freshly spawned*
//! thread instead is what this module replaces: a single map pan at zoom
//! issues dozens of tile requests, each holding its thread for up to the
//! proxy's 10 s upstream timeout, so a slow provider turned normal panning
//! into hundreds of live OS threads.
//!
//! Sizing follows what is actually useful: tiles come from one or two hosts,
//! and HTTP keep-alive plus per-host connection limits mean extra concurrency
//! buys nothing beyond a handful of in-flight fetches. Requests past the
//! queue bound are refused immediately with `503` rather than queued forever —
//! a tile the user has already panned away from is worth nothing, and
//! maplibre re-requests tiles that fail.

use std::borrow::Cow;
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::OnceLock;

use tauri::http;

/// In-flight upstream fetches. Small on purpose — see the module docs.
const WORKERS: usize = 6;

/// Requests allowed to wait for a worker before new ones are refused.
/// Roughly one screenful of tiles.
const QUEUE_CAPACITY: usize = 64;

type Job = Box<dyn FnOnce() + Send + 'static>;

fn sender() -> &'static SyncSender<Job> {
    static POOL: OnceLock<SyncSender<Job>> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = sync_channel::<Job>(QUEUE_CAPACITY);
        let rx = std::sync::Arc::new(parking_lot::Mutex::new(rx));
        for i in 0..WORKERS {
            let rx = rx.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("basemap-tile-{i}"))
                .spawn(move || loop {
                    // Hold the receiver lock only while dequeuing, never
                    // across the (slow) job itself.
                    let job = {
                        let guard = rx.lock();
                        guard.recv()
                    };
                    match job {
                        Ok(job) => job(),
                        // The sender is 'static, so this only happens at
                        // shutdown: stop the worker.
                        Err(_) => break,
                    }
                });
            if let Err(e) = spawned {
                tracing::warn!(worker = i, error = %e, "could not spawn basemap tile worker");
            }
        }
        tx
    })
}

/// Run `job` on a pool worker. Returns `false` when the queue is saturated,
/// in which case `job` is dropped and never runs.
pub fn try_submit(job: impl FnOnce() + Send + 'static) -> bool {
    match sender().try_send(Box::new(job)) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => false,
        Err(TrySendError::Disconnected(_)) => {
            tracing::warn!("basemap tile pool is shut down; request dropped");
            false
        }
    }
}

/// Response for a request refused because every worker is busy and the queue
/// is full. `503` with `Retry-After: 1` is the honest answer, and maplibre
/// re-requests tiles that fail.
pub fn overloaded_response() -> http::Response<Cow<'static, [u8]>> {
    http::Response::builder()
        .status(503)
        .header("Access-Control-Allow-Origin", "*")
        .header("Retry-After", "1")
        .body(Cow::Borrowed(b"tile proxy busy".as_slice()))
        .unwrap_or_else(|e| {
            tracing::error!("basemap pool: response build failed: {e}");
            http::Response::builder()
                .status(503)
                .body(Cow::Borrowed(b"".as_slice()))
                .expect("static fallback response")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn runs_submitted_jobs_on_a_worker() {
        let (tx, rx) = channel();
        assert!(try_submit(move || {
            let _ = tx.send(std::thread::current().name().map(str::to_string));
        }));
        let name = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("job must run");
        assert!(
            name.as_deref()
                .is_some_and(|n| n.starts_with("basemap-tile-")),
            "job must run on a pool worker, got {name:?}"
        );
    }

    #[test]
    fn overloaded_response_is_503_with_cors() {
        let res = overloaded_response();
        assert_eq!(res.status(), 503);
        assert_eq!(
            res.headers()
                .get("Access-Control-Allow-Origin")
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
    }
}
