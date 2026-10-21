# hecatectl

> **中文摘要（zh-TW）**
>
> 在 Linux 上取代原廠 HECATE 軟體，設定 **HECATE G3M PRO 升級版** 無線滑鼠（2.4G 接收器 35BB:D3FB／有線 35BB:D3FC）。
> 包含命令列工具 `hecatectl` 和圖形介面 `hecate-gui`。
>
> - 安裝：`scripts/install.sh`（先用 `--dry-run` 看它會做什麼）。它不會自己執行 sudo，只會印出安裝 udev 規則的指令。
> - 改設定前先備份：`hecatectl backup ~/hecate-backup.json`。出問題時用 `hecatectl restore ~/hecate-backup.json` 寫回，加 `--dry-run` 只顯示會寫入什麼。
> - 滑鼠休眠時指令會等它醒來，動一下滑鼠即可。
> - 不支援韌體更新，也不支援藍牙模式（原廠軟體同樣只能在 2.4G 或有線模式使用）。

A Linux replacement for the vendor's HECATE app for the **HECATE G3M PRO (updated / 升級版)** wireless mouse:
USB `35BB:D3FB` (2.4 GHz receiver) and `35BB:D3FC` (wired). It talks to the mouse over hidraw with the same packets
the vendor app sends.

- `hecatectl`: command-line tool
- `hecate-gui`: settings app (Qt Quick + Rust), laid out like the vendor app
- `hecate-core`: the library both use (protocol, settings model, slots, backup)

## Features

### Command line

| Command | What it does |
|---|---|
| `list` | detected mice, their hidraw nodes and receiver version |
| `info` | firmware and receiver versions, battery, report rate |
| `rate [125\|250\|500\|1000]` | show or set the report (polling) rate |
| `show` | all settings, decoded: DPI stages and colours, sensor options, sleep times, key mappings |
| `dump` | hex dump of the settings block (flash 0x000–0x0FF) |
| `slots` | decoded shortcut (combination / multimedia) and macro slots |
| `longrange [on\|off]` | show or set the long-distance radio mode |
| `backup <FILE>` | save everything the vendor app stores on the mouse to a new file |
| `restore <FILE> [--dry-run]` | write back what differs from a backup |
| `pair --yes` | put the receiver into pairing mode and wait for the mouse |
| `reset --yes` | factory reset: all settings, key mappings and macros |

`--device /dev/hidrawN` picks a device when more than one is found. `HECATECTL_DEBUG=1` prints every packet.

### GUI

- **Performance Settings**: report rate (125/250/500/1000 Hz), LP/HP mode, lift-off distance (1 or 2 mm), wave
  control, linear correction, motion sync, peak performance with sensor sleep time, DPI quantity (1–6 stages),
  per-stage DPI (50–32000) and colour, active stage.
- **Built-in Settings**: mouse sleep time, long-distance mode, receiver pairing, factory reset (with a confirmation),
  device info (connection, battery, mouse and receiver firmware).
- Key remapping and macro management are in progress.
- Traditional Chinese (default) and English, chosen from `LC_ALL` / `LC_MESSAGES` / `LANG`.
- Follows the Quickshell *ii* (matugen) colour scheme when its `colors.json` exists, with a built-in fallback.

### Not supported

- **Firmware upgrades.** Not implemented, on purpose.
- **Bluetooth mode.** The vendor app also works only in 2.4G or wired mode. Switch the mouse to 2.4G or plug in the
  cable.
- 2000/4000/8000 Hz. The vendor app hides them for this model, so they are refused.

## Safety model

- **Vendor-identical packets.** Every command and flash layout comes from static analysis of the vendor library.
  Nothing is probed or fuzzed, and commands the vendor app does not use for this mouse are never sent.
- **Diff-only writes with read-back.** A change writes only the settings groups that differ, each with its check byte,
  in the vendor's order. Every write is read back and compared.
- **Only values the vendor app can set.** Report rate, DPI, stage count, lift-off distance, sleep times and LP/HP are
  checked against the choices the vendor UI offers. Fields whose meaning is unknown are kept as they are and never
  written.
- **Stale-snapshot refusal.** Before writing, the settings are read again inside the same locked operation. If the
  mouse changed since they were read, nothing is written.
