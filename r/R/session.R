#' Keep a circuit alive to alter and re-run it
#'
#' `spice_session()` parses and builds a deck once; [spice_alter()] then
#' changes element values in place and [spice_rerun()] runs every analysis
#' card again without re-parsing -- for interactive exploration and for
#' fitting loops that evaluate the same circuit many times.
#'
#' With `warm_start = FALSE` (the default), `spice_rerun()` returns exactly
#' what [spice_run()] returns for the altered deck: the same plots, value
#' for value, and the same notes and diagnostics. With `warm_start = TRUE`
#' each operating point (`.op`, and the bias point of `.ac`, `.noise`,
#' `.tf`, `.sens`, `.pz`, `.disto`) continues from the previous one: faster
#' for nonlinear circuits, and equal to a fresh run to the Newton
#' tolerances (`reltol`, `vntol`, `abstol`) rather than bit for bit.
#' Transient operating points and decks with `.nodeset` are never
#' warm-started.
#'
#' A session is a **reference** object: `spice_alter()` and
#' `spice_set_temp()` modify it in place (every copy of it sees the
#' change) and return it invisibly, so calls can be piped.
#'
#' @inheritParams spice_run
#' @param warm_start Continue each operating point from the previous run's
#'   (see Details).
#' @return A `spice_session`.
#' @section Mirror of the Python package:
#' This is `spice.Session` in Python: `spice_alter()` is `Session.alter()`,
#' `spice_rerun()` is `Session.run()`, `spice_set_temp()` sets
#' `Session.temp`.
#' @seealso [spice_alter()], [spice_rerun()], [spice_param()].
#' @export
#' @examples
#' s <- spice_session("divider
#' v1 1 0 dc 10
#' r1 1 2 1k
#' r2 2 0 1k
#' .op
#' .end")
#' spice_plots(spice_rerun(s))[[1]]$values[["v(2)"]]
#' s |> spice_alter("r2", 3e3)
#' spice_plots(spice_rerun(s))[[1]]$values[["v(2)"]]
spice_session <- function(deck, includes = NULL, options = NULL, warm_start = FALSE) {
  call <- rlang::current_env()
  src <- check_deck(deck, call = call)
  inc <- check_includes(includes, is_path = src$is_path, call = call)
  opt <- check_options(options, call = call)
  check_flag(warm_start, "warm_start", call = call)

  ptr <- SpiceSession$open(
    src$deck, src$is_path, as.character(names(inc)), unname(as.character(inc)),
    as.character(names(opt)), unname(opt)
  )
  failed <- ptr$status()
  if (!is.null(failed)) {
    abort_failure(failed, call = call)
  }
  ptr$set_warm_start(warm_start)
  structure(list(ptr = ptr), class = "spice_session")
}

