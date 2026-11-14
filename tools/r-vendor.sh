#!/bin/sh
# Prepare r/ for a self-contained (CRAN) source tarball:
#
#   tools/r-vendor.sh && R CMD build r && R CMD check --as-cran spice_*.tar.gz
#
# Writes, all git-ignored and regenerated on every run:
#   r/src/rust/vendor.tar.xz   vendor/ (crates.io sources, `cargo vendor`)
#                              + dune/ (the engine crates and a trimmed
#                              copy of the workspace manifest they inherit)
#   r/tests/testthat/corpus/   the corpus files the testthat suite reads
#
# and regenerates r/inst/AUTHORS (committed) from the locked dependency
# graph. The package's configure picks the vendored sources up whenever
# the repository's crates/ is not next to the package (an unpacked
# tarball); src/Makevars then builds offline with at most two jobs.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
PKG="$ROOT/r"
CRATES="dune-core dune-netlist dune-raw dune-pic"

# macOS (bsdtar): no AppleDouble ._ files, no xattr/ACL pax records —
# GNU tar on the CRAN Linux builders warns on those.
export COPYFILE_DISABLE=1
TARFLAGS=""
if tar --version 2>/dev/null | grep -q bsdtar; then
  TARFLAGS="--no-xattrs --no-acls --no-fflags --no-mac-metadata"
fi

STAGE=$(mktemp -d "${TMPDIR:-/tmp}/spice-vendor.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT

# --- crates.io sources --------------------------------------------
# --locked: the tarball builds exactly r/src/rust/Cargo.lock. The
# dune path dependencies are not vendored by cargo (they are copied
# below); their registry dependencies are.
(cd "$PKG/src/rust" && cargo vendor --locked --versioned-dirs "$STAGE/vendor" >/dev/null)

# --- the dune engine crates ---------------------------------------
# Only what the library build reads: each crate's manifest and src/.
# The manifests inherit from [workspace.*], so a workspace manifest
# with just these members (and the root's package/dependency tables)
# goes with them, keeping the crates/ layout of the repository.
mkdir -p "$STAGE/dune"
for c in $CRATES; do
  (cd "$ROOT" && git ls-files "crates/$c/Cargo.toml" "crates/$c/src") |
    (cd "$ROOT" && tar $TARFLAGS -cf - -T -) | (cd "$STAGE/dune" && tar xf -)
done
# dune-core declares example and bench targets; a path dependency's
# manifest must still resolve its targets, so their sources go along
# (not the bench fixtures: nothing here builds the benches).
(cd "$ROOT" && git ls-files crates/dune-core/examples 'crates/dune-core/benches/*.rs') |
  (cd "$ROOT" && tar $TARFLAGS -cf - -T -) | (cd "$STAGE/dune" && tar xf -)
{
  echo "# Trimmed copy of the dune workspace manifest (tools/r-vendor.sh)."
  echo "[workspace]"
  echo 'resolver = "3"'
  printf 'members = ['
  sep=""
  for c in $CRATES; do printf '%s"crates/%s"' "$sep" "$c"; sep=", "; done
  echo "]"
  echo
  # [workspace.package] and [workspace.dependencies], verbatim; the
  # profiles stay behind (only the R crate's own profiles apply).
  awk '
    /^\[/ { keep = ($0 == "[workspace.package]" || $0 == "[workspace.dependencies]") }
    keep
  ' "$ROOT/Cargo.toml"
} >"$STAGE/dune/Cargo.toml"

rm -f "$PKG/src/rust/vendor.tar.xz"
# Sorted names, so reruns over the same lockfile archive the same tree.
(cd "$STAGE" && find vendor dune -print | LC_ALL=C sort >files &&
  tar $TARFLAGS --no-recursion -c -T files -f - | xz -9 -T0 >"$PKG/src/rust/vendor.tar.xz")

# --- the testthat corpus subset -----------------------------------
# Every corpus file the tests reach through corpus_path(), so the
# suite runs in R CMD check away from the repository. The whole-corpus
# test (test-corpus.R) needs the repository and skips without it.
T="$PKG/tests/testthat/corpus"
rm -rf "$T"
mkdir -p "$T"
(cd "$ROOT/corpus" && tar $TARFLAGS -cf - \
  ORACLE \
  basic/acrc.cir \
  four/four_sine.cir four/four_square.cir four/four_clip.cir four/four_rect.cir \
  four/golden/four_sine.four four/golden/four_square.four \
  four/golden/four_clip.four four/golden/four_rect.four \
  four/golden/four_sine.raw four/golden/four_square.raw \
  four/golden/four_clip.raw four/golden/four_rect.raw \
  golden/rc.raw golden/rca3040.raw \
  measure/meas_rlc.cir measure/golden/meas_rlc.raw measure/golden/meas_rlc.meas \
  pic \
  pss/pss_rc.cir pss/pss_rlc.cir pss/pss_rect.cir pss/pss_pulse.cir \
  pss/osc_vdp.cir pss/golden/osc_vdp.raw \
  spice3f5/examples/rc.cir spice3f5/examples/rca3040.cir \
  spice3f5/examples/diffpair.cir spice3f5/examples/mosmem.cir) |
  (cd "$T" && tar xf -)

# --- inst/AUTHORS ---------------------------------------------------
(cd "$PKG" && Rscript ../tools/r-authors.R)

du -h "$PKG/src/rust/vendor.tar.xz"
echo "vendored: $(ls "$STAGE/vendor" | wc -l | tr -d ' ') crates.io crates + $CRATES"
