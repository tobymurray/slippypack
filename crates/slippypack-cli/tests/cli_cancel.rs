//! Integration test for PLAN.md's test 6: SIGINT mid-build removes the
//! `.partial` file and leaves no `.rawtiles` artifact behind.
//!
//! Unix-only: relies on `libc::kill` to deliver `SIGINT` to a child
//! process. On Windows, `ctrlc` uses `CTRL_C_EVENT` which we'd need a
//! different invocation shape to test; the unit tests in
//! `build::tests` cover the cancellation logic itself, so the
//! Windows-side gap only misses the OS-signal hookup (well-tested by
//! the `ctrlc` crate's own CI matrix).
//!
//! ## How the test stays deterministic
//!
//! The synthetic build is ~16 tiles and runs in milliseconds, which is
//! too fast for a `SIGINT` racing against it to reliably land
//! mid-build. The CLI honors a `SLIPPYPACK_DEBUG_SLEEP_MS` env var that
//! sleeps between each per-tile iteration; the test sets it to 200ms so
//! the full build would take ~3.2s, and the test sends `SIGINT` after
//! 300ms — long enough to be inside the build loop, short enough to
//! leave plenty of remaining tiles.

#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_slippypack")
}

#[test]
fn sigint_mid_build_cleans_up_partial_and_leaves_no_output() {
    let out_path: PathBuf = std::env::temp_dir().join(format!(
        "slippypack-sigint-test-{}.rawtiles",
        std::process::id(),
    ));
    let partial_path = {
        let mut s = out_path.as_os_str().to_owned();
        s.push(".partial");
        PathBuf::from(s)
    };

    // Best-effort cleanup in case a previous run left files behind.
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&partial_path);

    let mut child = Command::new(binary_path())
        .arg("make")
        .arg("--source")
        .arg("synthetic")
        .arg("--out")
        .arg(&out_path)
        .arg("--timestamp")
        .arg("0")
        .env("SLIPPYPACK_DEBUG_SLEEP_MS", "200")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn slippypack binary");

    // Wait long enough that the build is inside the per-tile loop. At
    // 200 ms / tile and 16 tiles, this is well before completion.
    std::thread::sleep(Duration::from_millis(300));

    // `child.id()` is the child's PID; we send SIGINT via nix's safe
    // wrapper rather than calling `libc::kill` directly (the workspace
    // forbids `unsafe`).
    let pid = nix::unistd::Pid::from_raw(i32::try_from(child.id()).expect("child PID fits in i32"));
    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGINT).expect("kill child");

    // Wait for the child to exit with a bounded budget.
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("child did not exit within 5s after SIGINT");
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    // Per Unix convention, Ctrl-C-terminated processes exit with 130.
    // The CLI also surfaces "cancelled" on stderr.
    assert!(
        !status.success(),
        "process exited successfully after SIGINT (status: {status:?})",
    );
    if let Some(code) = status.code() {
        assert_eq!(code, 130, "expected exit code 130, got {code}");
    }

    assert!(
        !out_path.exists(),
        "SIGINT-cancelled build must not leave a final .rawtiles at {}",
        out_path.display(),
    );
    assert!(
        !partial_path.exists(),
        "SIGINT-cancelled build must not leave a .partial at {}",
        partial_path.display(),
    );
}