- **One process at a time.** Every operation takes an exclusive `flock` on the hidraw node, so the CLI and the GUI
  never interleave. Flash replies must echo the requested address and length.
- **Backup and restore.** `backup` saves flash 0x0000–0x1AFF. `restore` writes only what differs, using the same
  write shapes as the vendor app, and verifies the result. Bytes it cannot restore safely are listed, not written.

## Requirements

- Linux with hidraw.
- Rust with cargo: 1.87 or newer for the CLI; developed and tested with 1.90.
- For the GUI: Qt 6 with Qt Quick and Qt Quick Controls (Arch: `qt6-base qt6-declarative`), `qmake6` on `PATH` or
  `QMAKE` pointing to a Qt 6 qmake, and a C++ compiler. The CLI alone needs no Qt:
  `cargo build --release -p hecatectl`.
- Fonts used by the GUI (configurable): *Material Symbols Rounded* for icons, *Google Sans Flex* for text.

## Install

```sh
scripts/install.sh --dry-run    # show what it would do
scripts/install.sh              # build (release) and install
scripts/install.sh --uninstall  # remove it again
```

The script builds at low priority (`nice`/`ionice`), installs both programs to `~/.local/bin` and the desktop entry to
`$XDG_DATA_HOME/applications`. Running it again only replaces what changed. It never runs `sudo`.

### udev rule

To open the mouse without root, install `udev/70-hecatectl.rules`. The script prints these commands when the rule is
missing:

```sh
sudo install -m 0644 udev/70-hecatectl.rules /etc/udev/rules.d/70-hecatectl.rules
sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=hidraw
```

Then unplug and replug the receiver or cable.

## Usage

```sh
hecatectl list
hecatectl info
hecatectl rate 1000
hecatectl show
hecatectl slots
hecatectl backup ~/hecate-backup-$(date +%F).json
hecatectl restore ~/hecate-backup-2026-10-21.json --dry-run
```

### Back up before experimenting

Take a backup before trying settings, key mappings or a factory reset:

```sh
hecatectl backup ~/hecate-backup.json
```

The file is JSON with the flash image as hex lines, so two backups can be compared with `diff`. `restore --dry-run`
prints every planned write (address and bytes) and changes nothing. A backup restores only through the same
connection type it was taken with: receiver (D3FB) and cable (D3FC) count as different products.

### GUI

Start *HECATE G3M PRO* from the application launcher, or run `hecate-gui`. If your environment sets
`QT_QPA_PLATFORM=xcb`, start it with `QT_QPA_PLATFORM=wayland hecate-gui` to run natively on Wayland.

Settings live in `$XDG_CONFIG_HOME/hecate-gui/config.json`. They override the shipped defaults in
`crates/hecate-gui/resources/config.json` (language, fonts, poll intervals, theme file, macro folder).

## Troubleshooting

| Message | Meaning |
|---|---|
| `mouse is asleep, move it to wake it up...` | The receiver answers but the mouse sleeps. Move it; commands wait 15 s. |
| `device busy: another hecatectl or hecate-gui operation is running` | Another process holds the device lock (up to 10 s wait). Try again when it is done. |
| `permission denied (install udev/70-hecatectl.rules, ...)` | Install the udev rule (above), then replug. |
| `no HECATE G3M PRO (updated) found` | Receiver not plugged in, or the mouse is in Bluetooth mode. Use 2.4G or the cable. |
| `several mice found, pick one with --device` | Pass `--device /dev/hidrawN` (see `hecatectl list`). |
| `the mouse's settings changed since they were read` | The mouse changed a setting itself (for example the DPI button). Nothing was written; read again and retry. |

## Documentation

- [docs/protocol.md](docs/protocol.md): packet framing, commands, flash layout, encodings, with the vendor function
  each fact comes from.
- [docs/ui-features.md](docs/ui-features.md): the vendor app's controls for this model and the values they store.

## Credits and legal

The protocol was reverse engineered by static analysis of the vendor's Windows app (HECATE 1.1.44), solely for
interoperability. This repository contains no vendor code, binaries, images or other assets. HECATE and G3M PRO are
trademarks of their owners; this project is not affiliated with or endorsed by them.

Licensed under the [MIT License](LICENSE).
