//! Compile `tests/smoke.c` against `include/dune.h` and the shared
//! library cargo builds next to this test, then run it on the 3f5
//! RC example. Skipped (with a note) where no `cc` is installed.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The directory holding this test binary, where cargo also puts the
/// crate's cdylib when it builds the library for the tests.
fn deps_dir() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn c_program_runs_rc_cir() {
    if Command::new("cc").arg("--version").output().is_err() {
        eprintln!("note: no `cc` on PATH; skipping the C smoke test");
        return;
    }
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let deps = deps_dir();
    let lib = deps.join(if cfg!(target_os = "macos") {
        "libdune_ffi.dylib"
    } else {
        "libdune_ffi.so"
    });
    assert!(lib.exists(), "cdylib not found at {}", lib.display());

    let out_dir = tempfile::tempdir().unwrap();
    let exe = out_dir.path().join("smoke");
    let status = Command::new("cc")
        .args(["-std=c99", "-Wall", "-Wextra", "-Werror", "-pedantic"])
        .arg("-I")
        .arg(crate_dir.join("include"))
        .arg(crate_dir.join("tests/smoke.c"))
        .arg(&lib)
        .arg(format!("-Wl,-rpath,{}", deps.display()))
        .arg("-lm")
        .arg("-o")
        .arg(&exe)
        .status()
        .unwrap();
    assert!(status.success(), "cc failed on tests/smoke.c");

    let rc = crate_dir.join("../../corpus/spice3f5/examples/rc.cir");
    let out = Command::new(&exe).arg(&rc).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    print!("{stdout}");
    eprint!("{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "smoke exited with {}", out.status);
    assert!(stdout.ends_with("ok\n"));
}
