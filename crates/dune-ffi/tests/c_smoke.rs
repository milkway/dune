//! Compile `tests/smoke.c` against `include/dune.h` and the shared
//! library cargo builds next to this test, then run it on the 3f5
//! RC example. Skipped (with a note) where no `cc` is installed.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The crate's cdylib, which cargo builds for the tests. Up to cargo
/// 1.99 it sits next to this test binary (`target/<profile>/deps`);
/// from 1.100 each build unit has its own directory
/// (`build/dune-ffi/<hash>/out`), so the library is in a sibling of
/// the test's. With several candidates (stale units), the newest wins.
fn cdylib() -> Option<PathBuf> {
    let name = if cfg!(target_os = "macos") {
        "libdune_ffi.dylib"
    } else {
        "libdune_ffi.so"
    };
    let exe = std::env::current_exe().unwrap();
    let here = exe.parent().unwrap();
    let beside = here.join(name);
    if beside.exists() {
        return Some(beside);
    }
    let units = here.parent()?.parent()?;
    std::fs::read_dir(units)
        .ok()?
        .flatten()
        .map(|unit| unit.path().join("out").join(name))
        .filter(|lib| lib.exists())
        .max_by_key(|lib| lib.metadata().and_then(|m| m.modified()).ok())
}

#[test]
fn c_program_runs_rc_cir() {
    if Command::new("cc").arg("--version").output().is_err() {
        eprintln!("note: no `cc` on PATH; skipping the C smoke test");
        return;
    }
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib =
        cdylib().expect("cdylib not found next to the test binary or in a sibling build unit");
    let deps = lib.parent().unwrap().to_path_buf();

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
