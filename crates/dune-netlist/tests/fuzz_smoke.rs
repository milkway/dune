//! Deterministic mutation smoke test: a poor man's fuzzer that runs
//! in plain CI (the real libFuzzer target lives in `fuzz/`). It
//! mutates the corpus decks and raw noise through the parser and
//! only asserts the absence of panics and hangs.

use std::path::Path;

use dune_netlist::parse_with_loader;

struct NoFs;

impl dune_netlist::IncludeLoader for NoFs {
    fn load(&mut self, _: &Path, _: u64) -> Result<String, String> {
        Err("no file system in this test".into())
    }
}

/// xorshift64*: cheap, deterministic.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn parse(text: String) {
    let _ = parse_with_loader(Path::new("mut.cir"), text, &mut NoFs);
}

#[test]
fn mutated_corpus_never_panics() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/spice3f5/examples");
    let mut seeds: Vec<Vec<u8>> = Vec::new();
    for e in std::fs::read_dir(&dir).expect("corpus present").flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("cir")) {
            seeds.push(std::fs::read(&p).unwrap());
        }
    }
    assert!(!seeds.is_empty());

    let mut rng = Rng(0x5eed_cafe_f00d_0001);
    let noise: &[u8] = b".+*$;(){}=,\\\"'\x00\xff\x80 \t\np0123456789eEkKmM-";
    for round in 0..400 {
        let mut bytes = seeds[round % seeds.len()].clone();
        // A handful of point mutations, splices and truncations.
        for _ in 0..1 + rng.below(8) {
            match rng.below(4) {
                0 if !bytes.is_empty() => {
                    let i = rng.below(bytes.len());
                    bytes[i] = noise[rng.below(noise.len())];
                }
                1 => {
                    let i = rng.below(bytes.len() + 1);
                    let j = rng.below(noise.len());
                    bytes.splice(i..i, noise[j..].iter().copied().take(1 + rng.below(6)));
                }
                2 if bytes.len() > 2 => {
                    let i = rng.below(bytes.len());
                    bytes.truncate(i);
                }
                _ => {
                    // Duplicate a random slice (makes giant lines
                    // and repeated cards).
                    if !bytes.is_empty() {
                        let i = rng.below(bytes.len());
                        let len = rng.below(64).min(bytes.len() - i);
                        let slice: Vec<u8> = bytes[i..i + len].to_vec();
                        bytes.extend_from_slice(&slice);
                    }
                }
            }
        }
        parse(String::from_utf8_lossy(&bytes).into_owned());
    }
}

/// B-source expressions (the `inpptree.c` front end): the probe
/// decks' cards, mutated with the expression alphabet, both through
/// the deck parser and through `parse_expr` directly.
#[test]
fn mutated_expressions_never_panic() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/regress");
    let mut seeds: Vec<Vec<u8>> = Vec::new();
    for e in std::fs::read_dir(&dir).expect("corpus present").flatten() {
        let p = e.path();
        if p.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("bsrc_"))
        {
            let text = std::fs::read_to_string(&p).unwrap();
            for line in text.lines().filter(|l| l.starts_with('b')) {
                seeds.push(line.as_bytes().to_vec());
            }
        }
    }
    assert!(!seeds.is_empty());
    let mut rng = Rng(0x0e70_5eed_b5c0_0001);
    let alphabet: &[u8] = b"()+-*/^,= \tvViI0123456789.eEkKmM{}$;\x00\xffabcdefgx#:";
    for round in 0..2000 {
        let mut bytes = seeds[round % seeds.len()].clone();
        for _ in 0..1 + rng.below(6) {
            match rng.below(3) {
                0 if !bytes.is_empty() => {
                    let i = rng.below(bytes.len());
                    bytes[i] = alphabet[rng.below(alphabet.len())];
                }
                1 => {
                    let i = rng.below(bytes.len() + 1);
                    let j = rng.below(alphabet.len());
                    bytes.splice(i..i, alphabet[j..].iter().copied().take(1 + rng.below(4)));
                }
                _ if bytes.len() > 2 => {
                    let i = rng.below(bytes.len());
                    bytes.truncate(i);
                }
                _ => {}
            }
        }
        let card = String::from_utf8_lossy(&bytes).into_owned();
        parse(format!("t\n{card}\nr1 1 0 1k\n.end\n"));
        // The expression part alone (after the first `=`).
        let expr = card.split_once('=').map_or(card.as_str(), |(_, e)| e);
        let _ = dune_netlist::parse_expr(expr);
    }
}

