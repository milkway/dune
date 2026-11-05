//! `.pz` — pole-zero analysis (`ckt/pzan.c`, `ckt/cktpzset.c`,
//! `ckt/cktpzld.c`, `ckt/cktpzstr.c`, `ni/nipzmeth.c`). The roots of
//! the transfer function's denominator (poles) and numerator (zeros)
//! are hunted on the complex plane as zeros of the MNA determinant
//! after the column surgery of `CKTpzSetup`: a "zeroin" variant over
//! the real axis with Muller's method for complex pairs, deflation
//! of found roots, and the block-normalized (mantissa, base-2
//! exponent) arithmetic the 3f5 uses throughout.

use dune_raw::{Data, VarType};
use num_complex::Complex64;

use crate::circuit::Circuit;
use crate::engine::{Engine, PzSetup};
use crate::error::Error;
use crate::matrix::{AnKind, InitMode, Var};
use crate::options::Options;
use crate::solution::Solution;

/// `.pz` input coupling (`CUR`/`VOL`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PzInput {
    Current,
    Voltage,
}

/// What to find (`POL`/`ZER`/`PZ`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PzWhich {
    Poles,
    Zeros,
    Both,
}

/// Result of a `.pz` analysis.
#[derive(Clone, Debug)]
pub struct Pz {
    pub solution: Solution,
    pub notes: Vec<String>,
}

impl Circuit {
    /// Run `.pz NODE1 NODE2 NODE3 NODE4 CUR|VOL POL|ZER|PZ`.
    pub fn pz(
        &self,
        opts: &Options,
        nodes: [&str; 4],
        input: PzInput,
        which: PzWhich,
    ) -> Result<Pz, Error> {
        let built = self.build(opts)?;
        let notes = built.notes.clone();
        let mut engine = Engine::new(built);

        let var_of = |name: &str| -> Result<Var, Error> {
            let name = name.to_ascii_lowercase();
            if name == "0" {
                return Ok(0);
            }
            engine
                .vars
                .names
                .iter()
                .position(|n| *n == name)
                .ok_or_else(|| Error::Build(format!(".pz names unknown node `{name}`")))
        };
        let in_pos = var_of(nodes[0])?;
        let in_neg = var_of(nodes[1])?;
        let out_pos = var_of(nodes[2])?;
        let out_neg = var_of(nodes[3])?;

        // `PZinit` error checks.
        if in_pos == in_neg {
            return Err(Error::Build(".pz: input is shorted".into()));
        }
        if out_pos == out_neg {
            return Err(Error::Build(".pz: output is shorted".into()));
        }
        if input == PzInput::Voltage {
            if in_pos == out_pos && in_neg == out_neg {
                return Err(Error::Build(".pz: transfer function is unity".into()));
            }
            if in_pos == out_neg && in_neg == out_pos {
                return Err(Error::Build(".pz: transfer function is -1".into()));
            }
        }

        // Operating point + small-signal parameter pass (`PZan`).
        engine.set_kind(AnKind::DcOp, false);
        engine.op(opts)?;
        engine.load_all_public(InitMode::Smsig, opts);

        let vol = input == PzInput::Voltage;
        let mut poles: Vec<(Complex64, i32)> = Vec::new();
        let mut zeros: Vec<(Complex64, i32)> = Vec::new();
        if which != PzWhich::Zeros {
            let mut setup = engine.pz_setup(in_pos, in_neg, out_pos, out_neg, vol, false);
            poles = find_zeros(&mut engine, &mut setup)?;
        }
        if which != PzWhich::Poles {
            let mut setup = engine.pz_setup(in_pos, in_neg, out_pos, out_neg, vol, true);
            zeros = find_zeros(&mut engine, &mut setup)?;
        }

        // `PZpost`: expand multiplicity and conjugate pairs, poles
        // first, with the ngspice rawfile names.
        let mut variables = Vec::new();
        let mut columns: Vec<Vec<Complex64>> = Vec::new();
        let mut emit = |list: &[(Complex64, i32)], label: &str| {
            let mut k = 0usize;
            for &(s, mult) in list {
                for _ in 0..mult {
                    k += 1;
                    variables.push((format!("v({label}({k}))"), VarType::Voltage));
                    columns.push(vec![s]);
                    if s.im != 0.0 {
                        k += 1;
                        variables.push((format!("v({label}({k}))"), VarType::Voltage));
                        columns.push(vec![s.conj()]);
                    }
                }
            }
        };
        emit(&poles, "pole");
        emit(&zeros, "zero");
        if variables.is_empty() {
            return Err(Error::Build(".pz found no poles or zeros".into()));
        }

        Ok(Pz {
            solution: Solution {
                plotname: "Pole-Zero Analysis".to_string(),
                variables,
                data: Data::Complex(columns),
                scale: None,
            },
            notes,
        })
    }
}

// ---------------------------------------------------------------
// The root finder (`cktpzstr.c` / `nipzmeth.c`), ported with its
// heuristics intact. The 3f5 keeps trials in a doubly linked list
// ordered by `s`; here the list nodes live in a Vec and links are
// indices.
// ---------------------------------------------------------------

const NITER_LIM: usize = 200;

