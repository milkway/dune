# A fresh directory under the session's tempdir() (removed with it).
test_tempdir <- function() {
  dir <- tempfile("spice-test-")
  dir.create(dir)
  dir
}
