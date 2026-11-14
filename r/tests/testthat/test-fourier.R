# .four cards and spice_fourier() (the engine transforms; R wraps).
# Mirror of python/tests/test_fourier.py.

four_columns <- c(
  "card", "vector", "harmonic", "frequency", "magnitude", "phase",
  "norm_magnitude", "norm_phase", "thd", "gridsize", "polydegree",
  "nperiods", "error"
)

# ngspice-47's tables (17 digits) in corpus/four/golden/*.four.
read_four_golden <- function(path) {
  lines <- readLines(path)
  heads <- grep("^Fourier analysis for ", lines)
  lapply(heads, function(h) {
    info <- lines[[h + 1L]]
    n <- as.integer(sub(".*No. Harmonics: ([0-9]+),.*", "\\1", info))
    rows <- lines[h + 5L + seq_len(n) - 1L]
    tab <- do.call(rbind, lapply(strsplit(trimws(rows), "\\s+"), as.double))
    list(
      vector = sub("^Fourier analysis for (.*):$", "\\1", lines[[h]]),
      thd = as.double(sub(".*THD: (\\S+) %.*", "\\1", info)),
      gridsize = as.integer(sub(".*Gridsize: ([0-9]+),.*", "\\1", info)),
      # harmonic, frequency, magnitude, phase, norm. mag, norm. phase
      table = tab
    )
  })
}

# The `.four` cards of a deck: fundamental and vectors, in deck order.
four_cards <- function(cir) {
  cards <- grep("^\\.four", readLines(cir), value = TRUE)
  lapply(strsplit(cards, "\\s+"), function(w) {
    list(freq = as.double(sub("k$", "e3", w[[2]])), vectors = w[-(1:2)])
  })
}

# The deck's .options for .four, as spice_fourier() arguments.
four_options <- function(cir) {
  opt <- paste(grep("^\\.options", readLines(cir), value = TRUE), collapse = " ")
  get <- function(key, default) {
    m <- regmatches(opt, regexpr(paste0("\\b", key, "=[0-9]+"), opt))
    if (length(m) == 0L) default else as.integer(sub(".*=", "", m))
  }
  list(
    nfreqs = get("nfreqs", 10), nperiods = get("nperiods", 1),
    polydegree = get("polydegree", 1), gridsize = get("fourgridsize", 200)
  )
}

test_that("the deck's .four cards come back as a tibble", {
  res <- spice_run(corpus_path("four", "four_sine.cir"))
  f <- res$fourier
  expect_s3_class(f, "tbl_df")
  expect_named(f, four_columns)
  expect_equal(nrow(f), 20L)
  expect_equal(f$card, rep(1L, 20L))
  expect_equal(f$vector, rep(c("v(in)", "v(out)"), each = 10L))
  expect_equal(f$harmonic, rep(0:9, 2L))
  expect_equal(f$frequency, rep(0:9 * 1e3, 2L))
  expect_true(all(is.na(f$error)))
  expect_equal(unique(f$gridsize), 200L)
  expect_equal(unique(f$polydegree), 1L)
  expect_equal(unique(f$nperiods), 1L)
  # A 1 V sine: |V1| = 1, THD ~ 0; the RC (1 kHz, tau = 0.1 ms) lags.
  vin <- f[f$vector == "v(in)", ]
  expect_equal(vin$magnitude[[2]], 1, tolerance = 1e-3)
  expect_lt(vin$thd[[1]], 1e-3)
  expect_equal(length(unique(vin$thd)), 1L)
  vout <- f[f$vector == "v(out)", ]
  wrc <- 2 * pi * 1e3 * 1e-4
  expect_equal(vout$magnitude[[2]], 1 / sqrt(1 + wrc^2), tolerance = 1e-3)
  expect_equal(vout$phase[[2]], -atan(wrc) * 180 / pi, tolerance = 1e-2)
  expect_equal(vout$norm_magnitude[[2]], 1)
  expect_equal(vout$norm_phase[[2]], 0)
  expect_output(print(res), "2 Fourier analyses")
})

test_that("a deck without .four has an empty fourier tibble", {
  res <- spice_run(divider)
  expect_equal(nrow(res$fourier), 0L)
  expect_named(res$fourier, four_columns)
  expect_type(res$fourier$card, "integer")
  expect_type(res$fourier$magnitude, "double")
})

test_that("spice_fourier() gives the cards' numbers, bit for bit", {
  for (deck in c("four_sine", "four_square", "four_clip", "four_rect")) {
    cir <- corpus_path("four", paste0(deck, ".cir"))
    res <- spice_run(cir)
    tran <- spice_plots(res)[["Transient Analysis"]]
    opts <- four_options(cir)
    cards <- four_cards(cir)
    for (k in seq_along(cards)) {
      got <- rlang::inject(spice_fourier(tran, cards[[k]]$freq, cards[[k]]$vectors, !!!opts))
      want <- res$fourier[res$fourier$card == k, ]
      expect_true(all(is.na(got$card)))
      expect_identical(got[-1], want[-1], label = paste(deck, "card", k))
    }
  }
})

test_that("the deck's .options reach its .four cards", {
  res <- spice_run(corpus_path("four", "four_rect.cir"))
  f <- res$fourier
  expect_equal(unique(f$card), 1:2)
  expect_equal(unique(f$vector), c("v(out)", "i(v1)"))
  expect_equal(nrow(f), 32L)
  expect_equal(unique(f$gridsize), 1024L)
  expect_equal(unique(f$polydegree), 2L)
  expect_equal(unique(f$nperiods), 2L)
  expect_equal(f$frequency[f$card == 2L][[2]], 120)
})

