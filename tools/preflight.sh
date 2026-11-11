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
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
git -C "$root" archive HEAD | tar -x -C "$stage"
cd "$stage"
export CARGO_TARGET_DIR="${DUNE_PREFLIGHT_TARGET:-$HOME/.cargo/preflight-target}"
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
