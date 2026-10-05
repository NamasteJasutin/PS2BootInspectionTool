#!/bin/sh
# Analyse the IOP sound driver rom0:OSDSND (IRX, MIPS-I LE) with Ghidra headless and
# export decompiled C + indexes to analysis/osdsnd/ (uses its own project dir).
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export JAVA_HOME="$ROOT/vendor/jdk-21.0.12.1+1/Contents/Home"
mkdir -p "$ROOT/ghidra_proj_snd" "$ROOT/analysis/osdsnd"
"$ROOT/vendor/ghidra_12.1.4_PUBLIC/support/analyzeHeadless" "$ROOT/ghidra_proj_snd" osdsnd \
    -import "$ROOT/extracted/rom0/OSDSND" -overwrite \
    -processor "MIPS:LE:32:default" \
    -scriptPath "$ROOT/tools/ghidra" \
    -postScript OsdExport.java "$ROOT/analysis/osdsnd" \
    > "$ROOT/analysis/osdsnd/headless.log" 2>&1
