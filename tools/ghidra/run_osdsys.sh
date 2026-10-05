#!/bin/sh
# Analyse the unpacked OSDSYS image with Ghidra headless and export C + indexes.
set -e
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export JAVA_HOME="$ROOT/vendor/jdk-21.0.12.1+1/Contents/Home"
mkdir -p "$ROOT/ghidra_proj" "$ROOT/analysis/osdsys"
"$ROOT/vendor/ghidra_12.1.4_PUBLIC/support/analyzeHeadless" "$ROOT/ghidra_proj" osd \
    -import "$ROOT/extracted/osdsys_200000.bin" -overwrite \
    -loader BinaryLoader -loader-baseAddr 0x200000 -processor "r5900:LE:32:default" \
    -scriptPath "$ROOT/tools/ghidra" -preScript OsdPre.java \
    -postScript OsdExport.java "$ROOT/analysis/osdsys" \
    > "$ROOT/analysis/osdsys/headless.log" 2>&1
