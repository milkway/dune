test_that("rawfiles round-trip exactly in binary", {
  res <- spice_run(corpus_path("spice3f5", "examples", "rca3040.cir"))
  f <- tempfile(fileext = ".raw")
  expect_identical(spice_write_raw(res, f), f)
  back <- spice_read_raw(f)
  expect_equal(names(spice_plots(back)), names(spice_plots(res)))
  for (i in seq_along(res$plots)) {
    a <- res$plots[[i]]
    b <- back$plots[[i]]
    expect_identical(b$values, a$values)
    expect_identical(b$variables, a$variables)
    expect_identical(b$plotname, a$plotname)
    expect_identical(b$title, a$title)
    expect_identical(b$scale, a$scale)
  }
  expect_identical(spice_tidy(back), spice_tidy(res))
})

test_that("ASCII rawfiles keep 15 significant digits", {
  res <- spice_run(corpus_path("basic", "acrc.cir"))
  f <- tempfile(fileext = ".raw")
  spice_write_raw(res$plots[[1]], f, format = "ascii")
  expect_match(readLines(f, n = 1), "^Title:")
  back <- spice_read_raw(f)$plots[[1]]
  expect_equal(back$values, res$plots[[1]]$values, tolerance = 1e-14)
  expect_error(spice_write_raw(res, f, format = "hex"))
})

test_that("an operating point reads back with its first vector as scale", {
  res <- spice_run(divider)
  f <- tempfile(fileext = ".raw")
  spice_write_raw(res, f)
  back <- spice_read_raw(f)$plots[[1]]
  expect_identical(back$values, res$plots[[1]]$values)
  expect_equal(back$scale, "v(1)")
})

test_that("ngspice goldens read", {
  gold <- spice_read_raw(corpus_path("golden", "rca3040.raw"))
  expect_length(spice_plots(gold), 3L)
  expect_setequal(
    vapply(spice_plots(gold), `[[`, "", "plotname"),
    c("AC Analysis", "DC transfer characteristic", "Transient Analysis")
  )
})

test_that("bad rawfiles and arguments are errors", {
  f <- tempfile(fileext = ".raw")
  writeLines("not a rawfile", f)
  expect_error(spice_read_raw(f), class = "spice_error_raw")
  expect_error(spice_read_raw(tempfile()), class = "spice_error_io")
  expect_error(spice_write_raw(list(), f), class = "spice_error_input")
  p <- spice_plots(spice_run(divider))[[1]]
  p$values[[2]] <- c(1, 2)
  expect_error(spice_write_raw(p, f), class = "spice_error_raw")
})
