# tmax_divisor (issue #16): the deck with the card's TMAX divided, bit for bit.

rc_deck <- function(card) {
  paste0(
    "rc\nV1 Vin 0 PULSE(0 5 0 1u 1u 10m 20m)\nR1 Vin Vout 1k\nC1 Vout 0 1u\n",
    "D1 Vout 0 dm\n.model dm d is=1e-14\n", card, "\n.end"
  )
}
plain_deck <- rc_deck(".tran 20u 5m")
# Default TMAX min(20u, 5m/50) = 20u; a tenth is 2u.
edited_deck <- rc_deck(".options trtol=1\n.tran 20u 5m 0 2u")

values_of <- function(res) lapply(res$plots, `[[`, "values")

test_that("spice_run() with the divisor is the edited deck", {
  fine <- spice_run(plain_deck, options = list(trtol = 1), tmax_divisor = 10)
  expect_identical(values_of(fine), values_of(spice_run(edited_deck)))
  plain <- spice_run(plain_deck)
  expect_gt(length(fine$plots[[1]]$values[[1]]), length(plain$plots[[1]]$values[[1]]))
  expect_identical(values_of(spice_run(plain_deck, tmax_divisor = 1)), values_of(plain))
})

test_that("spice_sweep() with the divisor is the edited deck", {
  sw <- spice_sweep(plain_deck,
    r1 = c(1e3, 2e3), .options = list(trtol = 1),
    .tmax_divisor = 10
  )
  want <- spice_sweep(edited_deck, r1 = c(1e3, 2e3))
  expect_identical(lapply(sw$runs, values_of), lapply(want$runs, values_of))
})

test_that("a session toggles the divisor without rebuilding", {
  s <- spice_session(plain_deck, options = list(trtol = 1))
  expect_null(spice_tmax_divisor(s))
  spice_set_tmax_divisor(s, 10)
  expect_identical(spice_tmax_divisor(s), 10)
  expect_identical(values_of(spice_rerun(s)), values_of(spice_run(edited_deck)))
  spice_set_tmax_divisor(s, NULL)
  expect_identical(
    values_of(spice_rerun(s)),
    values_of(spice_run(plain_deck, options = list(trtol = 1)))
  )
  opened <- spice_session(plain_deck, options = list(trtol = 1), tmax_divisor = 10)
  expect_identical(values_of(spice_rerun(opened)), values_of(spice_run(edited_deck)))
})

test_that("bad divisors are options errors", {
  for (bad in list(0.5, 0, -1, NaN, Inf, NA_real_, "10", TRUE, c(2, 3))) {
    expect_error(spice_run(plain_deck, tmax_divisor = bad), class = "spice_error_options")
  }
  expect_error(spice_sweep(plain_deck, r1 = 1e3, .tmax_divisor = 0),
    class = "spice_error_options"
  )
  s <- spice_session(plain_deck, tmax_divisor = 4)
  expect_error(spice_set_tmax_divisor(s, 0.5), class = "spice_error_options")
  expect_identical(spice_tmax_divisor(s), 4)
  # Not an options name.
  expect_error(spice_run(plain_deck, options = list(tmaxdivisor = 10)),
    class = "spice_error_options"
  )
})
