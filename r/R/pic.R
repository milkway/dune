#' Extract the netlist of a pic figure
#'
#' Compiles an [rpic](https://github.com/milkway/rpic-lang) figure drawn
#' with the dune `spice.pic` prelude (`resistor()`, `vsource()`, `npn()`,
#' ...; the figure opts in with `spice = 1`) and extracts its SPICE3
#' netlist: one card per labeled element (the label is the refdes),
#' nodes from the wires, ground blocks and named places, and the
#' figure's column-1 `.` lines as the deck's analysis and control cards.
#' The result runs directly: `spice_pic("fig.pic") |> spice_run()`.
#'
#' Extraction problems in a figure that compiles -- a dangling pin, a
#' floating net, an element without a value -- are not errors here: the
#' netlist is still emitted and the problems are listed in
#' `diagnostics`. Running a figure whose diagnostics include an error
#' (with [spice_run()], [spice_session()], [spice_sweep()] or
#' [spice_pss()]) is refused with a `spice_error_pic` condition, as
#' `dune pic --run` refuses it.
#'
#' @param src The figure: the path to a `.pic` file, or the figure text
#'   as a character vector (a single string containing newlines, or one
#'   element per line). A length-one string without a newline is taken
#'   as a path, as in [spice_run()].
#' @param snap `NULL` (the default, 0.001 inch) or the distance, in pic
#'   inches, under which two terminals are the same point. A positive
#'   finite number.
#' @param includes For a figure given as text only: the files its
#'   netlist's `.include` cards read, as in [spice_run()] -- a named list
#'   (or named character vector) mapping the path written on the card to
#'   the file's text. A figure given as a path reads its `.include` files
#'   from disk, relative to the figure's own directory (as
#'   `dune pic --run` does), at extraction time, and must leave
#'   `includes` as `NULL`.
#'
#' @return A `spice_pic`: a list with
#'   * `netlist`: the SPICE3 netlist, one string ending in `.end`;
#'   * `diagnostics`: a tibble with one row per extraction problem:
#'     `severity` (`"error"` or `"warning"`), `kind` (`"dangling-pin"`,
#'     `"floating-net"`, `"missing-value"`, `"duplicate-refdes"`,
#'     `"unknown-element"`, `"missing-pin"`, `"unlabeled-element"`,
#'     `"node-name-conflict"`, `"prelude"`), `message`, and the span in
#'     the figure: `file` (`NA` for the figure itself, `"spice.pic"` for
#'     the prelude), 1-based `line`, `column` and an exclusive
#'     `end_column`;
#'   * `map`: the overlay map from netlist names back to the drawing of
#'     [spice_pic_svg()] (the SVG group `<g id="sN">` is shape `N`):
#'     - `elements`: a tibble, one row per card in card order: `refdes`;
#'       `shape_start`/`shape_end`, the half-open range of the element's
#'       shape numbers, and `ids`, the same as `"sN..sM"`; `principal`,
#'       the parameter `spice_alter(s, refdes, value)` changes (`NA` for
#'       `M`, `S`, `W`, `X`, `T`, `O`, `K`); `value`, the value argument
#'       verbatim as written in the figure, and its range `value_line`,
#'       `value_col`, `value_end_line`, `value_end_col` (1-based, end
#'       exclusive, columns in characters) -- all `NA` when there is no
#'       range an editor can trust (no value argument, a user wrapper
#'       macro, a `K`);
#'     - `pins`: a tibble, one row per terminal in card order: `refdes`,
#'       `pin` (the prelude's pin name, `P`, `N`, `C`, `B`, `E`, ...),
#'       `net` (the node name on the card) and the point `x`, `y` in pic
#'       inches;
#'     - `params`: a tibble of the alterable parameters spelled inside
#'       each value (`refdes`, `name` as [spice_alter()] names it, `text`,
#'       `line`, `col`, `end_line`, `end_col`);
#'     - `nets`: a tibble of the named nets (`name`, and `shapes`, a list
#'       of the wire, dot and ground shape numbers of each);
#'     - `canvas`: the SVG frame, `list(w, h, origin, rescale)`: the page
#'       size in inches (the `viewBox` is `0 0 w*96 h*96`) and the pic
#'       origin; the point `(x, y)` is at SVG user units
#'       `((x + origin[1]) * 96, (h - (y + origin[2])) * 96)`;
#'   * `includes`: the `.include` files the netlist reads, as a named
#'     character vector (path as written on the card = text);
#'   * `source`: the figure text;
#'   * `path`: the figure's path, or `NA` for figure text.
#'
#' @section Errors:
#' A figure that yields no netlist is signalled with [cli::cli_abort()]
#' as a condition of class `spice_error_pic` (and `spice_error`): rpic
#' rejected it (a syntax error, an unknown label, ...) or it is not
#' marked `spice = 1`. The message carries the figure's `line:col`; the
#' condition's fields `line` and `column` hold them (`NA` when there is
#' no position), and `diagnostics` holds the one diagnostic, of kind
#' `"compile"` or `"not-marked"`. An unreadable figure file is a
#' `spice_error_io`, a bad `snap` a `spice_error_input`.
#'
#' @section Mirror of the Python package:
#' This is `spice.pic.extract()` in Python; `spice_run(spice_pic(...))`
#' is `spice.simulate(spice.pic.extract(...))`.
#'
#' @seealso [spice_pic_svg()] to draw the figure, [spice_run()] to
#'   simulate it.
#' @export
#' @examples
#' fig <- ".PS
#' spice = 1
#' In: (0,1); Out: (2,1)
#' V1: vsource(In, (0,0), DC 5)
#' R1: resistor(In, Out, 1k)
#' C1: capacitor(Out, (2,0), 1u)
#' line from C1.N to V1.N; ground(V1.N)
#' .tran 10u 5m
#' .PE"
#' p <- spice_pic(fig)
#' p
#' cat(p$netlist)
#' p$map$pins
#'
#' # The extraction runs as a deck
#' res <- spice_run(p)
#' tail(spice_plots(res)[[1]]$values[["v(out)"]], 1)
spice_pic <- function(src, snap = NULL, includes = NULL) {
  call <- rlang::current_env()
  fig <- check_figure(src, call = call)
  inc <- check_includes(includes, is_path = fig$is_path, call = call)
  snap <- check_snap(snap, call = call)

  out <- ffi_pic_extract(
    fig$src, fig$is_path, as.character(names(inc)), unname(as.character(inc)), snap
  )
  if (!isTRUE(out$ok)) {
    abort_pic(out, call = call)
  }
  includes <- as.character(out$include_texts)
  names(includes) <- as.character(out$include_names)
  structure(
    list(
      netlist = out$netlist,
      diagnostics = pic_diagnostics(out$diagnostics),
      map = pic_map(out$map),
      includes = includes,
      source = out$source,
      path = if (fig$is_path) fig$src else NA_character_
    ),
    class = "spice_pic"
  )
}

