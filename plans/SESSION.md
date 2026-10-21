# SESSION — hecatectl

Goal: Linux CLI to configure the HECATE G3M PRO 升級版 (updated) mouse, first priority = set polling (report) rate
(mouse currently stuck at 125 Hz, measured 8.00 ms report interval on 2026-10-21).

## Device facts (verified)
- 2.4G receiver: USB 35BB:D3FB, interface 1 (`mi_01`), HID collection 5 (`col05`) = vendor page 0xFF02 usage 0x02,
  report ID 0x08, 16-byte input + 16-byte output. Linux node: the hidraw for `usb-...-3/input1` (currently /dev/hidraw1).
- Wired: 35BB:D3FC. Device JSON: `mid`=5, `cid`=45 (wireless), sensor PAW3395, 32000 DPI, battery supported.
- Same interface also has: report 1 mouse, report 2 vendor FF03 (7B in), report 5 consumer, report 3 system ctrl,
  report 6 vendor FF04 (7B feature).

## Sources (reverse engineering, read-only, nothing executed)
- Vendor app: HECATE 1.1.44 (Edifier CN, 2026-10-15), Inno Setup 6.4.3, unpacked with innoextract (dscho branch).
- Unpacked files: ~/hecate-re/dl/x/app/ (HECATE.exe Qt6 x64, HIDUsb.dll = protocol, data/devices/*.json)
- Decompiles: ~/hecate-re/hidusb.c (all of HIDUsb.dll), ~/hecate-re/hecate_targeted.c (JM* functions of HECATE.exe)
- Ghidra projects: ~/hecate-re/ghidra/{proj (HECATE.exe), hidusb (HIDUsb.dll)}; scripts in ~/hecate-re/scripts
- Old HECATE Connect (2024-09) does NOT support this mouse (only VID 0x320F Evision devices).

## Protocol (verified from HIDUsb.dll)
- Packet = 17 bytes: [0]=0x08 report ID, [1]=cmd, [2..15]=args/data, [16]=checksum
- checksum = 0x55 - (sum of bytes [0..15]) mod 256 (FUN_1800149f0)
- Transaction (FUN_180014fd0): write packet, read input reports until one has valid checksum and
  resp[0]==req[0] && resp[1]==req[1]; 200 ms timeout, 2 tries.
- Byte [5] gets 0x80 OR'd in when a per-device flag (*param_1) is set (meaning TBD).
- Known cmds: 0x01 device info (4 random bytes at [6..9]; resp[10..12] = mid,cid,?), 0x03 online (resp[6]!=0),
  0x04 battery (resp[6],resp[7],resp[8..9]), 0x05 (byte5=0x02) TBD, 0x06 TBD (2 bytes), 0x12 version (2 bytes),
  0xB3 TBD (2 bytes)

## Status
- ✅ Protocol spec (flash read/write, flash layout, report-rate encoding) → docs/protocol.md (subagent)
  - Key: rate = flash 0x0000 value + 0x0001 check (0x55-v); 125=08 250=04 500=02 1000=01 (2k/4k/8k=10/20/40 hidden for this model).
    Read = cmd 08 `08 08 00 AH AL 0A`, data resp[6..15]; write = cmd 07 `08 07 00 AH AL len data…`, diff-only, no commit.
    1000 Hz write: `08 07 00 00 00 02 01 54 00…00 EF`. 0x80 flag = keyboard variant, never set for this mouse. 0x05 = StartPair (don't send).
- ✅ Lead verified the spec's key claims against the decompile: read builder FUN_180013880, write builder
  FUN_18001e700, rate field at 0x0000 in serializer FUN_180005ca0, UI index→code FUN_1402374d0.
- ✅ Rust CLI: `list`, `info`, `rate [HZ]`, `dump`; 19 unit tests pass; `HECATECTL_DEBUG=1` prints packets.
  Live: receiver answers `08 03` with a valid checksum (`08 03 00 00 00 01 00 d5 9e 53 …`), resp[6]=0 = mouse
  asleep (no movement on event5 during the test). Commands wait up to 15 s for the mouse to wake.
- ✅ udev rule installed (/etc/udev/rules.d/70-hecatectl.rules, uaccess ACL on /dev/hidraw1).
  `~/.local/bin/sudo-askpass` (zenity) added for `sudo -A`; making SUDO_ASKPASS permanent is up to the owner.
- ✅ GUI approved (Gate 2, 2026-10-21): standalone QML+Rust (cxx-qt), scope = match vendor app, theme = ii matugen
  colors. Plan: plans/GUI-PLAN.md. Workspace split done (crates/hecate-core, crates/hecatectl), commit 93d9cf1.
- ✅ core-settings agent: crates/hecate-core/src/settings.rs (Settings/Checked<T>, decode, diff → FlashWrite in vendor
  order, validate, verify, encode_dpi/decode_dpi) + Mouse::read_settings/apply + `hecatectl show` (units, key
  functions from docs/ui-features.md); 24 new tests.
  DPI decoder = vendor FUN_140235080 (ex bits 6..7 + flags 0x03), not the inferred one in protocol.md §9.1.
  Their live read failure was transient (mouse near sleep): lead re-tested, reads answer in ~20 ms with no
  handshake; transport now retries 4×250 ms (commit d6f7dcf).
- ✅ core-settings agent, phase 3 part C (committed): slots.rs (Shortcut/Macro codecs = FUN_180006e60 /
  FUN_180002ac0 bytes), Mouse::read_slots/write_shortcut/write_macro, long_range/set_long_range, factory_reset,
  start_pairing/pairing_status, protocol::format_version, DeviceInfo.receiver_version; CLI `slots`, `longrange`,
  `reset --yes`, `pair --yes`, versions in `info`/`list`. Live read-only: firmware v3.19, receiver v1.02, long range
  off; shortcut slots 3/4 = Volume Down/Up (Keys[3]/[4] type 5); macro slots 3/4 are stale leftovers (Ctrl+Win+←/→)
  whose names are **Big5** bytes ("切回去:D", "切螢幕:D") → vendor writes names in the PC's ANSI code page.
  Nothing written to the mouse (no 05/07/09/16 sent).
- ✅ core-settings agent, backup/restore (committed): backup.rs (raw image 0x0000–0x1AFF + metadata, hand-written
  JSON with 16-byte hex lines, strict parse; pure `plan_restore` = settings::diff on groups valid in the backup +
  whole-slot writes / vendor disabled forms; `check_writes` guards vendor shapes), Mouse::read_image/restore, CLI
  `backup <file>`, `restore <file> [--dry-run]`. **Owner backup: ~/hecate-re/backup-2026-10-21.json** (1000 Hz,
  stages 500/950/1550/3800/7550/14100, HP, sensor sleep 15 min, volume keys); live dry-run vs. it = 0 writes.
  Unmodelled but valid groups seen live at 0x54 (3+c), 0x58, 0x5A, 0x5C, 0x5E, 0xBB: reported, never written.
- ✅ RE agent: docs/ui-features.md (vendor UI feature matrix for g3mpro_upgrades). Key: 4 tabs (KeySet, Performance
  Settings, Macro Management, Built-in Settings), no lighting page (only DPI stage colours); rates 125–1000 only; 5
  remappable keys = Keys[0..4]; units: sensorSleepTime/allLedOffTime = 10 s, LOD 1/2 = 1MM/2MM, macro delay = ms;
  DPI decoder found (FUN_140235080), slider 0..499, max 6 stages; reset has no confirm dialog.
- ✅ GUI skeleton (crates/hecate-gui, cxx-qt 0.10, Cargo-only build): QML singletons `Device` (worker thread owns the
  Mouse; mpsc commands in, `qt_thread().queue` events out; vendor re-probe 1 s offline / 5 s online via
  find_devices + online()), `Theme` (ii matugen
  colors.json, mtime poll, fallback resources/theme-fallback.json; matugen `on_*` roles are `*Content` in QML),
  `I18n` (i18n/zh_TW.json + en.json; LC_ALL > LC_MESSAGES > LANG, default zh_TW; `language` override in config).
  Tunables in resources/config.json, user override $XDG_CONFIG_HOME/hecate-gui/config.json. Pages: Performance (rate
  segmented buttons) and Built-in Settings (Device Info: status/connection/battery/firmware + refresh), named after
  the vendor tabs; vendor labels (zh-TW in Taiwan terminology: opencc s2twp, reviewed by hand), vendor battery (0 -> 5, cap
  100) and firmware (`v3.19`) formats in src/display.rs. 36 tests; build + clippy clean.
  Screenshots without touching the desktop: `QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software hecate-gui
  [--page N] --screenshot out.png`. Live read: fw 0x0313, battery 90%, rate code 01 = 1000 Hz (set by the lead with
  `hecatectl rate 1000`).
- ✅ GUI phase 3 task D (gui-skeleton agent): full Performance Settings + Built-in Settings pages. Worker keeps the
  `Settings` from `Mouse::read_settings` (on connect/wake, with version + `long_range`); QML gets
  `Device.settingsJson` (view) + `optionsJson`, sends JSON patches via `Device.change()` → src/settings_map.rs (parse,
  vendor-style apply incl. the DPI-Quantity shift of FUN_140236ef0) → `Mouse::apply`; any failure → full re-read.
  src/dpi_slider.rs = §7.2 slider (all 500 positions tested against a transcription of FUN_1402367d0 and `DpiValue`),
  typed DPI snapped like FUN_140236030. Device actions: long range, factory reset (+ our confirm dialog, then
  vendor "Hint" box), pairing (vendor confirm text, 200 ms × 100 polls, progress, result), receiver row
  (`display::receiver_version`), Product Specification URL in config. Polls 1 s until connected+loaded, then 5 s;
  "Please wait for the data to load completely..." until the first read. Poll errors shown only when they repeat.
  Pairing uses core PAIRING_POLLS/PAIRING_POLL_INTERVAL (poll, then sleep; StartPair reply ignored like the vendor).
  57 gui tests, workspace green, clippy clean. Found: concurrent flash reads from two processes cross replies (reported
  to lead; needs address matching / flock in core).
- ✅ Phase 0 hardware validation (lead, 2026-10-21): full 0x100 read + `hecatectl show` work; flash had rate 08 4d
  (125 Hz); `hecatectl rate 1000` wrote `08 07 00 00 00 02 01 54 … ef`, ack + read-back 01 54 = 1000 Hz.
- 🔄 core-settings: cross-process safety (echo match on 07/08, operation-scoped flock, stale-snapshot check in apply)
- 🔄 gui-skeleton: phase 4 KeySet + Macro Management pages
- ⏳ Live write tests (owner approved all four 2026-10-21; waiting for owner at the mouse, after the safety fix):
  longrange on/off, slot_rewrite 3, reset + restore from ~/hecate-re/backup-2026-10-21.json, pairing (L+wheel+R 3 s)
- ⏳ Owner: measure with /tmp/mouse-diag/rate.py while moving the mouse (expect ~1.00 ms intervals).

## Rules
- No builds while the owner is playing osu! (check `osu-state` / `hyprctl activewindow -j`); build with nice -n 19 ionice -c3.
- Never send commands to the mouse that are not documented from the vendor code (no fuzzing; flash writes only via the
  vendor's documented save path).
