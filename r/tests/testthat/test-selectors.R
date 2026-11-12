deck <- "rc low-pass
vin in 0 dc 2 ac 1 pulse(0 1 0 1u)
r1 in out 1k
c1 out 0 1u
.op
.ac dec 5 10 10k
.tran 0.1m 5m
.end"

test_that("short plot names select every plot of the analysis", {
  res <- spice_run(deck)
  expect_identical(unique(spice_tidy(res, "op")$plot), "Operating Point")
  expect_identical(
    spice_tidy(res, "tran"),
    spice_tidy(res, plots = "Transient Analysis")
  )
  expect_identical(unique(spice_tidy(res, c("ac", "op"))$plot), c("AC Analysis", "Operating Point"))
  expect_identical(spice_tidy(res, 3L), spice_tidy(res, "tran"))
  expect_error(spice_tidy(res, "noise"), class = "spice_error_input")
  expect_error(spice_tidy(res, "nope"), class = "spice_error_input")
})

test_that("vars keeps the named variables, a bare node being its voltage", {
  res <- spice_run(deck)
  t <- spice_tidy(res, "tran", "v(out)")
  expect_identical(unique(t$variable), "v(out)")
  expect_identical(spice_tidy(res, "tran", "OUT"), t)
  both <- res |> spice_tidy("tran", c("out", "v(in)"))
  expect_setequal(unique(both$variable), c("v(in)", "v(out)"))
  # Across plots: each keeps what it has.
  all <- spice_tidy(res, vars = "v(out)")
  expect_identical(unique(all$variable), "v(out)")
  expect_identical(unique(all$plot), c("Operating Point", "AC Analysis", "Transient Analysis"))
  expect_error(spice_tidy(res, "tran", "nowhere"), class = "spice_error_input")
  expect_error(spice_tidy(res, "tran", 1), class = "spice_error_input")
})

test_that("a complex plot carries mag, db and phase", {
  res <- spice_run(deck)
  ac <- spice_tidy(res, "ac", "out")
  expect_named(ac, c("plot", "point", "scale_name", "scale", "variable", "type",
                     "re", "im", "mag", "db", "phase"))
  z <- complex(real = ac$re, imaginary = ac$im)
  expect_identical(ac$mag, Mod(z))
  expect_identical(ac$db, 20 * log10(Mod(z)))
  expect_identical(ac$phase, Arg(z) * 180 / pi)
  # The RC pole: -3 dB and -45 degrees at 1/(2 pi RC).
  k <- which.min(abs(ac$scale - 1 / (2 * pi * 1e-3)))
  expect_lt(abs(ac$db[k] + 3), 0.5)
  expect_lt(abs(ac$phase[k] + 45), 5)
  # Mixed with real plots: NA where a column does not apply.
  mixed <- spice_tidy(res)
  expect_true(all(c("value", "re", "im", "mag", "db", "phase") %in% names(mixed)))
  expect_true(all(is.na(mixed$db[mixed$plot == "Transient Analysis"])))
  expect_true(all(is.na(mixed$value[mixed$plot == "AC Analysis"])))
})

test_that("spice_wide gives one column per variable", {
  res <- spice_run(deck)
  w <- res |> spice_wide("tran")
  expect_named(w, c("time", "v(in)", "i(vin)", "v(out)"), ignore.order = TRUE)
  expect_identical(names(w)[1], "time")
  tr <- spice_plots(res)[["Transient Analysis"]]
  expect_identical(w$`v(out)`, tr$values[["v(out)"]])
  expect_identical(nrow(w), length(tr$values$time))
  expect_named(spice_wide(res, "tran", "out"), c("time", "v(out)"))
  # Complex plots stay complex; the scale is real.
  ac <- spice_wide(res, "ac", "out")
  expect_type(ac$frequency, "double")
  expect_type(ac$`v(out)`, "complex")
  # A plot, or the only plot of a result, needs no name.
  expect_identical(spice_wide(tr), w)
  one <- spice_run("t\nv1 1 0 1\nr1 1 0 1k\n.tran 1m 2m\n.end")
  expect_identical(names(spice_wide(one))[1], "time")
  expect_error(spice_wide(res), class = "spice_error_input")
})

test_that("spice_op is a named vector", {
  res <- spice_run(deck)
  op <- spice_op(res)
  expect_type(op, "double")
  expect_identical(op[["v(in)"]], 2)
  expect_equal(op[["v(out)"]], 2)
  expect_identical(names(res |> spice_op("out")), "v(out)")
  expect_identical(names(spice_op(res, c("v(out)", "in"))), c("v(in)", "v(out)"))
  expect_error(spice_op(res, "nowhere"), class = "spice_error_input")
  expect_error(
    spice_op(spice_run("t\nv1 1 0 1\nr1 1 0 1k\n.tran 1m 2m\n.end")),
    class = "spice_error_input"
  )
})

test_that("sweeps take the selectors too", {
  sw <- spice_sweep(deck, r1 = c(500, 2000))
  t <- spice_tidy(sw, "tran", "out")
  expect_identical(unique(t$variable), "v(out)")
  expect_identical(sort(unique(t$r1)), c(500, 2000))
})
