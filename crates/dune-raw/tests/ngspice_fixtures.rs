//! Integration tests against rawfiles written by the real oracle
//! (ngspice-47; see `tests/data/README.md` for provenance).

use dune_raw::{Data, Format, Rawfile, VarType};

const ASCII: &[u8] = include_bytes!("data/rc_ascii.raw");
const BINARY: &[u8] = include_bytes!("data/rc_binary.raw");

fn check_rc(raw: &Rawfile) {
    assert_eq!(raw.plots.len(), 3, "AC + OP + TRAN");

    let ac = &raw.plots[0];
    assert_eq!(ac.name, "AC Analysis");
    assert_eq!(ac.title, "rc lowpass test fixture");
    assert!(ac.is_complex());
    assert_eq!(ac.num_points(), 13);
    assert_eq!(ac.variables.len(), 4);
    assert_eq!(ac.scale().unwrap().name, "frequency");
    assert_eq!(ac.scale().unwrap().vtype, VarType::Frequency);
    let f = ac.complex_values("frequency").unwrap();
    assert_eq!(f[0].re, 1e3);
    assert!((f[12].re - 1e6).abs() < 1e-3);
    // |v(out)| at the last decade is attenuated, input is not.
    let vin = ac.complex_values("v(in)").unwrap();
    let vout = ac.complex_values("v(out)").unwrap();
    assert_eq!(vin[0].re, 1.0);
    assert!(vout[12].norm() < vin[12].norm());

    let op = &raw.plots[1];
    assert_eq!(op.name, "Operating Point");
    assert!(!op.is_complex());
    assert_eq!(op.num_points(), 1);
    assert_eq!(op.real_values("v(in)").unwrap(), &[1.0]);
    assert_eq!(op.real_values("v(out)").unwrap(), &[1.0]);
    assert_eq!(op.real_values("i(v1)").unwrap(), &[0.0]);

    let tran = &raw.plots[2];
    assert_eq!(tran.name, "Transient Analysis");
    assert_eq!(tran.num_points(), 81);
    assert_eq!(tran.scale().unwrap().vtype, VarType::Time);
    let t = tran.real_values("time").unwrap();
    assert_eq!(t[0], 0.0);
    assert!(t.windows(2).all(|w| w[0] < w[1]), "time is increasing");
}

#[test]
fn reads_ngspice_ascii() {
    let raw = Rawfile::parse(ASCII).expect("parse ascii fixture");
    check_rc(&raw);
}

#[test]
fn reads_ngspice_binary() {
    let raw = Rawfile::parse(BINARY).expect("parse binary fixture");
    check_rc(&raw);
}

#[test]
fn ascii_and_binary_fixtures_agree() {
    // Same deck, same ngspice run parameters: data must match exactly
    // (ASCII carries 15 significant decimals; the handful of ULPs that
    // could differ don't, in practice, for this deck).
    let a = Rawfile::parse(ASCII).unwrap();
    let b = Rawfile::parse(BINARY).unwrap();
    assert_eq!(a.plots.len(), b.plots.len());
    for (pa, pb) in a.plots.iter().zip(&b.plots) {
        assert_eq!(pa.name, pb.name);
        assert_eq!(pa.variables, pb.variables);
        match (&pa.data, &pb.data) {
            (Data::Real(ca), Data::Real(cb)) => {
                for (va, vb) in ca.iter().zip(cb) {
                    for (x, y) in va.iter().zip(vb) {
                        assert!(
                            (x - y).abs() <= 1e-15 * x.abs().max(y.abs()),
                            "{x} vs {y} in {}",
                            pa.name
                        );
                    }
                }
            }
            (Data::Complex(ca), Data::Complex(cb)) => {
                for (va, vb) in ca.iter().zip(cb) {
                    for (x, y) in va.iter().zip(vb) {
                        assert!((x - y).norm() <= 1e-15 * x.norm().max(y.norm()));
                    }
                }
            }
            _ => panic!("real/complex mismatch in {}", pa.name),
        }
    }
}

#[test]
fn roundtrip_ascii() {
    let raw = Rawfile::parse(ASCII).unwrap();
    let written = raw.to_bytes(Format::Ascii);
    let again = Rawfile::parse(&written).expect("parse own ascii output");
    assert_eq!(raw.plots, again.plots);
}

