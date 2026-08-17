//! Hydra in a browser: the simulation engines compiled to WebAssembly.
//!
//! This is a third reference consumer of Hydra's public API, alongside the
//! CLI and the GUI, and it depends on `hydra-sdk` under exactly the
//! contract a third party would. Nothing engine-specific lives here — the
//! only reason the crate exists is that a browser has no filesystem, and
//! the CLI's run path is written around one.
//!
//! # What it is for
//!
//! Running a model without installing anything. Drop an INP file on a page
//! and read the same report `hydra run` would have printed, produced by
//! the same engine calls. The report text and the diagnostic lines are not
//! reimplementations — [`Run::report_text`] is `EngineSession`'s own
//! summary, and the diagnostics carry the CLI's codes.
//!
//! # What it is not
//!
//! Not the GUI ported to the web. There is no editing, no canvas and no
//! persistence: those are the parts of `hydra-gui` that need a host, and
//! they are the reason a browser build of the *application* is a much
//! larger question than a browser build of the *engines*.
//!
//! Not a way to read large results, either. Native builds stream `.out`
//! files from a path (`io::out_reader`) precisely so they never have to be
//! held whole; a browser can only hold them, under a 4 GB address ceiling
//! it cannot raise. Capturing results is therefore opt-in — see
//! [`sink::SharedSink`].
//!
//! # Layout
//!
//! Every decision is plain Rust in [`run`], [`aux_files`], [`diagnostic`]
//! and [`sink`], so `cargo test` covers them on the host with no browser
//! involved. This file is only the translation shell: `wasm_bindgen`
//! bindings that move bytes and JSON across the boundary and hold no
//! judgement of their own.

pub mod aux_files;
pub mod diagnostic;
pub mod examples;
pub mod progress;
pub mod run;
pub mod sink;

use wasm_bindgen::prelude::*;

use crate::aux_files::AuxFiles;
use crate::diagnostic::Failure;
use crate::run::{OpenRequest, Run};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}

/// Make a panic say what happened.
///
/// A Rust panic compiled to wasm becomes an `unreachable` instruction, and
/// the only thing that reaches JavaScript is the word "unreachable" — no
/// message, no location, nothing to act on. The engines are `Result`-based
/// throughout and should not panic, which is exactly why a panic that does
/// escape needs to be legible: it is a bug report, and one that arrives as
/// "unreachable" cannot be filed.
///
/// Runs automatically when the module initialises.
#[wasm_bindgen(start)]
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        console_error(&format!("Hydra panicked: {info}"));
    }));
}

/// Hydra's version, and each subsystem's — the same values `hydra --version`
/// prints.
#[wasm_bindgen(js_name = versionInfo)]
pub fn version_info() -> String {
    let info = serde_json::json!({
        "hydra": hydra::HYDRA_VERSION,
        "simulation": hydra::HYDRA_SIMULATION_VERSION,
        "hydraulics": hydra::HYDRA_HYDRAULICS_VERSION,
        "quality": hydra::HYDRA_QUALITY_VERSION,
        "analysis": hydra::HYDRA_ANALYSIS_VERSION,
    });
    info.to_string()
}

/// Every engine this build provides, as `hydra engines` lists them.
///
/// Includes the planned ones, with `available: false`, because a page that
/// hid them would misrepresent the registry — a planned engine is a
/// reserved key, not an absent one.
#[wasm_bindgen(js_name = engines)]
pub fn engines() -> String {
    let list: Vec<_> = hydra::common::ENGINES
        .iter()
        .map(|e| {
            serde_json::json!({
                "key": e.key,
                "label": e.label,
                "available": e.is_available(),
            })
        })
        .collect();
    serde_json::Value::Array(list).to_string()
}

/// What to open, built up call by call.
///
/// A builder rather than a wide `open(…)` because the auxiliary files are a
/// *list* of name-and-bytes pairs, and the alternatives for passing one
/// across this boundary are all worse: parallel arrays that can fall out of
/// step, or an array of typed arrays, which costs a `js-sys` dependency to
/// unpack. Adding them one at a time costs neither.
#[wasm_bindgen]
pub struct RunOptions {
    model: Vec<u8>,
    model_name: String,
    engine: Option<String>,
    aux: AuxFiles,
    capture_results: bool,
}

#[wasm_bindgen]
impl RunOptions {
    /// The model's bytes and the name it arrived with.
    #[wasm_bindgen(constructor)]
    pub fn new(model: Vec<u8>, model_name: String) -> Self {
        Self {
            model,
            model_name,
            engine: None,
            aux: AuxFiles::new(),
            capture_results: false,
        }
    }

    /// Name the engine explicitly. Left unset, the model is asked — and
    /// there is no default (common spec §2.5.1).
    #[wasm_bindgen(js_name = withEngine)]
    pub fn with_engine(&mut self, key: Option<String>) {
        self.engine = key.filter(|k| !k.is_empty());
    }

    /// Supply a file the model may declare by name (uds climate, hotstart
    /// and routing-inflow files).
    #[wasm_bindgen(js_name = withAuxFile)]
    pub fn with_aux_file(&mut self, name: String, bytes: Vec<u8>) {
        self.aux.insert(name, bytes);
    }

    /// Capture the binary `.out` results in memory — the CLI's
    /// `--results`. Off by default: the whole file has to be held.
    #[wasm_bindgen(js_name = withResults)]
    pub fn with_results(&mut self, on: bool) {
        self.capture_results = on;
    }
}

/// The bundled example models, as JSON, without their text.
///
/// For a picker: most visitors have no `.inp` file to hand, and a drop
/// target with nothing to drop on it demonstrates nothing.
#[wasm_bindgen(js_name = examples)]
pub fn example_catalog() -> String {
    examples::catalog()
}

