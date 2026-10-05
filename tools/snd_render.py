#!/usr/bin/env python3
"""Offline (approximate) render of an OSDSYS SSsq sequence through its SShd/BD bank.

usage: snd_render.py [SEQ] [seq_volume] [out.wav]
       defaults: SEQ=BOOTS, seq_volume=the value OSDSYS sets with cmd 0x5013, output
       extracted/snd_wav/SND<SEQ>_render.wav (48 kHz stereo)

Reimplements what rom0:OSDSND does per note (tone selection, pitch table lookup, volume/pan
product, raw ADSR registers, key on/off at 60 Hz-quantised update times) and a textbook SPU
ADSR + looped ADPCM voice.  NOT modelled: SPU2 reverb (Studio C, see notes/sound.md), Gaussian
interpolation (linear is used), voice stealing, vibrato, sequence loops, the stop/fade command.
Also prints a coarse loudness timeline.
"""
import struct
import sys
import wave
from pathlib import Path

import numpy as np

from snd_dump import SND, ROOT, parse_hd, parse_sq, tick_times
from snd_vag2wav import decode_vag

RATE = 48000
# Real update period: hardware timer fires every 256 HSYNCs (PAL line rate 15625 Hz).
UPDATE_HZ = 15625.0 / 256.0
# Per-sequence volume passed to cmd 0x5013 by OSDSYS FUN_002004b8.
SEQ_VOL = {"BOOTS": 0x42, "TNNLS": 0x2A, "CLOKS": 0x2D, "TM30S": 0x1B, "TM60S": 0x1B,
           "LOGOS": 0x1B, "WARNS": 0x36, "RCLKS": 0x36}

_mod = (ROOT / "extracted/rom0/OSDSND").read_bytes()
PITCH_TAB = struct.unpack_from("<608H", _mod, 0x1DBB0 + 0xA0)      # 16 steps/semitone, [208]=0x1000
PAN_TAB = _mod[0x1E070 + 0xA0:0x1E070 + 0xA0 + 64]                 # (L, R) byte pairs, index pan>>2


