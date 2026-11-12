#' Simulation plots
#'
#' A `spice_plot` is one rawfile plot: a set of named vectors of the
#' same length (one per circuit variable) sharing a scale (`time`,
#' `frequency`, the swept source of a `.dc`, ...). It is a list with
#' * `label`: the plot's name within its result (see [spice_run()]);
#' * `plotname`, `title`, `date`: the rawfile header fields;
#' * `scale`: the name of the scale vector, or `NA` for plots without
#'   one (operating point, `.tf`, `.pz`, DC `.sens`, integrated noise);
#' * `variables`: a tibble with the `name` and `type` (`"voltage"`,
#'   `"current"`, `"time"`, `"frequency"`, ...) of every vector;
#' * `values`: a named list of the vectors, double for real plots and
#'   complex for complex ones (`.ac`, `.pz`, AC `.sens`, `.disto`);
#' * `padded`, `commands`, `options`, `dimensions`: the remaining
#'   rawfile header fields, kept so a plot writes back unchanged.
#'
#' @section Long format:
#' `as_tibble()` returns one row per point and non-scale variable, with
#' the scale repeated on every row:
#' * `plot` (chr): the plot's label;
#' * `point` (int): the point index, from 1;
#' * `scale_name` (chr) and `scale` (dbl): the scale vector's name and
#'   value at that point (`NA` for plots without a scale; the real part
#'   of the scale for complex plots, whose scale is real by
#'   construction);
#' * `variable` (chr) and `type` (chr): the vector's name and type;
#' * for real plots, `value` (dbl);
#' * for complex plots, instead of `value`: `re` and `im` (dbl), and
#'   derived from them `mag` (the modulus), `db` (`20 * log10(mag)`)
#'   and `phase` (the argument, in degrees).
#'
#' This is the long ("tidy") layout `ggplot2` expects:
#' `ggplot(df, aes(scale, value, colour = variable)) + geom_line()`.
#'
#' @param x A `spice_plot`.
#' @param ... Unused.
#' @return A tibble in the long format described above.
#' @name spice_plot
#' @examples
#' res <- spice_run("rc\nvin 1 0 dc 0 ac 1\nr1 1 2 1k\nc1 2 0 1u\n.ac dec 5 10 10k\n.end")
#' ac <- spice_plots(res)[[1]]
#' ac
#' tibble::as_tibble(ac)
NULL

new_spice_plot <- function(raw) {
  names <- as.character(raw$var_names)
  values <- if (isTRUE(raw$complex)) {
    Map(function(re, im) complex(real = re, imaginary = im), raw$re, raw$im)
  } else {
    lapply(raw$re, as.double)
  }
  names(values) <- names
  scale <- if (is.na(raw$scale)) NA_character_ else names[[raw$scale]]
  structure(
    list(
      label = raw$plotname,
      plotname = raw$plotname,
      title = raw$title,
      date = raw$date,
      scale = scale,
      variables = tibble::tibble(name = names, type = as.character(raw$var_types)),
      values = values,
      padded = isTRUE(raw$padded),
      commands = as.character(raw$commands),
      options = as.character(raw$options),
      dimensions = as.double(raw$dimensions)
    ),
    class = "spice_plot"
  )
}

is_complex_plot <- function(x) {
  length(x$values) > 0L && is.complex(x$values[[1L]])
}

plot_points <- function(x) {
  if (length(x$values) == 0L) 0L else length(x$values[[1L]])
}

#' @rdname spice_plot
#' @export
as_tibble.spice_plot <- function(x, ...) {
  n <- plot_points(x)
  vars <- x$variables
  keep <- if (is.na(x$scale)) {
    rep(TRUE, nrow(vars))
  } else {
    vars$name != x$scale
  }
  keep_idx <- which(keep)
  k <- length(keep_idx)
  scale <- if (is.na(x$scale)) {
    rep(NA_real_, n)
  } else {
    Re(x$values[[x$scale]])
  }
  out <- tibble::tibble(
    plot = rep(x$label, n * k),
    point = rep(seq_len(n), times = k),
    scale_name = rep(x$scale, n * k),
    scale = rep(as.double(scale), times = k),
    variable = rep(vars$name[keep_idx], each = n),
    type = rep(vars$type[keep_idx], each = n)
  )
  vals <- unlist(unname(x$values[keep_idx]), use.names = FALSE)
  if (is.null(vals)) vals <- double()
  if (is_complex_plot(x)) {
    out$re <- Re(vals)
    out$im <- Im(vals)
    out$mag <- Mod(vals)
    out$db <- 20 * log10(out$mag)
    out$phase <- Arg(vals) * 180 / pi
  } else {
    out$value <- as.double(vals)
  }
  out
}

