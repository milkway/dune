# Decks that read `temper` and the seed sweep (D-0075). A deck whose
# values read `temper` is re-expanded by the engine whenever the circuit
# temperature changes, so a `temp` option, `spice_set_temp()` and a swept
# `temp` all give, bit for bit, the deck with `.options temp=` written in
# it (the mirror of python/tests/test_temper.py and the wasm unit test).

# temper in a model value, a device value (a behavioural R) and a
# subcircuit parameter.
temper_deck <- function(card = "") {
  paste0(
    "temper\n", card, "\n",
    ".model dm d is={1e-14*(1 + 0.005*(temper - 27))}\n",
    ".subckt load a\n.param rl={1k + 10*temper}\nr1 a 0 {rl}\n.ends\n",
    "v1 1 0 dc 2\nr1 1 2 {1k*(1 + 0.004*(temper - 27))}\nd1 2 0 dm\nx1 2 load\n",
    ".op\n.dc v1 0 2 0.5\n.end"
  )
}

mc_deck <- paste0(
  "mismatch\n.param rv=agauss(1k, 100, 3)\nv1 1 0 dc 1\nr1 1 2 {rv}\n",
  "r2 2 0 {1k + temper}\n.op\n.end"
)

values_of <- function(res) lapply(res$plots, `[[`, "values")

test_that("a temp override is the deck card", {
  for (temp in c(-40, 27, 85)) {
    want <- spice_run(temper_deck(paste0(".options temp=", temp)))
    got <- spice_run(temper_deck(), options = list(temp = temp))
    expect_identical(values_of(got), values_of(want))
    s <- spice_session(temper_deck())
    spice_set_temp(s, temp)
    expect_identical(values_of(spice_rerun(s)), values_of(want))
    sw <- spice_sweep(temper_deck(), temp = temp)
    expect_identical(values_of(sw$runs[[1]]), values_of(want))
  }
})

test_that("the temperature reaches the temper values, and back", {
  cold <- spice_run(temper_deck(), options = list(temp = -40))
  hot <- spice_run(temper_deck(), options = list(temp = 85))
  expect_false(identical(values_of(cold), values_of(hot)))
  s <- spice_session(temper_deck())
  for (temp in c(85, -40, 27)) {
    spice_set_temp(s, temp)
    want <- spice_run(temper_deck(paste0(".options temp=", temp)))
    expect_identical(values_of(spice_rerun(s)), values_of(want))
  }
})

test_that("a seed sweep is a Monte Carlo of the deck", {
  mc <- spice_sweep(mc_deck, seed = 1:8)
  expect_identical(names(mc$params), c("run", "seed"))
  expect_identical(mc$params$seed, as.double(1:8))
  for (i in 1:8) {
    expect_identical(mc$runs[[i]]$seed, as.double(i))
    expect_identical(mc$runs[[i]]$random_mode, "seeded")
    expect_identical(values_of(mc$runs[[i]]), values_of(spice_run(mc_deck, options = list(seed = i))))
  }
  long <- spice_tidy(mc, plots = "op")
  expect_identical(names(long)[1:2], c("run", "seed"))
  v2 <- long$value[long$variable == "v(2)"]
  expect_length(unique(v2), 8L)
})

test_that("a seed sweep composes with temp and threads", {
  grid <- spice_sweep(mc_deck, seed = c(3, 4), temp = c(0, 50))
  expect_identical(grid$params$seed, c(3, 3, 4, 4))
  expect_identical(grid$params$temp, c(0, 50, 0, 50))
  for (i in seq_len(4)) {
    want <- spice_run(mc_deck, options = list(seed = grid$params$seed[[i]], temp = grid$params$temp[[i]]))
    expect_identical(values_of(grid$runs[[i]]), values_of(want))
  }
  par <- spice_sweep(mc_deck, seed = c(3, 4), temp = c(0, 50), .threads = 2)
  expect_identical(lapply(par$runs, values_of), lapply(grid$runs, values_of))
  # A swept seed wins over options$seed.
  over <- spice_sweep(mc_deck, seed = 3, .options = list(seed = 99))
  expect_identical(over$runs[[1]]$seed, 3)
  expect_identical(values_of(over$runs[[1]]), values_of(spice_run(mc_deck, options = list(seed = 3))))
})

test_that("a seed sweep without random functions says so; bad seeds are sweep errors", {
  plain <- "div\nv1 1 0 dc 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end"
  res <- spice_sweep(plain, seed = c(1, 2))
  for (r in res$runs) expect_true(any(grepl("every run is the same circuit", r$notes, fixed = TRUE)))
  expect_identical(values_of(res$runs[[1]]), values_of(res$runs[[2]]))
  for (bad in c(0, -1, 1.5, 2^32)) {
    expect_error(spice_sweep(mc_deck, seed = c(1, bad)), class = "spice_error_sweep")
  }
})
