#!/bin/sh
# Regenerate corpus/four/golden/: for each corpus/four/*.cir, ngspice-47
#  1. runs the deck in true batch mode (`ngspice -b`, the `.four` path
#     of `dotcards.c::ft_cktcoms`) with `set numdgt=17` from a
#     `.spiceinit` in a private directory (`.options numdgt` does not
#     reach `cp_numdgt`), and keeps its Fourier tables verbatim in
#     <name>.four — 17 significant digits (16 for negatives, `pnum`);
#  2. runs it again with a `.control run/write` block and keeps the
#     transient plot it computed in <name>.raw (binary, exact doubles).
#
# dune-oracle --ci simulates each deck with Dune and compares its
# tables with <name>.four (rulers derived from the transient LTE
# budget), and transforms <name>.raw with Dune's .four code to check
# the port itself against the same tables at roundoff level.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
dir="$root/corpus/four"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
printf 'set numdgt=17\n' > "$tmp/.spiceinit"
for deck in "$dir"/*.cir; do
    name=$(basename "$deck" .cir)
    cp "$deck" "$tmp/$name.cir"
    (cd "$tmp" && ngspice -b "$name.cir" > "$tmp/out" 2> "$tmp/err") || true
    awk '/^Fourier analysis for/ {on = 1} /^Total analysis time/ {on = 0} on' "$tmp/out" \
        > "$dir/golden/$name.four"
    sed '/^\.end[[:space:]]*$/d' "$deck" > "$tmp/${name}_w.cir"
    printf '.control\nrun\nwrite %s\nquit\n.endc\n.end\n' "$dir/golden/$name.raw" >> "$tmp/${name}_w.cir"
    (cd "$tmp" && ngspice -b "${name}_w.cir" > /dev/null 2>&1) || true
    echo "$name: $(grep -c '^Fourier analysis for' "$dir/golden/$name.four") tables"
done
