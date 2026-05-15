//! Integration test for `slippypack inspect <pack>`.
//!
//! Builds a synthetic pack via `slippypack make`, then runs
//! `slippypack inspect` on it and asserts the summary contains the
//! key lines (file size, header metadata, per-zoom counts, extension
//! sections). This is the end-to-end `PoC` test: a producer writes a
//! pack and a consumer reads it back without writing any custom Rust.

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_slippypack")
}

fn temp_pack_path(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "slippypack-inspect-test-{}-{}.rawtiles",
        std::process::id(),
        label,
    ));
    let _ = std::fs::remove_file(&p);
    p
}

fn make_synthetic(out: &std::path::Path, extra_args: &[&str]) {
    let mut cmd = Command::new(binary_path());
    cmd.arg("make")
        .arg("--source")
        .arg("synthetic")
        .arg("--out")
        .arg(out);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let status = cmd.status().expect("spawn slippypack make");
    assert!(status.success(), "make failed: {status}");
}

fn run_inspect(pack: &std::path::Path) -> String {
    let output = Command::new(binary_path())
        .arg("inspect")
        .arg(pack)
        .output()
        .expect("spawn slippypack inspect");
    assert!(
        output.status.success(),
        "inspect failed: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout).expect("inspect output is valid UTF-8")
}

#[test]
fn inspect_synthetic_pack_shows_summary() {
    let out = temp_pack_path("synthetic_summary");
    make_synthetic(&out, &[]);

    let summary = run_inspect(&out);

    assert!(
        summary.contains("format_version: 1.0"),
        "missing format_version line:\n{summary}",
    );
    assert!(
        summary.contains("pack_uuid: "),
        "missing pack_uuid line:\n{summary}",
    );
    assert!(
        summary.contains("tile_count: 16"),
        "synthetic pack should contain 16 tiles:\n{summary}",
    );
    assert!(
        summary.contains("zoom 2: 16 tile(s)"),
        "synthetic pack should report 16 tiles at z=2:\n{summary}",
    );
    assert!(
        summary.contains("extensions: 0 section(s)"),
        "synthetic pack without --attribution should report no extensions:\n{summary}",
    );

    let _ = std::fs::remove_file(&out);
}

#[test]
fn inspect_renders_attr_section_payload_as_utf8() {
    let out = temp_pack_path("attr_payload");
    make_synthetic(
        &out,
        &["--attribution", "\u{00a9} OpenStreetMap contributors"],
    );

    let summary = run_inspect(&out);

    assert!(
        summary.contains("extensions: 1 section(s)"),
        "should report one extension section:\n{summary}",
    );
    assert!(
        summary.contains("ATTR (29 byte payload)"),
        "should report the ATTR tag and payload length:\n{summary}",
    );
    assert!(
        summary.contains("\"\u{00a9} OpenStreetMap contributors\""),
        "should render the ATTR payload as a UTF-8 string:\n{summary}",
    );

    let _ = std::fs::remove_file(&out);
}

#[test]
fn inspect_missing_file_exits_non_zero() {
    let bogus = std::env::temp_dir().join("definitely-does-not-exist.rawtiles");
    let _ = std::fs::remove_file(&bogus);
    let status = Command::new(binary_path())
        .arg("inspect")
        .arg(&bogus)
        .status()
        .expect("spawn slippypack inspect");
    assert!(!status.success(), "inspect on missing file should fail");
}

#[test]
fn inspect_non_pack_file_exits_non_zero() {
    // A non-empty file that isn't a valid .rawtiles pack (no magic).
    let path = temp_pack_path("garbage");
    std::fs::write(&path, b"this is not a rawtiles pack").expect("write garbage");
    let status = Command::new(binary_path())
        .arg("inspect")
        .arg(&path)
        .status()
        .expect("spawn slippypack inspect");
    assert!(!status.success(), "inspect on non-pack should fail");
    let _ = std::fs::remove_file(&path);
}