#' Change element values of a session
#'
#' The `alter` command of SPICE3, with ngspice's parameter names: an
#' unnamed value sets the element's principal value; named values set the
#' named parameters.
#'
#' | element | unnamed value | named parameters |
#' |---|---|---|
#' | R | resistance (nonzero) | `resistance` |
#' | C, L | capacitance, inductance | `capacitance`/`inductance`, `ic` |
#' | V, I | DC level | `dc`, `acmag`, `acphase` (degrees), and `pulse`, `sin`, `exp`, `pwl`, `sffm` (a numeric vector of the card's arguments; replaces the transient function) |
#' | E, F, G, H | gain | `gain` |
#' | M (every level, BSIM) | -- | `w`, `l` (meters); `temp` (degrees C, levels 1/2/3/6) |
#' | D, Q, J | area | `area`, `temp` (degrees C) |
#' | Z | area | `area` |
#'
#' The units are the card's. Element names are case-insensitive;
#' subcircuit elements go by their expanded name (`r.x1.r1`). The circuit
#' temperature is [spice_set_temp()].
#'
#' A value equals the altered card when it is the same double: the card
#' parser is SPICE3's, which reads `10u` as `10 * 1e-6`
#' (`9.999999999999999e-06`, not `1e-05`).
#'
#' @param session A [spice_session()].
#' @param refdes The element name, a string.
#' @param ... The new value(s): one unnamed number, or `name = value`
#'   pairs.
#' @return `session`, invisibly (modified in place).
#' @section Errors:
#' A refused value -- unknown element or parameter, non-finite value, zero
#' resistance, nonpositive `w`/`l`/`area` -- is an error of class
#' `spice_error_alter`; the parameters before it in the same call stay
#' altered, the refused one and those after it do not.
#' @export
#' @examples
#' s <- spice_session("amp
#' vdd 1 0 dc 5
#' vin 2 0 dc 1.2 ac 1
#' rd 1 3 10k
#' m1 3 2 0 0 nm l=2u w=10u
#' .model nm nmos level=1 vto=0.7 kp=50u
#' .op
#' .end")
#' s |>
#'   spice_alter("rd", 4.7e3) |>
#'   spice_alter("m1", w = 20e-6, l = 1e-6) |>
#'   spice_alter("vin", dc = 1.5)
#' spice_param(s, "m1", "w")
spice_alter <- function(session, refdes, ...) {
  call <- rlang::current_env()
  check_session(session, call = call)
  if (!rlang::is_string(refdes)) {
    cli::cli_abort(
      "{.arg refdes} must be a single string, not {.obj_type_friendly {refdes}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  values <- rlang::list2(...)
  if (length(values) == 0L) {
    cli::cli_abort(
      "Give a value to {.fn spice_alter}: {.code spice_alter(s, \"r1\", 2e3)} or {.code spice_alter(s, \"m1\", w = 4e-6)}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  params <- names(values)
  if (is.null(params)) params <- rep("", length(values))
  if (sum(params == "") > 1L) {
    cli::cli_abort(
      "Only one unnamed value (the principal one) per call.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  for (i in seq_along(values)) {
    v <- values[[i]]
    p <- params[[i]]
    if (!is.numeric(v) || anyNA(v) || length(v) == 0L) {
      cli::cli_abort(
        "The value of {.field {if (p == \"\") refdes else p}} must be a non-missing number or numeric vector.",
        class = c("spice_error_input", "spice_error"),
        call = call
      )
    }
    is_list <- tolower(p) %in% c("pulse", "sin", "exp", "pwl", "sffm")
    if (!is_list && length(v) != 1L) {
      cli::cli_abort(
        "{.field {if (p == \"\") refdes else p}} takes a single number, not {length(v)}.",
        class = c("spice_error_input", "spice_error"),
        call = call
      )
    }
    failed <- session$ptr$alter(refdes, p, as.double(v), is_list)
    if (!is.null(failed)) abort_failure(failed, call = call)
  }
  invisible(session)
}

#' Run a session's analyses again
#'
#' @param session A [spice_session()].
#' @return A `spice_result`, as from [spice_run()] -- the deck's
#'   `.measure` cards evaluated on the new plots included: with
#'   `warm_start = FALSE`, the result `spice_run()` gives for the altered
#'   deck.
#' @section Errors:
#' As [spice_run()]: an analysis failure is a `spice_error_analysis`.
#' @export
#' @examples
#' s <- spice_session("rc\nv1 1 0 pulse(0 1)\nr1 1 2 1k\nc1 2 0 1u\n.tran 0.1m 5m\n.end")
#' tau <- vapply(c(1e3, 2e3), function(r) {
#'   tr <- spice_plots(spice_rerun(spice_alter(s, "r1", r)))[[1]]$values
#'   tr$time[which.max(tr[["v(2)"]] > 1 - exp(-1))]
#' }, numeric(1))
#' tau
spice_rerun <- function(session) {
  call <- rlang::current_env()
  check_session(session, call = call)
  out <- session$ptr$run()
  diagnostics <- tibble::tibble(
    severity = out$diagnostics$severity,
    message = out$diagnostics$message
  )
  if (!isTRUE(out$ok)) {
    abort_engine(out$kind, out$message, diagnostics, call = call)
  }
  new_spice_result(out$title, out$plots, out$notes, diagnostics, out$measurements)
}

#' Read a session's element values
#'
#' `spice_param()` reads the current value of an alterable parameter
#' (names and units as [spice_alter()]); `spice_alterable()` lists the
#' parameters an element has.
#'
#' @param session A [spice_session()].
#' @param refdes The element name.
#' @param param The parameter name; `""` for the principal value.
#' @return `spice_param()`: a number, a numeric vector (a source
#'   function's arguments) or `NULL` for a value the element never got (a
#'   MOS `w` left to `.options defw`, a source without `AC`).
#'   `spice_alterable()`: a character vector.
#' @export
#' @examples
#' s <- spice_session("t\nv1 1 0 dc 1 sin(0 1 1k)\nr1 1 0 1k\n.op\n.end")
#' spice_param(s, "v1", "sin")
#' spice_alterable(s, "v1")
spice_param <- function(session, refdes, param = "") {
  call <- rlang::current_env()
  check_session(session, call = call)
  out <- session$ptr$param(refdes, param)
  if (!isTRUE(out$ok)) abort_failure(out, call = call)
  if (length(out$value) == 0L && !isTRUE(out$is_list)) NULL else out$value
}

#' @rdname spice_param
#' @export
spice_alterable <- function(session, refdes) {
  call <- rlang::current_env()
  check_session(session, call = call)
  out <- session$ptr$alterable(refdes)
  if (is.list(out)) abort_failure(out, call = call)
  out
}

#' Set a session's circuit temperature
#'
#' The `.options temp=` of the deck, in degrees Celsius. Instance `temp`
#' values (see [spice_alter()]) still win, as in SPICE3.
#'
#' @param session A [spice_session()].
#' @param celsius The temperature, degrees C.
#' @return `spice_set_temp()`: `session`, invisibly (modified in place).
#'   `spice_temp()`: the temperature, degrees C.
#' @export
#' @examples
#' s <- spice_session("t\nv1 1 0 dc 1\nd1 1 0 dm\n.model dm d\n.op\n.end")
#' spice_set_temp(s, 85)
#' spice_temp(s)
spice_set_temp <- function(session, celsius) {
  call <- rlang::current_env()
  check_session(session, call = call)
  if (!is.numeric(celsius) || length(celsius) != 1L || is.na(celsius)) {
    cli::cli_abort(
      "{.arg celsius} must be a single number, not {.obj_type_friendly {celsius}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  failed <- session$ptr$set_temp(as.double(celsius))
  if (!is.null(failed)) abort_failure(failed, call = call)
  invisible(session)
}

#' @rdname spice_set_temp
#' @export
spice_temp <- function(session) {
  check_session(session)
  session$ptr$temp()
}

#' @export
print.spice_session <- function(x, ...) {
  st <- x$ptr$warm_stats()
  out <- cli::format_inline("{.cls spice_session} {.strong {x$ptr$title()}}")
  out <- c(
    out,
    cli::format_inline("temperature {x$ptr$temp()} \u00b0C"),
    if (isTRUE(x$ptr$warm_start())) {
      cli::format_inline("warm start on ({st[[1]]} hit{?s}, {st[[2]]} miss{?es})")
    } else {
      "warm start off"
    }
  )
  cat(out, sep = "\n")
  invisible(x)
}

check_session <- function(session, arg = rlang::caller_arg(session),
                          call = rlang::caller_env()) {
  if (!inherits(session, "spice_session")) {
    cli::cli_abort(
      "{.arg {arg}} must be a {.cls spice_session}, not {.obj_type_friendly {session}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
}

check_flag <- function(x, arg, call) {
  if (!rlang::is_bool(x)) {
    cli::cli_abort(
      "{.arg {arg}} must be TRUE or FALSE, not {.obj_type_friendly {x}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
}

# A failure list from the Rust side (`ok = FALSE`, `kind`, `message`,
# diagnostics) as the classed condition `spice_run()` raises.
abort_failure <- function(failed, call) {
  diagnostics <- tibble::tibble(
    severity = failed$diagnostics$severity,
    message = failed$diagnostics$message
  )
  abort_engine(failed$kind, failed$message, diagnostics, call = call)
}
