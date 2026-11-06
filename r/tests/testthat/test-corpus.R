# Every corpus deck with an ngspice golden runs, and yields the same
# plots (by plotname) as ngspice-47 did. Values are the oracle's job
# (`dune-oracle --ci`); this checks the R path end to end.
test_that("every corpus deck runs and matches the golden's plots", {
  root <- dune_root()
  skip_if(is.null(root), "the dune corpus is not available (set DUNE_ROOT)")
  corpus <- file.path(root, "corpus")
  decks <- list.files(corpus, pattern = "\\.cir$", recursive = TRUE, full.names = TRUE)
  decks <- decks[!grepl("/spice3f5/src/", decks, fixed = TRUE)]
  ran <- 0L
  for (deck in decks) {
    stem <- sub("\\.cir$", "", basename(deck))
    golden <- file.path(corpus, "golden", paste0(stem, ".raw"))
    if (!file.exists(golden)) next
    res <- spice_run(deck)
    gold <- spice_read_raw(golden)
    expect_equal(
      sort(vapply(res$plots, `[[`, "", "plotname"), method = "radix"),
      sort(vapply(gold$plots, `[[`, "", "plotname"), method = "radix"),
      label = stem
    )
    ran <- ran + 1L
  }
  expect_gt(ran, 60L)
})
