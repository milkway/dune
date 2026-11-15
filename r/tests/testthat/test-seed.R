# `options = list(seed = ...)` and the deck's random functions (D-0075):
# the option is `.options seed=` written in the deck, bit for bit, in
# every entry point that takes options (the mirror of
# python/tests/test_seed.py).

# Mismatch through a .param (redrawn at each reference), a model and a
# subcircuit, plus a value that reads temper.
mismatch_deck <- function(card = "") {
  paste0(
    "mismatch\n", card, "\n",
    ".param rv=agauss(1k, 100, 3) q=aunif(1, 0.05)\n",
    ".model dm d is={1e-14*q}\n",
    ".subckt seg a b\nr1 a b {rv}\n.ends\n",
    "v1 1 0 dc 2\nx1 1 2 seg\nx2 2 3 seg\nd1 3 0 dm\nr9 3 0 {rv + temper}\n",
    ".op\n.dc v1 0 2 0.5\n.end"
  )
}

figure <- function(card = "") {
  paste0(
    ".PS\nspice = 1\n",
    "V1: vsource((0,1), (0,0), 1)\n",
    "R1: resistor((0,1), (1.5,1), \"{rv}\")\n",
    "R2: resistor((1.5,1), (1.5,0), 1k)\n",
    "line from R2.N to V1.N\nground((0.7,0))\n",
    ".param rv=agauss(1k, 100, 3)\n", card, "\n.op\n.PE\n"
  )
}

values_of <- function(res) lapply(res$plots, `[[`, "values")

test_that("random functions are nominal by default", {
  res <- spice_run(mismatch_deck())
  expect_identical(res$seed, NA_real_)
  expect_identical(res$random_mode, "nominal")
  expect_true(any(grepl("nominal value", res$diagnostics$message)))
  fixed <- sub("aunif(1, 0.05)", "1", sub("agauss(1k, 100, 3)", "1k", mismatch_deck(), fixed = TRUE),
    fixed = TRUE
  )
  expect_identical(values_of(res), values_of(spice_run(fixed)))
})

test_that("the same seed gives identical tables, another seed other numbers", {
  a <- spice_run(mismatch_deck(), options = list(seed = 9))
  b <- spice_run(mismatch_deck(), options = list(seed = 9L))
  c <- spice_run(mismatch_deck(), options = list(seed = 10))
  expect_identical(values_of(a), values_of(b))
  expect_identical(spice_tidy(a), spice_tidy(b))
  expect_false(identical(values_of(a), values_of(c)))
  expect_identical(a$seed, 9)
  expect_identical(a$random_mode, "seeded")
  expect_false(any(grepl("nominal value", a$diagnostics$message)))
})

test_that("the seed option is the deck card, in every entry point", {
  for (seed in c(1, 9, 2^32 - 1)) {
    want <- spice_run(mismatch_deck(paste0(".options seed=", format(seed, scientific = FALSE))))
    expect_identical(want$seed, seed)
    got <- spice_run(mismatch_deck(), options = list(seed = seed))
    expect_identical(values_of(got), values_of(want))
    expect_identical(got$seed, want$seed)
    expect_identical(got$random_mode, want$random_mode)
    # The option wins over the deck's own seed.
    over <- spice_run(mismatch_deck(".options seed=3"), options = list(seed = seed))
    expect_identical(values_of(over), values_of(want))
    s <- spice_session(mismatch_deck(), options = list(seed = seed))
    expect_identical(values_of(spice_rerun(s)), values_of(want))
    sw <- spice_sweep(mismatch_deck(), v1 = 2, .options = list(seed = seed))
    expect_identical(values_of(sw$runs[[1]]), values_of(want))
    expect_identical(sw$runs[[1]]$seed, seed)
  }
})