def drv_pitch(root, note, fine, bend=0x40, bend_range=0):
    """OSDSND FUN_00018914: SPU2 pitch register value for a note."""
    b = ((bend - 0x40) * bend_range) >> 2
    if note < root:
        d = root - note
        v = (PITCH_TAB[(12 - d % 12) * 16 + b + 0xD0 + fine] * 44100) >> (d // 12 + 1)
    else:
        d = note - root
        v = (PITCH_TAB[(d % 12) * 16 + b + 0xD0 + fine] * 44100) << (d // 12)
    return (v // 48000) & 0xFFFF


def drv_volume(seqvol, chvol, progvol, vel, tonevol, pan_amt):
    """OSDSND FUN_00018c04: SPU2 VOLL/VOLR register value."""
    return ((((seqvol * chvol * progvol * vel) >> 14) * tonevol * pan_amt) >> 14) & 0x7FFF


def drv_pan(chpan, tonepan):
    p = max(0, min(0x7F, chpan + tonepan - 0x40))
    return PAN_TAB[(p >> 2) * 2], PAN_TAB[(p >> 2) * 2 + 1]


def adsr_envelope(a1, a2, n_on, n_total):
    """SPU ADSR level (0..0x7fff) per output sample; key-off after n_on samples."""
    env = np.zeros(n_total, dtype=np.float32)
    ar, dr, sl = (a1 >> 8) & 0x7F, (a1 >> 4) & 0xF, a1 & 0xF
    a_exp = bool(a1 & 0x8000)
    s_exp, s_dec, sr = bool(a2 & 0x8000), bool(a2 & 0x4000), (a2 >> 6) & 0x7F
    r_exp, rr = bool(a2 & 0x20), a2 & 0x1F
    sus_level = (sl + 1) * 0x800
    level, phase, i = 0, "A", 0
    while i < n_total:
        if phase != "R" and i >= n_on:
            phase = "R"
        if phase == "A":
            shift, step, exp, dec = ar >> 2, 7 - (ar & 3), a_exp, False
        elif phase == "D":
            shift, step, exp, dec = dr, -8, True, True
        elif phase == "S":
            shift, exp, dec = sr >> 2, s_exp, s_dec
            step = (-8 + (sr & 3)) if s_dec else (7 - (sr & 3))
        else:
            shift, step, exp, dec = rr, -8, r_exp, True
        cycles = 1 << max(0, shift - 11)
        st = step << max(0, 11 - shift)
        if exp and not dec and level > 0x6000:
            cycles *= 4
        if exp and dec:
            st = (st * level) >> 15
        end = min(n_total, i + cycles)
        if phase != "R" and i < n_on < end:
            end = n_on
        env[i:end] = level
        i = end
        level = max(0, min(0x7FFF, level + st))
        if phase == "A" and level >= 0x7FFF:
            phase = "D"
        if phase == "D" and level <= sus_level:
            phase = "S"
        if phase == "R" and level == 0:
            break
    return env


def render(seq_name, seq_vol=None, tail=3.0):
    bank = "BOOT"
    hd = parse_hd((SND / f"SND{bank}H").read_bytes())
    bd = (SND / f"SND{bank}B").read_bytes()
    sq = (SND / f"SND{seq_name}").read_bytes()
    hdr, ev = parse_sq(sq)
    tick_times(hdr, ev)
    if seq_vol is None:
        seq_vol = SEQ_VOL.get(seq_name, hdr["vol"])
    chan = [dict(prog=c[2], vol=c[3], pan=c[4], bend=c[10]) for c in hdr["channels"]]
    total = int((ev[-1]["update"] / UPDATE_HZ + tail) * RATE)
    mix = np.zeros((total, 2), dtype=np.float64)
    samples = {}
    active = {}                                  # (ch, note) -> list of voice dicts
    voices = []
    for e in ev:
        t = int(e["update"] / UPDATE_HZ * RATE)
        c = chan[e["ch"]]
        if e["type"] == "prog":
            c["prog"], c["bend"] = e["prog"], 0x40
        elif e["type"] == "cc" and e["cc"] == 7:
            c["vol"] = e["val"]                  # (live voices are not re-scaled here)
        elif e["type"] == "cc" and e["cc"] == 10:
            c["pan"] = e["val"]
        elif e["type"] == "on":
            pr = hd["programs"].get(c["prog"])
            if not pr:
                continue
            for tn in pr["tones"]:
                if not tn["lo"] <= e["note"] <= tn["hi"]:
                    continue
                pl, pr_ = drv_pan(c["pan"], tn["pan"])
                v = dict(start=t, off=None, tone=tn, note=e["note"], ch=e["ch"],
                         pitch=drv_pitch(tn["root"], e["note"], tn["fine"], c["bend"], tn["bend"]),
                         vl=drv_volume(seq_vol, c["vol"], pr["vol"], hd["vel"][e["vel"]], tn["vol"], pl),
                         vr=drv_volume(seq_vol, c["vol"], pr["vol"], hd["vel"][e["vel"]], tn["vol"], pr_))
                voices.append(v)
                active.setdefault((e["ch"], e["note"]), []).append(v)
                if pr["kind"] == "split":
                    break
        elif e["type"] == "off":
            for v in active.pop((e["ch"], e["note"]), []):
                v["off"] = t
    for v in voices:
        tn = v["tone"]
        if tn["sample"] not in samples:
            pcm, loop_start, loops, _ = decode_vag(bd, tn["sample"])
            samples[tn["sample"]] = (np.array(pcm, dtype=np.float32), loop_start if loops else None)
        pcm, loop_start = samples[tn["sample"]]
        n = total - v["start"]
        n_on = n if v["off"] is None else v["off"] - v["start"]
        env = adsr_envelope(tn["adsr1"], tn["adsr2"], n_on, n)
        pos = np.arange(n, dtype=np.float64) * (v["pitch"] / 4096.0)
        idx = pos.astype(np.int64)
        frac = (pos - idx).astype(np.float32)
        L = len(pcm)
        if loop_start is not None:
            span = L - loop_start
            wrap = lambda k: np.where(k < L, k, loop_start + (k - loop_start) % span)  # noqa: E731
            s = pcm[wrap(idx)] * (1 - frac) + pcm[wrap(idx + 1)] * frac
        else:
            ok = idx + 1 < L
            s = np.zeros(n, dtype=np.float32)
            s[ok] = pcm[idx[ok]] * (1 - frac[ok]) + pcm[idx[ok] + 1] * frac[ok]
        s = s * env / 32768.0
        mix[v["start"]:, 0] += s * (v["vl"] / 16384.0)
        mix[v["start"]:, 1] += s * (v["vr"] / 16384.0)
        v["audible_end"] = v["start"] + (int(np.nonzero(np.abs(s) > 8)[0][-1]) if np.any(np.abs(s) > 8) else 0)
    mix *= 0x3FFF / 16384.0                      # master volume set by cmd 0x5012
    return hdr, ev, voices, mix


def main():
    seq = sys.argv[1] if len(sys.argv) > 1 else "BOOTS"
    vol = int(sys.argv[2], 0) if len(sys.argv) > 2 else None
    out = Path(sys.argv[3]) if len(sys.argv) > 3 else ROOT / "extracted/snd_wav" / f"SND{seq}_render.wav"
    hdr, ev, voices, mix = render(seq, vol)
    print(f"{len(voices)} SPU voices started; peak {np.abs(mix).max():.0f} (clip at 32767)")
    print("  start_s  off_s    end_s   ch note  pitch   rate_Hz  volL  volR  sample")
    for v in voices:
        off = f"{v['off'] / RATE:6.3f}" if v["off"] is not None else "   -  "
        print(f"  {v['start'] / RATE:6.3f}  {off}  {v['audible_end'] / RATE:6.3f}   {v['ch']}  {v['note']:3d}  "
              f"{v['pitch']:#06x}  {v['pitch'] * 48000 / 4096:7.0f}  {v['vl']:5d} {v['vr']:5d}  "
              f"{v['tone']['sample']:#07x}")
    out.parent.mkdir(parents=True, exist_ok=True)
    pcm = np.clip(mix, -32768, 32767).astype("<i2")
    with wave.open(str(out), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm.tobytes())
    print(f"wrote {out.relative_to(ROOT)} ({len(pcm) / RATE:.2f}s)")
    print("loudness (RMS of L+R per 0.25 s, dBFS):")
    step = RATE // 4
    for i in range(0, len(mix) - step + 1, step):
        r = np.sqrt(np.mean(mix[i:i + step] ** 2)) / 32768.0
        db = 20 * np.log10(r) if r > 0 else -120
        print(f"  {i / RATE:5.2f}s {db:7.1f} {'#' * max(0, int((db + 70) / 2))}")


if __name__ == "__main__":
    main()
