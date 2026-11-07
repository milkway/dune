#!/bin/sh
# Regenerate corpus/measure/golden/: for each corpus/measure/*.cir,
# ngspice-47 runs the deck once (interactive-style .control block, so
# that .meas is allowed next to a rawfile), writes its own plot to
# <name>.raw and its .meas results, at 15 digits, to <name>.meas.
# Failed measurements are recorded as `FAILED <name>` lines.
#
# The Dune test (crates/dune-core/tests/measure_ngspice.rs) measures
# ngspice's rawfile with Dune's measure layer and compares with
# ngspice's printout: measurement code against measurement code, on
# identical data.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
dir="$root/corpus/measure"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for deck in "$dir"/*.cir; do
    name=$(basename "$deck" .cir)
    raw="$dir/golden/$name.raw"
    sed '/^\.end[[:space:]]*$/d' "$deck" > "$tmp/$name.cir"
    printf '.control\nrun\nwrite %s\nquit\n.endc\n.end\n' "$raw" >> "$tmp/$name.cir"
    NGSPICE_MEAS_PRECISION=15 ngspice -b "$tmp/$name.cir" > "$tmp/out" 2> "$tmp/err" || true
    {
        awk '/Measurements for/ {on = 1; next} on && NF == 0 && seen {exit} on && NF > 0 {seen = 1; print}' "$tmp/out"
        sed -n 's/^Error: measure  \([^ ]*\) .*/FAILED \1/p' "$tmp/err"
    } > "$dir/golden/$name.meas"
    echo "$name: $(grep -c . "$dir/golden/$name.meas") results"
done
