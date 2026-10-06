#!/bin/sh
# Re-analyse the unpacked OSDSYS image with the symbol table applied and export C + indexes
# to analysis/osdsys_named/ (same layout as analysis/osdsys/).  Uses its own project dir.
# Rebuild the symbol table first with: .venv/bin/python tools/build_symbols.py
set -e
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export JAVA_HOME="$ROOT/vendor/jdk-21.0.12.1+1/Contents/Home"
OUT="$ROOT/analysis/osdsys_named"
mkdir -p "$ROOT/ghidra_proj_named" "$OUT"
"$ROOT/vendor/ghidra_12.1.4_PUBLIC/support/analyzeHeadless" "$ROOT/ghidra_proj_named" osd_named \
    -import "$ROOT/extracted/osdsys_200000.bin" -overwrite \
    -loader BinaryLoader -loader-baseAddr 0x200000 -processor "r5900:LE:32:default" \
    -scriptPath "$ROOT/tools/ghidra" \
    -preScript BootPre.java "bss=2C8F74:416A30:.bss" "bss=1F0000:200000:osdctx" "gp=2CFFF0" \
        "entry=200008:_start" "entry=209EB8:main" \
    -postScript ApplySymbols.java "$ROOT/analysis/symbols/osdsys.tsv" \
    -postScript OsdExport.java "$OUT" \
    > "$OUT/headless.log" 2>&1
echo "osdsys_named done"
