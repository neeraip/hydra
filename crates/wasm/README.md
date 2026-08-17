# hydra-wasm

Hydra's simulation engines compiled to WebAssembly, and a demo page that
runs a model in a browser tab and prints what the CLI would have printed.

```sh
just wasm-serve      # build the bundle and serve http://localhost:8000
just wasm-single     # build the whole thing as one portable HTML file
```

Drop an EPANET or SWMM `.inp` file on the page — or pick a bundled example
(EPANET's Net1, SWMM's Simulation1; provenance in `models/NOTICE.md`). It
is read, solved and reported locally — nothing is uploaded, and there is no
server beyond the one handing over the static files. A uds model's
`SAVE HOTSTART` ends as a download under the name the model declared.

The demo deploys to GitHub Pages at
[`/try`](https://neeraip.github.io/hydra/try/) (the Site workflow rebuilds
it on demo, docs or site changes, or on manual dispatch), and every library
release attaches the single-file build as `hydra-try-<version>.html`,
pinned to that release's engines.

## The portable file

`just wasm-single` produces `www/hydra.html`: about 620 kB, one file, no
server. Mail it, put it on a USB stick, open it from a Downloads folder —
it still solves a network.

Getting there means working around two things `file://` refuses, and both
shape the build:

- **ES modules.** A `file://` document has an opaque origin, so importing
  another `file://` script fails CORS. The demo's script is therefore a
  classic script exposing `startHydraDemo`, and the wasm bundle is built
  with wasm-pack's `no-modules` target.
- **Streaming instantiation.** `fetch` cannot read `file://` either, so the
  wasm is embedded in the page and instantiated from bytes rather than
  fetched.

Embedding is what costs size — base64 adds a third to a bundle already over
a megabyte — so it is gzipped first and inflated by `DecompressionStream`
in the browser. A browser without that (before Chrome 80, Safari 16.4,
Firefox 113) is told so rather than showing an empty page.

`scripts/build-wasm-single.py` does the assembly and refuses to write a
file that still references anything outside itself. Its tests are in
`scripts/tests/`, and run as part of `just ci`.

`app.js` is the same file in both builds. It takes the module as an
argument instead of importing it, so each entry point does its own loading
and there is one copy of the demo rather than one per delivery.

## What it is

A third reference consumer of Hydra's public API, alongside the CLI and the
GUI. It depends only on `hydra-sdk`, under the same contract a third party
building on Hydra has.

The output is not a reimplementation. `HydraRun.reportText()` returns
`EngineSession`'s own summary, so it is byte-for-byte what `hydra run
model.inp` writes to stdout, and the diagnostics carry the CLI's own codes
and exit classification.

| The CLI does | Here |
|---|---|
| Reads the model from a path or URL | The page hands over the dropped bytes |
| Resolves auxiliary files against the model's directory | Matched by name among the other dropped files |
| Writes `.out` to `--results` | Captured in memory, offered as a download |
| Writes the summary to stdout or `--summary` | Printed to the page |
| Writes diagnostics to stderr | Printed to the page, one JSON line each |

## What it is not

**Not the GUI on the web.** There is no editing, no canvas and no
persistence. Those are the parts of `hydra-gui` that need a host, and they
are why a browser build of the *application* is a much larger question than
a browser build of the *engines* — 95 Tauri commands and a filesystem
underneath them.

**Not a way to work with large results.** Native builds stream `.out` files
from a path (`io::out_reader`) so they never have to be held whole. A
browser can only hold them, under a 4 GB address ceiling it cannot raise,
so capturing results is opt-in.

## Layout

| File | Holds |
|---|---|
| `src/run.rs` | The CLI's run path without a filesystem — engine resolution, opening, the drive loop, error classification |
| `src/diagnostic.rs` | The CLI's stderr vocabulary: codes, exit codes, the JSON-line shape |
| `src/progress.rs` | The CLI's progress line, so a page renders the same one |
| `src/aux_files.rs` | Matching a model's declared file names against what the user supplied |
| `src/sink.rs` | An in-memory `.out` sink the caller can still read after the run |
| `src/examples.rs` | The bundled example models, compiled in so both delivery modes carry them |
| `models/` | Those models' unmodified upstream files, with provenance and licences in `NOTICE.md` |
| `src/lib.rs` | The `wasm_bindgen` shell, which holds no judgement of its own |
| `www/` | The demo page — `index.html` serves it, `app.js` is the demo itself, and `scripts/build-wasm-single.py` folds all of it into one file |

Every decision is plain Rust so `cargo test` covers it on the host.
`tests/browser.rs` covers what the host cannot: `just test-wasm` runs a
model in headless Chrome, which is the only check that executes engine code
on wasm at all. Both bugs found while bringing this up compiled cleanly and
passed every host test.

## Porting notes

The engines needed one change to run here: `chrono`'s `wasmbind` feature,
target-gated in `hydra-engine-wds`, because the report's date stamp asks the
host what time it is and `wasm32-unknown-unknown` has no clock behind
`SystemTime`. Nothing else in the engines, the report crate or the SDK
required an edit — they use no threads, and every filesystem call outside
`io::out_reader` is in test code.
