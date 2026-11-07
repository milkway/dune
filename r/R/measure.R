#' Measure a simulated plot
#'
#' Scalar measurements of a plot with the `.measure` grammar of
#' ngspice-47 (SPICE3f5 has none): rise and fall times and delays
#' (`TRIG ... TARG ...`), level crossings (`WHEN`), values at a point
#' or at an event (`FIND ... AT=` / `FIND ... WHEN`), derivatives
#' (`DERIV`, a Dune extension), and window statistics (`MIN`, `MAX`,
#' `MIN_AT`, `MAX_AT`, `PP`, `AVG`, `RMS`, `INTEG`). The engine does
#' the measuring, exactly as for the deck's own `.measure` cards (which
#' [spice_run()] returns in `$measurements`); measurements read the
#' plot only and never change a simulation.
#'
#' Each `spec` is what follows `.meas <analysis> <name>` on a card; the
#' analysis is the plot's own. Values take SPICE suffixes (`2n`, `1meg`).
#' Crossings interpolate linearly between points; `AVG` integrates by
#' trapezoids over the window's points, `RMS` and `INTEG` by Simpson's
#' rules with the window edges interpolated, all as ngspice does.
#'
#' @param plot A `spice_plot` of a `.tran`, `.ac`, `.dc` or `.sp`
#'   analysis, from [spice_run()] or [spice_read_raw()].
#' @param spec A character vector of measurements, e.g.
#'   `"trig v(out) val=0.1 rise=1 targ v(out) val=0.9 rise=1"`,
#'   `"when v(out)=0.5 cross=last"`, `"max v(out) from=1n to=5n"`. Its
#'   names, when given, name the results; unnamed ones are called
#'   `measure` (or `measure1`, `measure2`, ... for several).
#'
#' @return A tibble with one row per measurement:
#'   * `name` (chr), `analysis` (chr: `"tran"`, `"ac"`, `"dc"`, `"sp"`)
#'     and `plot` (chr: the plot's label);
#'   * `value` (dbl): the result -- a time for `WHEN` and delays, a value
#'     for `FIND`/`MIN`/`AVG`/..., the *scale value* for `MIN_AT`/`MAX_AT`;
#'     `NA` when the measurement failed;
#'   * `trig`, `targ`, `at`, `with`, `from`, `to` (dbl): the extra
#'     numbers ngspice prints beside the value (delay ends; where a
#'     `MIN`/`MAX` occurs; the extremum of `MIN_AT`/`MAX_AT`; the window
#'     of `AVG`/`RMS`/`INTEG`/`PP`), `NA` where they do not apply;
#'   * `error` (chr): why the measurement failed (`"out of interval
#'     (WHEN)"`: the event never happens in the window), `NA` otherwise.
#'
#'   An event that never happens is a row with `error` set, not an R
#'   error. A malformed `spec` is an error of class
#'   `spice_error_measure`.
#'
#' @section Mirror of the Python package:
#' This is `spice.measure()`; the deck's measurements are
#' `Result.measurements` there.
#'
#' @seealso [spice_run()] (its `measurements` field).
#' @export
#' @examples
#' res <- spice_run("rc step
#' v1 in 0 pulse(0 1 1n 1p 1p 20n 40n)
#' r1 in out 1k
#' c1 out 0 1p
#' .tran 10p 10n
#' .meas tran trise trig v(out) val=0.1 rise=1 targ v(out) val=0.9 rise=1
#' .end")
#' res$measurements
#' tran <- spice_plots(res)[["Transient Analysis"]]
#' spice_measure(tran, c(
#'   t50 = "when v(out)=0.5",
#'   slope = "deriv v(out) at=2n",
#'   vmax = "max v(out)"
#' ))
spice_measure <- function(plot, spec) {
  call <- rlang::current_env()
  if (!inherits(plot, "spice_plot")) {
    cli::cli_abort(
      "{.arg plot} must be a {.cls spice_plot}, not {.obj_type_friendly {plot}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  if (!is.character(spec) || length(spec) == 0L || anyNA(spec)) {
    cli::cli_abort(
      "{.arg spec} must be a character vector of measurements, not {.obj_type_friendly {spec}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  nms <- rlang::names2(spec)
  default <- if (length(spec) == 1L) "measure" else paste0("measure", seq_along(spec))
  nms[nms == ""] <- default[nms == ""]
  scale <- match(plot$scale, plot$variables$name)
  out <- ffi_measure(
    plot_to_raw(plot), if (is.na(scale)) 0L else as.integer(scale),
    tolower(nms), unname(spec)
  )
  if (!isTRUE(out$ok)) {
    msg <- out$message
    cli::cli_abort(
      c("Cannot measure the plot.", "x" = "{msg}"),
      class = c("spice_error_measure", "spice_error"),
      call = call
    )
  }
  measurements_tibble(out$measurements, plot$label)
}

# The ffi's parallel columns as the documented tibble; `labels` maps
# the 1-based plot index (NA for spice_measure()) to plot labels.
measurements_tibble <- function(m, labels) {
  if (is.null(m)) {
    m <- list(
      name = character(), analysis = character(), plot = integer(),
      value = double(), trig = double(), targ = double(), at = double(),
      with = double(), from = double(), to = double(), error = character()
    )
  }
  plot <- if (length(labels) == 1L) {
    rep(labels, length(m$name))
  } else {
    as.character(labels[m$plot])
  }
  tibble::tibble(
    name = as.character(m$name),
    analysis = as.character(m$analysis),
    plot = as.character(plot),
    value = as.double(m$value),
    trig = as.double(m$trig),
    targ = as.double(m$targ),
    at = as.double(m$at),
    with = as.double(m$with),
    from = as.double(m$from),
    to = as.double(m$to),
    error = as.character(m$error)
  )
}
