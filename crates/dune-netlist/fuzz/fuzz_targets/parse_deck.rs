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

/// Serves every path with the input itself, so `.lib`/`.include`
/// re-read the fuzzed bytes: sections, nesting, cycles and the byte
/// budgets get exercised without a file system. Deep paths are
/// refused to keep one execution fast (a fan-out over ever-new names
/// is bounded by the 256 MiB splice budget, too slow per input).
struct SelfFs(String);

impl dune_netlist::IncludeLoader for SelfFs {
    fn load(&mut self, path: &std::path::Path, limit: u64) -> Result<String, String> {
        if path.components().count() > 4 || self.0.len() as u64 > limit {
            return Err("refused under fuzzing".into());
        }
        Ok(self.0.clone())
    }
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data).into_owned();
    // B-source expressions (`inpptree.c` front end): the bytes as an
    // expression on their own and as the tail of a B card.
    let _ = dune_netlist::parse_expr(&text);
    let card = format!("t\nb1 1 0 v={}\n.end\n", text.replace(['\n', '\r'], " "));
    let _ = dune_netlist::parse_with_loader(std::path::Path::new("fuzz.cir"), card, &mut NoFs);
    // Parameter expressions (`pexpr.rs`): the bytes as a `.param`
    // body, inside `{…}` as a global value and an element value, and
    // as a bare value after `=` on a `.model`.
    let line = text.replace(['\n', '\r'], " ");
    let params = format!(
        "t\n.param p=2\n.param {line}\n.param q={{{line}}}\nr1 1 0 {{{line}}}\n\
         .model m r rsh={line}\nv1 2 0 {{q}}\n.end\n"
    );
    let _ = dune_netlist::parse_with_loader(std::path::Path::new("fuzz.cir"), params, &mut NoFs);
    // Subcircuit parameters (G3/G4/G6): the bytes as the parameters of
    // an X line, as a .subckt-line default list, as a body .param and
    // as the parameters of a local binned model, with the instance
    // nested, multiplied and repeated.
    let sub = format!(
        "t\n.param g=1\n.subckt leaf a w=1 {line}\n.param q={{w*g}} {line}\n\
         .model nm.1 nmos level=49 lmin=0 lmax=1 wmin=0 wmax=1 {line}\n\
         m1 a a 0 0 nm l={{q*1u}} w=1u\nr1 a 0 {{{line}}}\nx1 a leaf2 {line}\n.ends\n\
         .subckt leaf2 b {line}\nr2 b 0 {{w}}\n.ends\n\
         x1 1 leaf {line}\nx2 2 leaf m=2 w={{{line}}}\nx3 3 leaf w=2\n.end\n"
    );
    let _ = dune_netlist::parse_with_loader(std::path::Path::new("fuzz.cir"), sub, &mut NoFs);
    let _ =
        dune_netlist::parse_with_loader(std::path::Path::new("fuzz.cir"), text.clone(), &mut NoFs);
    let _ = dune_netlist::parse_with_loader(
        std::path::Path::new("fuzz.lib"),
        text.clone(),
        &mut SelfFs(text),
    );
});
