#!/bin/sh
# Pre-push parity with .github/workflows/ci.yml's check job, run on a
# FRESH `git archive HEAD` export so untracked files (fixtures the
# gitignore ate, generated goldens) cannot mask a broken repo — the
# pss-goldens incident. CI stays the x86_64 half; this is everything
# the laptop can prove. ~6 min warm.
#
#   ./tools/preflight.sh            # HEAD
#
# Exits non-zero on the first failure. Uses its own target dir (the
# shared one is unsafe across concurrent worktrees).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
export CARGO_TARGET_DIR="${DUNE_PREFLIGHT_TARGET:-$HOME/.cargo/preflight-target}"
# The export always lands on the same path, next to the target dir.
# `git archive` stamps every file with the commit time, so on a rerun
# of the same commit cargo finds its test binaries fresh and reuses
# them - with the CARGO_MANIFEST_DIR of the run that built them baked
# in (env!). Under a fresh mktemp dir per run that path was gone, and
# every test reading the corpus through it failed with "No such file".
stage="$CARGO_TARGET_DIR.stage"
rm -rf "$stage"
mkdir -p "$stage"
trap 'rm -rf "$stage"' EXIT
git -C "$root" archive HEAD | tar -x -C "$stage"
cd "$stage"
rustup update stable --no-self-update >/dev/null 2>&1 || true
echo "== toolchain: $(rustc +stable --version)"
echo "== fmt" && cargo +stable fmt --check
echo "== clippy" && cargo +stable clippy --all-targets -- -D warnings
echo "== clippy (parallel)" && cargo +stable clippy -p dune-core --features parallel --all-targets -- -D warnings
echo "== test" && cargo +stable test --workspace
echo "== parallel bit-identity" && cargo +stable test -p dune-core --features parallel --release --lib --test parload
echo "== oracle" && cargo +stable run -q -p dune-oracle -- --ci
echo "== accuracy" && cargo +stable run -q -p dune-oracle --release -- --accuracy
echo "== sparse backend" && cargo +stable clippy -p dune-core --features solver-xingo --all-targets -- -D warnings && cargo +stable test -q -p dune-core --features solver-xingo --release && cargo +stable run -q -p dune-oracle --release --features dune-core/solver-xingo -- --ci
echo "== wasm" && cargo +stable clippy -p dune-wasm --target wasm32-unknown-unknown -- -D warnings
echo "== preflight: tudo verde (falta só o x86_64 do CI)"