#' Results as one long tibble
#'
#' Binds [tibble::as_tibble()] of every plot of a result. For a
#' `spice_sweep` from [spice_sweep()], binds the tidy tibble of every
#' run, with the sweep's `run` index and swept parameters as the first
#' columns (one column per parameter, constant within a run): group by
#' `run` for one curve per run. When the
#' result mixes real and complex plots (e.g. a deck with `.tran` and
#' `.ac`), the tibble has `value` and the complex columns (`re`, `im`,
#' `mag`, `db`, `phase`), with `NA` where a column does not apply;
#' otherwise only the columns of its kind. See [spice_plot] for the
#' columns.
#'
#' @section Selecting plots and variables:
#' `plots` takes the labels of [spice_plots()], their positions, or a
#' short name of the analysis, which selects every plot of that kind:
#'
#' | short name | plot |
#' |---|---|
#' | `"op"` | Operating Point |
#' | `"dc"` | DC transfer characteristic |
#' | `"tran"` | Transient Analysis |
#' | `"ac"` | AC Analysis |
#' | `"noise"` | Noise Spectral Density Curves |
#' | `"inoise"` | Integrated Noise |
#' | `"tf"` | Transfer Function |
#' | `"pz"` | Pole-Zero Analysis |
#' | `"sens"` | Sensitivity Analysis |
#' | `"sp"` | SP Analysis |
#' | `"pss"` | Time Domain Periodic Steady State Analysis |
#' | `"pss_spectrum"` | Frequency Domain Periodic Steady State Analysis |
#'
#' `vars` keeps the named variables, matched without regard to case; a
#' bare node name stands for its voltage (`"out"` is `"v(out)"`).
#'
#' @param result A `spice_result`, or a `spice_sweep`.
#' @param plots Optional: labels, short names (see below) or indices of
#'   the plots to keep.
#' @param vars Optional: names of the variables to keep.
#' @return A tibble.
#' @seealso [spice_wide()] for one column per variable, [spice_op()]
#'   for the operating point as a named vector.
#' @export
#' @examples
#' res <- spice_run("rc\nvin 1 0 pulse(0 1)\nr1 1 2 1k\nc1 2 0 1u\n.tran 0.1m 5m\n.end")
#' spice_tidy(res)
#' res |> spice_tidy("tran", "v(2)")
spice_tidy <- function(result, plots = NULL, vars = NULL) {
  call <- rlang::current_env()
  if (inherits(result, "spice_sweep")) {
    return(sweep_tidy(result, plots, vars))
  }
  check_result(result)
  ps <- select_plots(result$plots, plots, call = call)
  if (!is.null(vars)) {
    ps <- select_vars(ps, vars, call = call)
  }
  if (length(ps) == 0L) {
    return(tibble::tibble(
      plot = character(), point = integer(), scale_name = character(),
      scale = double(), variable = character(), type = character(),
      value = double()
    ))
  }
  parts <- lapply(ps, as_tibble.spice_plot)
  cx <- vapply(ps, is_complex_plot, logical(1))
  if (any(cx) && !all(cx)) {
    cols <- c("value", "re", "im", "mag", "db", "phase")
    parts <- lapply(parts, function(t) {
      for (col in cols) {
        if (is.null(t[[col]])) t[[col]] <- rep(NA_real_, nrow(t))
      }
      t[c("plot", "point", "scale_name", "scale", "variable", "type", cols)]
    })
  }
  out <- do.call(rbind, unname(parts))
  tibble::as_tibble(out)
}

