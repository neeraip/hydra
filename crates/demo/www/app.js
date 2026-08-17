// The demo's whole job is to be a terminal: collect files, drive the run,
// and print what the engine says without editing it. Every string it shows
// — the progress line, the report, the diagnostics — is produced in Rust,
// so nothing here decides how a run reads.
//
// # Why this takes the module rather than importing it
//
// This file is loaded two ways. Served from a directory it sits beside an
// ES-module wasm bundle; inlined into a single portable HTML file it sits
// beside a no-modules one, because `file://` allows neither ES modules nor
// streaming instantiation. Both give the same four exports, so the page's
// entry point does the loading and hands them over — leaving one copy of
// the demo instead of one per delivery.

function startHydraDemo(hydra) {
  const { HydraRun, RunOptions, engines, versionInfo, examples, exampleModel } = hydra;

  const term = document.getElementById("term");
  const dropZone = document.getElementById("drop");
  const fileInput = document.getElementById("file");
  const engineSelect = document.getElementById("engine");
  const captureBox = document.getElementById("capture");
  const runButton = document.getElementById("run");
  const clearButton = document.getElementById("clear");
  const downloadButton = document.getElementById("download");
  const hotstartButton = document.getElementById("hotstart");
  const copyButton = document.getElementById("copy");
  const commandLabel = document.getElementById("cmd");
  const examplesRow = document.getElementById("examples");

  /** How long one budget of steps may take before we hand the frame back.
   *  Below a frame, so the page keeps painting; large enough that the
   *  boundary crossing costs little on a fast model. */
  const FRAME_BUDGET_MS = 12;

  /** Steps per call, adapted after each budget from how long it took. Starts
   *  small so the first frame of a slow model is not the one that janks. */
  let stepBudget = 16;

  let model = null;
  let auxFiles = [];
  let lastReport = "";
  let lastResults = null;
  let lastHotstart = null;
  let running = false;

  // ── Terminal ──────────────────────────────────────────────────────────────

  /** Append a line. Returns it, so a progress line can be rewritten in place
   *  rather than repeated — the page's carriage return. */
  /* The DOM is the browser-limit surface here: a big model's report is
   * tens of thousands of lines, and a span each would weigh the tab down
   * long before the engine does. The terminal keeps the newest lines and
   * counts the trimmed ones in a notice; "Copy report" is unaffected,
   * because it copies the engine's own string, never the DOM.
   * `?maxlines=N` exists so a small model can exercise the trim in tests. */
  const MAX_TERM_LINES =
    Number(new URLSearchParams(location.search).get("maxlines")) || 4000;
  let trimmedCount = 0;
  let trimNote = null;

  function trimTerm() {
    while (term.childElementCount - (trimNote ? 1 : 0) > MAX_TERM_LINES) {
      if (!trimNote) {
        trimNote = document.createElement("span");
        trimNote.className = "dim trim-note";
        term.prepend(trimNote);
      }
      term.removeChild(trimNote.nextSibling);
      trimmedCount += 1;
    }
    if (trimNote) {
      trimNote.textContent = `\u22ef ${trimmedCount.toLocaleString()} earlier line${
        trimmedCount === 1 ? "" : "s"
      } hidden to keep this tab light. "Copy report" still has the full text.`;
    }
  }

  function line(text, className) {
    const el = document.createElement("span");
    if (className) el.className = className;
    el.textContent = text;
    term.appendChild(el);
    trimTerm();
    term.scrollTop = term.scrollHeight;
    return el;
  }

  function blank() {
    line("");
  }

  function clearTerm() {
    trimmedCount = 0;
    trimNote = null;
    term.textContent = "";
  }

  /** Print a block of text one line per element, so long reports scroll
   *  without a single enormous text node.
   *
   *  Blank lines stay blank. An empty span collapses to no height, which the
   *  stylesheet handles; putting a space here instead would leave a character
   *  in the page's copy of the report that the engine never wrote, and this
   *  page's only claim is that it shows what the engine wrote. */
  function block(text, className) {
    for (const l of text.split("\n")) line(l, className);
  }

  /** A diagnostic from the CLI's stderr stream, rendered as a readable
   *  line rather than its raw JSON: level, code, and message, with the
   *  element prefixed only when the message does not already carry it. */
  function diagnostic(d) {
    const message = String(d.message ?? "");
    const id =
      d.object_id && !message.startsWith(String(d.object_id)) ? `${d.object_id}: ` : "";
    const at = d.time_step == null ? "" : `  (t=${d.time_step}s)`;
    line(`${d.level}  [${d.code}]  ${id}${message}${at}`, d.level === "error" ? "error" : "warn");
  }

  /** Pull the diagnostics JSON out of a rejected call. `HydraRun` rejects
   *  with an Error whose message is the failure's JSON, so a caller can both
   *  show it and read it. */
  function failureOf(err) {
    try {
      const parsed = JSON.parse(err.message);
      if (parsed && Array.isArray(parsed.diagnostics)) return parsed;
    } catch {
      // Not one of ours — a real JS error.
    }
    return null;
  }

  function reportError(err) {
    const failure = failureOf(err);
    if (!failure) {
      line(String(err && err.message ? err.message : err), "error");
      return;
    }
    for (const d of failure.diagnostics) diagnostic(d);
    blank();
    line(`exit ${failure.exit}`, "error");
  }

  // ── Files ─────────────────────────────────────────────────────────────────

  /** The model is the largest `.inp`, and everything else is auxiliary.
   *
   *  Largest rather than first because a drop arrives in no defined order,
   *  and a SWMM model's climate file can also be named `.inp` — between two
   *  candidates the model is the one with the sections in it. Auxiliary
   *  files carry whatever names the model declares, so only the model is
   *  extension-checked; a set with no `.inp` is either extra auxiliaries
   *  for the model already loaded, or a mistake worth saying out loud. */
  function sortFiles(files) {
    const inps = files.filter((f) => f.name.toLowerCase().endsWith(".inp"));
    if (!inps.length) return { model: null, aux: files };
    const chosen = inps.reduce((a, b) => (b.size > a.size ? b : a));
    return { model: chosen, aux: files.filter((f) => f !== chosen) };
  }

  async function accept(files) {
    if (!files.length) return;
    const sorted = sortFiles(files);
    if (!sorted.model && !model) {
      line(
        "None of that is a model: pick an EPANET or SWMM .inp file. " +
          "Auxiliary files are read alongside a model, matched by the names it declares.",
        "warn",
      );
      return;
    }
    if (!sorted.model) {
      // Auxiliaries for the model already loaded; newest name wins.
      for (const f of sorted.aux) {
        const bytes = new Uint8Array(await f.arrayBuffer());
        auxFiles = auxFiles.filter((a) => a.name !== f.name);
        auxFiles.push({ name: f.name, bytes });
        line(`  + ${f.name}  ${bytes.length.toLocaleString()} bytes`, "dim");
      }
      return;
    }
    model = { name: sorted.model.name, bytes: new Uint8Array(await sorted.model.arrayBuffer()) };
    auxFiles = [];
    for (const f of sorted.aux) {
      auxFiles.push({ name: f.name, bytes: new Uint8Array(await f.arrayBuffer()) });
    }
    runButton.disabled = false;
    updateCommand();
    clearTerm();
    line(`${model.name}  ${model.bytes.length.toLocaleString()} bytes`, "accent");
    for (const a of auxFiles) line(`  + ${a.name}  ${a.bytes.length.toLocaleString()} bytes`, "dim");
  }

  /** Load a bundled example, exactly as if its file had been dropped.
   *
   *  The same `model` slot, the same run path — an example is not a
   *  special mode, it is a file the page happens to already have. The
   *  engine label on the button is decoration; the run still routes the
   *  model by its own contents. */
  function chooseExample(example) {
    const text = exampleModel(example.id);
    if (text === undefined) return;
    model = { name: example.file_name, bytes: new TextEncoder().encode(text) };
    auxFiles = [];
    runButton.disabled = false;
    updateCommand();
    clearTerm();
    line(`${model.name}  ${model.bytes.length.toLocaleString()} bytes`, "accent");
    line(example.description, "dim");
    // The note exists so expected output does not read as failure — a
    // wall of engine warnings on a bundled example looks like a broken
    // example to anyone who was not told.
    if (example.note) line(example.note, "warn");
  }

  /** Mirror the run as the command that would produce it at a terminal. */
  function updateCommand() {
    if (!model) {
      commandLabel.textContent = "hydra run …";
      return;
    }
    const parts = ["hydra", "run", model.name];
    if (engineSelect.value) parts.push("--engine", engineSelect.value);
    if (captureBox.checked) parts.push("--results", model.name.replace(/\.inp$/i, ".out"));
    commandLabel.textContent = parts.join(" ");
  }

  // ── Running ───────────────────────────────────────────────────────────────

  async function run() {
    if (running || !model) return;
    running = true;
    runButton.disabled = true;
    downloadButton.hidden = true;
    hotstartButton.hidden = true;
    copyButton.hidden = true;
    lastResults = null;
    lastHotstart = null;
    clearTerm();

    const versions = JSON.parse(versionInfo());
    line(`Hydra v${versions.hydra}`);

    const options = new RunOptions(model.bytes, model.name);
    options.withEngine(engineSelect.value || undefined);
    options.withResults(captureBox.checked);
    for (const a of auxFiles) options.withAuxFile(a.name, a.bytes);

    let session;
    try {
      session = HydraRun.open(options);
    } catch (err) {
      reportError(err);
      finish();
      return;
    }

    line(`engine: ${session.engineLabel} (${session.engineKey})`, "dim");
    drain(session);

    let phaseStart = performance.now();
    let progressLine = line(session.progressLine(0));

    const step = () => {
      const started = performance.now();
      let progress;
      try {
        progress = JSON.parse(session.advance(stepBudget));
      } catch (err) {
        // Leave the failed progress line where it is and report beneath it,
        // as the CLI does when it abandons the line on an error.
        blank();
        reportError(err);
        finish();
        return;
      }
      const elapsed = performance.now() - started;
      stepBudget = nextBudget(stepBudget, elapsed);

      if (progress.completedPhase) {
        const wall = (performance.now() - phaseStart) / 1000;
        progressLine.textContent = session.doneLine(progress.completedPhase, wall);
        progressLine.className = "ok";
        drain(session);
        phaseStart = performance.now();
        if (!progress.done) progressLine = line(session.progressLine(0));
      } else {
        progressLine.textContent = session.progressLine((performance.now() - phaseStart) / 1000);
      }

      if (progress.done) {
        drain(session);
        report(session);
        finish();
        return;
      }
      requestAnimationFrame(step);
    };

    requestAnimationFrame(step);

    function finish() {
      running = false;
      runButton.disabled = false;
    }

    function report(session) {
      blank();
      try {
        lastReport = session.reportText();
        block(lastReport);
        copyButton.hidden = false;
      } catch (err) {
        reportError(err);
      }
      const bytes = session.resultsBytes();
      if (bytes) {
        lastResults = bytes;
        downloadButton.hidden = false;
        blank();
        line(`.out results: ${bytes.length.toLocaleString()} bytes`, "dim");
      }
      // A hotstart the model asked to save. The CLI writes it beside the
      // model; here it becomes a download under the model's own name.
      const hotstart = session.hotstartBytes();
      if (hotstart) {
        lastHotstart = { name: session.hotstartName, bytes: hotstart };
        hotstartButton.hidden = false;
        blank();
        line(
          `hotstart saved: ${lastHotstart.name}  ${hotstart.length.toLocaleString()} bytes`,
          "dim",
        );
      }
    }

    function drain(session) {
      const diagnostics = JSON.parse(session.takeDiagnostics());
      for (const d of diagnostics) diagnostic(d);
    }
  }

  /** Grow or shrink the step budget so one call lands near the frame budget.
   *  Clamped at both ends: never below one step (the run must progress) and
   *  never so high that one call blocks for long on a slow model. */
  function nextBudget(budget, elapsedMs) {
    if (elapsedMs <= 0) return Math.min(budget * 2, 4096);
    const scaled = Math.round((budget * FRAME_BUDGET_MS) / elapsedMs);
    return Math.max(1, Math.min(scaled, 4096));
  }

  // ── Wiring ────────────────────────────────────────────────────────────────

  dropZone.addEventListener("click", () => fileInput.click());
  dropZone.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      fileInput.click();
    }
  });
  fileInput.addEventListener("change", () => accept([...fileInput.files]));

  for (const type of ["dragenter", "dragover"]) {
    dropZone.addEventListener(type, (e) => {
      e.preventDefault();
      dropZone.classList.add("over");
    });
  }
  for (const type of ["dragleave", "drop"]) {
    dropZone.addEventListener(type, (e) => {
      e.preventDefault();
      dropZone.classList.remove("over");
    });
  }
  dropZone.addEventListener("drop", (e) => accept([...e.dataTransfer.files]));

  runButton.addEventListener("click", run);
  engineSelect.addEventListener("change", updateCommand);
  captureBox.addEventListener("change", updateCommand);
  clearButton.addEventListener("click", () => {
    clearTerm();
    downloadButton.hidden = true;
    hotstartButton.hidden = true;
    copyButton.hidden = true;
  });

  function offerDownload(bytes, name) {
    const url = URL.createObjectURL(new Blob([bytes], { type: "application/octet-stream" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.click();
    URL.revokeObjectURL(url);
  }

  copyButton.addEventListener("click", () => navigator.clipboard.writeText(lastReport));
  downloadButton.addEventListener("click", () => {
    if (lastResults) offerDownload(lastResults, model.name.replace(/\.inp$/i, ".out"));
  });
  hotstartButton.addEventListener("click", () => {
    if (lastHotstart) offerDownload(lastHotstart.bytes, lastHotstart.name);
  });

  document.getElementById("version").textContent = `v${JSON.parse(versionInfo()).hydra}`;

  // Planned engines are listed and disabled rather than hidden: a reserved
  // key is not an absent one, and a picker that omitted it would misdescribe
  // what this build provides.
  for (const e of JSON.parse(engines())) {
    const option = document.createElement("option");
    option.value = e.key;
    option.textContent = e.available ? `${e.label} (${e.key})` : `${e.label} (${e.key}), planned`;
    option.disabled = !e.available;
    engineSelect.appendChild(option);
  }

  // The bundled examples, one button each — most visitors have no .inp
  // file to hand, and a drop target with nothing to drop demonstrates
  // nothing.
  for (const example of JSON.parse(examples())) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = `${example.file_name} (${example.engine})`;
    button.title = example.description;
    button.addEventListener("click", () => chooseExample(example));
    examplesRow.appendChild(button);
  }

  line("Ready. Drop a model above, or pick an example.", "dim");

}
