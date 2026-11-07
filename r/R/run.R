#' Run a SPICE3 deck
#'
#' Parses a SPICE3 netlist, builds the circuit and runs every analysis
#' card in deck order, exactly as `spice3 -b` (and `dune -b`) would.
#'
#' @param deck Either the path to a deck file, or the netlist itself as
#'   a character vector: a single string containing newlines, or one
#'   element per line. A length-one string without a newline is taken
#'   as a path. As in SPICE3, the first line of a netlist is its title.
#'   A `spice_pic` (the extraction of a pic figure, [spice_pic()]) runs as
#'   its netlist, with the `.include` files that came with the figure; one
#'   whose diagnostics include an error is refused (`spice_error_pic`).
#' @param includes For a netlist given as text only: the files its
#'   `.include` cards may read, as a named list (or named character
#'   vector) mapping the path exactly as written on the card to the
#'   file's text. Nothing is read from disk for a text deck. A deck
#'   given as a path resolves `.include` on the file system, relative to
#'   the deck's own directory, and must leave `includes` as `NULL`; so
#'   must a `spice_pic`.
#' @param options `NULL` or a named list of simulator options that
#'   override the deck's `.options` card, with the card's names and
#'   units (temperatures `temp`/`tnom` in degrees Celsius): `reltol`,
#'   `abstol`, `vntol`, `gmin`, `itl1`, `itl2`, `itl4`, `gminsteps`,
#'   `srcsteps`, `temp`, `tnom`, `pivtol`, `pivrel`, `noopiter`,
#'   `trtol`, `chgtol`, `method` (`"trap"` or `"gear"`), `maxord`,
#'   `nopage`, `defl`, `defw`, `defad`, `defas`. `noopiter` and `nopage`
#'   take `TRUE`/`FALSE`. Unlike a `.options` card, which only warns, an
#'   unknown name or a value of the wrong type is an error.
#' @param tmax_divisor `NULL` (the default) or a finite number `>= 1` that
#'   divides the maximum step of every `.tran` card -- its TMAX argument,
#'   else the default `min(TSTEP, (TSTOP - TSTART) / 50)`. The run is, bit
#'   for bit, that of the deck with the card's TMAX edited to
#'   `TMAX / tmax_divisor`; with `options = list(trtol = 1)` it gives the
#'   "fine steps" view of a waveform. It is an argument rather than an
#'   `options` name because TMAX is a parameter of the analysis card, not
#'   an engine option.
#'
#' @return A `spice_result`: a list with
#'   * `title`: the deck's title line;
#'   * `plots`: a named list of `spice_plot` objects, one per rawfile
#'     plot, in the order `spice3 -b -r` writes them (one for most
#'     analyses; `.noise` adds an integrated-noise plot when the band has
#'     more than one frequency; `.disto` gives two or three). Names are
#'     the plot names (`"Transient Analysis"`, ...) made unique with
#'     [make.unique()] when an analysis appears twice;
#'   * `notes`: non-fatal remarks from the engine;
#'   * `diagnostics`: a tibble (`severity`, `message`) of the parser's
#'     warnings, each message prefixed with its `file:line:col`;
#'   * `measurements`: the deck's `.measure` cards (ngspice grammar),
#'     each evaluated on every plot of its analysis, as the tibble
#'     [spice_measure()] returns (zero rows without `.measure` cards).
#'     Failed measurements are rows with `error` set.
#'
#' @section Errors:
#' Failures are signalled with [cli::cli_abort()] as conditions of class
#' `spice_error_<kind>` and `spice_error`, where `kind` is the stage
#' that failed: `"options"`, `"io"` (unreadable deck file), `"parse"`,
#' `"elaborate"` (building the circuit), `"analysis"` or `"pss"` (a
#' `.pss` card whose shooting did not converge; see [spice_pss()] for
#' the condition's `iterations` and `residuals` fields) (and, from
#' [spice_alter()], `"alter"`; for a `spice_pic` with extraction errors,
#' `"pic"`). Parse errors
#' carry the `file:line:col` of the offending card in the message; the
#' condition's `diagnostics` field holds every diagnostic seen so far.
#'
#' @section Mirror of the Python package:
#' This is the R face of `spice.simulate()` in the Python package of
#' the same name. Both run the same engine with the same semantics;
#' the R side returns S3 objects and long tibbles where Python returns
#' polars frames.
#'
#' A long simulation cannot be interrupted from R: the call returns
#' when the engine finishes.
#'
#' @seealso [spice_plots()], [spice_tidy()], [spice_write_raw()].
#' @export
#' @examples
#' deck <- "rc low-pass
#' vin 1 0 pulse(0 1)
#' r1 1 2 1k
#' c1 2 0 1u
#' .tran 0.1m 5m
#' .end"
#' res <- spice_run(deck)
#' res
#' head(spice_tidy(res))
#'
#' # Options override the deck's .options card
#' spice_run(deck, options = list(reltol = 1e-4, method = "gear"))
#'
#' # Finer transient steps: trtol = 1 and a tenth of the maximum step
#' spice_run(deck, options = list(trtol = 1), tmax_divisor = 10)
spice_run <- function(deck, includes = NULL, options = NULL, tmax_divisor = NULL) {
  call <- rlang::current_env()
  src <- check_deck(deck, call = call)
  inc <- check_includes(includes, is_path = src$is_path, call = call, pic = src$pic_includes)
  opt <- check_options(options, call = call)
  div <- check_tmax_divisor(tmax_divisor, call = call)

  out <- ffi_run_deck(
    src$deck, src$is_path, as.character(names(inc)), unname(as.character(inc)),
    as.character(names(opt)), unname(opt), div
  )
  diagnostics <- tibble::tibble(
    severity = out$diagnostics$severity,
    message = out$diagnostics$message
  )
  if (!isTRUE(out$ok)) {
    abort_engine(out$kind, out$message, diagnostics, call = call, failure = out)
  }
  new_spice_result(out$title, out$plots, out$notes, diagnostics, out$measurements)
}

