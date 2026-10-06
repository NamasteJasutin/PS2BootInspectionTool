#!/bin/sh
# Decompile the EE boot chain before OSDSYS with Ghidra headless:
#   rom0:RESET  (EE+IOP reset vectors, base 0xBFC00000; RDRAM init module mapped at 0xBFC41000)
#   rom0:KERNEL (EE kernel, base 0x80000000; the ROM is mapped alongside so ROM calls resolve)
#   rom0:EELOAD (raw EE program, base 0x82000)
# Uses its own project dir (ghidra_proj_boot/) and writes analysis/{reset,kernel,eeload}/.
# Names come from analysis/symbols/{reset,kernel,eeload}.tsv (ApplySymbols.java).
# usage: run_boot.sh [reset|kernel|eeload ...]   (default: all three)
set -e
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export JAVA_HOME="$ROOT/vendor/jdk-21.0.12.1+1/Contents/Home"
HL="$ROOT/vendor/ghidra_12.1.4_PUBLIC/support/analyzeHeadless"
PROJ="$ROOT/ghidra_proj_boot"
SYM="$ROOT/analysis/symbols"
ROM="$ROOT/extracted/rom0"
mkdir -p "$PROJ"
targets="${*:-reset kernel eeload}"

for t in $targets; do
  out="$ROOT/analysis/$t"
  mkdir -p "$out"
  case "$t" in
  reset)
    "$HL" "$PROJ" reset -import "$ROM/RESET" -overwrite \
      -loader BinaryLoader -loader-baseAddr 0xBFC00000 -processor "r5900:LE:32:default" \
      -scriptPath "$ROOT/tools/ghidra" \
      -preScript BootPre.java "file=$ROM/RDRAM:BFC41000:RDRAM" \
        "bss=80000000:80020000:ram" \
        "entry=BFC00000:_reset_vector" "entry=BFC00800:ee_reset" "entry=BFC02000:iop_reset" \
        "entry=BFC00180:rom_exception_loop" "entry=BFC008FC:ee_flush_caches" \
        "entry=BFC00C00:ee_load_kernel" "entry=BFC41000:rdram_init" \
      -postScript ApplySymbols.java "$SYM/reset.tsv" \
      -postScript OsdExport.java "$out" > "$out/headless.log" 2>&1 ;;
  kernel)
    "$HL" "$PROJ" kernel -import "$ROM/KERNEL" -overwrite \
      -loader BinaryLoader -loader-baseAddr 0x80000000 -processor "r5900:LE:32:default" \
      -scriptPath "$ROOT/tools/ghidra" \
      -preScript BootPre.java "file=$ROM/RESET:BFC00000:ROM" "file=$ROM/RDRAM:BFC41000:RDRAM" \
        "bss=80016E30:80100000:kbss" \
        "entry=80001000:_kernel_entry" "entry=80000000:v_tlb_refill" "entry=80000080:v_counter" \
        "entry=80000100:v_debug" "entry=80000180:v_common" "entry=80000200:v_interrupt" \
        "entry=80005598:KLoadExec" "entry=80005988:ExecOSD" "entry=800059A0:ExitToBrowser" \
      -postScript ApplySymbols.java "$SYM/kernel.tsv" \
      -postScript OsdExport.java "$out" > "$out/headless.log" 2>&1 ;;
  eeload)
    "$HL" "$PROJ" eeload -import "$ROM/EELOAD" -overwrite \
      -loader BinaryLoader -loader-baseAddr 0x82000 -processor "r5900:LE:32:default" \
      -scriptPath "$ROOT/tools/ghidra" \
      -preScript BootPre.java "bss=91190:98000:.bss" "gp=A91F0" \
        "entry=82008:_start" "entry=82388:main" \
      -postScript ApplySymbols.java "$SYM/eeload.tsv" \
      -postScript OsdExport.java "$out" > "$out/headless.log" 2>&1 ;;
  esac
  echo "$t done"
done
