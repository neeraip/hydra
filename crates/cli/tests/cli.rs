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
