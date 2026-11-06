//! The C surface exercised through the `extern "C"` functions
//! themselves, as a C caller would use it.

use std::ffi::{CStr, CString, c_char};
use std::ptr;

use super::*;

const RC: &str = "Basic RC circuit
r 1 2 1.0
c 2 0 1.0
vin 1 0  pulse (0 1) ac 1
.tran  0.1 7.0
.end
";

fn cs(s: &str) -> CString {
    CString::new(s).unwrap()
}

/// Owned copy of a borrowed C string (`None` for NULL).
fn st(p: *const c_char) -> Option<String> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_string())
}

/// Frees on drop, so a failing assert does not leak.
struct Res(*mut DuneResult);

impl Drop for Res {
    fn drop(&mut self) {
        unsafe { dune_result_free(self.0) }
    }
}

fn sim(deck: &str) -> Res {
    let d = cs(deck);
    Res(unsafe { dune_simulate(d.as_ptr()) })
}

fn real_col(r: &Res, plot: usize, var: usize) -> Option<&[f64]> {
    let mut p = ptr::null();
    let mut n = 0;
    unsafe {
        dune_plot_column_real(r.0, plot, var, &mut p, &mut n)
            .then(|| std::slice::from_raw_parts(p, n))
    }
}

fn var_index(r: &Res, plot: usize, name: &str) -> usize {
    let n = unsafe { dune_plot_variable_count(r.0, plot) };
    (0..n)
        .find(|&v| st(unsafe { dune_plot_variable_name(r.0, plot, v) }).as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no variable {name}"))
}

#[test]
fn version_is_the_crate_version() {
    assert_eq!(st(dune_version()).unwrap(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn rc_transient_end_to_end() {
    let r = sim(RC);
    unsafe {
        assert!(dune_result_ok(r.0));
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::None);
        assert!(dune_result_error_message(r.0).is_null());
        assert_eq!(dune_result_plot_count(r.0), 1);
        assert_eq!(st(dune_plot_name(r.0, 0)).unwrap(), "Transient Analysis");
        assert_eq!(st(dune_plot_title(r.0, 0)).unwrap(), "Basic RC circuit");
        assert!(!dune_plot_is_complex(r.0, 0));
        let scale = dune_plot_scale_index(r.0, 0);
        assert!(scale >= 0);
        assert_eq!(
            st(dune_plot_variable_type(r.0, 0, scale as usize)).unwrap(),
            "time"
        );
        let np = dune_plot_npoints(r.0, 0);
        assert!(np > 10);
        let v2 = var_index(&r, 0, "v(2)");
        assert_eq!(st(dune_plot_variable_type(r.0, 0, v2)).unwrap(), "voltage");
        let col = real_col(&r, 0, v2).unwrap();
        assert_eq!(col.len(), np);
        let last = col[np - 1];
        // 1 - e^-7 after 7 time constants.
        assert!((last - (1.0 - (-7.0f64).exp())).abs() < 2e-3, "{last}");
        let t = real_col(&r, 0, scale as usize).unwrap();
        assert!((t[np - 1] - 7.0).abs() < 1e-9);
        // Real plot: the complex accessor refuses.
        let (mut p, mut n) = (ptr::NonNull::<f64>::dangling().as_ptr() as *const f64, 9);
        assert!(!dune_plot_column_complex(r.0, 0, v2, &mut p, &mut n));
        assert!(p.is_null());
        assert_eq!(n, 0);
    }
}

#[test]
fn op_plot_has_no_scale() {
    let r = sim("divider\nv1 1 0 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n");
    unsafe {
        assert!(dune_result_ok(r.0));
        assert_eq!(dune_plot_scale_index(r.0, 0), -1);
        assert_eq!(dune_plot_npoints(r.0, 0), 1);
    }
    let v2 = var_index(&r, 0, "v(2)");
    assert!((real_col(&r, 0, v2).unwrap()[0] - 1.0).abs() < 1e-9);
}

#[test]
fn complex_ac_layout_is_interleaved() {
    // Low-pass RC: H(jw) = 1 / (1 + jwRC), RC = 1.
    let r = sim("lp\nv1 1 0 ac 1\nr1 1 2 1\nc1 2 0 1\n.ac lin 3 1 3\n.end\n");
    unsafe {
        assert!(
            dune_result_ok(r.0),
            "{:?}",
            st(dune_result_error_message(r.0))
        );
        assert!(dune_plot_is_complex(r.0, 0));
        let np = dune_plot_npoints(r.0, 0);
        assert_eq!(np, 3);
        let v2 = var_index(&r, 0, "v(2)");
        assert!(real_col(&r, 0, v2).is_none());
        let (mut p, mut n) = (ptr::null(), 0);
        assert!(dune_plot_column_complex(r.0, 0, v2, &mut p, &mut n));
        assert_eq!(n, 2 * np);
        let d = std::slice::from_raw_parts(p, n);
        for (k, f) in [1.0, 2.0, 3.0].into_iter().enumerate() {
            let w = 2.0 * std::f64::consts::PI * f;
            let den = 1.0 + w * w;
            assert!((d[2 * k] - 1.0 / den).abs() < 1e-9);
            assert!((d[2 * k + 1] + w / den).abs() < 1e-9);
        }
        let s = dune_plot_scale_index(r.0, 0) as usize;
        assert_eq!(st(dune_plot_variable_type(r.0, 0, s)).unwrap(), "frequency");
    }
}

#[test]
fn complex64_is_two_doubles() {
    use num_complex::Complex64;
    use std::mem::{align_of, size_of};
    // The layout the zero-copy complex column relies on.
    assert_eq!(size_of::<Complex64>(), 2 * size_of::<f64>());
    assert_eq!(align_of::<Complex64>(), align_of::<f64>());
    let c = [Complex64::new(1.5, -2.5), Complex64::new(3.0, 4.0)];
    let p = c.as_ptr().cast::<f64>();
    let d = unsafe { std::slice::from_raw_parts(p, 4) };
    assert_eq!(d, [1.5, -2.5, 3.0, 4.0]);
}

#[test]
fn null_and_out_of_range_never_crash() {
    unsafe {
        let r = Res(dune_simulate(ptr::null()));
        assert!(!dune_result_ok(r.0));
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::Input);
        assert!(st(dune_result_error_message(r.0)).unwrap().contains("NULL"));
        let r = Res(dune_simulate_file(ptr::null()));
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::Input);
        let d = cs(RC);
        let r = Res(dune_simulate_with_includes(
            d.as_ptr(),
            ptr::null(),
            ptr::null(),
            2,
        ));
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::Input);
        let names = [ptr::null::<c_char>()];
        let texts = [d.as_ptr()];
        let r = Res(dune_simulate_with_includes(
            d.as_ptr(),
            names.as_ptr(),
            texts.as_ptr(),
            1,
        ));
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::Input);
        assert!(st(dune_result_error_message(r.0)).unwrap().contains("#0"));

        // A NULL result reads as an empty failed one.
        let n: *const DuneResult = ptr::null();
        assert!(!dune_result_ok(n));
        assert_eq!(dune_result_error_kind(n), DuneErrorKind::Input);
        assert!(dune_result_error_message(n).is_null());
        assert_eq!(dune_result_note_count(n), 0);
        assert!(dune_result_note(n, 0).is_null());
        assert_eq!(dune_result_diagnostic_count(n), 0);
        assert_eq!(dune_result_diagnostic_severity(n, 0), DuneSeverity::Error);
        assert!(dune_result_diagnostic_message(n, 0).is_null());
        assert!(dune_result_diagnostic_file(n, 0).is_null());
        assert_eq!(dune_result_diagnostic_line(n, 0), 0);
        assert_eq!(dune_result_diagnostic_column(n, 0), 0);
        assert_eq!(dune_result_plot_count(n), 0);
        assert!(dune_plot_name(n, 0).is_null());
        assert!(dune_plot_title(n, 0).is_null());
        assert_eq!(dune_plot_variable_count(n, 0), 0);
        assert!(dune_plot_variable_name(n, 0, 0).is_null());
        assert!(dune_plot_variable_type(n, 0, 0).is_null());
        assert_eq!(dune_plot_npoints(n, 0), 0);
        assert!(!dune_plot_is_complex(n, 0));
        assert_eq!(dune_plot_scale_index(n, 0), -1);
        let (mut p, mut len) = (ptr::null(), 7);
        assert!(!dune_plot_column_real(n, 0, 0, &mut p, &mut len));
        assert_eq!(len, 0);
        assert!(!dune_plot_column_complex(n, 0, 0, &mut p, &mut len));
        dune_result_free(ptr::null_mut());

        // Valid result, indices out of range, NULL out pointers.
        let r = sim(RC);
        let big = usize::MAX;
        assert!(dune_plot_name(r.0, 1).is_null());
        assert!(dune_plot_name(r.0, big).is_null());
        assert!(dune_plot_variable_name(r.0, 0, big).is_null());
        assert!(dune_plot_variable_type(r.0, 0, big).is_null());
        assert!(dune_result_note(r.0, big).is_null());
        assert!(dune_result_diagnostic_message(r.0, big).is_null());
        assert!(!dune_plot_column_real(r.0, 0, big, &mut p, &mut len));
        assert!(p.is_null() && len == 0);
        assert!(!dune_plot_column_real(r.0, 0, 0, ptr::null_mut(), &mut len));
        assert_eq!(len, 0);
        assert!(!dune_plot_column_real(r.0, 0, 0, &mut p, ptr::null_mut()));
        assert!(p.is_null());
        assert!(!dune_plot_column_real(
            r.0,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut()
        ));
    }
}

