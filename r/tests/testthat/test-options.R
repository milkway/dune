# `options =` names of the transient-noise, device-noise and keepopinfo
# cards: each override is, bit for bit, the deck with the `.options` card
# edited (the mirror of python/tests/test_options.py and the wasm smoke).

noisy_deck <- function(card = "") {
  paste0(
    "parity\nvn 1 0 dc 0.5 trnoise(10m 1u 0 0)\nr1 1 2 1k\nc1 2 0 2n\n",
    card, "\n.tran 1u 20u\n.end"
  )
}
ac_deck <- function(card = "") {
  paste0("keep\nv1 1 0 dc 1 ac 1\nr1 1 2 1k\nc1 2 0 1n\n", card, "\n.ac dec 2 1k 100k\n.end")
}

ltra_deck <- function(card = "") {
  paste0(
    "ltra\nv1 1 0 pulse(0 1 1n 1n 1n 20n 100n)\nr1 1 2 50\no1 2 0 3 0 lin\nr2 3 0 50\n",
    ".model lin ltra r=0.1 l=9.13u c=3.65p len=1\n", card, "\n.tran 0.5n 60n\n.end"
  )
}

values_of <- function(res) lapply(res$plots, `[[`, "values")

test_that("each new option is the edited deck", {
  cases <- list(
    list(ac_deck, list(keepopinfo = TRUE), ".options keepopinfo"),
    list(ltra_deck, list(trytocompact = TRUE), ".options trytocompact"),
    list(noisy_deck, list(seed = 9), ".options seed=9"),
    list(noisy_deck, list(seed = 9L), ".options seed=9"),
    list(noisy_deck, list(notrnoise = TRUE), ".options notrnoise"),
    list(noisy_deck, list(DevNoise = TRUE, devnt = 2e-7, seed = 5), ".options devnoise devnt=0.2u seed=5"),
    # TR-BDF2 is the `.tran` default (D-0046): the trapezoidal rule
    # and the SPICE3 profile are what change the deck now.
    list(noisy_deck, list(method = "trap"), ".options method=trap"),
    list(noisy_deck, list(compat = "spice3"), ".options compat=spice3")
  )
  for (cs in cases) {
    deck <- cs[[1]]
    got <- spice_run(deck(), options = cs[[2]])
    want <- spice_run(deck(cs[[3]]))
    expect_identical(names(got$plots), names(want$plots))
    expect_identical(values_of(got), values_of(want))
    expect_false(identical(values_of(got), values_of(spice_run(deck()))))
    s <- spice_session(deck(), options = cs[[2]])
    expect_identical(values_of(spice_rerun(s)), values_of(want))
  }
})

test_that("keepopinfo adds the operating point plot", {
  res <- spice_run(ac_deck(), options = list(keepopinfo = TRUE))
  expect_identical(names(res$plots), c("AC Operating Point", "AC Analysis"))
})

test_that("FALSE clears the deck's flags", {
  got <- spice_run(noisy_deck(".options notrnoise devnoise"),
    options = list(notrnoise = FALSE, devnoise = FALSE)
  )
  expect_identical(values_of(got), values_of(spice_run(noisy_deck())))
  ac <- spice_run(ac_deck(".options keepopinfo"), options = list(keepopinfo = FALSE))
  expect_identical(names(ac$plots), "AC Analysis")
})

test_that("the new names are listed and bad values are errors", {
  # ::: because the helper is internal (@noRd): the test must also
  # pass against the installed package, not only under load_all().
  expect_true(all(c("seed", "notrnoise", "devnoise", "devnt", "keepopinfo", "trytocompact") %in%
    spice:::spice_option_names()))
  bad <- list(
    list(seed = 0), list(seed = -2), list(seed = 1.5), list(seed = 2^53),
    list(seed = "random"), list(seed = TRUE),
    list(devnt = 0), list(devnt = -1e-9), list(devnt = Inf), list(devnt = TRUE),
    list(devnoise = 1), list(notrnoise = "yes"), list(keepopinfo = 0)
  )
  for (b in bad) {
    expect_error(spice_run(noisy_deck(), options = b), class = "spice_error_options")
  }
})
