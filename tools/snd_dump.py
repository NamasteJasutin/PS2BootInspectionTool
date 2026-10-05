#!/usr/bin/env python3
"""Dump the OSDSYS sound assets: an "SShd" bank header and/or an "SSsq" sequence.

usage: snd_dump.py                      (boot sound: SNDBOOTH + SNDBOOTS)
       snd_dump.py <file> [<file> ...]  (any SShd / SSsq files; type is auto-detected)
       snd_dump.py --all                (every SND* asset)

Layouts follow what rom0:OSDSND actually reads (see notes/sound.md); fields the driver
never touches are printed as "unk".
"""
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SND = ROOT / "extracted" / "SNDIMAGE_unpacked"
NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]

# Nominal sequencer update rate assumed by the driver's tempo maths (cmd 0x500a sets 60).
NOMINAL_HZ = 60.0
# Real update rate: IOP hardware timer counting 256 HSYNCs (PAL 15625 Hz line rate).
PAL_HZ = 15625.0 / 256.0


def note_name(n):
    return f"{NOTE_NAMES[n % 12]}{n // 12 - 1}"


# ----------------------------------------------------------------------------- SShd

def adsr_text(a1, a2):
    am = "exp" if a1 & 0x8000 else "lin"
    ar = (a1 >> 8) & 0x7F
    dr = (a1 >> 4) & 0xF
    sl = a1 & 0xF
    sm = "exp" if a2 & 0x8000 else "lin"
    sd = "dec" if a2 & 0x4000 else "inc"
    sr = (a2 >> 6) & 0x7F
    rm = "exp" if a2 & 0x20 else "lin"
    rr = a2 & 0x1F
    return f"A={am}:{ar:02x} D={dr:x} SL={sl:x} S={sm}/{sd}:{sr:02x} R={rm}:{rr:02x}"


def parse_hd(d):
    """Return a dict describing an SShd bank header."""
    assert d[0xC:0x10] == b"SShd"
    hd = {
        "hd_size": struct.unpack_from("<I", d, 0)[0],
        "bd_size": struct.unpack_from("<I", d, 4)[0],
        "off_prog": struct.unpack_from("<I", d, 0x10)[0],
        "off_vel": struct.unpack_from("<I", d, 0x14)[0],
        "off_lfo": struct.unpack_from("<I", d, 0x18)[0],
        "off_se": struct.unpack_from("<I", d, 0x2C)[0],
        "programs": {},
        "se": [],
    }
    if hd["off_prog"] != 0xFFFFFFFF:
        base = hd["off_prog"]
        max_prog, = struct.unpack_from("<H", d, base)
        for p in range(max_prog + 1):
            off, = struct.unpack_from("<H", d, base + 2 + 2 * p)
            if off == 0xFFFF:
                continue
            ph = d[base + off:base + off + 8]
            mode = ph[0]
            if mode == 0xFF:
                # Drum-kit style: tone index = note - ph[6]; tone count is not stored.
                kind, ntones = "drum", 0
            elif mode & 0x80:
                kind, ntones = "layer", (mode & 0x7F) + 1
            else:
                kind, ntones = "split", mode + 1
            tones = []
            for t in range(ntones):
                o = base + off + 8 + 16 * t
                lo, hi, root, fine = struct.unpack_from("<BBBb", d, o)
                smp, a1, a2 = struct.unpack_from("<HHH", d, o + 4)
                volmode, vol, pan, bend, lfo, flags = struct.unpack_from("<6B", d, o + 10)
                tones.append(dict(lo=lo, hi=hi, root=root, fine=fine, sample=smp * 8, adsr1=a1,
                                  adsr2=a2, volmode=volmode, vol=vol, pan=pan, bend=bend, lfo=lfo,
                                  flags=flags))
            hd["programs"][p] = dict(kind=kind, vol=ph[1], raw=ph, tones=tones)
    if hd["off_vel"] != 0xFFFFFFFF:
        o = hd["off_vel"]
        hd["vel"] = d[o + 2:o + 2 + 128]
    if hd["off_se"] != 0xFFFFFFFF:
        o = hd["off_se"]
        hd["se_vol"], last = struct.unpack_from("<II", d, o)
        for i in range(last + 1):
            e = o + 0x20 + i * 0x40        # 0x20-byte table header, 0x40 bytes per effect
            if e + 0x14 > len(d):
                break
            vl, vr, pitch, u3, a1, a2, u6, flags, smp = struct.unpack_from("<8HI", d, e)
            hd["se"].append(dict(vol_l=vl, vol_r=vr, pitch=pitch, adsr1=a1, adsr2=a2, flags=flags,
                                 sample=smp, unk3=u3, unk6=u6))
    return hd