#[test]
fn invalid_utf8_is_an_input_error() {
    let bad = CString::new(vec![b't', 0xff, b'\n']).unwrap();
    unsafe {
        let r = Res(dune_simulate(bad.as_ptr()));
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::Input);
        assert!(
            st(dune_result_error_message(r.0))
                .unwrap()
                .contains("UTF-8")
        );
    }
}

#[test]
fn error_kinds_and_diagnostics() {
    unsafe {
        // Parse error, with a positioned diagnostic.
        let r = sim("t\nv1 1 0 dc 1\nq1 1 1 0\n.op\n.end\n");
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::Parse);
        assert!(dune_result_diagnostic_count(r.0) >= 1);
        let i = (0..dune_result_diagnostic_count(r.0))
            .find(|&i| dune_result_diagnostic_severity(r.0, i) == DuneSeverity::Error)
            .unwrap();
        assert_eq!(st(dune_result_diagnostic_file(r.0, i)).unwrap(), "deck.cir");
        assert_eq!(dune_result_diagnostic_line(r.0, i), 3);
        assert!(dune_result_diagnostic_column(r.0, i) >= 1);
        assert!(
            !st(dune_result_diagnostic_message(r.0, i))
                .unwrap()
                .is_empty()
        );
        assert_eq!(dune_result_plot_count(r.0), 0);

        // Elaboration error: a resistor with a model name.
        let r = sim("t\nv1 1 0 dc 1\nr1 1 0 rmod\n.op\n.end\n");
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::Elaborate);
        assert!(!dune_result_error_message(r.0).is_null());

        // Analysis error: a sweep of a source that does not exist.
        let r = sim("t\nv1 1 0 1\nr1 1 0 1\n.dc vnope 0 1 0.5\n.end\n");
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::Analysis);

        // No analysis: success with a note.
        let r = sim("t\nv1 1 0 1\nr1 1 0 1\n.end\n");
        assert!(dune_result_ok(r.0));
        assert_eq!(dune_result_plot_count(r.0), 0);
        assert_eq!(dune_result_note_count(r.0), 1);
        assert!(
            st(dune_result_note(r.0, 0))
                .unwrap()
                .contains("no analyses")
        );

        // Unreadable deck file.
        let p = cs("/nonexistent/dune-ffi/deck.cir");
        let r = Res(dune_simulate_file(p.as_ptr()));
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::Io);
    }
}

