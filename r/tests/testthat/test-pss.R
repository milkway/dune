# spice_pss(): the .pss card's engine call, with its period and history.

rc_pss <- "rc low-pass, periodic steady state
v1 1 0 sin(0 1 1k)
r1 1 2 1k
c1 2 0 1u
.end"

with_card <- function(deck, card) sub(".end", paste0(card, "\n.end"), deck, fixed = TRUE)

expect_same_plots <- function(got, want) {
  expect_identical(got$title, want$title)
  expect_identical(got$notes, want$notes)
  expect_identical(got$diagnostics, want$diagnostics)
  expect_identical(names(got$plots), names(want$plots))
  for (k in names(want$plots)) {
    expect_identical(got$plots[[k]]$variables, want$plots[[k]]$variables)
    expect_identical(got$plots[[k]]$values, want$plots[[k]]$values, label = k)
  }
}

pss_names <- c(
  "Time Domain Periodic Steady State Analysis",
  "Frequency Domain Periodic Steady State Analysis"
)

test_that("spice_pss() is the .pss card, bit for bit", {
  res <- spice_pss(rc_pss, freq = 1e3, tstab = 5e-3, osc_node = "2")
  expect_s3_class(res, "spice_pss")
  expect_s3_class(res, "spice_result")
  expect_same_plots(res, spice_run(with_card(rc_pss, ".pss 1k 5m 2 128 10")))
  expect_identical(names(spice_plots(res)), pss_names)
  expect_identical(nrow(res$measurements), 0L)
})

test_that("the corpus decks match spice_run() of their cards", {
  cases <- list(
    pss_rc.cir = list(freq = 1e3, tstab = 5e-3, osc_node = "2"),
    pss_rlc.cir = list(freq = 1.5e3, tstab = 0, osc_node = "3", points = 256L),
    pss_rect.cir = list(freq = 50e3, tstab = 20e-6, osc_node = "2", points = 256L),
    pss_pulse.cir = list(freq = 1e6, tstab = 2e-6, osc_node = "3", points = 200L)
  )
  for (name in names(cases)) {
    path <- corpus_path("pss", name)
    got <- do.call(spice_pss, c(list(path), cases[[name]]))
    expect_same_plots(got, spice_run(path))
    expect_lte(got$residuals[[length(got$residuals)]], 1)
  }
})

test_that("the result carries period, t0, residuals and tidy tibbles", {
  res <- spice_pss(rc_pss, freq = 1e3, tstab = 5e-3)
  expect_equal(res$period, 1e-3)
  expect_equal(res$t0, 5e-3)
  expect_length(res$residuals, 2L)
  expect_gt(res$residuals[[1]], 1)
  expect_lte(res$residuals[[2]], 1)

  tm <- res$time
  expect_named(tm, c("time", "variable", "type", "value"))
  v2 <- tm[tm$variable == "v(2)", ]
  expect_identical(nrow(v2), 129L)
  expect_equal(range(v2$time), c(res$t0, res$t0 + res$period))
  expect_identical(v2$value, res$plots[[1]]$values[["v(2)"]])

  sp <- res$spectrum
  expect_named(sp, c("harmonic", "frequency", "variable", "type", "re", "im", "mag", "phase"))
  h1 <- sp[sp$variable == "v(2)" & sp$harmonic == 1L, ]
  expect_equal(h1$frequency, 1e3)
  # Closed form: v(2) = |H| sin(wt + arg H); the phasor is -1i * H.
  h <- 1 / complex(real = 1, imaginary = 2 * pi)
  expect_equal(h1$mag, Mod(h), tolerance = 5e-4)
  expect_equal(complex(real = h1$re, imaginary = h1$im), -1i * h, tolerance = 5e-4)
  expect_equal(h1$phase, Arg(-1i * h), tolerance = 5e-4)
  expect_identical(sp$harmonic[sp$variable == "v(2)"], 0:9)

  # spice_tidy() takes it like any result.
  td <- spice_tidy(res)
  expect_setequal(unique(td$plot), pss_names)
  expect_true(all(c("value", "re", "im") %in% names(td)))
  expect_output(print(res), "Periodic steady state: period 0.001 s")
})

