#' Quick look at a plot with ggplot2
#'
#' Draws every non-scale variable of a plot against its scale, one
#' line per variable, faceted by type (voltages and currents get their
#' own panels). Complex plots show the magnitude in dB. Requires the
#' ggplot2 package; for anything more specific, start from
#' [tibble::as_tibble()] of the plot.
#'
#' @param object A `spice_plot`.
#' @param ... Unused.
#' @return A ggplot object.
#' @exportS3Method ggplot2::autoplot
#' @examplesIf rlang::is_installed("ggplot2")
#' res <- spice_run("rc\nvin 1 0 pulse(0 1)\nr1 1 2 1k\nc1 2 0 1u\n.tran 0.1m 5m\n.end")
#' ggplot2::autoplot(spice_plots(res)[[1]])
autoplot.spice_plot <- function(object, ...) {
  rlang::check_installed("ggplot2", reason = "to draw a spice_plot.")
  if (is.na(object$scale)) {
    cli::cli_abort(
      "Plot {.val {object$label}} has no scale to draw against.",
      class = c("spice_error_input", "spice_error")
    )
  }
  df <- as_tibble.spice_plot(object)
  ylab <- "value"
  if (is_complex_plot(object)) {
    df$value <- 20 * log10(sqrt(df$re^2 + df$im^2))
    ylab <- "magnitude (dB)"
  }
  p <- ggplot2::ggplot(
    df,
    ggplot2::aes(x = .data$scale, y = .data$value, colour = .data$variable)
  ) +
    ggplot2::geom_line() +
    ggplot2::labs(x = object$scale, y = ylab, colour = NULL, title = object$label)
  if (length(unique(df$type)) > 1L) {
    p <- p + ggplot2::facet_wrap(ggplot2::vars(.data$type), ncol = 1L, scales = "free_y")
  }
  if (identical(object$variables$type[object$variables$name == object$scale], "frequency")) {
    p <- p + ggplot2::scale_x_log10()
  }
  p
}