test_that("spice_fourier() reads ngspice's own plot like ngspice", {
  # The golden tables are ngspice-47's printout (17 digits) of the very
  # samples of the golden rawfile.
  for (deck in c("four_sine", "four_square", "four_clip", "four_rect")) {
    cir <- corpus_path("four", paste0(deck, ".cir"))
    raw <- spice_read_raw(corpus_path("four", "golden", paste0(deck, ".raw")))
    tran <- spice_plots(raw)[[1]]
    opts <- four_options(cir)
    cards <- four_cards(cir)
    gold <- read_four_golden(corpus_path("four", "golden", paste0(deck, ".four")))
    for (g in gold) {
      card <- Find(function(cd) g$vector %in% cd$vectors, cards)
      got <- rlang::inject(spice_fourier(tran, card$freq, g$vector, !!!opts))
      label <- paste(deck, g$vector)
      tab <- g$table
      expect_equal(got$harmonic, as.integer(tab[, 1]), label = label)
      expect_equal(got$gridsize[[1]], g$gridsize, label = label)
      expect_equal(got$frequency, tab[, 2], tolerance = 1e-12, label = label)
      m1 <- tab[2, 3]
      expect_lt(max(abs(got$magnitude - tab[, 3])), 1e-10 * m1, label = label)
      # Phases where the harmonic stands clear of roundoff.
      big <- got$norm_magnitude > 1e-3
      expect_lt(max(abs(got$phase[big] - tab[big, 4])), 1e-6, label = label)
      # The header prints the THD with 6 digits.
      expect_equal(got$thd[[1]], g$thd, tolerance = 1e-5, label = label)
    }
  }
})

test_that("a vector without a table is a row with error set", {
  res <- spice_run(corpus_path("four", "four_sine.cir"))
  tran <- spice_plots(res)[[1]]
  f <- spice_fourier(tran, 1e3, c("v(9)", "v(out)"), nfreqs = 3)
  expect_equal(nrow(f), 4L)
  expect_equal(f$vector, c("v(9)", rep("v(out)", 3L)))
  expect_equal(f$error[[1]], "no such vector v(9)")
  expect_true(all(is.na(unlist(f[1L, c("harmonic", "magnitude", "thd", "gridsize")]))))
  expect_true(all(is.na(f$error[-1])))
  # 5 ms of run, 10 periods asked for: too short.
  short <- spice_fourier(tran, 1e3, "v(out)", nperiods = 10)
  expect_equal(short$error, "(10 * wavelength) longer than time span")
  expect_true(is.na(short$magnitude))
  # A deck card that fails is a row too.
  deck <- "rc\nv1 1 0 sin(0 1 1k)\nr1 1 2 1k\nc1 2 0 100n\n.tran 10u 0.5m\n.four 1k v(2) v(7)\n.end"
  rf <- spice_run(deck)$fourier
  expect_equal(rf$error, c("(1 * wavelength) longer than time span", "no such vector v(7)"))
  expect_equal(rf$card, c(1L, 1L))
})

test_that("all expands to every vector of the plot", {
  tran <- spice_plots(spice_run(corpus_path("four", "four_sine.cir")))[[1]]
  f <- spice_fourier(tran, 1e3, "all", nfreqs = 2)
  expect_equal(unique(f$vector), tran$variables$name)
})

test_that("malformed arguments and non-transient plots are classed errors", {
  tran <- spice_plots(spice_run(corpus_path("four", "four_sine.cir")))[[1]]
  expect_error(spice_fourier(list(), 1e3, "v(1)"), class = "spice_error_input")
  expect_error(spice_fourier(tran, 0, "v(out)"), class = "spice_error_input")
  expect_error(spice_fourier(tran, c(1, 2), "v(out)"), class = "spice_error_input")
  expect_error(spice_fourier(tran, 1e3, character()), class = "spice_error_input")
  expect_error(spice_fourier(tran, 1e3, "v(out)", nfreqs = 0), class = "spice_error_input")
  expect_error(spice_fourier(tran, 1e3, "v(out)", nperiods = 1.5), class = "spice_error_input")
  expect_error(spice_fourier(tran, 1e3, "v(out)", polydegree = -1), class = "spice_error_input")
  expect_error(spice_fourier(tran, 1e3, "v(out"), class = "spice_error_fourier")
  op <- spice_plots(spice_run(divider))[[1]]
  expect_error(spice_fourier(op, 1e3, "v(2)"), "not a transient", class = "spice_error_fourier")
})

test_that("sweep runs and session reruns carry their fourier", {
  deck <- "rc\nv1 1 0 sin(0 1 1k)\nr1 1 2 1k\nc1 2 0 100n\n.tran 10u 5m\n.four 1k v(2)\n.end"
  sw <- spice_sweep(deck, r1 = c(1e3, 2e3))
  m1 <- vapply(sw$runs, function(r) r$fourier$magnitude[[2]], double(1))
  expect_lt(m1[[2]], m1[[1]])
  s <- spice_session(deck)
  expect_identical(spice_rerun(s)$fourier, spice_run(deck)$fourier)
})