/// One bundled example's model text, by id. `undefined` for an unknown id.
#[wasm_bindgen(js_name = exampleModel)]
pub fn example_model(id: &str) -> Option<String> {
    examples::model(id).map(str::to_owned)
}

/// A model opened and ready to run.
///
/// Drive it by calling [`HydraRun::advance`] until the progress it returns
/// reports `done`, painting between calls. See [`run`] for why the caller
/// sets the pace rather than the engine.
#[wasm_bindgen]
pub struct HydraRun {
    inner: Run,
}

#[wasm_bindgen]
impl HydraRun {
    /// Open the model described by `options`.
    ///
    /// Rejects with the CLI's diagnostics as a JSON string: `{"exit":1,
    /// "diagnostics":[…]}`.
    #[wasm_bindgen(js_name = open)]
    pub fn open(options: &RunOptions) -> Result<HydraRun, JsError> {
        let inner = Run::open(OpenRequest {
            model: &options.model,
            model_name: &options.model_name,
            engine: options.engine.as_deref(),
            aux: &options.aux,
            capture_results: options.capture_results,
        })
        .map_err(failure_to_js)?;
        Ok(Self { inner })
    }

    /// The key of the engine that owns this model (`"wds"`, `"uds"`).
    #[wasm_bindgen(getter, js_name = engineKey)]
    pub fn engine_key(&self) -> String {
        self.inner.engine().key.to_string()
    }

    /// That engine's human-facing label.
    #[wasm_bindgen(getter, js_name = engineLabel)]
    pub fn engine_label(&self) -> String {
        self.inner.engine().label.to_string()
    }

    /// Total simulated duration (s).
    #[wasm_bindgen(getter)]
    pub fn duration(&self) -> f64 {
        self.inner.duration()
    }

    /// Where the run is, as JSON, without advancing it.
    #[wasm_bindgen(getter)]
    pub fn progress(&self) -> String {
        json_or_empty(&self.inner.progress())
    }

    /// Advance by at most `max_steps` engine steps, returning the progress
    /// as JSON. Returns early at a phase boundary or on completion.
    #[wasm_bindgen(js_name = advance)]
    pub fn advance(&mut self, max_steps: u32) -> Result<String, JsError> {
        let p = self.inner.advance(max_steps).map_err(failure_to_js)?;
        Ok(json_or_empty(&p))
    }

    /// The CLI's progress line for where the run currently is.
    ///
    /// `wall_seconds` is how long the current phase has been running, which
    /// the caller measures — there is no clock on this side of the boundary.
    #[wasm_bindgen(js_name = progressLine)]
    pub fn progress_line(&self, wall_seconds: f64) -> String {
        let p = self.inner.progress();
        progress::render_progress_line(p.phase, p.t, p.duration, wall_seconds)
    }

    /// The line a finished phase leaves behind, replacing its progress line.
    ///
    /// Takes the phase name rather than reading the current one: this is
    /// called at a boundary, where the run has already moved on and the
    /// current phase is the *next* one.
    #[wasm_bindgen(js_name = doneLine)]
    pub fn done_line(&self, phase: &str, wall_seconds: f64) -> String {
        progress::render_done_line(phase, self.inner.duration(), wall_seconds)
    }

    /// Diagnostics produced since the last call, as a JSON array. Each
    /// element is one line the CLI would have written to stderr.
    #[wasm_bindgen(js_name = takeDiagnostics)]
    pub fn take_diagnostics(&mut self) -> String {
        json_or_empty(&self.inner.take_diagnostics())
    }

    /// The engine's text summary — what `hydra run` prints to stdout.
    #[wasm_bindgen(js_name = reportText)]
    pub fn report_text(&self) -> Result<String, JsError> {
        self.inner.report_text().map_err(failure_to_js)
    }

    /// The engine's JSON summary, or `undefined` for an engine that does
    /// not offer one (which is what the CLI refuses a `.json` path for).
    #[wasm_bindgen(js_name = reportJson)]
    pub fn report_json(&self) -> Result<Option<String>, JsError> {
        self.inner.report_json().transpose().map_err(failure_to_js)
    }

    /// The binary `.out` results, when they were captured. `undefined`
    /// otherwise.
    #[wasm_bindgen(js_name = resultsBytes)]
    pub fn results_bytes(&self) -> Option<Vec<u8>> {
        self.inner.results_bytes()
    }

    /// The hotstart file the model asked to save, once the run has
    /// finished. `undefined` when it declared none.
    #[wasm_bindgen(js_name = hotstartBytes)]
    pub fn hotstart_bytes(&self) -> Option<Vec<u8>> {
        self.inner.hotstart().map(|(_, bytes)| bytes.to_vec())
    }

    /// The name the model declared that hotstart file under — what the same
    /// run writes at a terminal, and so what the download should be called.
    #[wasm_bindgen(getter, js_name = hotstartName)]
    pub fn hotstart_name(&self) -> Option<String> {
        self.inner.hotstart().map(|(name, _)| name.to_string())
    }
}

/// Serialise a value that is known to serialise, falling back to a JSON
/// literal rather than panicking — this runs across an FFI boundary where a
/// panic aborts the module rather than failing the call.
fn json_or_empty<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| String::from("null"))
}

/// Carry a [`Failure`] across the boundary as a JS `Error` whose message is
/// the failure's JSON, so a caller can both show it and inspect it.
fn failure_to_js(f: Failure) -> JsError {
    JsError::new(&json_or_empty(&f))
}
