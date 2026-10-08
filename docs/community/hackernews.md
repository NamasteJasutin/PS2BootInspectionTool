Title: What a PlayStation 2 does between power-on and the game
URL: <URL>

(No text body — HN submissions with a URL take no text. If asked for a comment, post the following as the first comment:)

Author here. This came out of a static reading of the BIOS; nothing was captured from hardware, so the durations are estimates and marked as such. The things I'd flag as not-widely-known: the boot screen towers are the console's play history from the memory card (21 records × 6 slots, towers at launches 1/14/24/34/44/54); the "PlayStation 2" logo bitmap is read from the first 12 sectors of the game disc and checksummed per region; and the OSD program is stored in a small custom LZ format behind a loader stub. The tool in the repo replays the boot from your own BIOS and PCSX2 memory card; there's no Sony data in it.