new_spice_result <- function(title, raw_plots, notes, diagnostics, measurements = NULL) {
  plots <- lapply(raw_plots, new_spice_plot)
  labels <- make.unique(vapply(plots, `[[`, "", "plotname"))
  for (i in seq_along(plots)) plots[[i]]$label <- labels[[i]]
  names(plots) <- labels
  structure(
    list(
      title = title,
      plots = plots,
      notes = as.character(notes),
      diagnostics = diagnostics,
      measurements = measurements_tibble(measurements, labels)
    ),
    class = "spice_result"
  )
}

# `failure` is the Rust side's failure list; for kind `pss` its
# shooting history (`iterations`, `residuals`) rides on the condition.
abort_engine <- function(kind, message, diagnostics, call, failure = NULL) {
  what <- switch(kind,
    options = "Invalid simulator options.",
    io = "Cannot read the deck.",
    parse = "The deck has errors.",
    elaborate = "Cannot build the circuit.",
    analysis = "The analysis failed.",
    pss = "The periodic steady state did not converge.",
    alter = "Cannot alter the circuit.",
    "The simulation failed."
  )
  extra <- list()
  if (identical(kind, "pss") && !is.null(failure)) {
    extra <- list(
      iterations = as.integer(failure$iterations),
      residuals = as.double(failure$residuals)
    )
  }
  rlang::inject(cli::cli_abort(
    c(what, "x" = "{message}"),
    class = c(paste0("spice_error_", kind), "spice_error"),
    kind = kind,
    diagnostics = diagnostics,
    !!!extra,
    call = call
  ))
}

