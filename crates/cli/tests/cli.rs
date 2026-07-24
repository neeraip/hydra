//! Black-box tests for the compiled `hydra` binary.
//!
//! These exercise the CLI end-to-end: argument handling, file I/O, report and
//! binary output writing, and the exit-code contract documented in
//! `src/main.rs`:
//!
//! - 0 — simulation completed (also `--help` / `--version`)
//! - 1 — usage/input error (bad arguments, bad INP, missing input file)
//! - 2 — solver error
//! - 3 — I/O error
//! - 4 — internal error (unexpected engine state; not cheaply triggerable
//!   end-to-end, so the mapping is pinned by unit tests in `src/main.rs`)
//!
//! The HTTP input path is tested against a one-shot localhost server spun up
//! inside the test itself — no external network access is required.

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

/// Magic number that opens and closes every Hydra binary `.out` file.
const OUT_MAGIC: i32 = 516114521;

/// Path to a small, stable fixture INP in the workspace-root `tests/fixtures`.
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("tests/fixtures")
        .join(name)
}

fn hydra() -> Command {
    Command::cargo_bin("hydra").expect("hydra binary builds")
}

// ── Happy path ───────────────────────────────────────────────────────────────

#[test]
fn valid_inp_writes_report_to_stdout_with_exit_0() {
    hydra()
        .arg(fixture_path("four_node_loop.inp"))
        .assert()
        .success()
        .stdout(predicate::str::contains("H Y D R A"))
        .stdout(predicate::str::contains("Number of Junctions"));
}

#[test]
fn valid_inp_writes_rpt_and_out_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let rpt = dir.path().join("net.rpt");
    let out = dir.path().join("net.out");

    hydra()
        .arg(fixture_path("four_node_loop.inp"))
        .arg(&rpt)
        .arg(&out)
        .assert()
        .success();

    let rpt_text = std::fs::read_to_string(&rpt).expect("report file written");
    assert!(
        rpt_text.contains("H Y D R A"),
        "report missing banner:\n{rpt_text}"
    );

    let out_bytes = std::fs::read(&out).expect("output file written");
    assert!(out_bytes.len() > 8, "output file too small");
    let prolog = i32::from_le_bytes(out_bytes[0..4].try_into().expect("4 bytes"));
    let epilog = i32::from_le_bytes(
        out_bytes[out_bytes.len() - 4..]
            .try_into()
            .expect("4 bytes"),
    );
    assert_eq!(prolog, OUT_MAGIC, "prolog magic mismatch");
    assert_eq!(epilog, OUT_MAGIC, "epilog magic mismatch");
}

#[test]
fn json_report_is_valid_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json_path = dir.path().join("net.json");

    hydra()
        .arg("--input")
        .arg(fixture_path("four_node_loop.inp"))
        .arg("--report")
        .arg(&json_path)
        .assert()
        .success();

    let text = std::fs::read_to_string(&json_path).expect("json report written");
    let value: serde_json::Value = serde_json::from_str(&text).expect("report is valid JSON");
    assert!(value.is_object(), "JSON report root must be an object");
}

#[test]
fn positional_json_report_is_valid_json() {
    // The `.json` suffix must select JSON output through the positional
    // report path too, not just the --report flag.
    let dir = tempfile::tempdir().expect("tempdir");
    let json_path = dir.path().join("net.json");

    hydra()
        .arg(fixture_path("four_node_loop.inp"))
        .arg(&json_path)
        .assert()
        .success();

    let text = std::fs::read_to_string(&json_path).expect("json report written");
    let value: serde_json::Value = serde_json::from_str(&text).expect("report is valid JSON");
    assert!(value.is_object(), "JSON report root must be an object");
}

#[test]
fn quiet_flag_is_accepted() {
    hydra()
        .arg("-q")
        .arg(fixture_path("four_node_loop.inp"))
        .assert()
        .success()
        .stdout(predicate::str::contains("H Y D R A"));
}

#[test]
fn help_exits_0() {
    hydra()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn version_exits_0() {
    hydra()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("Hydra version"))
        .stdout(predicate::str::contains("CLI version"));
}

#[test]
fn short_upper_v_prints_version_and_exits_0() {
    hydra()
        .arg("-V")
        .assert()
        .success()
        .stdout(predicate::str::contains("Hydra version"))
        .stdout(predicate::str::contains("CLI version"));
}

#[test]
fn short_lower_v_is_an_error_with_hint() {
    // -v used to mean --version; it is now rejected with a hint pointing at
    // -V (version) and -q/--quiet, and must NOT print version info.
    hydra()
        .arg("-v")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("Hydra version").not())
        .stderr(predicate::str::contains("-V"))
        .stderr(predicate::str::contains("--quiet"));
}

