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
    // B-source expressions (`inpptree.c` front end): the bytes as an
    // expression on their own and as the tail of a B card.
    let _ = dune_netlist::parse_expr(&text);
    let card = format!("t\nb1 1 0 v={}\n.end\n", text.replace(['\n', '\r'], " "));
    let _ = dune_netlist::parse_with_loader(std::path::Path::new("fuzz.cir"), card, &mut NoFs);
    let _ = dune_netlist::parse_with_loader(std::path::Path::new("fuzz.cir"), text, &mut NoFs);
});