/// Parameter expressions (`.param`, `{…}`, `'…'`, bare after `=`):
/// PDK-shaped seeds mutated with the language's alphabet, on `.param`
/// cards, element and model values.
#[test]
fn mutated_parameter_expressions_never_panic() {
    let seeds: &[&str] = &[
        ".param a=1 b='a*2' c={b+1}, d=max(a, b) e=agauss(1,0.1,3)",
        ".param mc=0 x={ mc*agauss(0, 1e-9, 3) + sqrt(abs(-2))*1.5u }",
        ".param p=(1>0) ? 2 : 3 q='p==2 && !0 || p!=3 ? -2^2 : 2**-1'",
        "r1 1 0 rm l={ max(1u, 2*p) } w='1u*(p>1?2:1)' m=p",
        ".model rm r rsh=p*2 tc1 = 'p<>3' tc2={\"-p/2\"} dw=pwr(-2,3)",
        "v1 1 0 dc { nint(2.5)+int(-2.7)+floor(1.1k)%7+ternary_fcn(1,2,3) }",
        ".param z={1/0} y={1/z} t={temper} f={nosuch(1)} g={1?0?1:2:3}",
    ];
    let mut rng = Rng(0x9a7a_5eed_e4e5_0001);
    let alphabet: &[u8] = b"(){}'\"+-*/%^!=<>&|?:,. \t0123456789eEkKmMuUxagsqrtpwbin_$;";
    for round in 0..3000 {
        let mut bytes = seeds[round % seeds.len()].as_bytes().to_vec();
        for _ in 0..1 + rng.below(6) {
            match rng.below(4) {
                0 if !bytes.is_empty() => {
                    let i = rng.below(bytes.len());
                    bytes[i] = alphabet[rng.below(alphabet.len())];
                }
                1 => {
                    let i = rng.below(bytes.len() + 1);
                    let j = rng.below(alphabet.len());
                    bytes.splice(i..i, alphabet[j..].iter().copied().take(1 + rng.below(4)));
                }
                2 if bytes.len() > 2 => {
                    let i = rng.below(bytes.len());
                    bytes.truncate(i);
                }
                _ => {
                    if !bytes.is_empty() {
                        let i = rng.below(bytes.len());
                        let len = rng.below(32).min(bytes.len() - i);
                        let slice: Vec<u8> = bytes[i..i + len].to_vec();
                        let at = rng.below(bytes.len() + 1);
                        bytes.splice(at..at, slice);
                    }
                }
            }
        }
        let card = String::from_utf8_lossy(&bytes).into_owned();
        parse(format!(
            "t\n.param p=2 mc=0\n{card}\nv9 9 0 {{p}}\n.model rx r rsh=p\n.end\n"
        ));
    }
}

#[test]
fn pure_noise_never_panics() {
    let mut rng = Rng(0xdead_beef_0bad_f00d);
    for _ in 0..200 {
        let len = rng.below(600);
        let bytes: Vec<u8> = (0..len).map(|_| (rng.next() & 0xff) as u8).collect();
        parse(String::from_utf8_lossy(&bytes).into_owned());
    }
}

