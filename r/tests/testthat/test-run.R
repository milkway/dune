test_that("an operating point from a text deck", {
  res <- spice_run(divider)
  expect_s3_class(res, "spice_result")
  expect_equal(res$title, "divider")
  expect_named(spice_plots(res), "Operating Point")
  op <- spice_plots(res)[[1]]
  expect_s3_class(op, "spice_plot")
  expect_true(is.na(op$scale))
  expect_equal(op$values[["v(2)"]], 1, tolerance = 1e-9)
  expect_equal(op$values[["i(v1)"]], -1e-3, tolerance = 1e-9)
})

test_that("a character vector is one line per element", {
  res <- spice_run(strsplit(divider, "\n")[[1]])
  expect_equal(spice_plots(res)[[1]]$values[["v(2)"]], 1, tolerance = 1e-9)
})

test_that("rc.cir transient matches the analytic response and ngspice", {
  res <- spice_run(corpus_path("spice3f5", "examples", "rc.cir"))
  expect_equal(res$title, "Basic RC circuit")
  tr <- spice_plots(res)[["Transient Analysis"]]
  expect_equal(tr$scale, "time")
  expect_equal(tr$variables$name, c("time", "v(1)", "v(2)", "i(vin)"))
  expect_equal(tr$variables$type, c("time", "voltage", "voltage", "current"))
  t <- tr$values$time
  v2 <- tr$values[["v(2)"]]
  expect_equal(t[[1]], 0)
  expect_equal(t[[length(t)]], 7)
  # pulse(0 1): a ramp over TR = TSTEP = 0.1 into RC = 1, so after
  # the ramp v(2) = 1 - (e^0.1 - 1)/0.1 * e^-t.
  analytic <- function(t) 1 - (exp(0.1) - 1) / 0.1 * exp(-t)
  expect_equal(v2[[length(v2)]], analytic(7), tolerance = 1e-3)
  expect_equal(approx(t, v2, xout = 1)$y, analytic(1), tolerance = 2e-3)
  expect_equal(approx(t, v2, xout = 3)$y, analytic(3), tolerance = 2e-3)

  # And against the ngspice-47 golden on its own grid.
  gold <- spice_plots(spice_read_raw(corpus_path("golden", "rc.raw")))[[1]]
  ours <- approx(t, v2, xout = gold$values$time)$y
  expect_lt(max(abs(ours - gold$values[["v(2)"]])), 1e-3)
})

test_that("a multi-analysis deck gives one plot per analysis, in deck order", {
  res <- spice_run(corpus_path("spice3f5", "examples", "rca3040.cir"))
  expect_length(spice_plots(res), 3L)
  expect_named(
    spice_plots(res),
    c("AC Analysis", "DC transfer characteristic", "Transient Analysis")
  )
  expect_equal(
    vapply(spice_plots(res), `[[`, "", "scale"),
    c("AC Analysis" = "frequency", "DC transfer characteristic" = "v(v-sweep)",
      "Transient Analysis" = "time")
  )
})

test_that("repeated analyses get unique labels", {
  deck <- paste(sub("\\.op", ".op\n.op", divider))
  res <- spice_run(deck)
  expect_named(spice_plots(res), c("Operating Point", "Operating Point.1"))
  expect_equal(unique(spice_tidy(res)$plot), c("Operating Point", "Operating Point.1"))
})

test_that("includes are served from the list for text decks", {
  deck <- "inc\n.include models.lib\nv1 1 0 dc 1\nd1 1 0 dmod\n.op\n.end"
  expect_error(spice_run(deck), class = "spice_error_parse")
  res <- spice_run(deck, includes = list(models.lib = ".model dmod d is=1e-14\n"))
  expect_named(spice_plots(res), "Operating Point")
  expect_error(
    spice_run(corpus_path("spice3f5", "examples", "rc.cir"), includes = list(a = "b")),
    class = "spice_error_input"
  )
})

test_that("a deck file resolves .include relative to itself", {
  dir <- test_tempdir()
  writeLines(".model dmod d is=1e-14", file.path(dir, "models.lib"))
  writeLines(
    c("inc", ".include models.lib", "v1 1 0 dc 1", "r1 1 2 1k", "d1 2 0 dmod", ".op", ".end"),
    file.path(dir, "deck.cir")
  )
  res <- spice_run(file.path(dir, "deck.cir"))
  v2 <- spice_plots(res)[[1]]$values[["v(2)"]]
  expect_gt(v2, 0.4)
  expect_lt(v2, 0.8)
})

