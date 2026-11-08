#' Periodic steady state of a driven circuit or an oscillator
#'
#' Runs the periodic steady state (`.pss`) of the circuit in `deck`, by
#' shooting--Newton: Newton on the period map `x(t0) -> x(t0 + T)` with
#' matrix-free GMRES, the Jacobian products by directional differences
#' over a frozen-grid replay of the period transient. A linear circuit
#' converges in one Newton step. The arguments are those of the card
#' `.pss FREQ TSTAB OSCNODE POINTS HARMS SCITER STEADYCOEFF UIC`, and
#' the plots are, value for value, what that card gives in
#' [spice_run()] -- the same engine call. The deck supplies the circuit
#' and its `.options`; its own analysis cards are not run.
#'
#' The method is Dune's: ngspice-47's `.pss` is a free-running
#' oscillator search that detunes from a driving source. `points` is the
#' accuracy knob: the error left is the integrator's phase error,
#' `(2 * pi / points)^2 / 12` of the amplitude with the default
#' trapezoidal rule.
#'
#' A circuit without a periodic source (`freq = 0` finds none) is an
#' autonomous oscillator: the period becomes a Newton unknown, with the
#' phase pinned where `v(osc_node)` rises through its mid-range, and is
#' first estimated by counting the crossings of `osc_node` over the
#' second half of `tstab` (both are required then; a `freq` given is
#' only a fallback for fewer than three crossings). The deck must start
#' the oscillation itself (`.ic` with `uic = TRUE`, or a ramp): there is
#' no automatic kick. The result's `periods` follows the period per
#' iteration. Transmission lines are not supported yet.
#'
#' @inheritParams spice_run
#' @param freq The fundamental in Hz; `0` (the default) takes it from
#'   the periodic sources -- the greatest common divisor of their
#'   frequencies (`SIN`/`SFFM` with an explicit frequency, `PULSE`
#'   `1/PER`).
#' @param tstab Stabilization time (s): a plain transient runs to the
#'   first multiple of the period at or after `tstab` (at least one
#'   period) before the shooting starts there.
#' @param osc_node `NULL` or ngspice's oscillation node. Required for
#'   an autonomous oscillator: its phase condition and period estimate
#'   (see Details). A driven circuit only checks that it exists.
#' @param points Samples per period, 4 to 10 000 000: the time plot has
#'   `points + 1` rows, and the period transient steps at most
#'   `period / points`.
#' @param harmonics Rows of the spectrum, the DC included (ngspice's
#'   count), 1 to `points %/% 2`.
#' @param max_iter Most shooting iterations (at least 1).
#' @param steady_coeff Scale of the convergence tolerance, `> 0`: `1`
#'   (the default) is the SPICE tolerances (`reltol`, `vntol`, `abstol`)
#'   on the period mismatch.
#' @param uic Start the stabilization transient from the initial
#'   conditions, as `.tran ... uic`.
#' @param tmax_divisor As in [spice_run()] (it divides the maximum step
#'   of the stabilization transient).
#'
#' @return A `spice_pss`, which is also a `spice_result` (so
#'   [spice_plots()], [spice_tidy()] and [spice_measure()] on its plots
#'   work as for [spice_run()]): a list with
#'   * `title`, `notes`, `diagnostics`, `measurements` (always zero rows:
#'     a deck's `.measure` cards do not apply to `.pss` plots) and
#'     `plots` -- the two `spice_plot`s, `"Time Domain Periodic Steady
#'     State Analysis"` and `"Frequency Domain Periodic Steady State
#'     Analysis"` (ngspice's plot names) -- as in [spice_run()];
#'   * `time`: one period as a long tibble, `points + 1` equispaced
#'     instants from `t0` to `t0 + period` per variable: `time`,
#'     `variable`, `type` (`"voltage"`, `"current"`), `value`;
#'   * `spectrum`: the complex harmonics as a long tibble: `harmonic`
#'     (int, 0 is the DC), `frequency`, `variable`, `type`, `re`, `im`
#'     and the derived `mag` (`Mod()`) and `phase` (`Arg()`, radians).
#'     The convention is `.ac`'s phasor: `x(t) = X_0 + sum Re(X_k
#'     exp(1i * k * w * t))`, phases referred to `t = 0`; `mag` is
#'     ngspice's (magnitude-only) plot;
#'   * `period` (s: the drive's for a driven circuit, the shooting's
#'     result for an oscillator), `t0` (s, the start of the reported
#'     period), `residuals`: the period mismatch
#'     `max_i |x_i(t0+T) - x_i(t0)| / tol_i` of each shooting iteration
#'     (the last one `<= steady_coeff`; `Inf` for an iteration whose
#'     transient failed), and `periods`: the period of each iteration
#'     (constant for a driven circuit, the Newton iterate of an
#'     oscillator; the last one is `period`).
#'
#' @section Errors:
#' As [spice_run()]; a shooting that does not converge in `max_iter`
#' iterations is a condition of class `spice_error_pss` (and
#' `spice_error`) whose fields `iterations` (int) and `residuals` (dbl)
#' hold the history, and `period` (dbl) the last period iterate of an
#' oscillator (`NULL` for a driven circuit) -- the same condition a
#' `.pss` card raises from [spice_run()], [spice_rerun()] and a run of
#' [spice_sweep()]. Other failures (an unknown `osc_node`, an
#' oscillator without `osc_node` or `tstab`, a shooting that settles
#' on the equilibrium, a transmission line) are `spice_error_analysis`;
#' an argument out of range is `spice_error_input`.
#'
#' @section Mirror of the Python package:
#' This is `spice.pss()`, whose `PssConvergenceError` is the
#' `spice_error_pss` condition here.
#'
#' @seealso [spice_run()] (a `.pss` card), [spice_measure()].
#' @export
#' @examples
#' rc <- "rc low-pass
#' v1 1 0 sin(0 1 1k)
#' r1 1 2 1k
#' c1 2 0 1u
#' .end"
#' res <- spice_pss(rc, freq = 1e3, tstab = 2e-3)
#' res
#' res$period
#' # First harmonic of v(2): |H| = 1 / sqrt(1 + (2 * pi)^2)
#' subset(res$spectrum, variable == "v(2)" & harmonic == 1)
#' # The period measures as a transient
#' spice_measure(res$plots[[1]], "pp v(2)")
#'
#' # A shooting cut short keeps its history on the condition
#' e <- tryCatch(spice_pss(rc, freq = 1e3, max_iter = 1), spice_error_pss = identity)
#' e$residuals
spice_pss <- function(deck, freq = 0, tstab = 0, osc_node = NULL, points = 128L,
                      harmonics = 10L, max_iter = 50L, steady_coeff = 1, uic = FALSE,
                      includes = NULL, options = NULL, tmax_divisor = NULL) {
  call <- rlang::current_env()
  freq <- check_pss_number(freq, call = call)
  if (freq < 0) pss_abort("{.arg freq} must be positive, or 0 to take it from the sources.", call)
  tstab <- check_pss_number(tstab, call = call)
  if (tstab < 0) pss_abort("{.arg tstab} must not be negative.", call)
  points <- check_pss_count(points, 4, 1e7, call = call)
  harmonics <- check_pss_count(harmonics, 1, points %/% 2L, call = call)
  max_iter <- check_pss_count(max_iter, 1, .Machine$integer.max, call = call)
  steady_coeff <- check_pss_number(steady_coeff, call = call)
  if (steady_coeff <= 0) pss_abort("{.arg steady_coeff} must be positive.", call)
  if (!rlang::is_bool(uic)) pss_abort("{.arg uic} must be {.code TRUE} or {.code FALSE}.", call)
  if (!is.null(osc_node) && !(rlang::is_string(osc_node) && nzchar(osc_node))) {
    pss_abort("{.arg osc_node} must be {.code NULL} or a node name.", call)
  }
  src <- check_deck(deck, call = call)
  inc <- check_includes(includes, is_path = src$is_path, call = call, pic = src$pic_includes)
  opt <- check_options(options, call = call)
  div <- check_tmax_divisor(tmax_divisor, call = call)

  out <- ffi_pss_deck(
    src$deck, src$is_path, as.character(names(inc)), unname(as.character(inc)),
    as.character(names(opt)), unname(opt), div,
    freq, tstab, if (is.null(osc_node)) "" else osc_node,
    points, harmonics, max_iter, steady_coeff, uic
  )
  diagnostics <- tibble::tibble(
    severity = out$diagnostics$severity,
    message = out$diagnostics$message
  )
  if (!isTRUE(out$ok)) {
    abort_engine(out$kind, out$message, diagnostics, call = call, failure = out)
  }
  res <- new_spice_result(out$title, out$plots, out$notes, diagnostics, out$measurements)
  res$time <- pss_time_tibble(res$plots[[1L]])
  res$spectrum <- pss_spectrum_tibble(res$plots[[2L]])
  res$period <- out$period
  res$t0 <- out$t0
  res$residuals <- as.double(out$residuals)
  res$periods <- as.double(out$periods)
  class(res) <- c("spice_pss", class(res))
  res
}