// Strategies (same numbering as the C so `< GUESS` keeps meaning).
const SHIFT_LEFT: i32 = 2;
const SHIFT_RIGHT: i32 = 3;
const SKIP_LEFT: i32 = 4;
const SKIP_RIGHT: i32 = 5;
const INIT: i32 = 6;
const GUESS: i32 = 7;
const SPLIT_LEFT: i32 = 8;
const SPLIT_RIGHT: i32 = 9;
const MULLER: i32 = 10;
const SYM: i32 = 11;
const SYM2: i32 = 12;
const COMPLEX_INIT: i32 = 13;
const COMPLEX_GUESS: i32 = 14;

// Moves.
const NEAR_LEFT: i32 = 4;
const MID_LEFT: i32 = 5;
const FAR_LEFT: i32 = 6;
const NEAR_RIGHT: i32 = 7;
const FAR_RIGHT: i32 = 8;
const MID_RIGHT: i32 = 9;

const ISAROOT: u8 = 2;
#[allow(dead_code)]
const ISAREPEAT: u8 = 4;
const ISAMINIMA: u8 = 16;

fn sgn(x: f64) -> i32 {
    if x < 0.0 {
        -1
    } else if x == 0.0 {
        0
    } else {
        1
    }
}

/// `zaddeq`: add two block-normalized reals.
fn zaddeq(x: f64, xmag: i32, y: f64, ymag: i32) -> (f64, i32) {
    let (mut x, mut y) = (x, y);
    let amag;
    if xmag > ymag {
        amag = xmag;
        if xmag > 50 + ymag {
            y = 0.0;
        } else {
            let mut d = xmag - ymag;
            while d > 0 {
                y /= 2.0;
                d -= 1;
            }
        }
    } else {
        amag = ymag;
        if ymag > 50 + xmag {
            x = 0.0;
        } else {
            let mut d = ymag - xmag;
            while d > 0 {
                x /= 2.0;
                d -= 1;
            }
        }
    }
    let mut a = x + y;
    let mut m = amag;
    if a == 0.0 {
        m = 0;
    } else {
        while a.abs() > 1.0 {
            a /= 2.0;
            m += 1;
        }
        while a.abs() < 0.5 {
            a *= 2.0;
            m -= 1;
        }
    }
    (a, m)
}

/// `R_NORM`.
fn r_norm(a: &mut f64, b: &mut i32) {
    if *a == 0.0 {
        *b = 0;
    } else {
        while a.abs() > 1.0 {
            *b += 1;
            *a /= 2.0;
        }
        while a.abs() < 0.5 {
            *b -= 1;
            *a *= 2.0;
        }
    }
}

/// `C_NORM`.
fn c_norm(a: &mut Complex64, b: &mut i32) {
    if a.re == 0.0 && a.im == 0.0 {
        *b = 0;
    } else {
        while a.re.abs() > 1.0 || a.im.abs() > 1.0 {
            *b += 1;
            a.re /= 2.0;
            a.im /= 2.0;
        }
        while a.re.abs() <= 0.5 && a.im.abs() <= 0.5 {
            *b -= 1;
            a.re *= 2.0;
            a.im *= 2.0;
        }
    }
}

/// `C_SQRT`.
fn c_sqrt(a: &mut Complex64) {
    if a.im == 0.0 {
        if a.re < 0.0 {
            a.im = (-a.re).sqrt();
            a.re = 0.0;
        } else {
            a.re = a.re.sqrt();
            a.im = 0.0;
        }
    } else {
        let mag = (a.re * a.re + a.im * a.im).sqrt();
        let half = (mag - a.re) / 2.0;
        if half <= 0.0 {
            a.re = mag.sqrt();
            a.im /= 2.0 * a.re;
        } else {
            let half = half.sqrt();
            a.re = a.im / (2.0 * half);
            a.im = half;
        }
    }
}

#[derive(Clone)]
struct Trial {
    s: Complex64,
    f_raw: Complex64,
    mag_raw: i32,
    f_def: Complex64,
    mag_def: i32,
    multiplicity: i32,
    count: i32,
    #[allow(dead_code)]
    seq: i32,
    flags: u8,
    prev: Option<usize>,
    next: Option<usize>,
    dead: bool,
}

struct Finder<'a> {
    eng: &'a mut Engine,
    setup: &'a mut PzSetup,
    nodes: Vec<Trial>,
    head: Option<usize>,
    zero_trial: Option<usize>,
    nzeros: usize,
    nflat: i32,
    max_zeros: usize,
    seq: i32,
    guess_param: f64,
    high_guess: f64,
    low_guess: f64,
    last_move: i32,
    consec_moves: i32,
    niter: usize,
    ntrials: i32,
    trapped: i32,
    aberr: i32,
    nipzk: f64,
    nipzk_mag: i32,
}

/// A trial candidate before it enters the list.
struct Cand {
    s: Complex64,
    seq: i32,
}

type Set = [Option<usize>; 3];