#' Draw a pic figure as SVG
#'
#' Compiles a figure with the dune `spice.pic` prelude, as [spice_pic()]
#' does, and renders it to SVG: the drawing whose shape numbers
#' `spice_pic()`'s overlay map refers to. A figure need not be marked
#' `spice = 1` to be drawn, and a figure whose extraction has errors still
#' draws.
#'
#' Labels written entirely in `$...$` (`"$T_1$"`, `"$V_{CC}$"`) are
#' typeset as math when the figure turns on rpic's `texlabels = 1` and the
#' package was built with its `texlabels` feature (the default; check with
#' `spice_pic_texlabels()`); otherwise they stay literal. Typesetting never
#' changes the netlist.
#'
#' @param x The figure: a `spice_pic` (its `source` is drawn), a path to a
#'   `.pic` file or the figure text, as in [spice_pic()].
#' @return The SVG document, a single string.
#' @section Errors:
#' A figure rpic rejects is a `spice_error_pic` condition with its
#' `line` and `column`, as for [spice_pic()].
#' @section Mirror of the Python package:
#' This is `spice.pic.render()` in Python; `spice_pic_texlabels()` is
#' `spice.pic.TEXLABELS`.
#' @export
#' @examples
#' fig <- '.PS
#' spice = 1
#' texlabels = 1
#' V1: vsource((0,1), (0,0), DC 5, "$V_1$")
#' R1: resistor((0,1), (1.5,1), 1k)
#' line from R1.N to (1.5,0) then to V1.N; ground(V1.N)
#' .op
#' .PE'
#' svg <- spice_pic_svg(fig)
#' substr(svg, 1, 60)
#' spice_pic_texlabels()
spice_pic_svg <- function(x) {
  call <- rlang::current_env()
  fig <- if (inherits(x, "spice_pic")) {
    list(src = x$source, is_path = FALSE)
  } else {
    check_figure(x, call = call)
  }
  out <- ffi_pic_svg(fig$src, fig$is_path)
  if (!isTRUE(out$ok)) {
    abort_pic(out, call = call)
  }
  out$svg
}

#' @rdname spice_pic_svg
#' @return `spice_pic_texlabels()`: `TRUE` when this build typesets
#'   `$...$` labels.
#' @export
spice_pic_texlabels <- function() {
  ffi_pic_texlabels()
}

