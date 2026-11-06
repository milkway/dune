#' spice: Circuit Simulation with a SPICE3f5-Derived Engine
#'
#' Runs SPICE3 netlists (`.op`, `.dc`, `.tran`, `.ac`, `.pz`, `.noise`,
#' `.disto`, `.tf`, `.sens`) on Dune, a circuit simulator written in
#' Rust and derived from Berkeley SPICE3f5, and returns the results as
#' tidy data.
#'
#' The main entry points are:
#'
#' * [spice_run()] to simulate a deck (a file or a character netlist);
#' * [spice_plots()] and [spice_tidy()] / [tibble::as_tibble()] to get
#'   at the results, one long tibble row per point and variable;
#' * [spice_read_raw()] and [spice_write_raw()] to exchange SPICE3
#'   rawfiles with ngspice and other tools;
#' * `ggplot2::autoplot()` for a quick look (when ggplot2 is installed).
#'
#' @keywords internal
#' @useDynLib spice, .registration = TRUE
#' @importFrom tibble as_tibble tibble
#' @importFrom rlang .data
"_PACKAGE"
