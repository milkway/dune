#!/usr/bin/env bash
# Build the npm package of dune-wasm into this directory (pkg/ + the
# version in package.json). Usage, from anywhere:
#
#   crates/dune-wasm/npm/build.sh            # release build, wasm-opt -Oz
#   NO_OPT=1 crates/dune-wasm/npm/build.sh   # skip wasm-opt
#
# Then `npm test` here runs the Node smoke test and `npm pack` makes
# the tarball. Needs the wasm32-unknown-unknown target and
# wasm-bindgen-cli at the exact version of the `wasm-bindgen` crate in
# Cargo.lock (found on PATH, in $WASM_BINDGEN, or in wasm-pack's cache;
# else `cargo install wasm-bindgen-cli --version <v>`). wasm-opt
# (binaryen) is optional: PATH, $WASM_OPT or wasm-pack's cache.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
cd "$root"

# The wasm-bindgen CLI must match the crate exactly (the glue embeds
# a schema version).
wb_ver="$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/version = |"/, ""); print; exit }' Cargo.lock)"
[ -n "$wb_ver" ] || { echo "build.sh: wasm-bindgen not in Cargo.lock" >&2; exit 1; }

caches=("$HOME/Library/Caches/.wasm-pack" "${XDG_CACHE_HOME:-$HOME/.cache}/.wasm-pack")

find_wasm_bindgen() {
  if [ -n "${WASM_BINDGEN:-}" ]; then echo "$WASM_BINDGEN"; return; fi
  if command -v wasm-bindgen >/dev/null &&
     [ "$(wasm-bindgen --version | awk '{print $2}')" = "$wb_ver" ]; then
    command -v wasm-bindgen; return
  fi
  for c in "${caches[@]}"; do
    local b="$c/wasm-bindgen-cargo-install-$wb_ver/wasm-bindgen"
    if [ -x "$b" ]; then echo "$b"; return; fi
  done
  echo "build.sh: wasm-bindgen $wb_ver not found; cargo install wasm-bindgen-cli --version $wb_ver" >&2
  exit 1
}

find_wasm_opt() {
  if [ -n "${WASM_OPT:-}" ]; then echo "$WASM_OPT"; return; fi
  if command -v wasm-opt >/dev/null; then command -v wasm-opt; return; fi
  for c in "${caches[@]}"; do
    for b in "$c"/wasm-opt-*/bin/wasm-opt; do
      if [ -x "$b" ]; then echo "$b"; return; fi
    done
  done
}

wasm_bindgen="$(find_wasm_bindgen)"

cargo build -p dune-wasm --target wasm32-unknown-unknown --release
target_dir="$(cargo metadata --format-version 1 --no-deps |
  node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).target_directory))')"
wasm="$target_dir/wasm32-unknown-unknown/release/dune_wasm.wasm"

rm -rf "$here/pkg"
# `--target web`: an ES module whose default export takes the .wasm
# URL/bytes — the shape @strategicprojects/rpic ships, which Vite
# (`?url`), Web Workers and Node all load.
"$wasm_bindgen" --target web --out-dir "$here/pkg" --out-name dune_wasm "$wasm"

bg="$here/pkg/dune_wasm_bg.wasm"
if [ -z "${NO_OPT:-}" ]; then
  wasm_opt="$(find_wasm_opt)"
  if [ -n "$wasm_opt" ]; then
    # Features rustc's wasm32 target emits by default.
    "$wasm_opt" -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
      --enable-sign-ext --enable-reference-types --enable-multivalue \
      --enable-mutable-globals "$bg" -o "$bg.opt"
    mv "$bg.opt" "$bg"
  else
    echo "build.sh: wasm-opt not found; shipping the unoptimized .wasm" >&2
  fi
fi

# package.json's version follows the crate's.
crate_ver="$(cargo metadata --format-version 1 --no-deps |
  node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).packages.find(p=>p.name==="dune-wasm").version))')"
node -e '
  const fs = require("fs"), f = process.argv[1], v = process.argv[2];
  const p = JSON.parse(fs.readFileSync(f, "utf8"));
  if (p.version !== v) { p.version = v; fs.writeFileSync(f, JSON.stringify(p, null, 2) + "\n"); }
' "$here/package.json" "$crate_ver"

raw=$(wc -c <"$bg" | tr -d ' ')
gz=$(gzip -9c "$bg" | wc -c | tr -d ' ')
echo "dune-wasm $crate_ver -> $here/pkg (wasm $raw B, gzip -9 $gz B)"
