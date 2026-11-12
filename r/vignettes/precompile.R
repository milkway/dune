# Precompute the example vignettes: knit each `*.Rmd.orig` here into the
# `*.Rmd` that ships, with its results and figures (vignettes/figures)
# written in. The schematics of all vignettes are drawn here too. R CMD check and CRAN then only render text, not run
# simulations. Run from the package root after installing the package:
#
#   Rscript vignettes/precompile.R
#
# and commit the regenerated `.Rmd` and figures.
# The schematics: each figure of vignettes/circuits (pic, drawn with the
# package's prelude) becomes vignettes/figures/circuit-<name>.svg.
for (src in list.files("vignettes/circuits", pattern = "[.]pic$", full.names = TRUE)) {
  out <- file.path("vignettes/figures", sub("[.]pic$", ".svg", paste0("circuit-", basename(src))))
  writeLines(spice::spice_pic_svg(src), out)
}
old <- setwd("vignettes")
for (src in list.files(pattern = "[.]Rmd[.]orig$")) {
  knitr::knit(src, sub("[.]orig$", "", src), envir = new.env())
}
setwd(old)
