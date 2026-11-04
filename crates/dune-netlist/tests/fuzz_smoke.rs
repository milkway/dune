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
    // Expression over the length cap.
    parse(format!("t\n.param x={}\n.end\n", "1+".repeat(400)));
    // Thousands of continuations.
    parse(format!("t\nr1 1 0 1k\n{}", "+ 1\n".repeat(5000)));
    // A card with a huge number and absurd exponent.
    parse("t\nr1 1 0 9e999999999\nc1 1 0 1e-999999999\n.end\n".into());
    // Many nested subcircuit scopes never closed.
    parse(format!("t\n{}.end\n", ".subckt s a b\n".repeat(200)));
}
