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
#' * for complex plots, `re` and `im` (dbl) instead of `value`. No
#'   magnitude or phase is derived for you: use `Mod(complex(real = re,
#'   imaginary = im))`, `Arg()`, or `20 * log10()` of the modulus.
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
#' `.ac`), the tibble has all of `value`, `re` and `im`, with `NA`
#' where a column does not apply; otherwise only the columns of its
#' kind. See [spice_plot] for the columns.
#'
#' @param result A `spice_result`, or a `spice_sweep`.
#' @param plots Optional: labels or indices of the plots to keep.
#' @return A tibble.
#' @export
#' @examples
#' res <- spice_run("rc\nvin 1 0 pulse(0 1)\nr1 1 2 1k\nc1 2 0 1u\n.tran 0.1m 5m\n.end")
#' spice_tidy(res)
spice_tidy <- function(result, plots = NULL) {
  if (inherits(result, "spice_sweep")) {
    return(sweep_tidy(result, plots))
  }
  check_result(result)
  ps <- result$plots
  if (!is.null(plots)) {
    bad <- if (is.character(plots)) {
      setdiff(plots, names(ps))
    } else if (is.numeric(plots)) {
      plots[plots < 1 | plots > length(ps) | plots != round(plots)]
    } else {
      plots
    }
    if (length(bad) > 0L) {
      cli::cli_abort(
        c(
          "{.arg plots} must name or index plots of {.arg result}.",
          "i" = "Plots: {.val {names(ps)}}."
        ),
        class = c("spice_error_input", "spice_error")
      )
    }
    ps <- ps[plots]
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
    parts <- lapply(parts, function(t) {
      for (col in c("value", "re", "im")) {
        if (is.null(t[[col]])) t[[col]] <- rep(NA_real_, nrow(t))
      }
      t[c("plot", "point", "scale_name", "scale", "variable", "type", "value", "re", "im")]
    })
  }
  out <- do.call(rbind, unname(parts))
  tibble::as_tibble(out)
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
