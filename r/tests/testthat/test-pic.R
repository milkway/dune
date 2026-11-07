# spice_pic() over the real figure corpus (corpus/pic). Mirrored test for
# test by python/tests/test_pic.py (same figures, same assertions): every
# figure extracts to its golden netlist byte for byte, simulates exactly as
# its golden deck, and draws.

# The CLI's texlabels test figure (crates/dune-cli/tests/texlabels.rs):
# display labels in math.
math_figure <- '.PS
spice = 1
texlabels = 1
B: (0.3,1.2)
Vcc: vsource((3.9,2.6), (3.9,0.3), 5, "$V_{CC}$")
Vb: vsource((0.3,1.2), (0.3,0.3), 0.8, "$G_1$")
Rb: resistor((0.3,1.2), (1.72,1.2), 10k)
Rc: resistor((2.36,2.6), (2.36,1.62), 1k)
Q1: npn((2.2,1.2), qn, "$T_1$")
line from Rc.P to Vcc.P
line from Q1.E to (Q1.E.x,0.3)
line from Vb.N to Vcc.N
dot at (Q1.E.x,0.3)
ground((1.2,0.3))
.model qn npn(bf=100)
.op
.PE
'
math_literals <- c("$T_1$", "$G_1$", "$V_{CC}$")

# R1 has no value (an error), R2.N is dangling (a warning).
flawed_figure <- ".PS
spice = 1
V1: vsource((0,1), (0,0), DC 1)
R1: resistor((0,1), (1,1))
R2: resistor((1,1), (2,1), 1k)
line from R1.N to (1,0) then to V1.N; ground(V1.N)
.op
.PE
"

# Line 4 names a label that does not exist: rpic rejects the figure.
broken_figure <- ".PS
spice = 1
R1: resistor((0,0), (1,0), 1k)
line from Q9.C to (1,1)
.PE
"

# R1.P sits 5 mil off V1.P: apart at the default snap (1 mil).
gap_figure <- ".PS
spice = 1
V1: vsource((0,1), (0,0), DC 2)
R1: resistor((0,1.005), (1,1.005), 1k)
R2: resistor((1,1.005), (1,0), 1k)
line from R2.N to V1.N; ground(V1.N)
.op
.PE
"

pic_figures <- function() {
  dir <- corpus_path("pic")
  figs <- sort(Sys.glob(file.path(dir, "*.pic")))
  figs[basename(figs) != "spice.pic"]
}

read_bytes <- function(path) readChar(path, file.size(path), useBytes = TRUE)

same_result <- function(a, b) {
  pa <- spice_plots(a)
  pb <- spice_plots(b)
  identical(names(pa), names(pb)) &&
    all(vapply(seq_along(pa), function(i) identical(pa[[i]]$values, pb[[i]]$values), TRUE))
}

test_that("every corpus figure extracts to its golden and simulates as it", {
  figs <- pic_figures()
  expect_gte(length(figs), 27L)
  for (fig in figs) {
    name <- sub("\\.pic$", "", basename(fig))
    golden <- read_bytes(file.path(dirname(fig), "golden", paste0(name, ".cir")))
    p <- spice_pic(fig)
    expect_equal(nrow(p$diagnostics), 0L, label = name)
    expect_identical(p$netlist, golden, label = name)
    expect_identical(p$path, fig)
    # The golden deck, given the figure's includes (it sits in golden/,
    # away from them), runs exactly as the extraction.
    inc <- if (length(p$includes) > 0L) as.list(p$includes) else NULL
    expect_true(same_result(spice_run(p), spice_run(golden, includes = inc)), label = name)
    svg <- spice_pic_svg(fig)
    expect_true(startsWith(svg, "<svg") && endsWith(trimws(svg), "</svg>"), label = name)
    expect_identical(spice_pic_svg(p), svg)
  }
})

test_that("the map of rc.pic", {
  p <- spice_pic(corpus_path("pic", "rc.pic"))
  m <- p$map
  expect_identical(m$elements$refdes, c("V1", "R1", "C1"))
  expect_identical(m$pins$refdes, c("V1", "V1", "R1", "R1", "C1", "C1"))
  expect_identical(m$pins$pin, c("P", "N", "P", "N", "P", "N"))
  expect_identical(m$pins$net, c("Vin", "0", "Vin", "Vout", "Vout", "0"))
  r1 <- m$elements[m$elements$refdes == "R1", ]
  expect_identical(r1$principal, "resistance")
  expect_identical(r1$value, "1k")
  expect_identical(c(r1$value_line, r1$value_col), c(6L, 25L))
  v1 <- m$params[m$params$refdes == "V1", ]
  expect_identical(v1$name, "pulse")
  expect_identical(v1$text, "PULSE(0 5 0 1u 1u 10m 20m)")
  expect_identical(r1$ids, paste0("s", r1$shape_start, "..s", r1$shape_end - 1L))
  expect_identical(m$nets$name[[1]], "0")
  expect_gt(length(m$nets$shapes[[1]]), 0L)
  expect_gt(m$canvas$w, 0)
  expect_gt(m$canvas$h, 0)
  expect_identical(m$canvas$rescale, 1)
  # The source line the value span points at holds the value.
  line <- strsplit(p$source, "\n", fixed = TRUE)[[1]][[r1$value_line]]
  expect_identical(substr(line, r1$value_col, r1$value_end_col - 1L), "1k")
  # Pins land inside the SVG page.
  sx <- (m$pins$x + m$canvas$origin[[1]]) * 96
  sy <- (m$canvas$h - (m$pins$y + m$canvas$origin[[2]])) * 96
  expect_true(all(sx >= 0 & sx <= m$canvas$w * 96 & sy >= 0 & sy <= m$canvas$h * 96))
})