// ── Usage/input errors (exit 1) ──────────────────────────────────────────────

#[test]
fn missing_input_file_exits_1() {
    hydra()
        .arg("definitely/not/a/real/file.inp")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("io/fetch"));
}

#[test]
fn unknown_flag_exits_1() {
    // Usage errors are remapped from clap's default exit 2 (reserved here for
    // solver errors) to exit 1.
    hydra()
        .arg("--no-such-flag")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--no-such-flag"));
}

#[test]
fn no_input_specified_exits_1() {
    hydra()
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no input file specified"));
}

#[test]
fn too_many_positional_args_exits_1() {
    hydra()
        .args(["a.inp", "b.rpt", "c.out", "d.extra"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("at most 3 positional arguments"));
}

#[test]
fn validation_failing_inp_exits_1() {
    // Syntactically valid INP whose network fails §2.9 validation (no
    // reservoir or tank) must be reported as an input/parse error (exit 1).
    let dir = tempfile::tempdir().expect("tempdir");
    let bad = dir.path().join("junctions_only.inp");
    std::fs::write(
        &bad,
        b"[JUNCTIONS]\nJ1    0    10\nJ2    0    5\n\n\
          [PIPES]\nP1    J1    J2    1000    12    100    0    Open\n\n\
          [OPTIONS]\nUnits    GPM\nHeadloss    H-W\n",
    )
    .expect("write INP");

    hydra()
        .arg(&bad)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("\"level\":\"error\""))
        .stderr(predicate::str::contains("validation/network"))
        .stderr(predicate::str::contains("network has no reservoir"));
}

#[test]
fn duplicate_id_inp_exits_1_and_names_the_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bad = dir.path().join("dup_id.inp");
    std::fs::write(
        &bad,
        b"[JUNCTIONS]\nJ1    0    10\nJ1    0    5\n\n\
          [RESERVOIRS]\nR1    100\n\n\
          [PIPES]\nP1    R1    J1    1000    12    100    0    Open\n\n\
          [OPTIONS]\nUnits    GPM\nHeadloss    H-W\n",
    )
    .expect("write INP");

    hydra()
        .arg(&bad)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("input/parse"))
        .stderr(predicate::str::contains("duplicate node ID 'J1'"));
}

#[test]
fn unparseable_inp_exits_1() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bad = dir.path().join("garbage.inp");
    std::fs::write(&bad, b"\x00\x01\x02 this is not an INP file").expect("write garbage");

    hydra()
        .arg(&bad)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("\"level\":\"error\""));
}

// ── I/O errors (exit 3) ──────────────────────────────────────────────────────

#[test]
fn report_to_missing_directory_exits_3() {
    let dir = tempfile::tempdir().expect("tempdir");
    let rpt = dir.path().join("no/such/dir/net.rpt");

    hydra()
        .arg(fixture_path("four_node_loop.inp"))
        .arg(&rpt)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("io/report"));
}

#[test]
fn output_to_missing_directory_exits_3() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("no/such/dir/net.out");

    hydra()
        .arg("--input")
        .arg(fixture_path("four_node_loop.inp"))
        .arg("--output")
        .arg(&out)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("io/output"));
}

// ── HTTP input path (localhost only) ─────────────────────────────────────────

/// Spawn a one-shot HTTP server on an ephemeral localhost port that answers
/// the first request with `status` and `body`, then shuts down.
fn one_shot_http_server(
    status: &'static str,
    body: Vec<u8>,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        // Read until the end of the request headers.
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let n = stream.read(&mut chunk).expect("read request");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write headers");
        stream.write_all(&body).expect("write body");
        stream.flush().expect("flush");
    });
    (format!("http://{addr}/model.inp"), handle)
}

#[test]
fn http_input_from_localhost_server_succeeds() {
    let inp = std::fs::read(fixture_path("four_node_loop.inp")).expect("read fixture");
    let (url, server) = one_shot_http_server("200 OK", inp);

    hydra()
        .arg(&url)
        .assert()
        .success()
        .stdout(predicate::str::contains("H Y D R A"));

    server.join().expect("server thread");
}

#[test]
fn http_404_exits_1() {
    let (url, server) = one_shot_http_server("404 Not Found", Vec::new());

    hydra()
        .arg(&url)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("HTTP 404"));

    server.join().expect("server thread");
}

#[test]
fn http_500_exits_3() {
    // 5xx is a server-side failure, classified as I/O (exit 3), not input.
    let (url, server) = one_shot_http_server("500 Internal Server Error", Vec::new());

    hydra()
        .arg(&url)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("HTTP 500"));

    server.join().expect("server thread");
}
