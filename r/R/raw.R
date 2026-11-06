#' Read and write SPICE3 rawfiles
#'
#' `spice_read_raw()` reads a SPICE3 rawfile (ASCII or binary, as
#' written by `spice3 -r`, `ngspice -r` or [spice_write_raw()]) into a
#' `spice_result`. `spice_write_raw()` writes a result's plots, or a
#' single plot, back to back as the SPICE3 `raw_write` does.
#'
#' A rawfile does not record which vector is the scale: like the 3f5
#' reader, `spice_read_raw()` takes the first vector of each plot as
#' its scale, so an operating-point plot read back has its first
#' variable as scale while the one from [spice_run()] has none. All
#' else (names, types, values, header fields) round-trips exactly
#' through the binary format; the ASCII format keeps 15 significant
#' digits, like SPICE3.
#'
#' @param path Path of the rawfile.
#' @return `spice_read_raw()`: a `spice_result` whose `notes` hold the
#'   reader's warnings and whose `title` is the first plot's title.
#' @export
#' @examples
#' res <- spice_run("rc\nvin 1 0 pulse(0 1)\nr1 1 2 1k\nc1 2 0 1u\n.tran 0.1m 5m\n.end")
#' f <- tempfile(fileext = ".raw")
#' spice_write_raw(res, f)
#' spice_read_raw(f)
spice_read_raw <- function(path) {
  if (!rlang::is_string(path)) {
    cli::cli_abort("{.arg path} must be a single string.",
      class = c("spice_error_input", "spice_error")
    )
  }
  if (!file.exists(path)) {
    cli::cli_abort("Cannot find the rawfile {.file {path}}.",
      class = c("spice_error_io", "spice_error")
    )
  }
  out <- tryCatch(
    ffi_read_raw(path.expand(path)),
    error = function(e) {
      msg <- conditionMessage(e)
      cli::cli_abort(c("Cannot read the rawfile.", "x" = "{msg}"),
        class = c("spice_error_raw", "spice_error"), call = NULL
      )
    }
  )
  title <- if (length(out$plots) > 0L) out$plots[[1L]]$title else ""
  new_spice_result(
    title, out$plots, out$warnings,
    tibble::tibble(severity = character(), message = character())
  )
}

#' @rdname spice_read_raw
#' @param x A `spice_result` or a `spice_plot`.
#' @param format `"binary"` (the default, as `spice3 -r` writes) or
#'   `"ascii"`.
#' @return `spice_write_raw()`: `path`, invisibly.
#' @export
spice_write_raw <- function(x, path, format = c("binary", "ascii")) {
  format <- rlang::arg_match(format)
  plots <- if (inherits(x, "spice_result")) {
    x$plots
  } else if (inherits(x, "spice_plot")) {
    list(x)
  } else {
    cli::cli_abort(
      "{.arg x} must be a {.cls spice_result} or a {.cls spice_plot}, not {.obj_type_friendly {x}}.",
      class = c("spice_error_input", "spice_error")
    )
  }
  if (!rlang::is_string(path)) {
    cli::cli_abort("{.arg path} must be a single string.",
      class = c("spice_error_input", "spice_error")
    )
  }
  flat <- lapply(unname(plots), plot_to_raw)
  tryCatch(
    ffi_write_raw(path.expand(path), flat, format == "binary"),
    error = function(e) {
      msg <- conditionMessage(e)
      cli::cli_abort(c("Cannot write the rawfile.", "x" = "{msg}"),
        class = c("spice_error_raw", "spice_error"), call = NULL
      )
    }
  )
  invisible(path)
}

plot_to_raw <- function(p) {
  cx <- is_complex_plot(p)
  vals <- unname(p$values)
  list(
    title = as.character(p$title),
    date = as.character(p$date),
    plotname = as.character(p$plotname),
    padded = isTRUE(p$padded),
    commands = as.character(p$commands),
    options = as.character(p$options),
    dimensions = as.double(p$dimensions),
    var_names = as.character(p$variables$name),
    var_types = as.character(p$variables$type),
    re = lapply(vals, function(v) as.double(Re(v))),
    im = if (cx) lapply(vals, function(v) as.double(Im(v))) else NULL
  )
}
