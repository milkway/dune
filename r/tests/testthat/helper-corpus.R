# Locate the dune repository's corpus/: next to the package sources
# (tests/testthat -> r/ -> repo root), or through DUNE_ROOT (R CMD
# check runs the tests from a copy outside the repository). Tests that
# need it skip when it is absent, so a check outside the repo passes.
dune_root <- function() {
  candidates <- c(
    Sys.getenv("DUNE_ROOT"),
    testthat::test_path("..", "..", "..")
  )
  for (r in candidates) {
    if (nzchar(r) && file.exists(file.path(r, "corpus", "ORACLE"))) {
      return(normalizePath(r))
    }
  }
  NULL
}

corpus_path <- function(...) {
  root <- dune_root()
  if (is.null(root)) {
    testthat::skip("the dune corpus is not available (set DUNE_ROOT)")
  }
  path <- file.path(root, "corpus", ...)
  if (!file.exists(path)) {
    testthat::skip(paste("missing corpus file", path))
  }
  path
}

divider <- "divider
v1 1 0 dc 2
r1 1 2 1k
r2 2 0 1k
.op
.end"