def dump_hd(path):
    d = Path(path).read_bytes()
    hd = parse_hd(d)
    print(f"== SShd bank {Path(path).name}: file {len(d):#x} bytes, header says hd={hd['hd_size']:#x} "
          f"bd={hd['bd_size']:#x}")
    print(f"   table offsets: programs={hd['off_prog']:#x} velocity={hd['off_vel']:#x} "
          f"lfo={hd['off_lfo']:#x} se={hd['off_se']:#x}")
    if "vel" in hd:
        v = hd["vel"]
        ident = all(v[i] == i for i in range(len(v)))
        print(f"   velocity curve: {'identity (0..127)' if ident else v.hex()}")
    samples = set()
    for p, pr in sorted(hd["programs"].items()):
        print(f"-- program {p} ({p:#04x}): {pr['kind']}, {len(pr['tones'])} tone(s), vol={pr['vol']}, "
              f"hdr={pr['raw'].hex(' ')}")
        for i, t in enumerate(pr["tones"]):
            samples.add(t["sample"])
            fl = []
            if t["flags"] & 1:
                fl.append("sustain")
            if t["flags"] & 0x20:
                fl.append("vibrato")
            if t["flags"] & 0x80:
                fl.append("reverb")
            print(f"   tone {i}: notes {t['lo']}..{t['hi']} root={t['root']}({note_name(t['root'])}) "
                  f"fine={t['fine']:+d}/16 sample@{t['sample']:#07x} vol={t['vol']} pan={t['pan']} "
                  f"bend={t['bend']} lfo={t['lfo']} volmode={t['volmode']:#x} "
                  f"flags={t['flags']:#04x}[{','.join(fl)}]")
            print(f"           ADSR1={t['adsr1']:#06x} ADSR2={t['adsr2']:#06x}  "
                  f"{adsr_text(t['adsr1'], t['adsr2'])}")
    if hd["se"]:
        print(f"-- sound-effect table: master vol={hd['se_vol']}, {len(hd['se'])} entries")
        for i, e in enumerate(hd["se"]):
            samples.add(e["sample"])
            print(f"   se {i:2d}: volL={e['vol_l']:#06x} volR={e['vol_r']:#06x} pitch={e['pitch']:#06x}"
                  f"({e['pitch'] * 44100 / 4096:.0f} Hz) sample@{e['sample']:#07x} "
                  f"flags={e['flags']:#06x}{'[reverb]' if e['flags'] & 0x80 else ''} "
                  f"unk3={e['unk3']:#x} unk6={e['unk6']:#x}")
            print(f"           ADSR1={e['adsr1']:#06x} ADSR2={e['adsr2']:#06x}  "
                  f"{adsr_text(e['adsr1'], e['adsr2'])}")
    print(f"   distinct sample offsets ({len(samples)}): "
          + " ".join(f"{s:#x}" for s in sorted(samples)))
    return hd


# ----------------------------------------------------------------------------- SSsq

CC_NAMES = {1: "vibrato depth", 2: "vibrato rate", 6: "data entry (loop count)", 7: "volume",
            10: "pan", 0x40: "damper (ignored)", 0x62: "nrpn lsb", 0x63: "nrpn msb (20=loop start, 30=loop end)"}


