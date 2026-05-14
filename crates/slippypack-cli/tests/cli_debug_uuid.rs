//! Integration test for `slippypack debug uuid` — the diagnostic
//! that emits the derived UUIDv5 (or canonical descriptor bytes for
//! `--bytes`) without doing a full pack build.

use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_slippypack")
}

#[test]
fn synthetic_uuid_is_stable_across_invocations() {
    let run = || {
        let out = Command::new(binary_path())
            .args(["debug", "uuid", "--source", "synthetic"])
            .output()
            .expect("spawn");
        assert!(
            out.status.success(),
            "non-zero exit: stderr={}",
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).unwrap()
    };
    let a = run();
    let b = run();
    assert_eq!(a, b, "two invocations gave different UUIDs");

    // Shape: 8-4-4-4-12 hex + trailing newline = 37 chars.
    assert_eq!(a.len(), 37, "got {a:?}");
    assert!(a.ends_with('\n'));
    let trimmed = a.trim_end();
    let parts: Vec<&str> = trimmed.split('-').collect();
    assert_eq!(parts.len(), 5);
    // UUIDv5 has version nibble 5 in the first hex char of group 3.
    assert!(parts[2].starts_with('5'), "version 5 expected: {trimmed}");
}

#[test]
fn bytes_mode_emits_canonical_json() {
    let out = Command::new(binary_path())
        .args(["debug", "uuid", "--source", "synthetic", "--bytes"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    // Canonical JSON, no trailing newline, starts with `{"bbox":` and
    // ends with `}`.
    let bytes = out.stdout;
    assert!(
        bytes.starts_with(b"{\"bbox\":"),
        "expected canonical JSON; got: {:?}",
        String::from_utf8_lossy(&bytes),
    );
    assert!(
        bytes.ends_with(b"}"),
        "expected closing brace; got: {:?}",
        String::from_utf8_lossy(&bytes),
    );
}

#[test]
fn url_template_requires_bbox_and_zoom() {
    // No --bbox / --zoom: error.
    let out = Command::new(binary_path())
        .args([
            "debug",
            "uuid",
            "--source",
            "https://example.com/{z}/{x}/{y}.png",
        ])
        .output()
        .expect("spawn");
    assert!(
        !out.status.success(),
        "URL template without --bbox should fail",
    );
}

#[test]
fn url_template_with_args_produces_uuid() {
    let out = Command::new(binary_path())
        .args([
            "debug",
            "uuid",
            "--source",
            "https://example.com/{z}/{x}/{y}.png",
            "--bbox",
            "-0.15,51.49,-0.10,51.52",
            "--zoom",
            "10-12",
        ])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
    let uuid_str = String::from_utf8(out.stdout).unwrap();
    assert_eq!(uuid_str.len(), 37);
    assert!(
        uuid_str
            .trim_end()
            .split('-')
            .nth(2)
            .unwrap()
            .starts_with('5')
    );
}

#[test]
fn debug_uuid_uuid_matches_make_output_for_synthetic() {
    // Invariant: running `debug uuid --source synthetic` produces the
    // same UUID that `make --source synthetic` embeds in the pack
    // header. This is the user-visible contract for the diagnostic.
    let debug_out = Command::new(binary_path())
        .args(["debug", "uuid", "--source", "synthetic"])
        .output()
        .expect("spawn debug");
    assert!(debug_out.status.success());
    let debug_uuid = String::from_utf8(debug_out.stdout).unwrap();
    let debug_uuid = debug_uuid.trim_end();

    let tmp_pack = std::env::temp_dir().join(format!(
        "slippypack-debug-uuid-cross-{}.rawtiles",
        std::process::id(),
    ));
    let _ = std::fs::remove_file(&tmp_pack);

    let make_out = Command::new(binary_path())
        .args(["make", "--source", "synthetic", "--timestamp", "0", "--out"])
        .arg(&tmp_pack)
        .output()
        .expect("spawn make");
    assert!(
        make_out.status.success(),
        "make failed: stderr={}",
        String::from_utf8_lossy(&make_out.stderr),
    );
    let pack_bytes = std::fs::read(&tmp_pack).expect("read pack");
    let _ = std::fs::remove_file(&tmp_pack);

    // pack_uuid is bytes 6..22 of the header (per F-001 / header.rs).
    let pack_uuid = &pack_bytes[6..22];
    let parsed = uuid_bytes_to_hyphenated(pack_uuid);
    assert_eq!(parsed, debug_uuid, "debug uuid must match make's pack_uuid");
}

fn uuid_bytes_to_hyphenated(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    assert_eq!(bytes.len(), 16);
    let mut hex = String::with_capacity(32);
    for b in bytes {
        write!(&mut hex, "{b:02x}").unwrap();
    }
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32],
    )
}
