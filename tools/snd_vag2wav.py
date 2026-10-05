#!/usr/bin/env python3
"""Decode the PS ADPCM ("VAG") samples of an OSDSYS sound bank body to WAV files.

usage: snd_vag2wav.py [BOOT|OSDD] [outdir]     (default: BOOT, extracted/snd_wav)

Sample start offsets come from the bank header (SND????H: tone table and/or SE table); each
sample runs until the first block whose flag byte has bit 0 (END) set.  Blocks are 16 bytes
(shift/filter, flags, 14 data bytes = 28 samples).  WAVs are written mono 16-bit at 44100 Hz,
which is the rate the driver plays a tone at its root note (it scales the 44.1 kHz-relative
pitch by 44100/48000 for the 48 kHz SPU2).
"""
import struct
import sys
import wave
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from snd_dump import SND, ROOT, parse_hd  # noqa: E402

FILTERS = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)]


def decode_vag(bd, start):
    """Decode one sample starting at byte offset `start`.

    Returns (pcm list, loop_start_sample or None, loops(bool), end_offset).
    """
    out = []
    s1 = s2 = 0
    loop_start = None
    pos = start
    loops = False
    while pos + 16 <= len(bd):
        hdr, flags = bd[pos], bd[pos + 1]
        shift, filt = hdr & 0xF, hdr >> 4
        f0, f1 = FILTERS[filt] if filt < 5 else (0, 0)
        if flags & 4:
            loop_start = len(out)
        for i in range(28):
            b = bd[pos + 2 + i // 2]
            nib = (b >> 4) if i & 1 else (b & 0xF)
            if nib >= 8:
                nib -= 16
            s = (nib << 12) >> shift if shift <= 12 else (nib << 12) >> 9
            s += (s1 * f0 + s2 * f1 + 32) >> 6
            s = max(-32768, min(32767, s))
            out.append(s)
            s2, s1 = s1, s
        pos += 16
        if flags & 1:
            loops = bool(flags & 2)
            break
    return out, loop_start, loops, pos


def bank_samples(name):
    """Return (bd bytes, sorted list of sample start offsets) for bank BOOT or OSDD."""
    hd = parse_hd((SND / f"SND{name}H").read_bytes())
    bd = (SND / f"SND{name}B").read_bytes()
    offs = set()
    for pr in hd["programs"].values():
        offs.update(t["sample"] for t in pr["tones"])
    offs.update(e["sample"] for e in hd["se"])
    return bd, sorted(offs)


def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "BOOT"
    outdir = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "extracted" / "snd_wav"
    outdir.mkdir(parents=True, exist_ok=True)
    bd, offs = bank_samples(name)
    print(f"bank {name}: body {len(bd):#x} bytes, {len(offs)} samples")
    for o in offs:
        pcm, loop_start, loops, end = decode_vag(bd, o)
        # Skip the leading all-zero block(s) only for the statistics, not in the output.
        peak = max((abs(x) for x in pcm), default=0)
        path = outdir / f"{name}_{o:05x}.wav"
        with wave.open(str(path), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(44100)
            w.writeframes(struct.pack(f"<{len(pcm)}h", *pcm))
        loop_txt = (f"loops from sample {loop_start} ({loop_start / 44100:.3f}s)"
                    if loops and loop_start is not None else "one-shot")
        print(f"  {o:#07x}..{end:#07x}  {len(pcm):6d} samples = {len(pcm) / 44100:6.3f}s @44.1k  "
              f"peak {peak:5d}  {loop_txt}  -> {path.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