/// `CKTpzFindZeros`: returns the roots found, ascending in `s`,
/// with their multiplicities.
fn find_zeros(eng: &mut Engine, setup: &mut PzSetup) -> Result<Vec<(Complex64, i32)>, Error> {
    let max_zeros = eng.vars.len() - 1;
    let mut f = Finder {
        eng,
        setup,
        nodes: Vec::new(),
        head: None,
        zero_trial: None,
        nzeros: 0,
        nflat: 0,
        max_zeros,
        seq: 1,
        guess_param: 0.0,
        high_guess: -1.0,
        low_guess: 1.0,
        last_move: 0,
        consec_moves: 0,
        niter: 0,
        ntrials: 0,
        trapped: 0,
        aberr: 0,
        nipzk: 0.0,
        nipzk_mag: 0,
    };

    let mut set: Set = [None; 3];
    f.reset(&mut set);

    loop {
        let mut strat;
        loop {
            strat = f.strat(&set);
            if !(strat < GUESS && f.trapped == 0) {
                break;
            }
            if !f.step(strat, &mut set) {
                strat = GUESS;
                break;
            }
        }

        f.niter += 1;

        let cand = f.eval(strat, &set)?;
        let idx = f.run_trial(cand, &set)?;

        if f.nodes[idx].flags & ISAROOT != 0 {
            f.verify(idx);
            f.niter = 0;
            f.reset(&mut set);
        } else if f.nodes[idx].flags & ISAMINIMA != 0 {
            set[0] = None;
            set[1] = Some(idx);
            set[2] = None;
        } else {
            f.update_set(&mut set, idx);
        }

        // Loop exit (the C's do/while condition).
        let spread_ok = f.high_guess - f.low_guess < 1e35;
        let window_ok = match (set[0], set[2]) {
            (Some(a), Some(c)) => f.trapped != 0 || f.nodes[c].s.re - f.nodes[a].s.re < 1e22,
            _ => true,
        };
        if !(spread_ok && f.nzeros < f.max_zeros && f.niter < NITER_LIM && f.aberr < 3 && window_ok)
        {
            break;
        }
    }

    if f.nzeros as i32 >= f.seq - 1 {
        return Err(Error::Build(
            ".pz: the input signal is shorted on the way to the output".into(),
        ));
    }

    // `clear_trials(0)`: keep the roots, ascending.
    let mut out = Vec::new();
    let mut cur = f.head;
    while let Some(i) = cur {
        let t = &f.nodes[i];
        if !t.dead && t.flags & ISAROOT != 0 {
            out.push((t.s, t.multiplicity));
        }
        cur = t.next;
    }
    Ok(out)
}

