//! The ngspice `.four` printout, rendered over
//! [`dune_core::FourOutcome`]s — `frontend/fourier.c::fourier` (the
//! table, `pnum`'s `%.6g`/`%.5g` numbers in 11-wide columns with their
//! trailing blanks) and the framing of `dotcards.c::ft_cktcoms` (two
//! newlines after each successful card; on failure the error on
//! stderr followed by "No transient data available for fourier
//! analysis").

use dune_core::{FourError, FourOutcome, Fourier};

/// `cp_numdgt` unset: six significant digits.
const NUMDGT: usize = 6;

/// Column width `fw = numdgt + 5 + shift` (`shift` is 0: `%1.1e` of
/// 0.0 is seven characters everywhere but old HP-UX).
const FW: usize = NUMDGT + 5;

/// C's `%.{prec}g`.
pub fn fmt_g(x: f64, prec: usize) -> String {
    let p = prec.max(1);
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf" } else { "inf" }.into();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let strip = |s: String| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        }
    };
    let sci = format!("{:.*e}", p - 1, x);
    let (mant, exp) = sci.split_once('e').expect("`{:e}` has an exponent");
    let exp: i32 = exp.parse().expect("integer exponent");
    if exp < -4 || exp >= p as i32 {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", strip(mant.to_string()), exp.abs())
    } else {
        strip(format!("{:.*}", (p as i32 - 1 - exp) as usize, x))
    }
}

/// `pnum`: one digit less for negative numbers (room for the sign).
fn pnum(x: f64) -> String {
    if x < 0.0 {
        fmt_g(x, NUMDGT - 1)
    } else {
        fmt_g(x, NUMDGT)
    }
}

/// One vector's table, as `fourier()` prints it.
pub fn table(out: &mut String, var: &str, f: &Fourier) {
    out.push_str(&format!("Fourier analysis for {var}:\n"));
    out.push_str(&format!(
        "  No. Harmonics: {}, THD: {} %, Gridsize: {}, Interpolation Degree: {}, \
         No. Periods: {}\n\n",
        f.harmonics.len(),
        fmt_g(f.thd, 6),
        f.gridsize,
        f.polydegree,
        f.nperiods
    ));
    out.push_str(&format!(
        "Harmonic {:<FW$} {:<FW$} {:<FW$} {:<FW$} {:<FW$}\n",
        "Frequency", "Magnitude", "Phase", "Norm. Mag", "Norm. Phase"
    ));
    out.push_str(&format!(
        "-------- {:<FW$} {:<FW$} {:<FW$} {:<FW$} {:<FW$}\n",
        "---------", "---------", "-----", "---------", "-----------"
    ));
    for (i, h) in f.harmonics.iter().enumerate() {
        out.push_str(&format!(
            " {i:<4}    {:<FW$} {:<FW$} {:<FW$} {:<FW$} {:<FW$}\n",
            pnum(h.freq),
            pnum(h.mag),
            pnum(h.phase),
            pnum(h.norm_mag),
            pnum(h.norm_phase)
        ));
    }
    out.push('\n');
}

/// Render the outcomes of one `.four` card: `(stdout, stderr)`.
/// Errors that make ngspice abandon the card (`goto done`: no
/// transient plot, span too short, interpolation failure) stop it
/// here too; per-vector ones (unknown vector, length mismatch) skip
/// that vector.
pub fn render_card(outcomes: &[&FourOutcome]) -> (String, String) {
    let mut out = String::new();
    let mut err = String::new();
    for o in outcomes {
        match &o.result {
            Ok(f) => table(&mut out, &o.var, f),
            Err(e @ (FourError::UnknownVector(_) | FourError::LengthMismatch(..))) => {
                err.push_str(&format!("Error: {e}\n"));
            }
            Err(FourError::NoTransient) => {
                err.push_str("No transient data available for fourier analysis\n");
                return (out, err);
            }
            Err(e) => {
                err.push_str(&format!("Error: {e}\n"));
                err.push_str("No transient data available for fourier analysis\n");
                return (out, err);
            }
        }
    }
    out.push_str("\n\n");
    (out, err)
}

