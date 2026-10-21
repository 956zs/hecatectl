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
- Ghidra projects: ~/hecate-re/ghidra/{proj (HECATE.exe), hidusb (HIDUsb.dll)}; scripts in /tmp/hecate-re-scripts
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
- ⏳ Owner test with the mouse awake: `hecatectl info`, then `hecatectl rate 1000`, then measure with
  /tmp/mouse-diag/rate.py (expect ~1.00 ms intervals).

## Rules
- No builds while the owner is playing osu! (check `osu-state` / `hyprctl activewindow -j`); build with nice -n 19 ionice -c3.
- Never send commands to the mouse that are not documented from the vendor code (no fuzzing; flash writes only via the
  vendor's documented save path).