#[test]
fn roundtrip_binary() {
    let raw = Rawfile::parse(BINARY).unwrap();
    let written = raw.to_bytes(Format::Binary);
    let again = Rawfile::parse(&written).expect("parse own binary output");
    assert_eq!(raw.plots, again.plots);
}

#[test]
fn roundtrip_across_formats_is_lossless_for_binary_source() {
    // binary -> ascii -> parse: 15 decimals round-trip f64 exactly
    // (17 would be the guaranteed bound; SPICE uses 15 and the fixture
    // values survive it — asserted here so a regression is visible).
    let raw = Rawfile::parse(BINARY).unwrap();
    let ascii = raw.to_bytes(Format::Ascii);
    let again = Rawfile::parse(&ascii).unwrap();
    for (pa, pb) in raw.plots.iter().zip(&again.plots) {
        match (&pa.data, &pb.data) {
            (Data::Real(ca), Data::Real(cb)) => {
                for (va, vb) in ca.iter().zip(cb) {
                    for (x, y) in va.iter().zip(vb) {
                        assert!((x - y).abs() <= 1e-15 * x.abs().max(1.0));
                    }
                }
            }
            (Data::Complex(ca), Data::Complex(cb)) => {
                for (va, vb) in ca.iter().zip(cb) {
                    for (x, y) in va.iter().zip(vb) {
                        assert!((x - y).norm() <= 1e-15 * x.norm().max(1.0));
                    }
                }
            }
            _ => panic!("real/complex mismatch"),
        }
    }
}

#[test]
fn hostile_input_never_panics() {
    // Truncations of a valid file and assorted junk must produce Err or
    // a lossy Ok, never a panic (project principle 4).
    for cut in (0..ASCII.len()).step_by(997) {
        let _ = Rawfile::parse(&ASCII[..cut]);
    }
    for cut in (0..BINARY.len()).step_by(499) {
        let _ = Rawfile::parse(&BINARY[..cut]);
    }
    let junk: &[&[u8]] = &[
        b"",
        b"\n\n\n",
        b"garbage\n",
        b"Title: t\nPlotname: p\nFlags: real\nNo. Variables: 1\nNo. Points: 2\nValues:\n",
        b"Plotname: p\nNo. Variables: 1000000\nNo. Points: 9999999999\nVariables:\n",
        b"Title: \xff\xfe\nPlotname: p\n",
        b"No. Points: -1\n",
        b"Values:\n1 2 3\n",
        // dims= product overflows usize: must not panic (debug) nor
        // wrap into a bogus length (release).
        b"Plotname: p\nFlags: real\nNo. Variables: 1\nNo. Points: 1\n\
          Variables:\n\t0\tv\tvoltage dims=4294967296,4294967296,4294967296\n\
          Values:\n0\t1.0\n",
        b"Plotname: p\nNo. Points: 1\nDimensions: 4294967296,4294967296,4294967296\n",
    ];
    for j in junk {
        let _ = Rawfile::parse(j);
    }
}

#[test]
fn hostile_counts_fail_fast() {
    use std::time::Instant;
    // A huge No. Points over an empty Binary: section must be rejected
    // up front, not spun over point by point.
    let spin = b"Plotname: p\nFlags: real unpadded\nNo. Variables: 1\nNo. Points: 9999999999\n\
                 Variables:\n\t0\tv\tvoltage dims=0\nBinary:\n";
    let t = Instant::now();
    let _ = Rawfile::parse(spin); // empty plot or error; must not spin
    assert!(t.elapsed().as_secs() < 1, "parser spun on hostile count");

    // Unpadded with one short vector: points past its length consume no
    // bytes, so the loop must stop at the longest real length, not at a
    // hostile No. Points.
    let spin2 = b"Plotname: p\nFlags: real unpadded\nNo. Variables: 1\nNo. Points: 9999999999\n\
                  Variables:\n\t0\tv\tvoltage dims=1\nBinary:\n\0\0\0\0\0\0\0\0";
    let t = Instant::now();
    let _ = Rawfile::parse(spin2);
    assert!(t.elapsed().as_secs() < 1, "parser spun past vector lengths");

    // Huge declared counts with a tiny body must not allocate huge
    // columns up front (the reserve is bounded by the input size).
    let tiny = b"Plotname: p\nFlags: real\nNo. Variables: 1\nNo. Points: 9999999\n\
                 Variables:\n\t0\tv\tvoltage\nValues:\n0\t1.0\n";
    assert!(Rawfile::parse(tiny).is_err(), "truncated values must fail");
}