def parse_sq(d):
    """Decode an SSsq file exactly as OSDSND walks it.

    Returns (header dict, [event dict]).  Event times are in sequence ticks.
    """
    assert d[0xC:0x10] == b"SSsq"
    hdr = dict(vol=d[0], ppqn=struct.unpack_from("<H", d, 2)[0], bpm=struct.unpack_from("<H", d, 4)[0],
               channels=[d[0x10 + 16 * c:0x20 + 16 * c] for c in range(16)])
    ev = []
    pos, tick, status = 0x110, 0, 0
    while pos < len(d):
        start = pos
        if d[pos] & 0x80:
            status = d[pos]
        else:
            pos -= 1                       # running status
        d1 = d[pos + 1] if pos + 1 < len(d) else 0
        d2 = d[pos + 2] if pos + 2 < len(d) else 0
        hi, ch = status & 0xF0, status & 0xF
        e = dict(tick=tick, off=start, ch=ch)
        if hi == 0x90 and d2:
            e.update(type="on", note=d1, vel=d2); pos += 3
        elif hi in (0x80, 0x90):
            e.update(type="off", note=d1); pos += 3
        elif hi == 0xB0:
            e.update(type="cc", cc=d1, val=d2); pos += 3
        elif hi == 0xC0:
            e.update(type="prog", prog=d1); pos += 2
        elif hi == 0xE0:
            e.update(type="bend", val=d1); pos += 2
        elif status == 0xFF and d1 == 0x2F:
            e.update(type="end"); ev.append(e)
            break
        elif status == 0xFF and d1 == 0x51:
            e.update(type="tempo", bpm=struct.unpack_from("<H", d, pos + 2)[0]); pos += 4
        else:
            e.update(type="unknown", status=status); ev.append(e)
            break
        ev.append(e)
        delta = 0
        while True:                        # MIDI-style variable-length delta follows each event
            b = d[pos]; pos += 1
            delta = (delta << 7) | (b & 0x7F)
            if not b & 0x80:
                break
        tick += delta
    return hdr, ev


def tick_times(hdr, ev):
    """Attach nominal seconds and the 60 Hz update index at which the driver fires each event."""
    bpm, ppqn = hdr["bpm"], hdr["ppqn"]
    # The driver keeps a 20.12 fixed-point countdown: every update subtracts `inc`, and events
    # are consumed while the countdown is <= 0, each adding delta*4096.
    acc, upd, last_tick = 0, 0, 0
    sec = 0.0
    for e in ev:
        dt = e["tick"] - last_tick
        last_tick = e["tick"]
        sec += dt * 60.0 / (bpm * ppqn)
        acc += dt * 0x1000
        inc = (ppqn * bpm * 0x1000 // 60) // 60
        while acc >= 1:
            acc -= inc
            upd += 1
        e["sec"] = sec
        e["update"] = upd
        if e["type"] == "tempo":
            bpm = e["bpm"]
    return ev


def dump_sq(path):
    d = Path(path).read_bytes()
    hdr, ev = parse_sq(d)
    tick_times(hdr, ev)
    print(f"== SSsq sequence {Path(path).name}: {len(d):#x} bytes, vol={hdr['vol']} ppqn={hdr['ppqn']} "
          f"bpm={hdr['bpm']}  (1 tick = {60000.0 / (hdr['bpm'] * hdr['ppqn']):.4f} ms)")
    for c, ch in enumerate(hdr["channels"]):
        if ch[2] != 0xFF:
            print(f"   ch{c:<2d} initial: program={ch[2]} vol={ch[3]} pan={ch[4]} vibrate={ch[12]} raw={ch.hex(' ')}")
    print("   offset  tick  nominal_s  upd  pal_s   event")
    for e in ev:
        t = e["type"]
        if t == "on":
            txt = f"ch{e['ch']} NOTE ON  {e['note']} ({note_name(e['note'])}) vel={e['vel']}"
        elif t == "off":
            txt = f"ch{e['ch']} note off {e['note']} ({note_name(e['note'])})"
        elif t == "cc":
            txt = f"ch{e['ch']} cc {e['cc']}={e['val']}  [{CC_NAMES.get(e['cc'], 'ignored')}]"
        elif t == "prog":
            txt = f"ch{e['ch']} program {e['prog']}"
        elif t == "bend":
            txt = f"ch{e['ch']} pitch bend {e['val']}"
        elif t == "tempo":
            txt = f"tempo {e['bpm']} bpm"
        elif t == "end":
            txt = "END OF SEQUENCE"
        else:
            txt = f"?? status {e['status']:#x}"
        print(f"   {e['off']:#06x} {e['tick']:5d} {e['sec']:9.3f} {e['update']:4d} "
              f"{e['update'] / PAL_HZ:6.3f}  {txt}")
    return hdr, ev


def main():
    args = sys.argv[1:]
    if not args:
        files = [SND / "SNDBOOTH", SND / "SNDBOOTS"]
    elif args == ["--all"]:
        files = sorted(p for p in SND.iterdir() if p.name[-1] in "HS")
    else:
        files = [Path(a) for a in args]
    for f in files:
        magic = f.read_bytes()[0xC:0x10]
        if magic == b"SShd":
            dump_hd(f)
        elif magic == b"SSsq":
            dump_sq(f)
        else:
            print(f"== {f}: not SShd/SSsq")
        print()


if __name__ == "__main__":
    main()
