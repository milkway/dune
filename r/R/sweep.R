#' Run a deck over a set of parameter values (sweeps and Monte Carlo)
#'
#' Runs a SPICE3 deck once per assignment of the swept parameters. The
#' deck stays pure SPICE3 (there is no `.step` card): it is parsed once,
#' and each run writes its values into the parsed deck, builds the
#' circuit afresh and runs every analysis card, exactly as [spice_run()]
#' would on the deck edited by hand -- bit for bit.
#'
#' The simulator holds no random generator: a Monte Carlo is a `"zip"`
#' sweep over draws you make with [stats::rnorm()] and friends, so it is
#' reproducible from your own [set.seed()].
#'
#' @section Parameter names:
#' Matched case-insensitively:
#' * `temp`: circuit temperature in degrees Celsius (as `.options temp=`);
#' * an element name: `r1`, `c1`, `l1` for its value, `vin`, `iin` for a
#'   source's DC level;
#' * the ngspice form `@name[param]`: `@r1[resistance]`,
#'   `@c1[capacitance]`, `@l1[inductance]`, `@vin[dc]`, `@vin[acmag]`,
#'   `@vin[acphase]` (degrees), `@m1[w]`, `@m1[l]` (MOSFET width and
#'   length, m). Non-syntactic names need backticks:
#'   `` spice_sweep(deck, `@m1[w]` = c(2e-6, 4e-6)) ``.
#'
#' Elements inside subcircuits go by their expanded names (`r.x1.r1`).
#' Model parameters are not sweepable yet.
#'
#' @param deck The deck, as in [spice_run()].
#' @param ... Named numeric vectors: swept parameter = its values. A
#'   data frame or list of draws can be spliced with `!!!`.
#' @param .mode `"grid"` runs every combination, the first parameter
#'   varying slowest (as `tidyr::expand_grid()`); `"zip"` runs row by
#'   row and needs equal lengths (length-one values are recycled).
#' @param .includes,.options As `includes` and `options` in
#'   [spice_run()]; the options apply to every run (a swept `temp`
#'   overrides `options$temp`).
#' @param .threads Worker threads: `1` (the default) runs sequentially,
#'   `0` uses one per core, `n` uses `n`. Results are identical either
#'   way: every run is an independent circuit.
#'
#' @return A `spice_sweep`: a list with
#'   * `title`: the deck's title line;
#'   * `params`: a tibble with `run` (the run index, from 1) and one
#'     double column per swept parameter, named in canonical form
#'     (`"r1"`, `"@m1[w]"`, `"temp"`);
#'   * `runs`: a list of `spice_result`, one per run;
#'   * `diagnostics`: the deck's parser warnings, as in [spice_run()].
#'
#'   [spice_tidy()] turns it into one long tibble.
#'
#' @section Errors:
#' An unknown element or parameter, a parameter given twice or a
#' non-finite value is a `spice_error_sweep`. A run that fails to build
#' or simulate stops the sweep with the error class of [spice_run()]
#' (`spice_error_elaborate`, `spice_error_analysis`), naming the run
#' and its values.
#'
#' @section Mirror of the Python package:
#' This is the R face of `spice.sweep()` in Python. Run indices are
#' 1-based here and 0-based in Python.
#'
#' @seealso [spice_tidy()], [spice_run()].
#' @export
#' @examples
#' deck <- "rc low-pass
#' vin 1 0 dc 0 ac 1
#' r1 1 2 1k
#' c1 2 0 1u
#' .ac dec 5 10 10k
#' .end"
#' sw <- spice_sweep(deck, r1 = c(500, 1000, 2000))
#' sw
#' head(spice_tidy(sw))
#'
#' # Monte Carlo: 5% resistor and capacitor, reproducible from the seed
#' set.seed(1)
#' mc <- spice_sweep(deck,
#'   r1 = rnorm(20, 1000, 50), c1 = rnorm(20, 1e-6, 5e-8),
#'   .mode = "zip"
#' )
#' mc$params
spice_sweep <- function(deck, ..., .mode = c("grid", "zip"), .includes = NULL,
                        .options = NULL, .threads = 1L) {
  call <- rlang::current_env()
  .mode <- rlang::arg_match(.mode)
  src <- check_deck(deck, call = call)
  inc <- check_includes(.includes, is_path = src$is_path, call = call)
  opt <- check_options(.options, call = call)
  threads <- check_threads(.threads, call = call)
  params <- check_sweep_params(rlang::list2(...), call = call)

  values <- if (.mode == "grid") {
    sweep_grid(params)
  } else {
    sweep_zip(params, call = call)
  }

  out <- ffi_sweep_deck(
    src$deck, src$is_path, as.character(names(inc)), unname(as.character(inc)),
    as.character(names(opt)), unname(opt),
    names(params), as.double(unlist(values, use.names = FALSE)), threads
  )
  diagnostics <- tibble::tibble(
    severity = out$diagnostics$severity,
    message = out$diagnostics$message
  )
  if (!isTRUE(out$ok)) {
    if (identical(out$kind, "sweep")) {
      cli::cli_abort(
        c("Invalid sweep.", "x" = "{out$message}"),
        class = c("spice_error_sweep", "spice_error"),
        kind = "sweep",
        call = call
      )
    }
    abort_engine(out$kind, out$message, diagnostics, call = call)
  }

  keys <- as.character(out$params)
  n <- length(values[[1L]])
  tab <- tibble::tibble(run = seq_len(n))
  for (j in seq_along(keys)) tab[[keys[[j]]]] <- as.double(values[[j]])

  runs <- vector("list", n)
  for (i in seq_len(n)) {
    r <- out$runs[[i]]
    if (!isTRUE(r$ok)) {
      vals <- paste0(keys, "=", format(vapply(values, `[[`, 0, i), digits = 6), collapse = ", ")
      abort_engine(r$kind, paste0("run ", i, " (", vals, "): ", r$message),
        diagnostics,
        call = call
      )
    }
    runs[[i]] <- new_spice_result(out$title, r$plots, r$notes, diagnostics[0, ])
  }
  structure(
    list(title = out$title, params = tab, runs = runs, diagnostics = diagnostics),
    class = "spice_sweep"
  )
}

