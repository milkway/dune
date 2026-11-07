divider_dc <- "divider
v1 1 0 dc 2
r1 1 2 1k
r2 2 0 1k
.op
.dc v1 0 2 1
.end"

test_that("a grid runs every combination, first parameter slowest", {
  sw <- spice_sweep(divider_dc, R2 = c(1e3, 3e3), v1 = c(1, 2, 4))
  expect_s3_class(sw, "spice_sweep")
  expect_named(sw$params, c("run", "r2", "v1"))
  expect_equal(sw$params$run, 1:6)
  expect_equal(sw$params$r2, rep(c(1e3, 3e3), each = 3))
  expect_equal(sw$params$v1, rep(c(1, 2, 4), 2))
  expect_length(sw$runs, 6)
  v2 <- vapply(sw$runs, function(r) spice_plots(r)[[1]]$values[["v(2)"]], 0)
  expect_equal(v2, sw$params$v1 * sw$params$r2 / (1e3 + sw$params$r2), tolerance = 1e-12)

  td <- spice_tidy(sw)
  expect_equal(names(td)[1:4], c("run", "r2", "v1", "plot"))
  op <- spice_tidy(sw, plots = "Operating Point")
  expect_equal(unique(op$plot), "Operating Point")
  expect_equal(op$value[op$variable == "v(2)"], v2, tolerance = 0)
  dc <- spice_tidy(sw, plots = 2)
  expect_equal(nrow(dc), 6 * 3 * nrow(spice_plots(sw$runs[[1]])[[2]]$variables[-1, ]))
  expect_output(print(sw), "6 runs")
})

test_that("zip mode pairs values and recycles length one", {
  sw <- spice_sweep(divider_dc, r2 = c(1e3, 2e3, 3e3), v1 = 4, .mode = "zip")
  expect_equal(sw$params$v1, c(4, 4, 4))
  expect_error(
    spice_sweep(divider_dc, r2 = c(1e3, 2e3), v1 = c(1, 2, 3), .mode = "zip"),
    class = "spice_error_input"
  )
})

test_that("bad sweeps are classed errors", {
  expect_error(spice_sweep(divider_dc, r9 = 1), "no element", class = "spice_error_sweep")
  expect_error(spice_sweep(divider_dc, `@r1[w]` = 1), class = "spice_error_sweep")
  expect_error(spice_sweep(divider_dc, r1 = 1, `@R1[r]` = 2), "twice", class = "spice_error_sweep")
  expect_error(spice_sweep(divider_dc, r1 = NA_real_), class = "spice_error_input")
  expect_error(spice_sweep(divider_dc, 1:3), class = "spice_error_input")
  expect_error(spice_sweep(divider_dc), class = "spice_error_input")
  expect_error(spice_sweep(divider_dc, r1 = 1, .threads = -1), class = "spice_error_input")
  expect_error(spice_sweep(divider_dc, r2 = c(1e3, 0)), "run 2", class = "spice_error_elaborate")
})

diffpair_all <- function() {
  text <- readLines(corpus_path("spice3f5", "examples", "diffpair.cir"))
  sub("^\\*(\\.(tf|dc|ac) )", "\\1", text)
}

test_that("a sweep reproduces individual runs bit for bit", {
  deck <- diffpair_all()
  sw <- spice_sweep(deck, rc1 = c(8e3, 12.5e3), vcc = c(10, 12), temp = c(-40, 85))
  expect_length(sw$runs, 8)
  for (i in seq_len(8)) {
    p <- sw$params[i, ]
    # sprintf("%.17g") round-trips the double through the deck text.
    edited <- deck
    edited <- sub("^rc1 4 8 10k$", sprintf("rc1 4 8 %.17g", p$rc1), edited)
    edited <- sub("^vcc 8 0 12$", sprintf("vcc 8 0 %.17g", p$vcc), edited)
    edited <- sub("^\\.tran 5ns", sprintf(".options temp=%.17g\n.tran 5ns", p$temp), edited)
    single <- spice_run(edited)
    got <- sw$runs[[i]]$plots
    expect_equal(names(got), names(single$plots))
    for (k in seq_along(got)) {
      # identical(): bitwise, no tolerance.
      expect_true(identical(got[[k]]$values, single$plots[[k]]$values))
    }
  }
})

test_that("threads give the same bits", {
  deck <- diffpair_all()
  set.seed(10)
  draws <- data.frame(rc1 = rnorm(12, 10e3, 500), temp = runif(12, 0, 70))
  seq_run <- spice_sweep(deck, !!!draws, .mode = "zip")
  par_run <- spice_sweep(deck, draws, .mode = "zip", .threads = 4)
  expect_identical(spice_tidy(seq_run), spice_tidy(par_run))
})

test_that("Monte Carlo draws are reproducible from the seed", {
  deck <- corpus_path("spice3f5", "examples", "rc.cir")
  mc <- function(seed) {
    set.seed(seed)
    spice_sweep(deck, r = rnorm(20, 1, 0.05), c = rnorm(20, 1, 0.05), .mode = "zip")
  }
  a <- spice_tidy(mc(42))
  expect_identical(a, spice_tidy(mc(42)))
  expect_false(identical(a, spice_tidy(mc(7))))
  last <- a[a$variable == "v(2)", ]
  last <- last[!duplicated(last$run, fromLast = TRUE), ]
  tau <- last$r * last$c
  want <- 1 - (exp(0.1 / tau) - 1) / (0.1 / tau) * exp(-last$scale / tau)
  expect_equal(last$value, want, tolerance = 2e-3)
})

test_that("MOSFET geometry and AC levels sweep", {
  sw <- spice_sweep(corpus_path("spice3f5", "examples", "mosmem.cir"),
    `@m1[W]` = c(200e-6, 300e-6)
  )
  expect_named(sw$params, c("run", "@m1[w]"))
  v5 <- lapply(sw$runs, function(r) spice_plots(r)[[1]]$values[["v(5)"]])
  expect_false(identical(v5[[1]], v5[[2]]))

  rc <- "rc\nvin 1 0 dc 0 ac 1\nr1 1 2 1k\nc1 2 0 1u\n.ac dec 5 10 10k\n.end"
  ac <- spice_sweep(rc, `@vin[acmag]` = c(1, 2))
  a1 <- spice_plots(ac$runs[[1]])[[1]]$values[["v(2)"]]
  a2 <- spice_plots(ac$runs[[2]])[[1]]$values[["v(2)"]]
  expect_identical(a2, 2 * a1)
})