#[test]
fn pathological_shapes_terminate() {
    // Deep unary chains in .param expressions.
    parse(format!(
        "t\n.param x={}1\nr1 1 0 {{x}}\n.end\n",
        "-".repeat(500)
    ));
    // Expression over the length cap (64 KiB), and just under it.
    parse(format!("t\n.param x={{{}1}}\n.end\n", "1+".repeat(40_000)));
    parse(format!(
        "t\n.param x={{{}1}}\nr1 1 0 {{x}}\n.end\n",
        "1+".repeat(30_000)
    ));
    // Parameter-expression nesting at and past the bound, through
    // every construct that recurses; quotes and braces as groups.
    for expr in [
        format!("{}1{}", "(".repeat(5000), ")".repeat(5000)),
        format!("{}1{}", "{".repeat(5000), "}".repeat(5000)),
        format!("{}1", "!".repeat(5000)),
        format!("{}3", "1?2:".repeat(5000)),
        format!("{}1{}", "abs(".repeat(3000), ")".repeat(3000)),
        format!("{}1{}", "'(".repeat(2000), ")'".repeat(2000)),
        vec!["2"; 20_000].join("^"),
        vec!["x"; 20_000].join("+"),
    ] {
        parse(format!(
            "t\n.param x=1\n.param y={{{expr}}}\nr1 1 0 {{{expr}}}\n.end\n"
        ));
    }
    // Unclosed delimiters swallowing the rest of a long card.
    parse(format!("t\nr1 1 0 {}\n.end\n", "{".repeat(50_000)));
    parse(format!("t\n.param {}\n.end\n", "a='".repeat(20_000)));
    // Thousands of continuations.
    parse(format!("t\nr1 1 0 1k\n{}", "+ 1\n".repeat(5000)));
    // A card with a huge number and absurd exponent.
    parse("t\nr1 1 0 9e999999999\nc1 1 0 1e-999999999\n.end\n".into());
    // Measurement cards cut at every byte.
    let meas = ".meas tran d trig v(1,2) val= 0.5 rise=last targ v(3)=v(4) fall=2 \u{e9}";
    for cut in 0..meas.len() {
        if meas.is_char_boundary(cut) {
            parse(format!("t\nr1 1 0 1k\n{}\n.end\n", &meas[..cut]));
        }
    }
    parse(format!("t\n.meas ac x when {}\n.end\n", "= ".repeat(500)));
    // B-source expressions at the parser's bounds: deep parentheses,
    // long sums, unary-minus and power chains, nested calls, and a
    // B card inside a subcircuit (the expression is renamed).
    for expr in [
        format!("{}1{}", "(".repeat(3000), ")".repeat(3000)),
        vec!["v(1)"; 12_000].join("+"),
        format!("{}1", "-".repeat(3000)),
        vec!["2"; 3000].join("^"),
        format!("{}v(1){}", "sin(".repeat(600), ")".repeat(600)),
        format!("{}v(1)", "-(".repeat(400)),
        "v(".repeat(500),
    ] {
        parse(format!("t\nb1 1 0 v={expr}\n.end\n"));
        let _ = dune_netlist::parse_expr(&expr);
    }
    parse("t\n.subckt s a b\nb1 a b i=v(a,b)*i(vx)+v(n)\nvx b 0 0\n.ends\nx1 1 2 s\n.end\n".into());
    // Many nested subcircuit scopes never closed.
    parse(format!("t\n{}.end\n", ".subckt s a b\n".repeat(200)));
}

/// Serves every path with the deck's own text, so a mutated `.lib` or
/// `.include` reads the mutated bytes again: sections, nesting,
/// cycles and the nesting limit get exercised. Paths more than four
/// directories deep are refused: a mutation like `.lib x/self.lib s`
/// otherwise names a new file at every level and fans out up to the
/// 256 MiB splice budget, which is correct but takes seconds per deck
/// in a debug build (`deck.rs` tests that budget on its own).
struct SelfFs(String);

impl dune_netlist::IncludeLoader for SelfFs {
    fn load(&mut self, path: &Path, limit: u64) -> Result<String, String> {
        if path.components().count() > 4 || path.to_string_lossy().contains("missing") {
            return Err("no such file".into());
        }
        if self.0.len() as u64 > limit {
            return Err("over budget".into());
        }
        Ok(self.0.clone())
    }
}

