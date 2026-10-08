#!/bin/sh
# Opens each venue in your real Firefox with the form pre-filled where the site allows it
# (Reddit, Hacker News, GitHub issues take title/url/body as URL parameters) and puts the
# text on the clipboard where it does not (PSX-Place, Discord). No browser automation, so
# nothing trips bot detection; you review and press Submit.
#
#   docs/community/post.sh <psx-place|reddit|hn|pcsx2|ghidra-ee|ghidra-awesome> [canonical URL]
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
URL="${2:-https://github.com/NamasteJasutin/PS2BootInspectionTool/blob/main/docs/writeup.md}"
enc() { python3 -c 'import sys,urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$1"; }
title_of() { sed -n '1s/^Title: //p' "$1"; }
body_of()  { sed '1,2d' "$1" | sed "s#<URL>#$URL#g"; }
clip_steps() {  # file: clipboard = title, wait, clipboard = body
  title_of "$1" | tr -d '\n' | pbcopy
  echo "Clipboard = TITLE. Click the title field, Cmd+V, then press Enter here."; read -r _
  body_of "$1" | pbcopy
  echo "Clipboard = BODY. Click the body field, Cmd+V, review, press Post."
}
case "$1" in
  psx-place)
    open -a Firefox "https://www.psx-place.com/forums/"
    echo "Go to the PS2 development forum → Post thread."; clip_steps "$HERE/psx-place.md" ;;
  reddit)
    f="$HERE/reddit-emulation.md"
    open -a Firefox "https://www.reddit.com/r/emulation/submit?type=link&url=$(enc "$URL")&title=$(enc "$(title_of "$f")")"
    body_of "$f" | pbcopy; echo "Title and link are pre-filled; clipboard = the text body if you want to add it as a comment/body." ;;
  hn)
    f="$HERE/hackernews.md"
    open -a Firefox "https://news.ycombinator.com/submitlink?u=$(enc "$URL")&t=$(enc "$(title_of "$f")")"
    sed -n '/first comment:)/,$p' "$f" | sed '1d' | sed "s#<URL>#$URL#g" | pbcopy
    echo "Submission is pre-filled; clipboard = the author comment for after it is up." ;;
  pcsx2)
    f="$HERE/pcsx2-post.md"
    open -a Firefox "https://forums.pcsx2.net/"
    sed '1d' "$f" | sed "s#<link to the published write-up>#$URL#g" | pbcopy
    echo "Clipboard = the whole post (title is its first line)." ;;
  ghidra-ee)
    t="Link to ps2-bios-ghidra in the README?"
    b="Would you take a link to ps2-bios-ghidra (https://github.com/NamasteJasutin/ps2-bios-ghidra) in the README's related projects? It is a headless analyzeHeadless wrapper that uses this extension's r5900 language to decompile PS2 BIOS modules (RESET, KERNEL, EELOAD, OSDSYS, PS2LOGO) with the right memory blocks, \$gp and entry points, plus 750+ symbol names for ROM 2.00 E from a boot-path write-up: $URL . Happy to open the PR."
    open -a Firefox "https://github.com/chaoticgd/ghidra-emotionengine-reloaded/issues/new?title=$(enc "$t")&body=$(enc "$b")" ;;
  ghidra-awesome)
    t="Add ps2-bios-ghidra"
    b="Scripts section: [ps2-bios-ghidra](https://github.com/NamasteJasutin/ps2-bios-ghidra) - Pre-analysis, symbol-application and C-export scripts plus per-BIOS profiles for the PlayStation 2 BIOS."
    open -a Firefox "https://github.com/AllsafeCyberSecurity/awesome-ghidra/issues/new?title=$(enc "$t")&body=$(enc "$b")" ;;
  *) sed -n '2,8p' "$0"; exit 2 ;;
esac
