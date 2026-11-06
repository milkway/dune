#' A knitr engine for SPICE3 decks
#'
#' Loading spice registers a `spice` engine with knitr (when knitr is
#' installed), so an R Markdown or Quarto document can hold a deck in a
#' chunk of its own and plot the results in the R chunks that follow:
#'
#' ````
#' ```{spice rc}
#' rc low-pass
#' vin 1 0 pulse(0 1)
#' r1 1 2 1k
#' c1 2 0 1u
#' .tran 0.1m 5m
#' .end
#' ```
#'
#' ```{r}
#' ggplot2::autoplot(spice_plots(rc)[[1]])
#' ```
#' ````
#'
#' The chunk body is the netlist, title line first, exactly as
#' [spice_run()] takes it as text (`.include` cards are not read from
#' disk; see `spice.includes` below). The engine runs it and assigns the
#' `spice_result` to a variable in the knitting environment.
#'
#' @section Chunk options:
#' * `echo`: show the deck (default `TRUE`, as for any chunk).
#' * `spice.output` (or `output`): what the chunk prints. `"tidy"`
#'   (default) prints [spice_tidy()] of the result, as a tibble prints
#'   (its first rows), followed by any notes and parser diagnostics;
#'   `"raw"` prints the result itself (see [spice_run()]: the plots and
#'   their sizes, notes and diagnostics). `results = "hide"` prints
#'   nothing. `output` is read only when it is one of these two
#'   strings, so Quarto's own `output: false` keeps its meaning.
#' * `spice.var`: the name of the variable that receives the result.
#'   Defaults to the chunk label; chunks without a label (knitr's
#'   `unnamed-chunk-N`) assign nothing unless `spice.var` is given.
#'   `spice.var = FALSE` assigns nothing. A label such as `rc-tran` is
#'   used as is, and is then read back as `` `rc-tran` ``.
#' * `spice.options`: a named list passed as `options` to [spice_run()].
#' * `spice.includes`: a named list passed as `includes` to
#'   [spice_run()].
#'
#' @section Errors:
#' A deck that fails to parse, build or simulate stops the knitting
#' with the `spice_error` of [spice_run()], whose message carries the
#' `file:line:col` of the offending card (the chunk body is the file
#' `deck.cir`). With the chunk option `error = TRUE` (or `0`) the
#' message is printed in the document instead and knitting goes on.
#' Note that [knitr::knit()] defaults to `error = TRUE`, while R
#' Markdown and Quarto default to `error = FALSE`.
#'
#' @name spice_knitr
#' @aliases spice-knitr
#' @examplesIf rlang::is_installed("knitr")
#' # The engine knitr calls for a ```{spice} chunk:
#' "spice" %in% names(knitr::knit_engines$get())
NULL

# Called by .onLoad(): register now when knitr is loaded, otherwise as
# soon as it is (spice never loads knitr itself).
register_knitr_engine <- function() {
  if (isNamespaceLoaded("knitr")) {
    set_knitr_engine()
  } else {
    setHook(packageEvent("knitr", "onLoad"), function(...) set_knitr_engine())
  }
  invisible()
}

set_knitr_engine <- function() {
  knitr::knit_engines$set(spice = eng_spice)
}

eng_spice <- function(options) {
  code <- as.character(options$code)
  if (!isTRUE(options$eval)) {
    return(knitr::engine_output(options, code, character()))
  }
  mode <- knitr_output_mode(options)
  # A trailing newline keeps even a one-line body a text netlist.
  deck <- paste0(paste(code, collapse = "\n"), "\n")
  res <- tryCatch(
    spice_run(
      deck,
      includes = options[["spice.includes"]],
      options = options[["spice.options"]]
    ),
    spice_error = function(e) {
      # error = TRUE, or knitr's numeric equivalent 0, keeps knitting.
      err <- options$error
      if (!(isTRUE(err) || (is.numeric(err) && length(err) == 1L && err == 0))) {
        stop(e)
      }
      e
    }
  )
  if (inherits(res, "spice_error")) {
    out <- paste("Error:", conditionMessage(res))
    return(knitr::engine_output(options, code, out))
  }
  var <- knitr_result_var(options)
  if (!is.null(var)) {
    # The environment knit() evaluates the document in (its `envir`).
    knit_env <- knitr::knit_global()
    assign(var, res, envir = knit_env)
  }
  out <- if (identical(mode, "raw")) {
    utils::capture.output(print(res))
  } else {
    knitr_tidy_lines(res)
  }
  knitr::engine_output(options, code, out)
}

knitr_output_mode <- function(options) {
  mode <- options[["spice.output"]]
  if (is.null(mode)) {
    out <- options[["output"]]
    mode <- if (rlang::is_string(out) && out %in% c("tidy", "raw")) out else "tidy"
  }
  if (!rlang::is_string(mode) || !mode %in% c("tidy", "raw")) {
    cli::cli_abort(
      "Chunk option {.field spice.output} must be {.val tidy} or {.val raw}.",
      class = c("spice_error_input", "spice_error"),
      call = NULL
    )
  }
  mode
}

knitr_result_var <- function(options) {
  var <- options[["spice.var"]]
  if (isFALSE(var)) {
    return(NULL)
  }
  if (!is.null(var)) {
    if (!rlang::is_string(var) || !nzchar(var)) {
      cli::cli_abort(
        "Chunk option {.field spice.var} must be a variable name or {.code FALSE}.",
        class = c("spice_error_input", "spice_error"),
        call = NULL
      )
    }
    return(var)
  }
  label <- options$label
  if (!rlang::is_string(label) || !nzchar(label)) {
    return(NULL)
  }
  prefix <- knitr::opts_knit$get("unnamed.chunk.label")
  if (is.null(prefix)) prefix <- "unnamed-chunk"
  if (startsWith(label, paste0(prefix, "-"))) {
    return(NULL)
  }
  label
}

knitr_tidy_lines <- function(res) {
  out <- utils::capture.output(print(spice_tidy(res)))
  if (length(res$notes) > 0L) {
    out <- c(out, paste("note:", res$notes))
  }
  if (nrow(res$diagnostics) > 0L) {
    out <- c(out, res$diagnostics$message)
  }
  out
}
