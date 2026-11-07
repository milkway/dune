# spice_session(): alter + rerun is spice_run() of the altered deck.

session_deck <- "session deck
vin 1 0 dc 1 ac 1 pulse(0 1 1u 1u 1u 4u 10u)
r1 1 2 1k
c1 2 0 1n
l1 2 3 1u
r2 3 0 50
d1 2 0 dm
m1 4 2 0 0 nm l=2u w=10u
rd 5 4 10k
vdd 5 0 dc 5
.model dm d is=1e-14 cjo=1p
.model nm nmos level=1 vto=0.7 kp=50u cgso=1n cgdo=1n
.op
.dc vin 0 2 0.5
.ac dec 5 1k 1meg
.tran 0.2u 10u
.end"

# The double SPICE3's card parser reads for <mantissa>e<k>: integer
# mantissa times a power of ten (`10u` is 10 * 1e-6, not 1e-05).
card <- function(mantissa, k = 0) mantissa * as.numeric(paste0("1e", k))

# Identical: same plots, vectors bit for bit, notes and diagnostics.
expect_same_result <- function(got, want) {
  expect_identical(got$title, want$title)
  expect_identical(got$notes, want$notes)
  expect_identical(got$diagnostics, want$diagnostics)
  expect_identical(names(got$plots), names(want$plots))
  for (k in names(want$plots)) {
    expect_identical(got$plots[[k]]$variables, want$plots[[k]]$variables)
    expect_identical(got$plots[[k]]$values, want$plots[[k]]$values, label = k)
  }
}

test_that("an unaltered rerun is spice_run()", {
  s <- spice_session(session_deck)
  expect_s3_class(s, "spice_session")
  expect_same_result(spice_rerun(s), spice_run(session_deck))
  expect_output(print(s), "warm start off")
})

test_that("R, C, L, sources, W/L and TEMP match the altered deck", {
  s <- spice_session(session_deck)
  s |>
    spice_alter("R1", 2048) |>
    spice_alter("c1", capacitance = 2e-9) |>
    spice_alter("l1", 2e-6) |>
    spice_alter("vin", dc = card(15, -1), acphase = 30) |>
    spice_alter("vin", pulse = c(0, 2, card(c(1, 1, 1, 4, 10), -6))) |>
    spice_alter("m1", w = card(20, -6), l = card(4, -6)) |>
    spice_set_temp(60)
  altered <- session_deck
  for (r in list(
    c("r1 1 2 1k", "r1 1 2 2048"),
    c("c1 2 0 1n", "c1 2 0 2n"),
    c("l1 2 3 1u", "l1 2 3 2u"),
    c("dc 1 ac 1 pulse(0 1", "dc 1.5 ac 1 30 pulse(0 2"),
    c("l=2u w=10u", "l=4u w=20u"),
    c(".op\n", ".options temp=60\n.op\n")
  )) {
    altered <- sub(r[[1]], r[[2]], altered, fixed = TRUE)
  }
  expect_same_result(spice_rerun(s), spice_run(altered))
  expect_equal(spice_temp(s), 60)
})

test_that("decks from files and options behave as in spice_run()", {
  path <- corpus_path("spice3f5", "examples", "rc.cir")
  s <- spice_session(path, options = list(reltol = 1e-4))
  expect_same_result(spice_rerun(s), spice_run(path, options = list(reltol = 1e-4)))
})

test_that("reads and refusals", {
  s <- spice_session(session_deck)
  expect_equal(spice_param(s, "r1"), 1e3)
  expect_equal(spice_param(s, "vin", "pulse"), c(0, 1, card(c(1, 1, 1, 4, 10), -6)))
  expect_null(spice_param(s, "vin", "sin"))
  expect_null(spice_param(s, "m1", "temp"))
  expect_identical(spice_alterable(s, "m1"), c("w", "l", "temp"))
  expect_error(spice_alter(s, "zz", 1), class = "spice_error_alter")
  expect_error(spice_alter(s, "r1", 0), class = "spice_error_alter")
  expect_error(spice_alter(s, "r1", w = 1), class = "spice_error_alter")
  expect_error(spice_alter(s, "m1", w = -1e-6), class = "spice_error_alter")
  expect_error(spice_alter(s, "r1", c(1, 2)), class = "spice_error_input")
  expect_error(spice_alter(s, "r1", "1k"), class = "spice_error_input")
  expect_error(spice_alter(s, "r1"), class = "spice_error_input")
  expect_error(spice_param(s, "zz"), class = "spice_error_alter")
  expect_error(spice_set_temp(s, NA), class = "spice_error_input")
  expect_error(spice_rerun(list()), class = "spice_error_input")
  expect_error(spice_session("t\nr1 1 0\n.end"), class = "spice_error_elaborate")
  expect_error(spice_session("t\nr1 1 0 1k\n.foo\n.end"), class = "spice_error_parse")
  expect_same_result(spice_rerun(s), spice_run(session_deck))
})

test_that("warm start agrees with a cold run to Newton tolerance", {
  op <- sub(".tran 0.2u 10u\n", "", sub(".dc vin 0 2 0.5\n", "", session_deck, fixed = TRUE), fixed = TRUE)
  cold <- spice_session(op)
  warm <- spice_session(op, warm_start = TRUE)
  for (w in c(12e-6, 14e-6, 9e-6)) {
    a <- spice_plots(spice_rerun(spice_alter(cold, "m1", w = w)))[["Operating Point"]]$values
    b <- spice_plots(spice_rerun(spice_alter(warm, "m1", w = w)))[["Operating Point"]]$values
    for (k in names(a)) {
      floor <- if (startsWith(k, "i(")) 1e-12 else 1e-6
      expect_lte(abs(a[[k]] - b[[k]]), 2 * (1e-3 * max(abs(a[[k]]), abs(b[[k]])) + floor))
    }
  }
  expect_output(print(warm), "warm start on")
})

test_that("spice_rerun() measures the deck's .measure cards on the new plots", {
  deck <- "rc rise
v1 1 0 pulse(0 1 0 1n 1n 10m 20m)
r1 1 2 1k
c1 2 0 1u
.tran 2u 8m
.measure tran trise trig v(2) val=0.1 rise=1 targ v(2) val=0.9 rise=1
.end"
  s <- spice_session(deck)
  m1 <- spice_rerun(s)$measurements
  expect_identical(m1, spice_run(deck)$measurements)
  t1 <- m1$value[m1$name == "trise"]
  expect_equal(t1, log(9) * 1e-3, tolerance = 1e-2)
  m2 <- spice_rerun(spice_alter(s, "r1", 2e3))$measurements
  expect_identical(m2, spice_run(sub("r1 1 2 1k", "r1 1 2 2k", deck, fixed = TRUE))$measurements)
  expect_equal(m2$value[m2$name == "trise"] / t1, 2, tolerance = 1e-2)
})
