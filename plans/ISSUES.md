# ISSUES — hecatectl

Source: independent write-path review (2026-10-21). Repro tests for R2, R3, R6 and R7 are in
`~/hecate-re/review-scratch/src/review.rs` (port them into the repo along with each fix).

The review found no path that sends bytes outside the vendor shapes, and no forbidden command.

| ID | Sev | Area | Issue | Fix | Owner | Status |
|---|---|---|---|---|---|---|
| R3 | 🔴 P1 | gui settings_map / QML | A stage patch queued behind a DPI Quantity change lands on the shifted stage: the active 1600 stage becomes 900, the intended 800 stays hidden. Validate passes, so wrong values get written. | Disable editing while `Device.busy`, **and** carry the stage's expected register in the patch; refuse on mismatch. | gui-skeleton | open |
| R2 | 🟡 P2 | gui keymap / worker | No stale guard for slots: the slot write is skipped when the cached slot equals the new content, so a slot changed by another process stays wrong (or the key binds to foreign content). | Always write the slot (whole-slot write + read-back), or re-read it inside the exclusive section first. | gui-skeleton | open |
| R1 | 🟡 P2 | gui worker | Never re-reads while the mouse stays online. The on-mouse DPI or rate button, or a CLI change, leaves the page stale; the next edit is refused as stale. | Each online poll reads the 10 bytes at 0x000 and marks the snapshot stale (re-read) on mismatch. | gui-skeleton | open |
| R4 | 🟡 P2 | gui worker | After a failed write or reset, if the re-read also fails, the old snapshot stays published and editable. | Clear settings and slots back to the loading state on any failed write, reset or full read. | gui-skeleton | open |
| R7 | 🟡 P2 | gui macro_name | Names are decoded UTF-8 first, so Big5 names whose bytes are valid UTF-8 are misread (讀 → "Ū"), then re-encoded wrongly. | Decode with the configured encoding first, then fall back. | gui-skeleton | open |
| R5 | 🟡 P2 | cli | `pair` isn't tied to the receiver (it could send 0x05 over the cable, which the vendor never does), and `--device` isn't checked against find_devices. | Resolve every command through device_info; `pair` requires the 2.4G receiver. | core-settings | open |
| R6 | 🟡 P2 | core backup | Restore applies no value checks: an edited or foreign backup can plan rate code 0x40 (hidden 8 kHz), count 8 / current 7. | Refuse rate codes outside SETTABLE_RATES, stage count > 6, current ≥ count. Report other skipped groups. | core-settings | open |
| R8 | 🟡 P2 | core + gui | `require_online` runs inside `exclusive`: a sleeping mouse holds the flock up to 15 s, longer than LOCK_WAIT 10 s, so other processes fail with "device busy". | Wait for online before taking the lock; inside the lock, do a single online check. | core-settings (core), gui-skeleton (worker) | open |
