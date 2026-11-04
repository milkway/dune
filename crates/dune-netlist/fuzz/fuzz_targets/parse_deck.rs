//! libFuzzer target: the parser must never panic, hang or touch the
//! file system, whatever bytes arrive.
//!
//! Run with: `cargo +nightly fuzz run parse_deck` (from
//! `crates/dune-netlist`; needs `cargo install cargo-fuzz`).

#![no_main]

use libfuzzer_sys::fuzz_target;

struct NoFs;

impl dune_netlist::IncludeLoader for NoFs {
    fn load(&mut self, _: &std::path::Path, _: u64) -> Result<String, String> {
        Err("no file system under fuzzing".into())
    }
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data).into_owned();
    let _ = dune_netlist::parse_with_loader(std::path::Path::new("fuzz.cir"), text, &mut NoFs);
});
