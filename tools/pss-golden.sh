#!/bin/sh
# Regenerate corpus/pss/golden/: for each corpus/pss/*.cir carrying a
# `*tran-golden: .tran ...` comment, ngspice-47 runs the deck with its
# `.pss` card replaced by that `.tran` (a long transient whose saved
# window is the last period, after the start-up has decayed) and
# writes the plot to <name>.raw (only the vectors of a
# `*tran-golden-vectors: v(a) v(b)` comment, when the deck has one).
#
# A deck carrying `*pss-ngspice: SCITER STEADYCOEFF` also runs
# ngspice's own .pss (its card needs those two trailing fields), and
# the frequency-domain plot it writes (harmonic magnitudes) goes to
# <name>.pss.raw. An autonomous-oscillator deck (FREQ 0 on Dune's
# card, which ngspice cannot take: its FREQ is a mandatory guess)
# carries the whole card ngspice runs instead, `*pss-ngspice-card:
# .pss ...`.
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
mkdir -p "$dir/golden"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
# Optional arguments: deck names to regenerate (default: every deck).
for deck in "$dir"/*.cir; do
    name=$(basename "$deck" .cir)
    if [ $# -gt 0 ]; then
        case " $* " in *" $name "*) ;; *) continue ;; esac
    fi
    tran=$(sed -n 's/^\*tran-golden: *//p' "$deck")
    [ -n "$tran" ] || continue
    raw="$dir/golden/$name.raw"
    sed -e "s/^\.pss .*/$tran/" -e '/^\.end[[:space:]]*$/d' "$deck" > "$tmp/$name.cir"
    vecs=$(sed -n 's/^\*tran-golden-vectors: *//p' "$deck")
    printf '.control\nset filetype=ascii\nrun\nwrite %s %s\nquit\n.endc\n.end\n' "$raw" "$vecs" >> "$tmp/$name.cir"
    ngspice -b "$tmp/$name.cir" > "$tmp/out" 2>&1 || { cat "$tmp/out"; exit 1; }
    echo "$name: $(grep -c . "$raw") lines"
    extra=$(sed -n 's/^\*pss-ngspice: *//p' "$deck")
    card=$(sed -n 's/^\*pss-ngspice-card: *//p' "$deck")
    [ -n "$extra$card" ] || continue
    raw="$dir/golden/$name.pss.raw"
    if [ -n "$card" ]; then
        sed -e "s/^\.pss .*/$card/" -e '/^\.end[[:space:]]*$/d' "$deck" > "$tmp/$name.cir"
    else
        sed -e "s/^\(\.pss .*\)$/\1 $extra/" -e '/^\.end[[:space:]]*$/d' "$deck" > "$tmp/$name.cir"
    fi
    printf '.control\nset filetype=ascii\nrun\nsetplot pss2\nwrite %s\nquit\n.endc\n.end\n' "$raw" >> "$tmp/$name.cir"
    ngspice -b "$tmp/$name.cir" > "$tmp/out" 2>&1 || { cat "$tmp/out"; exit 1; }
    grep 'predicted fundamental' "$tmp/out" || true
    echo "$name: ngspice .pss, $(grep -c . "$raw") lines"
done