#[test]
fn include_map_resolves_and_misses_fail() {
    let deck = cs("inc\n.include sub/r.inc\nv1 1 0 2\n.op\n.end\n");
    let name = cs("sub/r.inc");
    let text = cs("r1 1 2 1k\nr2 2 0 1k\n");
    let names = [name.as_ptr()];
    let texts = [text.as_ptr()];
    unsafe {
        let r = Res(dune_simulate_with_includes(
            deck.as_ptr(),
            names.as_ptr(),
            texts.as_ptr(),
            1,
        ));
        assert!(
            dune_result_ok(r.0),
            "{:?}",
            st(dune_result_error_message(r.0))
        );
        let v2 = var_index(&r, 0, "v(2)");
        assert!((real_col(&r, 0, v2).unwrap()[0] - 1.0).abs() < 1e-9);

        // The same deck without the map: the include is not found.
        let r = Res(dune_simulate(deck.as_ptr()));
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::Parse);
        assert!(
            st(dune_result_error_message(r.0))
                .unwrap()
                .contains("not among the supplied includes")
        );
    }
}

#[test]
fn file_deck_from_the_corpus() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/spice3f5/examples/rc.cir"
    );
    let p = cs(path);
    let r = Res(unsafe { dune_simulate_file(p.as_ptr()) });
    unsafe {
        assert!(
            dune_result_ok(r.0),
            "{:?}",
            st(dune_result_error_message(r.0))
        );
        assert_eq!(dune_result_plot_count(r.0), 1);
    }
    let v2 = var_index(&r, 0, "v(2)");
    let col = real_col(&r, 0, v2).unwrap();
    assert!((col[col.len() - 1] - 1.0).abs() < 2e-3);
}

#[test]
fn panics_become_internal_errors() {
    // No deck panics (that would be a bug to fix); force one through
    // the same guard the dune_simulate* functions use.
    let r = Res(boxed(DuneResult::guarded(|| panic!("boom"))));
    unsafe {
        assert_eq!(dune_result_error_kind(r.0), DuneErrorKind::Internal);
        assert!(st(dune_result_error_message(r.0)).unwrap().contains("boom"));
    }
}

#[test]
fn interior_nul_in_messages_is_replaced() {
    assert_eq!(result::c_string("a\0b").to_str().unwrap(), "a\u{FFFD}b");
}
