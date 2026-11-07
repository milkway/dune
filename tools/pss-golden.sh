#!/bin/sh
# Regenerate corpus/pss/golden/: for each corpus/pss/*.cir carrying a
# `*tran-golden: .tran ...` comment, ngspice-47 runs the deck with its
# `.pss` card replaced by that `.tran` (a long transient whose saved
# window is the last period, after the start-up has decayed) and
# writes the plot to <name>.raw.
#
# A deck carrying `*pss-ngspice: SCITER STEADYCOEFF` also runs
# ngspice's own .pss (its card needs those two trailing fields), and
# the frequency-domain plot it writes (harmonic magnitudes) goes to
# <name>.pss.raw.
#
# The Dune test (crates/dune-core/tests/pss.rs) compares Dune's .pss
# period with the tail on the D-0010 ruler, and its harmonics with
# ngspice's where ngspice's .pss finds the drive: ngspice's .pss is a
# free-running oscillator search that re-tunes the frequency, and on
# most driven circuits it settles off the drive (see
# docs/dune-core.md), so only decks where it lands on the drive get
# the marker.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
dir="$root/corpus/pss"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for deck in "$dir"/*.cir; do
    name=$(basename "$deck" .cir)
    tran=$(sed -n 's/^\*tran-golden: *//p' "$deck")
    [ -n "$tran" ] || continue
    raw="$dir/golden/$name.raw"
    sed -e "s/^\.pss .*/$tran/" -e '/^\.end[[:space:]]*$/d' "$deck" > "$tmp/$name.cir"
    printf '.control\nset filetype=ascii\nrun\nwrite %s\nquit\n.endc\n.end\n' "$raw" >> "$tmp/$name.cir"
    ngspice -b "$tmp/$name.cir" > "$tmp/out" 2>&1 || { cat "$tmp/out"; exit 1; }
    echo "$name: $(grep -c . "$raw") lines"
    extra=$(sed -n 's/^\*pss-ngspice: *//p' "$deck")
    [ -n "$extra" ] || continue
    raw="$dir/golden/$name.pss.raw"
    sed -e "s/^\(\.pss .*\)$/\1 $extra/" -e '/^\.end[[:space:]]*$/d' "$deck" > "$tmp/$name.cir"
    printf '.control\nset filetype=ascii\nrun\nsetplot pss2\nwrite %s\nquit\n.endc\n.end\n' "$raw" >> "$tmp/$name.cir"
    ngspice -b "$tmp/$name.cir" > "$tmp/out" 2>&1 || { cat "$tmp/out"; exit 1; }
    grep 'predicted fundamental' "$tmp/out" || true
    echo "$name: ngspice .pss, $(grep -c . "$raw") lines"
done