check_threads <- function(threads, call) {
  ok <- is.numeric(threads) && length(threads) == 1L && !is.na(threads) &&
    threads >= 0 && threads == round(threads) && threads < .Machine$integer.max
  if (!ok) {
    cli::cli_abort(
      "{.arg .threads} must be a single non-negative whole number.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  as.integer(threads)
}

check_sweep_params <- function(params, call) {
  # A spliced data frame arrives as its columns already; a single
  # unnamed data frame or list is spliced for convenience.
  if (length(params) == 1L && is.null(names(params)) && is.list(params[[1L]])) {
    params <- as.list(params[[1L]])
  }
  if (length(params) == 0L) {
    cli::cli_abort(
      "Give at least one swept parameter in {.arg ...}, as {.code name = values}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  if (!rlang::is_named(params)) {
    cli::cli_abort(
      "Every swept parameter in {.arg ...} must be named.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  for (nm in names(params)) {
    v <- params[[nm]]
    if (!is.numeric(v) || length(v) == 0L || anyNA(v) || any(!is.finite(v))) {
      cli::cli_abort(
        "Values of {.field {nm}} must be a non-empty numeric vector of finite numbers.",
        class = c("spice_error_input", "spice_error"),
        call = call
      )
    }
  }
  lapply(params, as.double)
}

# Cartesian product, first parameter slowest (tidyr::expand_grid order).
sweep_grid <- function(params) {
  lens <- lengths(params)
  total <- prod(lens)
  out <- vector("list", length(params))
  for (j in seq_along(params)) {
    inner <- prod(lens[-seq_len(j)])
    out[[j]] <- rep(rep(params[[j]], each = inner), length.out = total)
  }
  names(out) <- names(params)
  out
}

sweep_zip <- function(params, call) {
  lens <- lengths(params)
  n <- max(lens)
  if (any(lens != n & lens != 1L)) {
    cli::cli_abort(
      c(
        "In {.code .mode = \"zip\"} every parameter needs the same number of values.",
        "i" = "Lengths: {paste0(names(params), '=', lens, collapse = ', ')}."
      ),
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  lapply(params, rep_len, length.out = n)
}

#' @export
print.spice_sweep <- function(x, ...) {
  n <- nrow(x$params)
  keys <- setdiff(names(x$params), "run")
  out <- cli::format_inline("{.cls spice_sweep} {.strong {x$title}}")
  out <- c(out, cli::format_inline("{n} run{?s} over {.field {keys}}"))
  if (n > 0L) {
    out <- c(out, cli::format_inline("plots: {.val {names(x$runs[[1L]]$plots)}}"))
  }
  cat(out, sep = "\n")
  invisible(x)
}

sweep_tidy <- function(sweep, plots) {
  params <- sweep$params
  parts <- lapply(seq_along(sweep$runs), function(i) {
    t <- spice_tidy(sweep$runs[[i]], plots = plots)
    lead <- params[rep(i, nrow(t)), , drop = FALSE]
    tibble::as_tibble(c(as.list(lead), as.list(t)))
  })
  # Runs of one deck share their plots; mixed real/complex columns
  # line up because spice_tidy() pads them the same way in every run.
  tibble::as_tibble(do.call(rbind, parts))
}