# The short names of `spice_tidy()`'s `plots`: the rawfile plotname of
# each analysis.
plot_aliases <- c(
  op = "Operating Point",
  dc = "DC transfer characteristic",
  tran = "Transient Analysis",
  ac = "AC Analysis",
  noise = "Noise Spectral Density Curves",
  inoise = "Integrated Noise",
  tf = "Transfer Function",
  pz = "Pole-Zero Analysis",
  sens = "Sensitivity Analysis",
  sp = "SP Analysis",
  pss = "Time Domain Periodic Steady State Analysis",
  pss_spectrum = "Frequency Domain Periodic Steady State Analysis"
)

# The plots of `ps` (a named list of `spice_plot`) that `plots` selects:
# labels, short names (every plot of that analysis) or positions.
select_plots <- function(ps, plots, call = rlang::caller_env()) {
  if (is.null(plots)) {
    return(ps)
  }
  bad <- function() {
    cli::cli_abort(
      c(
        "{.arg plots} must name or index plots of {.arg result}.",
        "i" = "Plots: {.val {names(ps)}}.",
        "i" = "Short names: {.val {names(plot_aliases)}}."
      ),
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  if (is.numeric(plots)) {
    if (any(plots < 1 | plots > length(ps) | plots != round(plots))) bad()
    return(ps[plots])
  }
  if (!is.character(plots) || anyNA(plots)) bad()
  plotnames <- vapply(ps, function(p) p$plotname, character(1))
  idx <- integer()
  for (key in plots) {
    hit <- which(names(ps) == key)
    if (length(hit) == 0L && tolower(key) %in% names(plot_aliases)) {
      hit <- which(plotnames == plot_aliases[[tolower(key)]])
    }
    if (length(hit) == 0L) bad()
    idx <- c(idx, hit)
  }
  ps[unique(idx)]
}

# The positions in `names` (a plot's vectors) that `vars` selects:
# case-insensitive, a bare node name standing for its voltage.
match_vars <- function(names, vars) {
  low <- tolower(names)
  key <- tolower(vars)
  which(low %in% key | low %in% paste0("v(", key, ")"))
}

# `ps` with only the vectors `vars` selects (and each plot's scale).
# Every entry of `vars` must select something in at least one plot.
select_vars <- function(ps, vars, call = rlang::caller_env()) {
  if (!is.character(vars) || anyNA(vars)) {
    cli::cli_abort(
      "{.arg vars} must be a character vector of variable names.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  all_names <- unique(unlist(lapply(ps, function(p) p$variables$name)))
  missing <- vars[vapply(vars, function(v) length(match_vars(all_names, v)) == 0L, logical(1))]
  if (length(missing) > 0L) {
    cli::cli_abort(
      c(
        "{.arg vars} names {cli::qty(length(missing))}variable{?s} the selected plot{?s} do{?es/} not have: {.val {missing}}.",
        "i" = "Variables: {.val {all_names}}."
      ),
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  lapply(ps, function(p) {
    keep <- match_vars(p$variables$name, vars)
    if (!is.na(p$scale)) keep <- union(which(p$variables$name == p$scale), keep)
    keep <- sort(keep)
    p$variables <- p$variables[keep, ]
    p$values <- p$values[keep]
    p
  })
}

#' One plot with a column per variable
#'
#' The wide layout: one row per point, the plot's scale first (`time`,
#' `frequency`, the swept source, ...) and then one column per
#' variable, named as the variable. Columns of a complex plot (`.ac`)
#' are complex vectors, so `Mod()`, `Arg()` and arithmetic between
#' signals work directly. Use it when the computation is across
#' variables (`` `v(out)` - `v(in)` ``); [spice_tidy()] is the layout
#' 'ggplot2' wants.
#'
#' @param result A `spice_result`, or a `spice_plot`.
#' @param plot The plot: a label, short name (see [spice_tidy()]) or
#'   position. Optional when the result has a single plot.
#' @param vars Optional: names of the variables to keep, as in
#'   [spice_tidy()].
#' @return A tibble with `nrow` = the plot's points.
#' @export
#' @examples
#' res <- spice_run("rc\nvin 1 0 pulse(0 1)\nr1 1 2 1k\nc1 2 0 1u\n.tran 0.1m 5m\n.end")
#' w <- res |> spice_wide("tran")
#' w
#' # The voltage across r1
#' w$`v(1)` - w$`v(2)`
spice_wide <- function(result, plot = NULL, vars = NULL) {
  call <- rlang::current_env()
  if (inherits(result, "spice_plot")) {
    ps <- list(result)
  } else {
    check_result(result)
    ps <- select_plots(result$plots, plot, call = call)
    if (length(ps) != 1L) {
      cli::cli_abort(
        c(
          "{.fn spice_wide} takes one plot, and {.arg plot} selects {length(ps)}.",
          "i" = "Plots: {.val {names(result$plots)}}."
        ),
        class = c("spice_error_input", "spice_error"),
        call = call
      )
    }
  }
  if (!is.null(vars)) {
    ps <- select_vars(ps, vars, call = call)
  }
  p <- ps[[1L]]
  values <- p$values
  if (!is.na(p$scale)) {
    values[[p$scale]] <- as.double(Re(values[[p$scale]]))
    values <- values[c(p$scale, setdiff(names(values), p$scale))]
  }
  tibble::as_tibble(values, .name_repair = "minimal")
}

#' The operating point as a named vector
#'
#' The values of a result's operating-point plot (`.op`), named by
#' variable: node voltages and source currents.
#'
#' @param result A `spice_result` with an operating point.
#' @param vars Optional: names of the variables to keep, as in
#'   [spice_tidy()], in the order of the plot.
#' @return A named double vector.
#' @export
#' @examples
#' res <- spice_run("divider\nv1 1 0 dc 10\nr1 1 2 1k\nr2 2 0 3k\n.op\n.end")
#' spice_op(res)
#' res |> spice_op("v(2)")
spice_op <- function(result, vars = NULL) {
  call <- rlang::current_env()
  check_result(result)
  plotnames <- vapply(result$plots, function(p) p$plotname, character(1))
  hit <- which(plotnames == plot_aliases[["op"]])
  if (length(hit) == 0L) {
    cli::cli_abort(
      c(
        "{.arg result} has no operating point.",
        "i" = "Add a {.code .op} card to the deck. Plots: {.val {names(result$plots)}}."
      ),
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  ps <- result$plots[hit[1L]]
  if (!is.null(vars)) {
    ps <- select_vars(ps, vars, call = call)
  }
  vapply(ps[[1L]]$values, function(v) as.double(v[[1L]]), double(1))
}

plot_summary <- function(x) {
  n <- plot_points(x)
  nv <- nrow(x$variables)
  kind <- if (is_complex_plot(x)) "complex" else "real"
  scale <- if (is.na(x$scale)) "" else paste0(", scale ", x$scale)
  label <- if (identical(x$label, x$plotname)) x$label else paste0(x$label, " (", x$plotname, ")")
  sprintf("%s: %d variable%s x %d point%s, %s%s",
    label, nv, if (nv == 1L) "" else "s", n, if (n == 1L) "" else "s", kind, scale)
}

#' @export
print.spice_plot <- function(x, ...) {
  s <- plot_summary(x)
  out <- cli::format_inline("{.cls spice_plot} {s}")
  n <- plot_points(x)
  vars <- x$variables
  show <- utils::head(seq_len(nrow(vars)), 12L)
  lines <- vapply(show, function(i) {
    v <- x$values[[i]]
    rng <- if (length(v) == 0L) {
      ""
    } else if (is.complex(v)) {
      m <- Mod(v)
      sprintf("|.| in [%s, %s]", format(min(m), digits = 4), format(max(m), digits = 4))
    } else if (n == 1L) {
      format(v, digits = 6)
    } else {
      sprintf("[%s, %s]", format(min(v), digits = 4), format(max(v), digits = 4))
    }
    sprintf("%s <%s> %s", vars$name[[i]], vars$type[[i]], rng)
  }, "")
  out <- c(out, paste0("  ", lines))
  if (nrow(vars) > length(show)) {
    more <- nrow(vars) - length(show)
    out <- c(out, cli::format_inline("... and {more} more variable{?s}"))
  }
  cat(out, sep = "\n")
  invisible(x)
}
