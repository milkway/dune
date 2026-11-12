# Precompute the example vignettes: knit each `*.Rmd.orig` here into the
# `*.Rmd` that ships, with its results and figures (vignettes/figures)
# written in. R CMD check and CRAN then only render text, not run
# simulations. Run from the package root after installing the package:
#
#   Rscript vignettes/precompile.R
#
# and commit the regenerated `.Rmd` and figures.
old <- setwd("vignettes")
for (src in list.files(pattern = "[.]Rmd[.]orig$")) {
  knitr::knit(src, sub("[.]orig$", "", src), envir = new.env())
}
setwd(old)
