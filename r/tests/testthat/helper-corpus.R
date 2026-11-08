# Locate the dune repository's corpus/: next to the package sources
# (tests/testthat -> r/ -> repo root), or through DUNE_ROOT. The whole-
# corpus test (test-corpus.R) needs this and skips without it.
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

# Where corpus_path() reads: the repository, else the subset that a
# release tarball carries in tests/testthat/corpus (tools/r-vendor.sh
# copies what the tests below ask for), so R CMD check away from the
# repository runs them instead of skipping.
corpus_root <- function() {
  root <- dune_root()
  if (!is.null(root)) {
    return(root)
  }
  bundled <- testthat::test_path(".")
  if (file.exists(file.path(bundled, "corpus", "ORACLE"))) {
    return(normalizePath(bundled))
  }
  NULL
}

corpus_path <- function(...) {
  root <- corpus_root()
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