impl Finder<'_> {
    fn t(&self, i: usize) -> &Trial {
        &self.nodes[i]
    }

    /// `pzseek`.
    fn seek(&mut self, from: Option<usize>, dir: i32) -> Option<usize> {
        self.guess_param = dir as f64;
        let mut t = from?;
        if dir == 0 && self.t(t).flags & (ISAROOT | ISAMINIMA) == 0 {
            return Some(t);
        }
        loop {
            let next = if dir >= 0 {
                self.t(t).next
            } else {
                self.t(t).prev
            };
            t = next?;
            if self.t(t).flags & (ISAROOT | ISAMINIMA) == 0 {
                return Some(t);
            }
        }
    }

    /// `CKTpzReset`.
    fn reset(&mut self, set: &mut Set) {
        self.trapped = 0;
        self.consec_moves = 0;
        set[1] = self.seek(self.zero_trial, 0);
        if set[1].is_some() {
            set[0] = self.seek(set[1], -1);
            set[2] = self.seek(set[1], 1);
        } else {
            set[0] = None;
            set[2] = None;
        }
    }

    /// `CKTpzStep`.
    fn step(&mut self, strat: i32, set: &mut Set) -> bool {
        match strat {
            INIT => {
                if set[1].is_none() {
                    set[1] = self.seek(self.zero_trial, 0);
                } else if set[2].is_none() {
                    set[2] = self.seek(set[1], 1);
                } else if set[0].is_none() {
                    set[0] = self.seek(set[1], -1);
                }
            }
            SKIP_LEFT => set[0] = self.seek(set[0], -1),
            SKIP_RIGHT => set[2] = self.seek(set[2], 1),
            SHIFT_LEFT => {
                set[2] = set[1];
                set[1] = set[0];
                set[0] = self.seek(set[0], -1);
            }
            SHIFT_RIGHT => {
                set[0] = set[1];
                set[1] = set[2];
                set[2] = self.seek(set[2], 1);
            }
            _ => {}
        }
        set[0].is_some() && set[1].is_some() && set[2].is_some()
    }

    /// `CKTpzStrat`.
    fn strat(&mut self, set: &Set) -> i32 {
        let mut new_trap = 0;
        let suggestion;

        if set[1].is_some() && self.t(set[1].unwrap()).flags & ISAMINIMA != 0 {
            suggestion = COMPLEX_INIT;
        } else if set[0].is_some() && self.t(set[0].unwrap()).s.im != 0.0 {
            if set[1].is_none() || set[2].is_none() {
                suggestion = COMPLEX_GUESS;
            } else {
                suggestion = MULLER;
            }
        } else if set[0].is_none() || set[1].is_none() || set[2].is_none() {
            suggestion = INIT;
        } else {
            let t0 = self.t(set[0].unwrap()).clone();
            let t1 = self.t(set[1].unwrap()).clone();
            let t2 = self.t(set[2].unwrap()).clone();
            let mut sug;
            if sgn(t0.f_def.re) != sgn(t1.f_def.re) {
                new_trap = 1;
                sug = SYM2;
            } else if sgn(t1.f_def.re) != sgn(t2.f_def.re) {
                new_trap = 2;
                sug = SYM2;
            } else {
                let (a, a_mag) = zaddeq(t1.f_def.re, t1.mag_def, -t0.f_def.re, t0.mag_def);
                let (b, b_mag) = zaddeq(t2.f_def.re, t2.mag_def, -t1.f_def.re, t1.mag_def);
                if self.trapped == 0 {
                    let k1 = t1.s.re - t0.s.re;
                    let k2 = t2.s.re - t1.s.re;
                    if a_mag + 10 < t0.mag_def
                        && a_mag + 10 < t1.mag_def
                        && b_mag + 10 < t1.mag_def
                        && b_mag + 10 < t2.mag_def
                    {
                        sug = if k1 > k2 { SKIP_RIGHT } else { SKIP_LEFT };
                    } else if sgn(a) != -sgn(b) {
                        if a == 0.0 {
                            sug = SKIP_LEFT;
                        } else if b == 0.0 {
                            sug = SKIP_RIGHT;
                        } else if sgn(a) == sgn(t1.f_def.re) {
                            sug = SHIFT_LEFT;
                        } else {
                            sug = SHIFT_RIGHT;
                        }
                    } else if sgn(a) == -sgn(t1.f_def.re) {
                        new_trap = 3;
                        sug = SYM;
                    } else if k1 > k2 {
                        sug = SKIP_RIGHT;
                    } else {
                        sug = SKIP_LEFT;
                    }
                } else {
                    new_trap = 3;
                    if sgn(a) != sgn(b) {
                        sug = SYM;
                    } else if a_mag > b_mag || (a_mag == b_mag && a.abs() > b.abs()) {
                        sug = SPLIT_LEFT;
                    } else {
                        sug = SPLIT_RIGHT;
                    }
                }
            }
            if self.consec_moves >= 3 && self.trapped == new_trap {
                if self.last_move == MID_LEFT || self.last_move == NEAR_RIGHT {
                    sug = SPLIT_LEFT;
                } else if self.last_move == MID_RIGHT || self.last_move == NEAR_LEFT {
                    sug = SPLIT_RIGHT;
                }
                // (the C aborts on any other move; unreachable)
                self.consec_moves = 0;
            }
            suggestion = sug;
        }

        self.trapped = new_trap;
        suggestion
    }

    /// `PZeval`.
    fn eval(&mut self, strat: i32, set: &Set) -> Result<Cand, Error> {
        let seq = self.seq;
        self.seq += 1;
        let mut s = Complex64::new(0.0, 0.0);

        match strat {
            GUESS => {
                if self.high_guess < self.low_guess {
                    self.guess_param = 0.0;
                } else if self.guess_param > 0.0 {
                    if self.high_guess > 0.0 {
                        self.guess_param = self.high_guess * 10.0;
                    } else {
                        self.guess_param = 1.0;
                    }
                } else if self.low_guess < 0.0 {
                    self.guess_param = self.low_guess * 10.0;
                } else {
                    self.guess_param = -1.0;
                }
                if self.guess_param > self.high_guess {
                    self.high_guess = self.guess_param;
                }
                if self.guess_param < self.low_guess {
                    self.low_guess = self.guess_param;
                }
                s.re = self.guess_param;
                s.im = set[1].map_or(0.0, |i| self.t(i).s.im);
            }
            SYM | SYM2 => {
                s = self.sym2(set);
                // Fix-ups for trapped intervals.
                let (s0, s1, s2) = (
                    set[0].map(|i| self.t(i).s),
                    set[1].map(|i| self.t(i).s),
                    set[2].map(|i| self.t(i).s),
                );
                if self.trapped == 1 {
                    let (a, b) = (s0.unwrap(), s1.unwrap());
                    if s.re < a.re || s.re > b.re {
                        s.re = (a.re + b.re) / 2.0;
                    }
                } else if self.trapped == 2 {
                    let (b, c) = (s1.unwrap(), s2.unwrap());
                    if s.re < b.re || s.re > c.re {
                        s.re = (b.re + c.re) / 2.0;
                    }
                } else if self.trapped == 3 {
                    let (a, b, c) = (s0.unwrap(), s1.unwrap(), s2.unwrap());
                    if s.re <= a.re || (s.re == b.re && s.im == b.im) || s.re >= c.re {
                        s.re = (a.re + c.re) / 2.0;
                        if s.re == b.re {
                            if self.last_move == MID_LEFT || self.last_move == NEAR_RIGHT {
                                s.re = (a.re + b.re) / 2.0;
                            } else {
                                s.re = (b.re + c.re) / 2.0;
                            }
                        }
                    }
                }
            }
            COMPLEX_INIT => {
                s.re = self.t(set[1].unwrap()).s.re;
                if self.nipzk != 0.0 && self.nipzk_mag > -10 {
                    while self.nipzk_mag > 0 {
                        self.nipzk *= 2.0;
                        self.nipzk_mag -= 1;
                    }
                    while self.nipzk_mag < 0 {
                        self.nipzk /= 2.0;
                        self.nipzk_mag += 1;
                    }
                    s.im = self.nipzk;
                } else {
                    s.im = 10000.0;
                }
                self.nipzk = 0.0;
                self.nipzk_mag = 0;
            }
            COMPLEX_GUESS => {
                s.re = self.t(set[0].unwrap()).s.re;
                s.im = if set[2].is_none() { 1.0e8 } else { 1.0e12 };
            }
            MULLER => s = self.muller(set),
            SPLIT_LEFT => {
                s.re = (self.t(set[0].unwrap()).s.re + 2.0 * self.t(set[1].unwrap()).s.re) / 3.0;
            }
            SPLIT_RIGHT => {
                s.re = (self.t(set[2].unwrap()).s.re + 2.0 * self.t(set[1].unwrap()).s.re) / 3.0;
            }
            _ => return Err(Error::Build(".pz: internal — unknown step type".into())),
        }

        Ok(Cand { s, seq })
    }

    /// `NIpzSym2`: quadratic (or linear) estimate on the real axis,
    /// with block-normalized arithmetic. Also records the minima
    /// radical in `nipzk` (the complex-pair seed).
    #[allow(clippy::many_single_char_names)]
    #[allow(clippy::if_same_then_else)] // branch structure kept from the C
    fn sym2(&mut self, set: &Set) -> Complex64 {
        let t0 = self.t(set[0].unwrap()).clone();
        let t1 = self.t(set[1].unwrap()).clone();
        let t2 = self.t(set[2].unwrap()).clone();

        let mut dx0 = t1.s.re - t0.s.re;
        let mut dx1 = t2.s.re - t1.s.re;
        let x0 = (t0.s.re + t1.s.re) / 2.0;
        let mut d2x = (t2.s.re - t0.s.re) / 2.0;

        let (mut a, mut a_mag) = zaddeq(t1.f_def.re, t1.mag_def, -t0.f_def.re, t0.mag_def);
        let mut tmag = 0;
        r_norm(&mut dx0, &mut tmag);
        a /= dx0;
        a_mag -= tmag;
        r_norm(&mut a, &mut a_mag);

        let (mut b, mut b_mag) = zaddeq(t2.f_def.re, t2.mag_def, -t1.f_def.re, t1.mag_def);
        tmag = 0;
        r_norm(&mut dx1, &mut tmag);
        b /= dx1;
        b_mag -= tmag;
        r_norm(&mut b, &mut b_mag);

        let (mut c, mut c_mag) = zaddeq(b, b_mag, -a, a_mag);
        tmag = 0;
        r_norm(&mut d2x, &mut tmag);
        c /= d2x;
        c_mag -= tmag;
        r_norm(&mut c, &mut c_mag);

        let mut out = Complex64::new(0.0, 0.0);

        // The C's condition contains `(b = 0.0 || c_mag < b_mag-40)`
        // — an assignment that clobbers `b` with 1.0/0.0 whenever
        // the first operand of `&&` was true. Ported literally.
        let first = a == 0.0 || c_mag < a_mag - 40;
        let second = if first {
            b = if c_mag < b_mag - 40 { 1.0 } else { 0.0 };
            b != 0.0
        } else {
            false
        };
        if c == 0.0 || (first && second) {
            // Linear path.
            if a == 0.0 {
                a = b;
                a_mag = b_mag;
            }
            if a != 0.0 {
                out.re = -t1.f_def.re / a;
                let mut m = a_mag - t1.mag_def;
                // (the C subtracts mag_def from a_mag and scales by
                // the result)
                while m > 0 {
                    out.re /= 2.0;
                    m -= 1;
                }
                while m < 0 {
                    out.re *= 2.0;
                    m += 1;
                }
                out.re += t1.s.re;
            } else {
                out.re = t1.s.re;
            }
        } else {
            // Quadratic power series about set[1].s.re.
            a /= c;
            r_norm(&mut a, &mut a_mag);
            a_mag -= c_mag;

            let mut diff = t1.s.re - x0;
            tmag = 0;
            r_norm(&mut diff, &mut tmag);
            let (na, na_mag) = zaddeq(a, a_mag, diff, tmag);
            a = na;
            a_mag = na_mag;

            let mut bq = 2.0 * t1.f_def.re / c;
            let mut bq_mag = t1.mag_def - c_mag;
            r_norm(&mut bq, &mut bq_mag);

            let mut disc = a * a;
            let mut disc_mag = 2 * a_mag;
            let (nd, nd_mag) = zaddeq(disc, disc_mag, -bq, bq_mag);
            disc = nd;
            disc_mag = nd_mag;

            // (`new_mag` is uninitialized in the C when the
            // discriminant is non-negative; 0 is the sane reading)
            let mut new_mag = 0;
            if disc < 0.0 {
                disc = -disc;
                new_mag = 1;
            }
            if disc_mag % 2 == 0 {
                disc = disc.sqrt();
            } else {
                disc = (2.0 * disc).sqrt();
                disc_mag -= 1;
            }
            disc_mag /= 2;

            if new_mag != 0 {
                if self.nipzk == 0.0 {
                    self.nipzk = disc;
                    self.nipzk_mag = disc_mag;
                }
                disc = 0.0;
                disc_mag = 0;
            }

            let (mut cc, mut cc_mag);
            if a * disc >= 0.0 {
                let r = zaddeq(a, a_mag, disc, disc_mag);
                cc = r.0;
                cc_mag = r.1;
            } else {
                let r = zaddeq(a, a_mag, -disc, disc_mag);
                cc = r.0;
                cc_mag = r.1;
            }

            // Second root = C / (first root).
            if cc != 0.0 {
                bq /= cc;
                bq_mag -= cc_mag;
            } else {
                bq = 0.0;
                // (the C's `b = 0;` typo leaves b_mag unchanged)
            }

            let (mut rb, mut rb_mag) = zaddeq(t1.s.re, 0, -bq, bq_mag);
            let r = zaddeq(t1.s.re, 0, -cc, cc_mag);
            cc = r.0;
            cc_mag = r.1;

            while rb_mag > 0 {
                rb *= 2.0;
                rb_mag -= 1;
            }
            while rb_mag < 0 {
                rb /= 2.0;
                rb_mag += 1;
            }
            while cc_mag > 0 {
                cc *= 2.0;
                cc_mag -= 1;
            }
            while cc_mag < 0 {
                cc /= 2.0;
                cc_mag += 1;
            }

            let in_range = |x: f64| x >= t0.s.re && x <= t2.s.re;
            if !in_range(rb) {
                if !in_range(cc) {
                    if self.trapped == 1 {
                        out.re = (t0.s.re + t1.s.re) / 2.0;
                    } else if self.trapped == 2 {
                        out.re = (t1.s.re + t2.s.re) / 2.0;
                    } else if self.trapped == 3 {
                        if (t1.s.re - cc).abs() < (t1.s.re - rb).abs() {
                            out.re = (t1.s.re + cc) / 2.0;
                        } else {
                            out.re = (t1.s.re + rb) / 2.0;
                        }
                    } else {
                        // "Lost numerical stability" — fall back to
                        // the midpoint rather than aborting.
                        out.re = (t0.s.re + t2.s.re) / 2.0;
                    }
                } else {
                    out.re = cc;
                }
            } else if !in_range(cc) {
                out.re = rb;
            } else if a > 0.0 {
                out.re = rb;
            } else {
                out.re = cc;
            }
        }

        out.im = 0.0;
        out
    }

    /// `NIpzMuller`.
    fn muller(&mut self, set: &Set) -> Complex64 {
        let t: Vec<Trial> = (0..3).map(|i| self.t(set[i].unwrap()).clone()).collect();

        let mut min = -999999i32;
        let mut j = 0i32;
        let mut total = 0i32;
        for ti in &t {
            if ti.f_def.re != 0.0 || ti.f_def.im != 0.0 {
                if min < ti.mag_def - 50 {
                    min = ti.mag_def - 50;
                }
                total += ti.mag_def;
                j += 1;
            }
        }
        let mut magx = if j != 0 { total / j } else { 0 };
        if magx < min {
            magx = min;
        }

        let mut scale = [1.0f64; 3];
        for i in 0..3 {
            let mut mag = t[i].mag_def - magx;
            scale[i] = 1.0;
            while mag > 0 {
                scale[i] *= 2.0;
                mag -= 1;
            }
            if mag < -90 {
                scale[i] = 0.0;
            } else {
                while mag < 0 {
                    scale[i] /= 2.0;
                    mag += 1;
                }
            }
        }

        let h0 = t[0].s - t[1].s;
        let h1 = t[1].s - t[2].s;
        // `C_DIVEQ` divides by the conjugate form — ordinary complex
        // division.
        let lambda_i = h0 / h1;
        let delta_i = lambda_i + 1.0;

        let f0 = scale[0] * t[0].f_def;
        let f1 = scale[1] * t[1].f_def;
        let f2 = scale[2] * t[2].f_def;

        let mut a = (f2 * lambda_i - f1 * delta_i + f0) * lambda_i;
        let mut b = f2 * lambda_i * lambda_i - f1 * delta_i * delta_i + f0 * (lambda_i + delta_i);
        let mut c = delta_i * f0;

        while a.re.abs() > 1.0
            || a.im.abs() > 1.0
            || b.re.abs() > 1.0
            || b.im.abs() > 1.0
            || c.re.abs() > 1.0
            || c.im.abs() > 1.0
        {
            a /= 2.0;
            b /= 2.0;
            c /= 2.0;
        }

        let mut d = b * b - 4.0 * a * c;
        c_sqrt(&mut d);

        // Maximize the denominator.
        let q = b.re * d.re + b.im * d.im;
        if q > 0.0 {
            b += d;
        } else {
            b -= d;
        }

        let lambda = c / (-0.5 * b);
        t[0].s + h0 * lambda
    }

    /// `CKTpzRunTrial`: deflation, shifting, the determinant
    /// evaluation, and insertion into the ordered list. Returns the
    /// index of the trial that now represents this point.
    fn run_trial(&mut self, cand: Cand, set: &Set) -> Result<usize, Error> {
        let mut s = cand.s;
        if s.im < 0.0 {
            s.im = -s.im;
        }

        let mut pretest = false;
        let mut shifted = false;
        let mut def_mag: i32;
        let mut def_frac: Complex64;
        let mut prev: Option<usize>;
        let mut matched: Option<usize>;
        let mut hit: Option<usize>;

        loop {
            def_mag = 0;
            def_frac = Complex64::new(1.0, 0.0);
            let was_shifted = shifted;
            shifted = false;
            prev = None;
            matched = None;
            hit = None;

            let mut cur = self.head;
            while let Some(i) = cur {
                let p = self.nodes[i].clone();
                cur = p.next;

                let diff = p.s - s;
                if diff.re < 0.0 || (diff.re == 0.0 && diff.im < 0.0) {
                    prev = Some(i);
                }

                let (abstol, reltol) = if p.flags & ISAROOT != 0 {
                    (1e-5, 1e-6)
                } else {
                    (1e-20, 1e-12)
                };

                if diff.im == 0.0 && diff.re.abs() / (p.s.re.abs() + abstol / reltol) < reltol {
                    if was_shifted || p.count >= 3 || !self.alter(&mut s, set[1], abstol, reltol) {
                        self.nodes[i].count = 0;
                        pretest = true;
                        hit = Some(i);
                        break;
                    } else {
                        self.nodes[i].count += 1;
                    }
                    shifted = true;
                    break;
                } else {
                    if self.trapped == 0 {
                        self.nodes[i].count = 0;
                    }
                    if p.flags & ISAROOT != 0 {
                        let mut diff_mag = 0i32;
                        let mut df = diff;
                        c_norm(&mut df, &mut diff_mag);
                        if df.im != 0.0 {
                            df = Complex64::new(df.re * df.re + df.im * df.im, 0.0);
                            diff_mag *= 2;
                        }
                        c_norm(&mut df, &mut diff_mag);
                        for _ in 0..p.multiplicity {
                            def_frac *= df;
                            def_mag += diff_mag;
                            c_norm(&mut def_frac, &mut def_mag);
                        }
                    } else if matched.is_none() {
                        matched = Some(i);
                    }
                }
            }

            if !shifted {
                break;
            }
        }

        let mut singular = false;
        if pretest {
            let i = hit.unwrap();
            let hflags = self.nodes[i].flags;
            if hflags & ISAROOT == 0
                && self.trapped == 3
                && self.nipzk != 0.0
                && self.nipzk_mag > -10
            {
                self.nodes[i].flags |= ISAMINIMA;
                return Ok(i);
            } else if hflags & ISAROOT != 0 {
                self.nodes[i].flags |= ISAREPEAT;
                self.nodes[i].multiplicity += 1;
                return Ok(i);
            } else {
                // Regular zero, as precise as we can get it.
                singular = true;
            }
        }

        // Evaluate the determinant (unless the pretest already
        // declared the point a root).
        let mut f_raw = Complex64::new(0.0, 0.0);
        let mut mag_raw = 0i32;
        if !pretest {
            let (det, mag) = self.eng.pz_det(self.setup, s)?;
            f_raw = det;
            mag_raw = mag;
        }

        let mut trial = Trial {
            s,
            f_raw,
            mag_raw,
            f_def: f_raw,
            mag_def: mag_raw,
            multiplicity: 0,
            count: 0,
            seq: cand.seq,
            flags: 0,
            prev: None,
            next: None,
            dead: false,
        };

        if singular || (trial.f_raw.re == 0.0 && trial.f_raw.im == 0.0) {
            trial.f_raw = Complex64::new(0.0, 0.0);
            trial.mag_raw = 0;
            trial.f_def = Complex64::new(0.0, 0.0);
            trial.mag_def = 0;
            trial.flags = ISAROOT;
        } else {
            // Deflate by the roots found so far.
            trial.f_def = trial.f_raw / def_frac;
            trial.mag_def = trial.mag_raw - def_mag;
            c_norm(&mut trial.f_def, &mut trial.mag_def);
        }

        // Link into the ordered list.
        let idx = self.nodes.len();
        let is_root = trial.flags & ISAROOT != 0;
        match prev {
            Some(p) => {
                trial.next = self.nodes[p].next;
                trial.prev = Some(p);
                if let Some(nx) = self.nodes[p].next {
                    self.nodes[nx].prev = Some(idx);
                }
                self.nodes.push(trial);
                self.nodes[p].next = Some(idx);
            }
            None => {
                trial.next = self.head;
                trial.prev = None;
                if let Some(h) = self.head {
                    self.nodes[h].prev = Some(idx);
                } else {
                    self.zero_trial = Some(idx);
                }
                self.nodes.push(trial);
                self.head = Some(idx);
            }
        }
        self.ntrials += 1;

        if !is_root {
            if let Some(m) = matched {
                self.check_flat(m, idx);
            } else {
                self.nflat = 1;
            }
        }

        Ok(idx)
    }

    /// `CKTpzVerify`: count the root, deflate the surviving trials
    /// and drop the ones it absorbs.
    fn verify(&mut self, root: usize) {
        self.nzeros += 1;
        if self.nodes[root].s.im != 0.0 {
            self.nzeros += 1;
        }
        self.nflat = 0;

        if self.nodes[root].multiplicity == 0 {
            self.nodes[root].flags |= ISAROOT;
            self.nodes[root].multiplicity = 1;
        }

        let root_s = self.nodes[root].s;
        let mut prev: Option<usize> = None;
        let mut cur = self.head;
        while let Some(i) = cur {
            let next = self.nodes[i].next;
            if self.nodes[i].flags & ISAROOT != 0 {
                prev = Some(i);
                cur = next;
                continue;
            }

            let mut diff = root_s - self.nodes[i].s;
            if root_s.im != 0.0 {
                diff = Complex64::new(diff.re * diff.re + diff.im * diff.im, 0.0);
            }
            let tdiff = diff.re;
            if diff.re != 0.0 {
                let mut diff_mag = 0i32;
                c_norm(&mut diff, &mut diff_mag);
                diff_mag = -diff_mag;
                let f = self.nodes[i].f_def / diff;
                let mut f = f;
                let mut m = diff_mag;
                c_norm(&mut f, &mut m);
                self.nodes[i].f_def = f;
                self.nodes[i].mag_def += m;
            }

            if self.nodes[i].s.im != 0.0 || tdiff.abs() / (root_s.re.abs() + 200.0) < 0.005 {
                // Unlink and drop.
                if let Some(p) = prev {
                    self.nodes[p].next = next;
                }
                if let Some(nx) = next {
                    self.nodes[nx].prev = prev;
                }
                self.ntrials -= 1;
                if self.zero_trial == Some(i) {
                    self.zero_trial = next.or(self.nodes[i].prev);
                }
                if self.head == Some(i) {
                    self.head = next;
                }
                self.nodes[i].dead = true;
            } else {
                if let Some(p) = prev {
                    self.check_flat(p, i);
                } else {
                    self.nflat = 1;
                }
                self.nodes[i].flags &= !ISAMINIMA;
                prev = Some(i);
            }
            cur = next;
        }
    }

    /// `CKTpzUpdateSet`.
    fn update_set(&mut self, set: &mut Set, new: usize) {
        let mut this_move = 0;
        let n = self.nodes[new].clone();

        if n.s.im != 0.0 {
            set[2] = set[1];
            set[1] = set[0];
            set[0] = Some(new);
        } else if set[1].is_none() {
            set[1] = Some(new);
        } else if set[2].is_none() && n.s.re > self.t(set[1].unwrap()).s.re {
            set[2] = Some(new);
        } else if set[0].is_none() {
            set[0] = Some(new);
        } else if n.flags & ISAMINIMA != 0 {
            set[1] = Some(new);
        } else if n.s.re < self.t(set[0].unwrap()).s.re {
            set[2] = set[1];
            set[1] = set[0];
            set[0] = Some(new);
            this_move = FAR_LEFT;
        } else if n.s.re < self.t(set[1].unwrap()).s.re {
            let t1 = self.t(set[1].unwrap());
            if self.trapped == 0
                || n.mag_def < t1.mag_def
                || (n.mag_def == t1.mag_def && n.f_def.re.abs() < t1.f_def.re.abs())
            {
                set[2] = set[1];
                set[1] = Some(new);
                this_move = MID_LEFT;
            } else {
                set[0] = Some(new);
                this_move = NEAR_LEFT;
            }
        } else if set[2].is_some() && n.s.re < self.t(set[2].unwrap()).s.re {
            let t1 = self.t(set[1].unwrap());
            if self.trapped == 0
                || n.mag_def < t1.mag_def
                || (n.mag_def == t1.mag_def && n.f_def.re.abs() < t1.f_def.re.abs())
            {
                set[0] = set[1];
                set[1] = Some(new);
                this_move = MID_RIGHT;
            } else {
                set[2] = Some(new);
                this_move = NEAR_RIGHT;
            }
        } else {
            set[0] = set[1];
            set[1] = set[2];
            set[2] = Some(new);
            this_move = FAR_RIGHT;
        }

        if self.trapped != 0 && this_move == self.last_move {
            self.consec_moves += 1;
        } else {
            self.consec_moves = 0;
        }
        self.last_move = this_move;
    }

    /// `alter`: nudge a candidate that landed on an existing trial.
    fn alter(
        &mut self,
        s: &mut Complex64,
        nearto: Option<usize>,
        abstol: f64,
        reltol: f64,
    ) -> bool {
        let Some(nt) = nearto else {
            return false;
        };
        let near = self.nodes[nt].clone();

        let p1 = if self.trapped != 2 {
            let mut p1 = near.s.re;
            if near.flags & ISAROOT != 0 {
                p1 -= 1e-6 * near.s.re + 1e-5;
            }
            if let Some(pv) = near.prev {
                p1 += self.nodes[pv].s.re;
            } else {
                p1 -= 10.0 * (p1.abs() + 1.0);
            }
            p1 / 2.0
        } else {
            near.s.re
        };

        let p2 = if self.trapped != 1 {
            let mut p2 = near.s.re;
            if near.flags & ISAROOT != 0 {
                p2 += 1e-6 * near.s.re + 1e-5;
            }
            if let Some(nx) = near.next {
                p2 += self.nodes[nx].s.re;
            } else {
                p2 += 10.0 * (p2.abs() + 1.0);
            }
            p2 / 2.0
        } else {
            near.s.re
        };

        let close =
            |x: f64, other: f64| (x - other).abs() / (other.abs() + abstol / reltol) < reltol;
        let bail_prev = near.prev.is_some_and(|pv| close(p1, self.nodes[pv].s.re));
        let bail_next = near.next.is_some_and(|nx| close(p2, self.nodes[nx].s.re));
        if bail_prev || bail_next {
            return false;
        }

        if self.trapped != 2 && near.s.re - p1 > p2 - near.s.re {
            s.re = p1;
        } else {
            s.re = p2;
        }
        true
    }

    /// `check_flat`.
    fn check_flat(&mut self, a: usize, b: usize) {
        let ta = &self.nodes[a];
        let tb = &self.nodes[b];
        let diff_mag = ta.mag_def - tb.mag_def;
        if diff_mag.abs() <= 1 {
            let mult = match diff_mag {
                1 => 2.0,
                -1 => 0.5,
                _ => 1.0,
            };
            let diff = mult * ta.f_def - tb.f_def;
            let mag2 = diff.re * diff.re + diff.im * diff.im;
            if mag2 < 1.0e-20 {
                self.nflat += 1;
            }
        }
    }
}