#' @export
print.spice_pic <- function(x, ...) {
  n_err <- sum(x$diagnostics$severity == "error")
  n_warn <- sum(x$diagnostics$severity == "warning")
  what <- if (is.na(x$path)) "figure text" else x$path
  cli::cat_line(
    "<spice_pic> ", what, ": ", nrow(x$map$elements), " element",
    if (nrow(x$map$elements) == 1L) "" else "s", ", ",
    nrow(x$map$nets), " named net", if (nrow(x$map$nets) == 1L) "" else "s",
    ", ", n_err, " error", if (n_err == 1L) "" else "s", ", ",
    n_warn, " warning", if (n_warn == 1L) "" else "s"
  )
  lines <- strsplit(x$netlist, "\n", fixed = TRUE)[[1]]
  cli::cat_line("  ", lines)
  for (i in seq_len(nrow(x$diagnostics))) {
    d <- x$diagnostics[i, ]
    at <- if (is.na(d$line)) "" else paste0(d$line, ":", d$column, ": ")
    cli::cat_line("! ", at, d$severity, ": ", d$message)
  }
  invisible(x)
}

# --- helpers ---------------------------------------------------------

check_figure <- function(src, call) {
  if (!is.character(src) || length(src) == 0L || anyNA(src)) {
    cli::cli_abort(
      "{.arg src} must be a figure path or the figure text, not {.obj_type_friendly {src}}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  if (length(src) == 1L && !grepl("\n", src, fixed = TRUE)) {
    path <- path.expand(src)
    if (!file.exists(path) || dir.exists(path)) {
      cli::cli_abort(
        c(
          "Cannot find the figure file {.file {src}}.",
          "i" = "A figure given as text needs at least two lines ({.code .PS} ... {.code .PE})."
        ),
        class = c("spice_error_io", "spice_error"),
        call = call
      )
    }
    return(list(src = path, is_path = TRUE))
  }
  list(src = paste(src, collapse = "\n"), is_path = FALSE)
}

check_snap <- function(snap, call) {
  if (is.null(snap)) {
    return(numeric())
  }
  if (!is.numeric(snap) || length(snap) != 1L || !is.finite(snap) || snap <= 0) {
    cli::cli_abort(
      "{.arg snap} must be a single positive finite number (pic inches), or {.code NULL}.",
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  as.double(snap)
}

pic_diagnostics <- function(d) {
  tibble::tibble(
    severity = as.character(d$severity),
    kind = as.character(d$kind),
    message = as.character(d$message),
    file = as.character(d$file),
    line = as.integer(d$line),
    column = as.integer(d$column),
    end_column = as.integer(d$end_column)
  )
}

pic_map <- function(m) {
  list(
    elements = tibble::as_tibble(m$elements),
    pins = tibble::as_tibble(m$pins),
    params = tibble::as_tibble(m$params),
    nets = tibble::tibble(name = as.character(m$nets$name), shapes = m$nets$shapes),
    canvas = m$canvas
  )
}

# A failure list from ffi_pic_extract()/ffi_pic_svg() as a condition.
abort_pic <- function(out, call) {
  diagnostics <- pic_diagnostics(out$diagnostics)
  kind <- out$kind
  if (identical(kind, "io")) {
    cli::cli_abort(
      c("Cannot read the figure.", "x" = "{out$message}"),
      class = c("spice_error_io", "spice_error"),
      kind = "io",
      call = call
    )
  }
  if (identical(kind, "options")) {
    cli::cli_abort(
      c("Invalid extraction options.", "x" = "{out$message}"),
      class = c("spice_error_input", "spice_error"),
      call = call
    )
  }
  first <- if (nrow(diagnostics) > 0L) diagnostics[1L, ] else NULL
  what <- if (!is.null(first) && identical(first$kind, "not-marked")) {
    "The figure is not a SPICE figure."
  } else {
    "The figure does not compile."
  }
  cli::cli_abort(
    c(what, "x" = "{out$message}"),
    class = c("spice_error_pic", "spice_error"),
    kind = "pic",
    line = if (is.null(first)) NA_integer_ else first$line,
    column = if (is.null(first)) NA_integer_ else first$column,
    diagnostics = diagnostics,
    call = call
  )
}

# A spice_pic handed to spice_run() & co.: its netlist as a text deck,
# its includes as the deck's; refused when extraction found errors.
pic_deck <- function(x, call) {
  errs <- x$diagnostics[x$diagnostics$severity == "error", ]
  if (nrow(errs) > 0L) {
    first <- errs[1L, ]
    at <- if (is.na(first$line)) "" else paste0(first$line, ":", first$column, ": ")
    cli::cli_abort(
      c(
        "The figure has extraction errors; its netlist is not runnable.",
        "x" = "{at}{first$message}",
        "i" = "See the {.field diagnostics} of the {.cls spice_pic}."
      ),
      class = c("spice_error_pic", "spice_error"),
      kind = "pic",
      line = first$line,
      column = first$column,
      diagnostics = x$diagnostics,
      call = call
    )
  }
  list(deck = x$netlist, is_path = FALSE, pic_includes = x$includes)
}
