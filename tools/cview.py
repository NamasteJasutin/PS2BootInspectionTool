#!/usr/bin/env python3
"""Print decompiled functions compactly (no blank lines / local declarations).

usage: cview.py [-d <analysis dir>] <addr> [addr...]   (default analysis/osdsys)
"""
import re
import sys
from pathlib import Path

CDIR = Path(__file__).resolve().parent.parent / "analysis" / "osdsys" / "c"
if len(sys.argv) > 2 and sys.argv[1] == "-d":
    CDIR = Path(sys.argv[2]) / "c"
    del sys.argv[1:3]
DECL = re.compile(r"^  (undefined\d?|int|uint|long|ulong|float|double|bool|byte|char|short|ushort|"
                  r"code|undefined)\s*\**\s*\*?[a-zA-Z_]\w*(\s*\[\w+\])*;$")

for a in sys.argv[1:]:
    text = (CDIR / f"{int(a, 16):08x}.c").read_text()
    for line in text.splitlines():
        if not line.strip() or DECL.match(line) or line.startswith("/* WARNING"):
            continue
        print(line)
