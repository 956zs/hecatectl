# GUI plan — hecate-gui (QML + Rust)

Owner request (2026-10-21): standalone window app, QML + Rust, feature set "盡量對齊原廠" (as close to the
vendor HECATE app as possible) for the G3M PRO updated version.

## Architecture

Cargo workspace:

| Crate | Kind | Contents |
|---|---|---|
| `crates/hecate-core` | lib | `protocol` (framing, checksum, packets), `hidraw` (discovery, transactions), `mouse` (device ops), `settings` (typed model of flash 0x000–0x0FF: decode, encode, diff → writes), later `keys` / `macros` (slots 0x100+ / 0x300+) |
| `crates/hecatectl` | bin | the current CLI, moved onto hecate-core |
| `crates/hecate-gui` | bin | cxx-qt + Qt 6.11 QML app |

GUI internals:
- One device worker thread owns the hidraw handle. The UI sends commands over a channel; results come back to
  QObjects through `CxxQtThread::queue`. The UI thread never blocks on USB.
- Hot-plug and sleep: poll `/sys/class/hidraw` every 2 s; show "asleep, move the mouse" while `08 03` says offline.
- Write policy = vendor policy: every change writes immediately, only the changed field, and is read back.
  A failed write triggers a full re-read so the UI never shows values that are not in flash.
- Fields that are UNCONFIRMED or buggy in the vendor code (0xDB, unknown floats) are kept byte-for-byte and never
  written.
- UI text lives in shipped resource files (zh-TW and en), not literals in QML.
- No firmware upgrade, ever. Factory reset (`08 09`) is behind a confirmation dialog. It is also the recovery path
  if a setting goes wrong.

Pages (the final list comes from `docs/ui-features.md`, which the RE subagent is producing):
- Performance: report rate, DPI stage count, active stage, per-stage DPI and colour
- Buttons: remap, combo keys, macros
- Sensor: LOD, Motion Sync, linear correction, ripple control, LP/HP, peak performance + sleep time
- Lighting: DPI LED, LED-off time
- Device: battery, firmware version, long-range mode, pairing, factory reset

## Phases

0. **Hardware validation (owner needed, blocks every write feature)**
   - `hecatectl info` with the mouse awake
   - `hecatectl dump`, compared against the spec defaults
   - `hecatectl rate 1000`, then measure with the rate probe
1. **Workspace and core**
   - Move to the workspace, then build `settings` decode/encode/diff
   - Unit tests from the vendor `defaultconfiguration.json` and the dump from phase 0
   - Parallel: core model (worker A) and workspace/CLI move (lead)
2. **GUI skeleton**
   - cxx-qt build, window, device worker
   - Performance page working end to end on the real mouse
3. **Remaining pages**
   - Parallel, partitioned by page file (QML page + Rust bridge object per page)
4. **Buttons and macros**
   - Slot encode/decode
   - Key-type semantics from `docs/ui-features.md`; extra RE where still UNCONFIRMED
5. **Review and verification**
   - Code review, clippy, tests
   - Live check of every control: write, read back, and observe the mouse behaviour where possible

## Risks

- Key and macro writes are the riskiest part because their semantics are partly UNCONFIRMED. They stay last, and
  factory reset is the fallback.
- The cxx-qt C++ build is heavy. Builds run at nice 19 / ionice idle and never while osu! is being played.