test_that("\"random\" returns its seed, re-running with it repeats the run", {
  res <- spice_run(mismatch_deck(), options = list(seed = "random"))
  expect_true(is.double(res$seed) && res$seed >= 1 && res$seed <= 2^32 - 1)
  expect_identical(res$seed, round(res$seed))
  expect_identical(res$random_mode, "seeded")
  expect_true(any(grepl(format(res$seed, scientific = FALSE), res$notes, fixed = TRUE)))
  again <- spice_run(mismatch_deck(), options = list(seed = res$seed))
  expect_identical(values_of(again), values_of(res))
  s <- spice_rerun(spice_session(mismatch_deck(), options = list(seed = "RANDOM")))
  expect_identical(
    values_of(spice_run(mismatch_deck(), options = list(seed = s$seed))),
    values_of(s)
  )
})

test_that("set.seed() governs the seed \"random\" draws", {
  set.seed(42)
  a <- spice_run(mismatch_deck(), options = list(seed = "random"))
  set.seed(42)
  b <- spice_run(mismatch_deck(), options = list(seed = "random"))
  expect_identical(a$seed, b$seed)
  expect_identical(values_of(a), values_of(b))
})

test_that("a deck asking for a random seed gets one", {
  res <- spice_run(mismatch_deck(".options seed=random"))
  expect_false(is.na(res$seed))
  expect_identical(res$random_mode, "seeded")
  expect_false(any(grepl("seed=random", res$diagnostics$message, fixed = TRUE)))
  want <- spice_run(mismatch_deck(paste0(".options seed=", format(res$seed, scientific = FALSE))))
  expect_identical(values_of(res), values_of(want))
  # Through the session and the sweep too.
  s <- spice_rerun(spice_session(mismatch_deck(".options seed=random")))
  expect_false(is.na(s$seed))
  sw <- spice_sweep(mismatch_deck(".options seed=random"), v1 = c(1, 2))
  expect_false(is.na(sw$runs[[1]]$seed))
  expect_identical(sw$runs[[1]]$seed, sw$runs[[2]]$seed)
  # The caller's integer seed wins over the deck's request.
  fixed <- spice_run(mismatch_deck(".options seed=random"), options = list(seed = 4))
  expect_identical(fixed$seed, 4)
  expect_identical(values_of(fixed), values_of(spice_run(mismatch_deck(".options seed=4"))))
})

test_that("a deck without random functions reports the seed it ran with", {
  plain <- "div\nv1 1 0 dc 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end"
  expect_identical(spice_run(plain)$seed, NA_real_)
  res <- spice_run(plain, options = list(seed = 12))
  expect_identical(res$seed, 12)
  expect_identical(res$random_mode, "seeded")
  expect_identical(values_of(res), values_of(spice_run(plain)))
})

test_that("the seed reaches the transient noise", {
  noisy <- function(card) {
    paste0("n\nvn 1 0 dc 0 trnoise(10m 1u 0 0)\nr1 1 0 1k\n", card, "\n.tran 1u 10u\n.end")
  }
  got <- spice_run(noisy(""), options = list(seed = 77))
  expect_identical(values_of(got), values_of(spice_run(noisy(".options seed=77"))))
})

test_that("pss and figures take the seed", {
  rc <- function(card = "") {
    paste0(
      "rc\n", card, "\n.param rv=agauss(1k, 100, 3)\nv1 1 0 sin(0 1 1k)\n",
      "r1 1 2 {rv}\nc1 2 0 100n\n.end"
    )
  }
  want <- spice_pss(rc(".options seed=5"), freq = 1e3, tstab = 2e-3)
  got <- spice_pss(rc(), freq = 1e3, tstab = 2e-3, options = list(seed = 5))
  expect_identical(values_of(got), values_of(want))
  expect_identical(got$seed, 5)
  expect_identical(spice_pss(rc(), freq = 1e3, tstab = 2e-3)$seed, NA_real_)

  want <- spice_run(spice_pic(figure(".options seed=6")))
  got <- spice_run(spice_pic(figure()), options = list(seed = 6))
  expect_identical(values_of(got), values_of(want))
  expect_identical(got$seed, 6)
})