/// Render every card in deck order (rawfile mode, where no `.print`
/// output interleaves).
pub fn render_all(outcomes: &[FourOutcome]) -> (String, String) {
    let mut out = String::new();
    let mut err = String::new();
    let mut cards: Vec<usize> = outcomes.iter().map(|o| o.card).collect();
    cards.dedup();
    for c in cards {
        let mine: Vec<&FourOutcome> = outcomes.iter().filter(|o| o.card == c).collect();
        let (o, e) = render_card(&mine);
        out.push_str(&o);
        err.push_str(&e);
    }
    (out, err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dune_core::Harmonic;

    #[test]
    fn fmt_g_matches_c() {
        for (x, p, want) in [
            (0.999632, 6, "0.999632"),
            (1000.0, 6, "1000"),
            (-1.7034e-05, 5, "-1.7034e-05"),
            (1.44934e-07, 6, "1.44934e-07"),
            (-80.901, 5, "-80.901"),
            (4.11498e-05, 6, "4.11498e-05"),
            (11.2618, 6, "11.2618"),
            (0.0, 6, "0"),
            (-0.0, 6, "-0"),
            (123456.7, 6, "123457"),
            (1234567.0, 6, "1.23457e+06"),
            (0.0001, 6, "0.0001"),
            (0.00001, 6, "1e-05"),
            (179.99999, 6, "180"),
            (-270.01, 5, "-270.01"),
            (1e100, 6, "1e+100"),
        ] {
            assert_eq!(fmt_g(x, p), want, "{x} at {p}");
        }
    }

    /// A row ngspice-47 printed for the diode probe (`v(2)`,
    /// harmonic 3), rebuilt from its own numbers.
    #[test]
    fn table_layout_matches_ngspice() {
        let h = |freq, mag, phase, norm_mag, norm_phase| Harmonic {
            freq,
            mag,
            phase,
            norm_mag,
            norm_phase,
        };
        let f = Fourier {
            fund: 1e3,
            thd: 11.2618,
            gridsize: 200,
            polydegree: 1,
            nperiods: 1,
            harmonics: vec![
                h(0.0, -0.052502, 0.0, 0.0, 0.0),
                h(1000.0, 0.811933, 0.00012793, 1.0, 0.0),
                h(2000.0, 0.0750973, 89.9974, 0.092492, 89.9973),
                h(3000.0, 0.0469764, -0.0044322, 0.0578575, -0.0045601),
            ],
        };
        let mut s = String::new();
        table(&mut s, "v(2)", &f);
        let want = "Fourier analysis for v(2):\n\
\x20 No. Harmonics: 4, THD: 11.2618 %, Gridsize: 200, Interpolation Degree: 1, No. Periods: 1\n\
\n\
Harmonic Frequency   Magnitude   Phase       Norm. Mag   Norm. Phase\n\
-------- ---------   ---------   -----       ---------   -----------\n \
0       0           -0.052502   0           0           0          \n \
1       1000        0.811933    0.00012793  1           0          \n \
2       2000        0.0750973   89.9974     0.092492    89.9973    \n \
3       3000        0.0469764   -0.0044322  0.0578575   -0.0045601 \n\n";
        assert_eq!(s, want);
    }

    #[test]
    fn aborting_errors_end_the_card() {
        let o = |var: &str, result| FourOutcome {
            card: 0,
            var: var.into(),
            result,
        };
        let a = o("v(1)", Err(FourError::SpanTooShort { nperiods: 2 }));
        let b = o("v(2)", Err(FourError::SpanTooShort { nperiods: 2 }));
        let (out, err) = render_card(&[&a, &b]);
        assert_eq!(out, "");
        assert_eq!(
            err,
            "Error: (2 * wavelength) longer than time span\n\
             No transient data available for fourier analysis\n"
        );
        let c = o("v(9)", Err(FourError::UnknownVector("v(9)".into())));
        let (out, err) = render_card(&[&c]);
        assert_eq!(out, "\n\n");
        assert_eq!(err, "Error: no such vector v(9)\n");
    }
}
