# Every corpus deck with an ngspice golden runs, and yields the same
# plots (by plotname) as ngspice-47 did. Values are the oracle's job
# (`dune-oracle --ci`); this checks the R path end to end.
#
# Bistable decks (D-0022) follow the oracle's registry, `corpus/XFAIL`:
# an entry with an `accept-error=TEXT` flag may, on some platforms,
# end in a structured engine error containing TEXT instead of a
# rawfile (ltra_3 on x86_64 stops at the transient step budget). That
# error is accepted for that deck only; every other deck must run.
# The decks are not skipped: on a platform where they run they are
# checked like the rest.
xfail_accepted_errors <- function(corpus) {
  path <- file.path(corpus, "XFAIL")
  if (!file.exists(path)) {
    return(character())
  }
  lines <- trimws(readLines(path, warn = FALSE))
  lines <- lines[nzchar(lines) & !startsWith(lines, "#")]
  out <- character()
  for (line in lines) {
    # stem|plot|cap|flags|reason
    fields <- strsplit(line, "|", fixed = TRUE)[[1]]
    if (length(fields) < 5L) next
    flags <- trimws(strsplit(fields[[4]], ",", fixed = TRUE)[[1]])
    accept <- flags[startsWith(flags, "accept-error=")]
    if (length(accept) == 1L) {
      out[[trimws(fields[[1]])]] <- trimws(sub("^accept-error=", "", accept))
    }
  }
  out
}

test_that("every corpus deck runs and matches the golden's plots", {
  root <- dune_root()
  skip_if(is.null(root), "the dune corpus is not available (set DUNE_ROOT)")
  corpus <- file.path(root, "corpus")
  accepted <- xfail_accepted_errors(corpus)
  expect_true("ltra_3" %in% names(accepted))
  decks <- list.files(corpus, pattern = "\\.cir$", recursive = TRUE, full.names = TRUE)
  decks <- decks[!grepl("/spice3f5/src/", decks, fixed = TRUE)]
  ran <- 0L
  for (deck in decks) {
    stem <- sub("\\.cir$", "", basename(deck))
    golden <- file.path(corpus, "golden", paste0(stem, ".raw"))
    if (!file.exists(golden)) next
    res <- if (stem %in% names(accepted)) {
      tryCatch(spice_run(deck), error = function(e) {
        # cli may wrap the bullet; compare with whitespace collapsed.
        msg <- gsub("\\s+", " ", conditionMessage(e))
        expect_true(
          grepl(accepted[[stem]], msg, fixed = TRUE),
          label = paste0(stem, " failed with an error corpus/XFAIL does not accept: ", msg)
        )
        NULL
      })
    } else {
      spice_run(deck)
    }
    ran <- ran + 1L
    if (is.null(res)) next
    gold <- spice_read_raw(golden)
    expect_equal(
      sort(vapply(res$plots, `[[`, "", "plotname"), method = "radix"),
      sort(vapply(gold$plots, `[[`, "", "plotname"), method = "radix"),
      label = stem
    )
  }
  expect_gt(ran, 60L)
})
