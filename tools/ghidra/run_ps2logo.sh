#!/bin/sh
# Analyse the unpacked PS2LOGO image (loads at 0x100000) with Ghidra headless and export C + indexes.
# Prerequisite: .venv/bin/python tools/osd_unpack.py --raw extracted/rom0/PS2LOGO 0x1000 extracted/ps2logo_100000.bin
set -e
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export JAVA_HOME="$ROOT/vendor/jdk-21.0.12.1+1/Contents/Home"
mkdir -p "$ROOT/ghidra_proj_logo" "$ROOT/analysis/ps2logo"
"$ROOT/vendor/ghidra_12.1.4_PUBLIC/support/analyzeHeadless" "$ROOT/ghidra_proj_logo" logo \
    -import "$ROOT/extracted/ps2logo_100000.bin" -overwrite \
    -loader BinaryLoader -loader-baseAddr 0x100000 -processor "r5900:LE:32:default" \
    -scriptPath "$ROOT/tools/ghidra" -preScript LogoPre.java \
    -postScript OsdExport.java "$ROOT/analysis/ps2logo" \
    > "$ROOT/analysis/ps2logo/headless.log" 2>&1