check_deck <- function(deck, call) {
  if (inherits(deck, "spice_pic")) {
    return(pic_deck(deck, call = call))
  }
  if (!is.character(deck) || length(deck) == 0L || anyNA(deck)) {
    cli::cli_abort(
      "{.arg deck} must be a file path or a character netlist, not {.obj_type_friendly {deck}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  if (length(deck) == 1L && !grepl("\n", deck, fixed = TRUE)) {
    path <- path.expand(deck)
    if (!file.exists(path) || dir.exists(path)) {
      cli::cli_abort(
        c(
          "Cannot find the deck file {.file {deck}}.",
          "i" = "A netlist given as text needs at least two lines (the first is the title)."
        ),
        class = c("spice_error_io", "spice_error"),
        call = call
      )
    }
    return(list(deck = path, is_path = TRUE))
  }
  list(deck = paste(deck, collapse = "\n"), is_path = FALSE)
}

# `pic`: the includes of a `spice_pic` deck (`check_deck()`'s
# `pic_includes`), which come with the figure.
check_includes <- function(includes, is_path, call, pic = NULL) {
  if (!is.null(pic)) {
    if (!is.null(includes)) {
      cli::cli_abort(
        c(
          "{.arg includes} does not apply to a {.cls spice_pic}.",
          "i" = "Its {.code .include} files come with the figure: pass them to {.fn spice_pic}."
        ),
        class = c("spice_error_input", "spice_error"),
        call = call
      )
    }
    return(pic)
  }
  if (is.null(includes)) {
    return(character())
  }
  if (is_path) {
    cli::cli_abort(
      c(
        "{.arg includes} applies to a netlist given as text only.",
        "i" = "A deck file reads its {.code .include} files from disk, relative to itself."
      ),
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  ok <- (is.list(includes) || is.character(includes)) &&
    (length(includes) == 0L || rlang::is_named(includes)) &&
    all(vapply(includes, rlang::is_string, logical(1)))
  if (!ok) {
    cli::cli_abort(
      "{.arg includes} must be a named list of strings (path = file text).",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  vapply(includes, identity, character(1))
}

#' Names of the simulator options accepted by `spice_run()`
#' @noRd
spice_option_names <- function() ffi_option_names()

check_options <- function(options, call) {
  if (is.null(options) || length(options) == 0L) {
    return(list())
  }
  if (!is.list(options) || !rlang::is_named(options)) {
    cli::cli_abort(
      "{.arg options} must be a named list, not {.obj_type_friendly {options}}.",
      class = c("spice_error_options", "spice_error"),
      call = call
    )
  }
  nms <- tolower(names(options))
  valid <- spice_option_names()
  valid_list <- cli::cli_vec(valid, list("vec-trunc" = length(valid)))
  unknown <- setdiff(nms, valid)
  if (length(unknown) > 0L) {
    cli::cli_abort(
      c(
        "Unknown simulator option{?s} {.val {unknown}}.",
        "i" = "Valid options: {.val {valid_list}}."
      ),
      class = c("spice_error_options", "spice_error"),
      call = call
    )
  }
  dup <- unique(nms[duplicated(nms)])
  if (length(dup) > 0L) {
    cli::cli_abort(
      "Simulator option{?s} {.val {dup}} given more than once.",
      class = c("spice_error_options", "spice_error"),
      call = call
    )
  }
  values <- lapply(seq_along(options), function(i) {
    v <- options[[i]]
    if (length(v) != 1L || is.na(v) ||
      !(is.numeric(v) || is.logical(v) || is.character(v))) {
      cli::cli_abort(
        "Option {.field {nms[[i]]}} must be a single non-missing number, logical or string.",
        class = c("spice_error_options", "spice_error"),
        call = call
      )
    }
    if (is.integer(v)) as.double(v) else unname(v)
  })
  names(values) <- nms
  values
}

# `NULL` or a finite number >= 1, as the double vector the FFI takes
# (`numeric(0)` for none).
check_tmax_divisor <- function(tmax_divisor, arg = rlang::caller_arg(tmax_divisor),
                               call = rlang::caller_env()) {
  if (is.null(tmax_divisor)) {
    return(numeric(0))
  }
  if (!is.numeric(tmax_divisor) || length(tmax_divisor) != 1L ||
    is.na(tmax_divisor) || !is.finite(tmax_divisor) || tmax_divisor < 1) {
    cli::cli_abort(
      "{.arg {arg}} must be {.code NULL} or a single finite number >= 1, not {.obj_type_friendly {tmax_divisor}}.",
      class = c("spice_error_options", "spice_error"),
      call = call
    )
  }
  as.double(tmax_divisor)
}

#' Plots of a simulation result
#'
#' @param result A `spice_result` from [spice_run()] or [spice_read_raw()].
#' @return A named list of `spice_plot` objects in rawfile order.
#' @export
#' @examples
#' res <- spice_run("divider\nv1 1 0 dc 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.dc v1 0 2 1\n.end")
#' names(spice_plots(res))
spice_plots <- function(result) {
  check_result(result)
  result$plots
}

check_result <- function(result, arg = rlang::caller_arg(result),
                         call = rlang::caller_env()) {
  if (!inherits(result, "spice_result")) {
    cli::cli_abort(
      "{.arg {arg}} must be a {.cls spice_result}, not {.obj_type_friendly {result}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
}

#' @export
print.spice_result <- function(x, ...) {
  # cli formats, cat() prints: print methods write to stdout.
  out <- cli::format_inline("{.cls spice_result} {.strong {x$title}}")
  n <- length(x$plots)
  if (n == 0L) {
    out <- c(out, "No plots.")
  } else {
    out <- c(out, cli::format_inline("{n} plot{?s}:"))
    out <- c(out, bullets("*", vapply(x$plots, plot_summary, "")))
  }
  if (length(x$notes) > 0L) {
    out <- c(out, cli::format_inline("{length(x$notes)} note{?s}:"), bullets("i", x$notes))
  }
  nm <- if (is.null(x$measurements)) 0L else nrow(x$measurements)
  if (nm > 0L) {
    m <- x$measurements
    shown <- ifelse(is.na(m$error), format(m$value, digits = 6), paste("failed:", m$error))
    out <- c(out, cli::format_inline("{nm} measurement{?s}:"), bullets("*", paste(m$name, "=", shown)))
  }
  nd <- nrow(x$diagnostics)
  if (nd > 0L) {
    out <- c(out, cli::format_inline("{nd} diagnostic{?s}:"), bullets("!", x$diagnostics$message))
  }
  cat(out, sep = "\n")
  invisible(x)
}

# Bullet lines of literal text (never interpolated by cli).
bullets <- function(kind, text) {
  text <- as.character(text)
  sym <- switch(kind,
    "*" = cli::symbol$bullet,
    "i" = cli::col_cyan(cli::symbol$info),
    "!" = cli::col_yellow("!")
  )
  if (length(text) == 0L) character() else paste(sym, text)
}
