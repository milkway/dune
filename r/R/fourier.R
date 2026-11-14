#' Fourier analysis of a transient plot
#'
#' The harmonics and the total harmonic distortion (THD) of transient
#' waveforms, as the `.four` card computes them (ngspice-47's
#' `fourier`, itself the 'SPICE3f5' one plus `nperiods`). The engine
#' does the computing, exactly as for the deck's own `.four` cards
#' (which [spice_run()] returns in `$fourier`); the analysis reads the
#' plot only and never changes a simulation.
#'
#' The last `nperiods` periods of the run (`nperiods / freq` seconds,
#' ending at the last time point) are resampled onto `gridsize *
#' nperiods` uniform points by piecewise polynomial interpolation of
#' degree `polydegree` (`polydegree = 0` takes the raw samples as if
#' they were uniform), and the first `nfreqs` harmonics of `freq`, DC
#' included, are computed by a discrete Fourier transform. Phases are
#' in degrees, referred to a sine (a sine at zero phase reads 0, a
#' cosine 90); the normalized columns divide by the fundamental and
#' subtract its phase (not wrapped into +/-180). The THD, in percent,
#' is `100 * sqrt(sum(norm_magnitude[harmonic >= 2]^2))`.
#'
#' @param plot A `spice_plot` of a `.tran` analysis (or the time plot of
#'   a [spice_pss()] result), from [spice_run()] or [spice_read_raw()].
#' @param freq The fundamental frequency, in hertz: a positive number.
#' @param vectors A character vector of output expressions, as on a
#'   `.four` or `.print` card: `"v(out)"`, `"v(a,b)"`, `"i(vin)"`, a
#'   vector name as the plot has it, or `"all"` (every vector of the
#'   plot, the time scale included).
#' @param nfreqs The number of harmonics computed, DC included (a
#'   whole number `>= 1`; the `.options` variable `nfreqs`).
#' @param nperiods The number of periods analysed (`>= 1`; `nperiods`).
#' @param polydegree The interpolation degree (`>= 0`; `polydegree`).
#' @param gridsize The number of interpolation points per period
#'   (`>= 1`; the `.options` variable `fourgridsize`).
#'
#' @return A tibble with one row per harmonic of each vector:
#'   * `card` (int): for the deck's `.four` cards, the card's position
#'     among them (1 for the first `.four` card); `NA` here;
#'   * `vector` (chr): the vector as ngspice names it (`"v(out)"`);
#'   * `harmonic` (int): 0 for DC, then 1 to `nfreqs - 1`;
#'   * `frequency` (dbl): `harmonic * freq`, in hertz;
#'   * `magnitude` (dbl): the amplitude of the harmonic (the signed
#'     mean for DC);
#'   * `phase` (dbl): its phase, in degrees (0 for DC);
#'   * `norm_magnitude`, `norm_phase` (dbl): magnitude over the
#'     fundamental's and phase minus the fundamental's (0 for DC);
#'   * `thd` (dbl): the vector's total harmonic distortion, in percent,
#'     repeated on each of its rows;
#'   * `gridsize`, `polydegree`, `nperiods` (int): the points
#'     transformed (`gridsize * nperiods`, or the number of samples when
#'     `polydegree = 0`), the degree and the periods used;
#'   * `error` (chr): why the vector has no table (an unknown vector, a
#'     run shorter than `nperiods / freq`, ...), `NA` otherwise.
#'
#'   A vector that cannot be analysed is one row with `error` set and
#'   the other columns `NA`, not an R error. Malformed arguments, an
#'   unparsable expression and a plot that is not a transient are errors
#'   of class `spice_error_input` (arguments) or `spice_error_fourier`.
#'
#' @section Mirror of the Python package:
#' This is `spice.fourier()`; the deck's `.four` cards are
#' `Result.fourier` there.
#'
#' @seealso [spice_run()] (its `fourier` field), [spice_measure()].
#' @export
#' @examples
#' res <- spice_run("diode clipper
#' v1 in 0 sin(0 2 1k)
#' r1 in out 1k
#' d1 out 0 dmod
#' d2 0 out dmod
#' .model dmod d
#' .tran 5u 4m
#' .four 1k v(out)
#' .end")
#' res$fourier
#' tran <- spice_plots(res)[["Transient Analysis"]]
#' spice_fourier(tran, 1e3, c("v(in)", "v(out)"), nfreqs = 5)
spice_fourier <- function(plot, freq, vectors, nfreqs = 10, nperiods = 1,
                          polydegree = 1, gridsize = 200) {
  call <- rlang::current_env()
  if (!inherits(plot, "spice_plot")) {
    cli::cli_abort(
      "{.arg plot} must be a {.cls spice_plot}, not {.obj_type_friendly {plot}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  if (!is.numeric(freq) || length(freq) != 1L || !is.finite(freq) || freq <= 0) {
    cli::cli_abort(
      "{.arg freq} must be a positive number, not {.obj_type_friendly {freq}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  if (!is.character(vectors) || length(vectors) == 0L || anyNA(vectors)) {
    cli::cli_abort(
      "{.arg vectors} must be a character vector of output expressions, not {.obj_type_friendly {vectors}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  nfreqs <- check_four_count(nfreqs, "nfreqs", 1, call)
  nperiods <- check_four_count(nperiods, "nperiods", 1, call)
  polydegree <- check_four_count(polydegree, "polydegree", 0, call)
  gridsize <- check_four_count(gridsize, "gridsize", 1, call)
  scale <- match(plot$scale, plot$variables$name)
  out <- ffi_fourier(
    plot_to_raw(plot), if (is.na(scale)) 0L else as.integer(scale),
    as.double(freq), unname(vectors), nfreqs, nperiods, polydegree, gridsize
  )
  if (!isTRUE(out$ok)) {
    msg <- out$message
    cli::cli_abort(
      c("Cannot analyse the plot.", "x" = "{msg}"),
      class = c("spice_error_fourier", "spice_error"),
      call = call
    )
  }
  fourier_tibble(out$fourier)
}

# A whole number >= `min` that fits an R integer, as an integer.
check_four_count <- function(x, arg, min, call) {
  if (!is.numeric(x) || length(x) != 1L || !is.finite(x) || x != trunc(x) ||
    x < min || x > .Machine$integer.max) {
    cli::cli_abort(
      "{.arg {arg}} must be a whole number >= {min}, not {.obj_type_friendly {x}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  as.integer(x)
}

# The ffi's parallel columns as the documented tibble.
fourier_tibble <- function(f) {
  if (is.null(f)) {
    f <- list(
      card = integer(), vector = character(), harmonic = integer(),
      frequency = double(), magnitude = double(), phase = double(),
      norm_magnitude = double(), norm_phase = double(), thd = double(),
      gridsize = integer(), polydegree = integer(), nperiods = integer(),
      error = character()
    )
  }
  tibble::tibble(
    card = as.integer(f$card),
    vector = as.character(f$vector),
    harmonic = as.integer(f$harmonic),
    frequency = as.double(f$frequency),
    magnitude = as.double(f$magnitude),
    phase = as.double(f$phase),
    norm_magnitude = as.double(f$norm_magnitude),
    norm_phase = as.double(f$norm_phase),
    thd = as.double(f$thd),
    gridsize = as.integer(f$gridsize),
    polydegree = as.integer(f$polydegree),
    nperiods = as.integer(f$nperiods),
    error = as.character(f$error)
  )
}
