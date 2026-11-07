rc_step <- "rc step
v1 in 0 pulse(0 1 1n 1p 1p 20n 40n)
r1 in out 1k
c1 out 0 1p
.tran 10p 10n
.meas tran trise trig v(out) val=0.1 rise=1 targ v(out) val=0.9 rise=1
.meas tran vmax max v(out)
.meas tran never when v(out)=5
.end"

test_that("the deck's .measure cards come back as a tibble", {
  res <- spice_run(rc_step)
  m <- res$measurements
  expect_s3_class(m, "tbl_df")
  expect_equal(m$name, c("trise", "vmax", "never"))
  expect_equal(unique(m$analysis), "tran")
  expect_equal(unique(m$plot), "Transient Analysis")
  # 10-90 % rise of a 1 ns RC: tau * log(9) (simulation error ~1e-5).
  expect_equal(m$value[[1]], 1e-9 * log(9), tolerance = 1e-4)
  expect_equal(m$targ[[1]] - m$trig[[1]], m$value[[1]])
  expect_equal(m$at[[2]], 10e-9)
  expect_true(is.na(m$at[[1]]) && is.na(m$from[[2]]))
  expect_true(is.na(m$value[[3]]))
  expect_equal(m$error, c(NA, NA, "out of interval (WHEN)"))
  expect_output(print(res), "3 measurements")
})

test_that("a deck without .measure has an empty measurements tibble", {
  res <- spice_run(divider)
  expect_equal(nrow(res$measurements), 0L)
  expect_named(res$measurements, c(
    "name", "analysis", "plot", "value", "trig", "targ", "at",
    "with", "from", "to", "error"
  ))
})

test_that("spice_measure() gives the cards' numbers", {
  res <- spice_run(rc_step)
  tran <- spice_plots(res)[["Transient Analysis"]]
  m <- spice_measure(tran, c(
    trise = "trig v(out) val=0.1 rise=1 targ v(out) val=0.9 rise=1",
    "when v(out)=0.5",
    slope = "deriv v(out) at=2n"
  ))
  expect_equal(m$name, c("trise", "measure2", "slope"))
  expect_identical(m$value[[1]], res$measurements$value[[1]])
  k <- 1e-9 / 1e-12 * expm1(1e-3)
  expect_equal(m$value[[3]], k / 1e-9 * exp(-1), tolerance = 1e-3)
  expect_equal(spice_measure(tran, "max v(out)")$name, "measure")
  expect_equal(spice_measure(tran, "when v(out)=7")$error, "out of interval (WHEN)")
})

test_that("spice_measure() reads ngspice's own plot like ngspice", {
  # The golden .meas lines are ngspice-47's printout (15 digits) on
  # the very samples of the golden rawfile.
  res <- spice_read_raw(corpus_path("measure", "golden", "meas_rlc.raw"))
  tran <- spice_plots(res)[[1]]
  gold <- readLines(corpus_path("measure", "golden", "meas_rlc.meas"))
  want <- as.double(sub("^[^=]*=\\s*(\\S+).*$", "\\1", gold))
  names(want) <- trimws(sub("=.*$", "", gold))
  cards <- grep("^\\.meas", readLines(corpus_path("measure", "meas_rlc.cir")), value = TRUE)
  spec <- sub("^\\.meas\\s+tran\\s+\\S+\\s+", "", cards)
  names(spec) <- sub("^\\.meas\\s+tran\\s+(\\S+).*$", "\\1", cards)
  got <- spice_measure(tran, spec)
  expect_equal(got$value, unname(want[got$name]), tolerance = 1e-14)
})

test_that("malformed measurements and unmeasurable plots are classed errors", {
  tran <- spice_plots(spice_run(rc_step))[[1]]
  expect_error(spice_measure(tran, "max v(out) bogus=1"), class = "spice_error_measure")
  op <- spice_plots(spice_run(divider))[[1]]
  expect_error(spice_measure(op, "max v(2)"), "cannot be measured", class = "spice_error_measure")
  expect_error(spice_measure(tran, 1), class = "spice_error_input")
  expect_error(spice_measure(list(), "max v(1)"), class = "spice_error_input")
})