#[test]
fn mutated_library_decks_never_panic() {
    let seeds: &[&str] = &[
        "t\n.lib self.lib s\n.lib s\nr1 1 0 1\n.lib 'self.lib' t\n.endl s\n.lib t\n.param a=1\n.endl\n",
        "t\n  .LIB \"x/../self.lib\" S\n.inc 'missing.inc'\n.lib s\n.include self.lib\n.endl\n",
        "t\n.lib a.lib s\n.lib s\n.lib b.lib s\n.lib b.lib s\n.lib b.lib s\n.endl\n.endl x\n",
        "t\n.endl\n.lib\n.lib only\nr1 1 0 1\n.endl\n.lib 'unterminated s\n",
    ];
    let mut rng = Rng(0x11b5_eed5_0000_0001);
    let alphabet: &[u8] = b".lLiInNcCeEdDbB '\"/\\\n\t;$s+x0";
    for round in 0..600 {
        let mut bytes = seeds[round % seeds.len()].as_bytes().to_vec();
        for _ in 0..1 + rng.below(6) {
            match rng.below(4) {
                0 if !bytes.is_empty() => {
                    let i = rng.below(bytes.len());
                    bytes[i] = alphabet[rng.below(alphabet.len())];
                }
                1 => {
                    let i = rng.below(bytes.len() + 1);
                    let j = rng.below(alphabet.len());
                    bytes.splice(i..i, alphabet[j..].iter().copied().take(1 + rng.below(4)));
                }
                2 if bytes.len() > 2 => {
                    let i = rng.below(bytes.len());
                    bytes.truncate(i);
                }
                _ => {
                    // Repeat a line: more requests for the same section.
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    if let Some(line) = text.lines().nth(rng.below(8)) {
                        let line = format!("{line}\n");
                        bytes.extend(line.repeat(1 + rng.below(8)).into_bytes());
                    }
                }
            }
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let _ = parse_with_loader(Path::new("self.lib"), text.clone(), &mut SelfFs(text));
    }
}

/// libFuzzer timeout of 2026-11-14 (13 s before the reader charged
/// every entry into a file or section): `.lib`/`.inc` lines naming
/// the input itself under new spellings fan out within the nesting
/// limit while splicing next to nothing. Served the way the fuzz
/// target serves it.
#[test]
fn fuzz_timeout_lib_fan_out_is_fast() {
    struct FuzzFs(String);
    impl dune_netlist::IncludeLoader for FuzzFs {
        fn load(&mut self, path: &Path, limit: u64) -> Result<String, String> {
            if path.components().count() > 4 || self.0.len() as u64 > limit {
                return Err("refused under fuzzing".into());
            }
            Ok(self.0.clone())
        }
    }
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fuzz/lib-fanout-timeout.bin");
    let text = String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned();
    let t = std::time::Instant::now();
    let _ = parse_with_loader(Path::new("fuzz.lib"), text.clone(), &mut FuzzFs(text));
    assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
}

/// The two libFuzzer timeouts of 2026-11-14 in `subckt::Expander::emit`
/// (`tests/fixtures/fuzz/emit-timeout-{1,2}.bin`, 13 s and >10 s under
/// the sanitizers; 0.25 s and 0.8 s in release): mangled corpus decks
/// whose subcircuits call themselves through a fan-out, so the
/// expansion ran to the 2²⁰ budget — 190 000 and 660 000 elements
/// with names up to 64 levels deep, and one diagnostic per path
/// (665 000). Recursion is now refused at the first repeated
/// definition, the budget charges the bytes of emitted names, and the
/// expansion reports each distinct problem once.
fn emit_timeout(fixture: &str) {
    struct FuzzFs(String);
    impl dune_netlist::IncludeLoader for FuzzFs {
        fn load(&mut self, path: &Path, limit: u64) -> Result<String, String> {
            if path.components().count() > 4 || self.0.len() as u64 > limit {
                return Err("refused under fuzzing".into());
            }
            Ok(self.0.clone())
        }
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fuzz")
        .join(fixture);
    let text = String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned();
    let t = std::time::Instant::now();
    let a = parse_with_loader(Path::new("fuzz.cir"), text.clone(), &mut NoFs);
    let b = parse_with_loader(Path::new("fuzz.lib"), text.clone(), &mut FuzzFs(text));
    assert!(t.elapsed().as_secs() < 2, "{:?}", t.elapsed());
    for out in [a, b] {
        assert!(
            out.netlist.elements.len() < 10_000,
            "{}",
            out.netlist.elements.len()
        );
        assert!(out.diags.len() < 2_000, "{}", out.diags.len());
    }
}

#[test]
fn fuzz_timeout_recursive_fan_out_1() {
    emit_timeout("emit-timeout-1.bin");
}

#[test]
fn fuzz_timeout_recursive_fan_out_2() {
    emit_timeout("emit-timeout-2.bin");
}

/// A non-recursive subcircuit bomb: 40 distinct levels, each calling
/// the next twice through long instance names, plus a parameter and
/// a local model at every level. The work budget (which charges name
/// bytes, cards parsed per instance and models evaluated) stops it.
#[test]
fn deep_parameterised_bomb_is_bounded() {
    let mut deck = String::from("bomb\n");
    let long = "x".repeat(200);
    for i in 0..40 {
        deck.push_str(&format!(
            ".subckt s{i} a w=1\n.param q={{w*2}}\n.model rm{i} r rsh={{q}}\n\
             {long}a a s{n} w={{q}}\n{long}b b s{n} w={{q+1}}\nr1 a b rm{i} l=1 w=1\n.ends\n",
            n = i + 1
        ));
    }
    deck.push_str(".subckt s40 a w=1\nr1 a 0 {w}\n.ends\nx1 1 s0\n.end\n");
    let t = std::time::Instant::now();
    let out = parse_with_loader(Path::new("bomb.cir"), deck, &mut NoFs);
    assert!(t.elapsed().as_secs() < 10, "{:?}", t.elapsed());
    assert!(
        out.diags
            .iter()
            .any(|d| d.message.contains("subcircuit expansion exceeds")),
        "{:?}",
        &out.diags[..out.diags.len().min(5)]
    );
}