pss_abort <- function(msg, call, .envir = parent.frame()) {
  cli::cli_abort(msg, class = c("spice_error_input", "spice_error"), call = call, .envir = .envir)
}

check_pss_number <- function(x, arg = rlang::caller_arg(x), call) {
  if (!is.numeric(x) || length(x) != 1L || is.na(x) || !is.finite(x)) {
    cli::cli_abort(
      "{.arg {arg}} must be a single finite number, not {.obj_type_friendly {x}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  as.double(x)
}

check_pss_count <- function(x, low, high, arg = rlang::caller_arg(x), call) {
  if (!is.numeric(x) || length(x) != 1L || is.na(x) || !is.finite(x) ||
    x != round(x) || x < low || x > high) {
    cli::cli_abort(
      "{.arg {arg}} must be a whole number from {low} to {format(high, scientific = FALSE)}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  as.integer(x)
}

# One period, long: time, variable, type, value.
pss_time_tibble <- function(plot) {
  long <- as_tibble.spice_plot(plot)
  tibble::tibble(
    time = long$scale,
    variable = long$variable,
    type = long$type,
    value = long$value
  )
}

# The harmonics, long: harmonic, frequency, variable, type, re, im, mag, phase.
pss_spectrum_tibble <- function(plot) {
  long <- as_tibble.spice_plot(plot)
  z <- complex(real = long$re, imaginary = long$im)
  tibble::tibble(
    harmonic = long$point - 1L,
    frequency = long$scale,
    variable = long$variable,
    type = long$type,
    re = long$re,
    im = long$im,
    mag = Mod(z),
    phase = Arg(z)
  )
}

#' @export
print.spice_pss <- function(x, ...) {
  NextMethod()
  r <- x$residuals
  # An oscillator's period moves with the Newton iterations.
  what <- if (length(unique(x$periods)) > 1L) " (autonomous oscillator)" else ""
  cat(
    cli::format_inline(
      "Periodic steady state{what}: period {format(x$period, digits = 6)} s from t0 = ",
      "{format(x$t0, digits = 6)} s; {length(r)} shooting iteration{?s}, residual ",
      "{format(r[[length(r)]], digits = 3)}."
    ),
    sep = "\n"
  )
  invisible(x)
}
