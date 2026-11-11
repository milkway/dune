# Regenerate r/inst/AUTHORS from the locked Rust dependency graph of
# the R package (run from r/; tools/r-vendor.sh calls it). CRAN's Rust
# policy asks for the authors and licenses of the vendored crates.

meta <- jsonlite::fromJSON(
  system2(
    "cargo",
    c(
      "metadata", "--format-version", "1", "--locked",
      "--manifest-path", "src/rust/Cargo.toml"
    ),
    stdout = TRUE
  ),
  simplifyVector = FALSE
)

# Registry crates only: the spice crate itself and the dune engine
# crates (path dependencies, source = null) are the package's own code.
pkgs <- Filter(function(p) !is.null(p$source), meta$packages)
pkgs <- pkgs[order(
  vapply(pkgs, function(p) p$name, ""),
  vapply(pkgs, function(p) p$version, "")
)]

row <- function(p) {
  authors <- if (length(p$authors)) paste(unlist(p$authors), collapse = "; ") else "(not stated)"
  license <- if (is.null(p$license)) "(see license file)" else p$license
  sprintf("%-28s %-10s %-34s %s", p$name, p$version, license, authors)
}

header <- c(
  "The spice package statically links compiled Rust code. The Rust",
  "dependencies ('crates') below are vendored into src/rust/vendor.tar.xz",
  "for the offline build; their authors are copyright holders of the",
  "compiled code. Each crate's full license text ships inside the",
  "vendored sources. The simulation engine itself (the dune-core,",
  "dune-netlist, dune-raw and dune-pic crates, also in the archive) is",
  "part of this package: see DESCRIPTION and LICENSE.",
  "",
  sprintf("%-28s %-10s %-34s %s", "crate", "version", "license", "authors"),
  strrep("-", 100)
)

dir.create("inst", showWarnings = FALSE)
writeLines(c(header, vapply(pkgs, row, "")), "inst/AUTHORS", useBytes = TRUE)
cat("wrote inst/AUTHORS:", length(pkgs), "crates\n")
