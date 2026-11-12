long_cols <- c("plot", "point", "scale_name", "scale", "variable", "type")

test_that("a real plot is long: one row per point and non-scale variable", {
  res <- spice_run(corpus_path("spice3f5", "examples", "rc.cir"))
  tr <- spice_plots(res)[[1]]
  df <- tibble::as_tibble(tr)
  expect_s3_class(df, "tbl_df")
  expect_named(df, c(long_cols, "value"))
  n <- length(tr$values$time)
  expect_equal(nrow(df), n * 3L)
  expect_equal(unique(df$variable), c("v(1)", "v(2)", "i(vin)"))
  expect_equal(unique(df$type), c("voltage", "current"))
  expect_equal(unique(df$scale_name), "time")
  v2 <- df[df$variable == "v(2)", ]
  expect_equal(v2$scale, tr$values$time)
  expect_equal(v2$value, tr$values[["v(2)"]])
  expect_equal(v2$point, seq_len(n))
  expect_identical(spice_tidy(res), df)
})

test_that("a complex .ac plot has re/im and no value column", {
  res <- spice_run(corpus_path("basic", "acrc.cir"))
  ac <- spice_plots(res)[["AC Analysis"]]
  expect_true(is.complex(ac$values[["v(2)"]]))
  df <- tibble::as_tibble(ac)
  expect_named(df, c(long_cols, "re", "im", "mag", "db", "phase"))
  expect_false("value" %in% names(df))
  nvar <- nrow(ac$variables) - 1L
  expect_equal(nrow(df), length(ac$values$frequency) * nvar)
  expect_type(df$scale, "double")
  # The corner of r = 1k, c = 159.155n is 1 kHz: |v(2)| = 1/sqrt(2), -45 deg.
  at1k <- df[df$variable == "v(2)" & abs(df$scale - 1000) < 1e-6, ]
  expect_equal(nrow(at1k), 1L)
  z <- complex(real = at1k$re, imaginary = at1k$im)
  expect_equal(Mod(z), 1 / sqrt(2), tolerance = 1e-5)
  expect_equal(Arg(z) * 180 / pi, -45, tolerance = 1e-4)
})

test_that("plots without a scale get NA scale columns", {
  df <- spice_tidy(spice_run(divider))
  expect_true(all(is.na(df$scale_name)))
  expect_true(all(is.na(df$scale)))
  expect_equal(nrow(df), 3L) # v(1), v(2), i(v1)
})

test_that("a result mixing real and complex plots has value, re and im", {
  res <- spice_run(corpus_path("spice3f5", "examples", "rca3040.cir"))
  df <- spice_tidy(res)
  expect_named(df, c(long_cols, "value", "re", "im", "mag", "db", "phase"))
  ac <- df[df$plot == "AC Analysis", ]
  expect_true(all(is.na(ac$value)))
  expect_false(anyNA(ac$re))
  tr <- df[df$plot == "Transient Analysis", ]
  expect_true(all(is.na(tr$re)))
  expect_false(anyNA(tr$value))
  only <- spice_tidy(res, plots = "Transient Analysis")
  expect_named(only, c(long_cols, "value"))
  expect_equal(nrow(only), nrow(tr))
  expect_equal(spice_tidy(res, plots = 3), only)
  expect_error(spice_tidy(res, plots = "nope"), class = "spice_error_input")
  expect_error(spice_tidy(list()), class = "spice_error_input")
})

test_that("autoplot draws a plot", {
  skip_if_not_installed("ggplot2")
  res <- spice_run(corpus_path("spice3f5", "examples", "rca3040.cir"))
  for (p in spice_plots(res)) {
    expect_s3_class(ggplot2::autoplot(p), "ggplot")
  }
  expect_error(
    ggplot2::autoplot(spice_plots(spice_run(divider))[[1]]),
    class = "spice_error_input"
  )
})