test_that("the opamp's .include resolves next to the figure", {
  fig <- corpus_path("pic", "opamp.pic")
  p <- spice_pic(fig)
  sub <- read_bytes(corpus_path("pic", "opamp_ideal.sub"))
  expect_identical(p$includes, c(opamp_ideal.sub = sub))
  dc <- spice_plots(spice_run(p))[[1]]
  vin <- dc$values[["v(vin)"]]
  vout <- dc$values[["v(vout)"]]
  # Finite-gain inverting amplifier, A = 200k.
  gain <- -10 / (1 + 11 / 200e3)
  expect_true(all(abs(vout - gain * vin) <= pmax(1e-5 * abs(gain * vin), 1e-9)))
  # Figure text has no directory: its includes are given, or missing.
  text <- read_bytes(fig)
  e <- expect_error(spice_run(spice_pic(text)), class = "spice_error_parse")
  expect_match(conditionMessage(e), "opamp_ideal.sub", fixed = TRUE)
  given <- spice_pic(text, includes = list(opamp_ideal.sub = sub))
  expect_true(same_result(spice_run(given), spice_run(p)))
  expect_error(spice_run(p, includes = list(opamp_ideal.sub = sub)), class = "spice_error_input")
})

test_that("texlabels typeset math labels", {
  plain <- sub("texlabels = 1\n", "", math_figure, fixed = TRUE)
  # Presentation only: the netlist does not change.
  expect_identical(spice_pic(math_figure)$netlist, spice_pic(plain)$netlist)
  lit <- spice_pic_svg(plain)
  for (s in math_literals) expect_true(grepl(s, lit, fixed = TRUE))
  expect_identical(lengths(regmatches(lit, gregexpr("<svg", lit, fixed = TRUE))), 1L)
  svg <- spice_pic_svg(math_figure)
  if (spice_pic_texlabels()) {
    for (s in math_literals) expect_false(grepl(s, svg, fixed = TRUE))
    expect_gte(lengths(regmatches(svg, gregexpr("<svg x=", svg, fixed = TRUE))), 3L)
  } else {
    for (s in math_literals) expect_true(grepl(s, svg, fixed = TRUE))
  }
})

test_that("extraction diagnostics are data and block the run", {
  p <- spice_pic(flawed_figure)
  d <- p$diagnostics
  expect_identical(d$severity, c("error", "warning"))
  expect_identical(d$kind, c("missing-value", "dangling-pin"))
  expect_identical(d$file, c(NA_character_, NA_character_))
  expect_identical(d$line, c(4L, 5L))
  expect_identical(d$column, c(5L, 5L))
  expect_false(anyNA(d$end_column))
  expect_match(p$netlist, "R1 n1 0\n", fixed = TRUE)
  e <- expect_error(spice_run(p), class = "spice_error_pic")
  expect_identical(e$kind, "pic")
  expect_identical(c(e$line, e$column), c(4L, 5L))
  expect_error(spice_session(p), class = "spice_error_pic")
})

test_that("compile errors are conditions with their position", {
  e <- expect_error(spice_pic(broken_figure), class = "spice_error_pic")
  expect_s3_class(e, "spice_error")
  expect_identical(e$kind, "pic")
  expect_identical(c(e$line, e$column), c(4L, 11L))
  expect_identical(nrow(e$diagnostics), 1L)
  expect_identical(e$diagnostics$kind, "compile")
  expect_identical(e$diagnostics$file, NA_character_)
  expect_identical(e$diagnostics$line, 4L)
  expect_match(conditionMessage(e), "Q9", fixed = TRUE)
  expect_error(spice_pic_svg(broken_figure), class = "spice_error_pic")
})

test_that("unmarked figures draw but do not extract", {
  fig <- ".PS\nbox\n.PE\n"
  e <- expect_error(spice_pic(fig), class = "spice_error_pic")
  expect_identical(e$diagnostics$kind, "not-marked")
  expect_identical(e$line, NA_integer_)
  expect_true(startsWith(spice_pic_svg(fig), "<svg"))
})

test_that("snap joins near terminals", {
  apart <- spice_pic(gap_figure)
  expect_true(any(apart$diagnostics$kind == "dangling-pin"))
  joined <- spice_pic(gap_figure, snap = 0.01)
  expect_identical(nrow(joined$diagnostics), 0L)
  op <- spice_plots(spice_run(joined))[[1]]
  expect_equal(op$values[["v(n2)"]], 1, tolerance = 1e-9)
})

test_that("bad arguments", {
  rc <- corpus_path("pic", "rc.pic")
  expect_error(spice_pic(gap_figure, snap = -1), class = "spice_error_input")
  expect_error(spice_pic(rc, includes = list()), class = "spice_error_input")
  expect_error(spice_pic(file.path(tempdir(), "missing.pic")), class = "spice_error_io")
  expect_error(spice_pic(42), class = "spice_error_input")
  expect_s3_class(spice_pic(rc), "spice_pic")
  expect_output(print(spice_pic(rc)), "3 elements")
})