test_that("freq = 0 takes the fundamental from the sources", {
  a <- spice_pss(rc_pss, tstab = 5e-3)
  b <- spice_pss(rc_pss, freq = 1e3, tstab = 5e-3)
  expect_identical(a$plots[[1]]$values, b$plots[[1]]$values)
  expect_true(any(grepl("from the sources", a$notes, fixed = TRUE)))
})

test_that("the deck's analysis cards are not run", {
  res <- spice_pss(with_card(rc_pss, ".tran 10u 1m\n.op"), freq = 1e3)
  expect_identical(names(res$plots), pss_names)
})

test_that("a shooting that does not converge is spice_error_pss with its history", {
  e <- expect_error(
    spice_pss(rc_pss, freq = 1e3, tstab = 5e-3, max_iter = 1),
    class = "spice_error_pss"
  )
  expect_s3_class(e, "spice_error")
  expect_identical(e$kind, "pss")
  expect_identical(e$iterations, 1L)
  expect_length(e$residuals, 1L)
  expect_gt(e$residuals, 1)
  expect_match(conditionMessage(e), "did not converge")
  deck <- with_card(rc_pss, ".pss 1k 5m 2 128 10 1")
  card <- expect_error(spice_run(deck), class = "spice_error_pss")
  expect_identical(card$residuals, e$residuals)
  expect_error(spice_rerun(spice_session(deck)), class = "spice_error_pss")
})

test_that("other failures keep their classes", {
  expect_error(spice_pss("dc only\nv1 1 0 dc 1\nr1 1 0 1k\n.end"), class = "spice_error_analysis")
  expect_error(spice_pss(rc_pss, freq = 1e3, osc_node = "nope"), "oscnode", class = "spice_error_analysis")
  expect_error(spice_pss("t\nq1 1 1 0\n.end"), class = "spice_error_parse")
  expect_error(spice_pss(rc_pss, options = list(reltl = 1)), class = "spice_error_options")
})

test_that("arguments are checked", {
  bad <- list(
    list(freq = -1), list(freq = Inf), list(freq = "1k"), list(tstab = -1),
    list(points = 3), list(points = 128.5), list(harmonics = 0), list(harmonics = 65),
    list(max_iter = 0), list(steady_coeff = 0), list(uic = NA), list(osc_node = 2)
  )
  for (args in bad) {
    expect_error(do.call(spice_pss, c(list(rc_pss), args)), class = "spice_error_input")
  }
})

test_that("the pss plots measure as tran and ac", {
  res <- spice_pss(rc_pss, freq = 1e3, tstab = 5e-3)
  h <- Mod(1 / complex(real = 1, imaginary = 2 * pi))
  m <- spice_measure(res$plots[[1]], c(vpp = "pp v(2)"))
  expect_identical(m$analysis, "tran")
  expect_equal(m$value, 2 * h, tolerance = 1e-3)
  a <- spice_measure(res$plots[[2]], c(h1 = "find vm(2) at=1k"))
  expect_identical(a$analysis, "ac")
  expect_equal(a$value, Mod(res$plots[[2]]$values[["v(2)"]][[2]]))
})

test_that("a session runs a .pss deck as spice_run() does", {
  deck <- with_card(rc_pss, ".pss 1k 5m 2 128 10")
  s <- spice_session(deck)
  expect_same_plots(spice_rerun(s), spice_run(deck))
  spice_alter(s, "r1", 2e3)
  expect_same_plots(spice_rerun(s), spice_run(sub("r1 1 2 1k", "r1 1 2 2k", deck, fixed = TRUE)))
})
