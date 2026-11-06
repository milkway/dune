knit_text <- function(text, envir = new.env()) {
  skip_if_not_installed("knitr")
  dir <- test_tempdir()
  input <- file.path(dir, "doc.Rmd")
  writeLines(text, input)
  output <- file.path(dir, "doc.md")
  old <- setwd(dir)
  on.exit(setwd(old), add = TRUE)
  suppressMessages(knitr::knit(input, output, quiet = TRUE, envir = envir))
  list(md = readLines(output), env = envir)
}

rc_chunk <- function(header) {
  c(
    paste0("```{", header, "}"),
    "rc low-pass",
    "vin 1 0 pulse(0 1)",
    "r1 1 2 1k",
    "c1 2 0 1u",
    ".tran 0.5m 5m",
    ".end",
    "```"
  )
}

test_that("the engine is registered with knitr", {
  skip_if_not_installed("knitr")
  expect_true("spice" %in% names(knitr::knit_engines$get()))
})

test_that("a spice chunk runs, echoes the deck and assigns the result", {
  out <- knit_text(c(
    rc_chunk("spice rc"),
    "",
    "```{r}",
    "nrow(spice_tidy(rc)) > 0",
    "```"
  ))
  md <- out$md
  expect_true(any(md == "vin 1 0 pulse(0 1)"))
  expect_true(any(grepl("A tibble", md, fixed = TRUE)))
  expect_true(any(grepl("Transient Analysis", md, fixed = TRUE)))
  expect_true(any(grepl("[1] TRUE", md, fixed = TRUE)))
  expect_s3_class(out$env$rc, "spice_result")
})

test_that("spice.output = 'raw', echo = FALSE and spice.var", {
  out <- knit_text(rc_chunk("spice, echo = FALSE, spice.output = 'raw', spice.var = 'tr'"))
  md <- out$md
  expect_false(any(md == "vin 1 0 pulse(0 1)"))
  expect_true(any(grepl("spice_result", md, fixed = TRUE)))
  expect_false(any(grepl("A tibble", md, fixed = TRUE)))
  expect_s3_class(out$env$tr, "spice_result")
})

test_that("output = 'raw' is honoured; spice.var = FALSE and unlabelled chunks assign nothing", {
  out <- knit_text(c(
    rc_chunk("spice r1, output = 'raw', spice.var = FALSE"),
    rc_chunk("spice")
  ))
  expect_true(any(grepl("spice_result", out$md, fixed = TRUE)))
  expect_length(ls(out$env), 0L)
})

test_that("spice.options reach spice_run()", {
  out <- knit_text(rc_chunk("spice g1, spice.options = list(method = 'gear')"))
  expect_s3_class(out$env$g1, "spice_result")
  expect_error(
    knit_text(rc_chunk("spice g2, error = FALSE, spice.options = list(bogus = 1)")),
    class = "spice_error_options"
  )
})

test_that("a bad deck stops knitting with the span, or prints it with error = TRUE", {
  bad <- c("```{spice bad, error = FALSE}", "title", "r1 1", ".op", ".end", "```")
  expect_error(knit_text(bad), "deck.cir:2", class = "spice_error_parse")
  bad[[1]] <- "```{spice bad, error = TRUE}"
  out <- knit_text(bad)
  expect_true(any(grepl("deck.cir:2", out$md, fixed = TRUE)))
  expect_false(exists("bad", envir = out$env, inherits = FALSE))
})

test_that("an invalid spice.output is an error", {
  expect_error(
    knit_text(rc_chunk("spice x, error = FALSE, spice.output = 'wide'")),
    class = "spice_error_input"
  )
})

test_that("eval = FALSE shows the deck without running it", {
  out <- knit_text(rc_chunk("spice ne, eval = FALSE"))
  expect_true(any(out$md == "vin 1 0 pulse(0 1)"))
  expect_false(exists("ne", envir = out$env, inherits = FALSE))
})