test_that("a missing deck file is an io error", {
  expect_error(spice_run("no/such/deck.cir"), class = "spice_error_io")
  expect_error(spice_run(42), class = "spice_error_input")
})

test_that("parse errors carry the span in the condition message", {
  bad <- "t\nv1 1 0 dc 1\nq1 1 1 0\n.op\n.end\n"
  err <- expect_error(spice_run(bad), class = "spice_error_parse")
  expect_s3_class(err, "spice_error")
  expect_match(conditionMessage(err), "deck.cir:3:8: error: missing model name", fixed = TRUE)
  expect_equal(err$kind, "parse")
  expect_true("error" %in% err$diagnostics$severity)

  # From a file, the span names the file.
  f <- tempfile(fileext = ".cir")
  writeLines(bad, f)
  err <- expect_error(spice_run(f), class = "spice_error_parse")
  expect_match(conditionMessage(err), paste0(basename(f), ":3:8:"), fixed = TRUE)
})

test_that("engine failures are classified", {
  expect_error(
    spice_run("t\nv1 1 0 dc 1\nr1 1 0 rmod\n.op\n.end"),
    class = "spice_error_elaborate"
  )
  expect_error(
    spice_run("t\nv1 1 0 dc 1\nr1 1 0 1k\n.dc vnope 0 1 0.1\n.end"),
    class = "spice_error_analysis"
  )
})

test_that("unknown options are an error listing the valid ones", {
  err <- expect_error(spice_run(divider, options = list(reltl = 1e-4)),
    class = "spice_error_options"
  )
  msg <- cli::ansi_strip(conditionMessage(err))
  expect_match(msg, "reltl", fixed = TRUE)
  expect_match(msg, "reltol", fixed = TRUE)
  expect_match(msg, "defas", fixed = TRUE)
  expect_error(spice_run(divider, options = list(1e-4)), class = "spice_error_options")
  expect_error(spice_run(divider, options = list(reltol = "a")), class = "spice_error_options")
  expect_error(spice_run(divider, options = list(itl1 = 2.5)), class = "spice_error_options")
  expect_error(spice_run(divider, options = list(method = "euler")), class = "spice_error_options")
  expect_error(spice_run(divider, options = list(nopage = 1)), class = "spice_error_options")
  expect_error(
    spice_run(divider, options = list(reltol = 1e-4, RELTOL = 1e-5)),
    class = "spice_error_options"
  )
})

test_that("options override the deck and take effect", {
  rc <- "rc\nvin 1 0 pulse(0 1)\nr1 1 2 1k\nc1 2 0 1u\n.tran 0.1m 5m\n.end"
  n_default <- length(spice_plots(spice_run(rc))[[1]]$values$time)
  n_tight <- length(spice_plots(spice_run(rc, options = list(reltol = 1e-6)))[[1]]$values$time)
  expect_gt(n_tight, n_default)
  expect_s3_class(
    spice_run(rc, options = list(method = "gear", maxord = 3L, nopage = TRUE)),
    "spice_result"
  )
  # temp in degrees Celsius: a diode's forward drop falls when hot.
  d <- "d\nv1 1 0 dc 1\nr1 1 2 1k\nd1 2 0 dmod\n.model dmod d is=1e-14\n.op\n.end"
  vd <- function(...) spice_plots(spice_run(d, ...))[[1]]$values[["v(2)"]]
  expect_lt(vd(options = list(temp = 100)), vd())
  # The deck's own .options apply when not overridden.
  d2 <- sub(".op", ".options temp=100\n.op", d, fixed = TRUE)
  expect_equal(vd2 <- spice_plots(spice_run(d2))[[1]]$values[["v(2)"]], vd(options = list(temp = 100)))
  expect_equal(spice_plots(spice_run(d2, options = list(temp = 27)))[[1]]$values[["v(2)"]], vd())
})

test_that("print methods are compact", {
  res <- spice_run(corpus_path("spice3f5", "examples", "rca3040.cir"))
  expect_output(print(res), "3 plots")
  expect_output(print(res$plots[[1]]), "more variables")
  utils::capture.output(vis <- withVisible(print(res)))
  expect_false(vis$visible)
  expect_identical(vis$value, res)
})
