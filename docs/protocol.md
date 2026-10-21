# HECATE G3M PRO (updated / 升級版) — configuration protocol

Static reverse engineering of the vendor app HECATE 1.1.44 (`HIDUsb.dll` = protocol library,
`HECATE.exe` = Qt UI). Nothing was executed and no device was touched.
Every statement cites the function it comes from:

- `HIDUsb.dll` functions are cited as `FUN_1800xxxxx` / exported name (decompile: `~/hecate-re/hidusb.c`).
- `HECATE.exe` functions are cited as `FUN_14xxxxxxx` (decompiles: `~/hecate-re/hecate_targeted.c`,
  `~/hecate-re/h_batch{1..4}.c`, `~/hecate-re/h_ui_range.c`).

**UNCONFIRMED** marks anything not proven by code. All numbers are hex unless written in decimal.

HECATE.exe imports `UsbMouseC_*`. These are aliases of the `CS_UsbMouse_*` exports at the same address,
e.g. `UsbMouseC_LoadFlashData` = `CS_UsbMouse_LoadFlashData` @ 0x180014760 (export table comment there).

---

## 1. Transport and framing

| Item | Value | Source |
|---|---|---|
| Interface | 35BB:D3FB (2.4G receiver) or 35BB:D3FC (wired), `mi_01&col05`. Report ID 08, 16-byte output and 16-byte input | HECATE.exe strings `vid_35BB&pid_D3FB&mi_01&col05` (FUN_1400013b0 etc.), SESSION.md |
| Request | 17 bytes, written as an **output report** (WriteFile on the HID handle) | FUN_180014fd0 |
| Layout | `[0]=08` report ID, `[1]=cmd`, `[2..15]=args/data`, `[16]=checksum` | all command builders |
| Checksum | `chk(buf, a, b) = (0x55 - sum(buf[a..b-1])) & 0xFF`. The packet checksum is `chk(pkt, 0, 16)` stored at `[16]` | FUN_1800149f0 |

On Linux hidraw you write the same 17 bytes (the first byte is the report ID 08), and a read returns 17 bytes
that start with 08.

### Transaction (FUN_180014fd0)

1. Start an overlapped read of 17 bytes (it is started **before** the write).
2. Write the 17-byte request.
3. Wait up to `timeout` ms (200 for almost every command) for an input report. A report is accepted only when
   `resp[16] == chk(resp,0,16)` **and** `resp[0] == req[0]` (08) **and** `resp[1] == req[1]` (cmd).
   If a report arrives but does not match, it is dropped and the code waits again (up to 200 ms each time).
4. If the wait times out, the request is written again. There are 2 tries in total (`ClearSetting` uses 1 try).
5. No other response byte is checked. Bytes `[2..5]` of the response are never compared with the request, so
   whether the device echoes address or length is **UNCONFIRMED**.

Unsolicited reports `08 0A …` and `08 B8 …` can arrive at any time on the same interface (see §8). A reader
must skip them, which the matching rule in step 3 does.
Recommendation (not vendor behaviour): drain pending input before each request. Matching only on `[0..1]`
could otherwise accept a stale reply to an earlier request that used the same cmd.

---

## 2. Report (polling) rate — TOP PRIORITY

### Where it lives

The rate is a **flash config field**, not a direct command. It is at **flash address 0x0000** (1 value byte),
followed by its check byte at **0x0001**, where `check = (0x55 - value) & 0xFF`.

- Parse: config struct byte `[0]` ← flash `[0x00]` if `flash[0x01] == chk(flash,0,1)`. Otherwise it defaults to
  `01` (FUN_180005840).
- Write: if `new[0] != old[0]`, the code queues a write of the 2 bytes `[value, 0x55-value]` at address 0x0000
  (FUN_180005ca0, first block).
- JSON name `reportRate` = struct `[0]` (HECATE `GetMouseDateToFile` FUN_14011fe30). The vendor default
  `defaultconfiguration.json` has `"reportRate": 1`.

### Encoding (HECATE.exe FUN_1402374d0 = UI index → value; FUN_1402372e0 = value → UI index; labels FUN_14000f5e0)

| Rate | value @0x0000 | check @0x0001 | UI index / label |
|---|---|---|---|
| 125 Hz | `08` | `4D` | 0 / "125" |
| 250 Hz | `04` | `51` | 1 / "250" |
| 500 Hz | `02` | `53` | 2 / "500" |
| 1000 Hz | `01` | `54` | 3 / "1000" |
| 2000 Hz | `10` | `45` | 4 / "2000" |
| 4000 Hz | `20` | `35` | 5 / "4000" |
| 8000 Hz | `40` | `15` | 6 / "8000" |

**For this model only 125/250/500/1000 are offered.** FUN_14022e760 (the "Report Rate Set" panel) calls
`setVisible(false)` on the buttons with index ≥ 4 when the device id is `g3mpro_upgrades` (also `g2mpro` and
`g3multra`). Do **not** write `10`/`20`/`40` to this mouse. What the firmware would do with them is
**UNCONFIRMED**.

### What the vendor does when the user clicks a rate

1. At connect, a full flash read fills two identical structs: `new` (= `JMMouseDeviceMessage+0x10`) and
   `old`/shadow (= `+0x18`) (FUN_1400f8520 → `UsbMouseC_LoadFlashData(path, 0, new, old, 0)`, retried until it
   succeeds).
2. The UI writes the code into `new[0]` (FUN_1402374d0. The UI's `singleton+0x18` pointer equals
   `JMMouseDeviceMessage+0x10`, assigned in FUN_140119770).
3. It immediately calls `Write` (FUN_140118e60 → FUN_1400f9470 → `UsbMouseC_SaveFlashData(path, new, old)`).
   That function diffs `new` against `old` and sends only the changed fields. For a rate change this is exactly
   **one** packet. There is no commit command (§4.5).

### Minimal read-modify-write that reproduces the vendor's traffic

All bytes are hex. The packet checksum is the last byte.

```
1) online check   -> 08 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 4A
                     need resp[6] != 0                      (FUN_180015de0 / UsbMouse_GetOnLine)
2) read 0x0000    -> 08 08 00 00 00 0A 00 00 00 00 00 00 00 00 00 00 3B
                     resp[6] = current value, resp[7] = check. Valid iff resp[7] == (0x55-resp[6])&0xFF
3) write 0x0000 (only if it differs), e.g. 1000 Hz:
                  -> 08 07 00 00 00 02 01 54 00 00 00 00 00 00 00 00 EF
                     need a reply that starts 08 07 and has a valid checksum
4) verify: repeat step 2 and expect resp[6..7] == 01 54
```

Write packets for each rate. The packet checksum is always `EF`, because value + check = 0x55:

```
125  Hz: 08 07 00 00 00 02 08 4D 00 00 00 00 00 00 00 00 EF
250  Hz: 08 07 00 00 00 02 04 51 00 00 00 00 00 00 00 00 EF
500  Hz: 08 07 00 00 00 02 02 53 00 00 00 00 00 00 00 00 EF
1000 Hz: 08 07 00 00 00 02 01 54 00 00 00 00 00 00 00 00 EF
```

Notes:
- The vendor never writes the rate without having read the flash first. Step 2 replaces the vendor's full read,
  because the full read is only needed to build the diff, and for this change the diff is just address 0.
  The bytes sent in step 3 are identical to what `SaveFlashData` emits for this change.
- The vendor also re-reads the rate on its own when the mouse reports a change (event `08 0A`, `[6]` bit 1 →
  MouseFlashId 2 → reads address 0x0000. §3.4, §8). So the mouse can change the rate by itself (a button combo
  is likely; **UNCONFIRMED**).
- Whether the new rate applies immediately, and whether it applies in the same way over 2.4G and over the cable,
  is **UNCONFIRMED**. The vendor sends nothing after the write.
- Diagnostic hint, **UNCONFIRMED**: flash 0xB9 (`sensorPowerSavingModeEnable`, UI "LP"=0 / "HP"=1) and 0xB5
  (`sensorCustomSleepTimeEnable`) are power-related. Read the whole block 0x000–0x0FF before concluding why the
  mouse polls at 125 Hz.

---

## 3. Flash read — cmd 0x08

### 3.1 Request / response (FUN_180013880)

```
req:  [0]=08 [1]=08 [2]=00 [3]=addr>>8 [4]=addr&FF [5]=0A [6..15]=00 [16]=chk
resp: [0]=08 [1]=08 ... [6..15] = 10 data bytes from flash[addr .. addr+9]
```

- The address is 16-bit big-endian in `[3..4]`. `[2]` is always 00.
- `[5]` is always `0A` (10), including for the last partial chunk. With the keyboard flag set it would be `8A`
  (see §5. Never set for the mouse).
- Chunking: `ceil(len/10)` requests at `addr, addr+10, addr+20, …`. Each copies `resp[6..15]`, and the last one
  copies only `len % 10` (or 10) bytes.
- On a transaction failure the function returns early and leaves the rest of the buffer zero. The callers
  ignore this (§3.3).

### 3.2 Full read, MouseFlashId 0 (FUN_180014510 → FUN_180015de0 → FUN_180013a20 → FUN_1800063c0)

1. Online check `08 03` (FUN_180015de0). If it fails or `resp[6]==0`, `LoadFlashData` returns 0 and nothing
   else is sent.
2. Size probes for 16 shortcut slots: read 10 bytes at `0x100 + 0x20*i` (i = 0..15). `n = byte0`. The slot
   length is `3n+2` if `2 ≤ n ≤ 6`, else 0.
3. Size probes for 16 macro slots: read 10 bytes at `0x31F + 0x180*j` (j = 0..15). `n = byte0` (the count byte
   at slot offset 0x1F). The slot length is `5n+0x21` if `2 ≤ n ≤ 0x46`, else 0.
4. Read 0x100 bytes at 0x000 (26 requests: 0x000, 0x00A, …, 0x0FA).
5. For each shortcut slot with length > 0: read it from `0x100+0x20*i`.
6. For each macro slot with length > 0: read it from `0x300+0x180*j`.
7. The 0x2000-byte image (zero-filled, unread bytes stay 0) is parsed into the `new` struct by FUN_1800063c0,
   then `memcpy(old, new, 0x28DC)`.

`LoadFlashData` returns success when step 1 passed, **even if later reads failed** (FUN_180014510 returns 1
unconditionally). Failed fields then fail their check byte and fall back to defaults. An implementation must
treat read failures as errors and must not write diffs based on such data.

### 3.3 Flash map and size

The host-side image is 0x2000 bytes (`memset(…,0,0x2000)` in FUN_180014510). The used ranges are:

| Range | Content | Parsed by |
|---|---|---|
| 0x0000–0x00FF | config block (rate, DPI, LEDs, keys, sensor options), §9 | FUN_1800063c0, FUN_180005840 |
| 0x0100–0x02FF | 16 shortcut-key slots × 0x20 | FUN_180006d60 |
| 0x0300–0x1AFF | 16 macro slots × 0x180 | FUN_180002ba0 |

The physical flash size of the device is **UNCONFIRMED**. 0x1B00–0x1FFF is never read or written.

### 3.4 MouseFlashId (partial reads, FUN_180013c30; HECATE `JMMouseDeviceMessage::ReadFlash(MouseFlashId)` FUN_1400f9cc0)

A partial read updates **both** `new` and `old`, so it never triggers a write.

| Id | Reads | Updates | Trigger in HECATE |
|---|---|---|---|
| 0 | everything (§3.2) | all | connect (FUN_1400f8520); event bit 0x04 |
| 1 | 10 bytes @0x002 | `maxDPI` (stage count), `currentDPI` | event bit 0x01 |
| 2 | 2 bytes @0x000 (one 10-byte request) | `reportRate` | event bit 0x02 |
| 3 | 8 bytes @0x04C | DPI-LED mode/brightness/speed/enable | event bit 0x08 |
| 4 | 9 bytes @0x0A0 | LED bar block + enable | event bit 0x20 |
| 5 | 0x40 bytes @0x060 | 16 key mappings | — |
| 6..0x15 | shortcut slot `id-6` @`0x100+0x20*(id-6)` | that slot | — |
| 0x16..0x25 | macro slot `id-0x16` @`0x300+0x180*(id-0x16)` | that slot | — |

Event → id mapping: FUN_1400f8310 (parses `08 0A`, `[6]` bits) → signal → FUN_140118e20 → FUN_1400fc6e0 →
ReadFlash(id).

---

## 4. Flash write — cmd 0x07

### 4.1 Packet (FUN_18001e700)

```
[0]=08 [1]=07 [2]=00 [3]=addr>>8 [4]=addr&FF [5]=len (1..10; |0x80 only with the keyboard flag)
[6..6+len-1]=data  [rest up to 15]=00  [16]=chk(pkt,0,16)
```

The chunk size is 10 (`local_48 = 10` in `UsbMouse_SaveFlashData` @0x180014830). A queued item of `L` bytes
becomes `ceil(L/10)` packets at consecutive addresses (`addr += chunk`), each with `len` = 10 except the last.

### 4.2 Sending (FUN_180003480)

All queued packets are sent in queue order through FUN_180014fd0 (2 tries, 200 ms each). A packet succeeds
when a reply starting `08 07` with a valid checksum arrives. **Sending stops at the first failure.**
No online check is done before writing.

### 4.3 Diff semantics (FUN_180006630, called as `SaveFlashData(path, new, old)`)

`param_2`/`new` is the desired state and `param_3`/`old` is the shadow of what is believed to be in flash.
For every field group, if `new != old`, the code copies new→old **and** queues a write of only that group.
Unchanged groups are never written. The shadow is updated before sending, so after a failed send it no
longer matches the device (vendor bug). Always re-read after a failure.

Queue order (this is also the order on the wire):

1. Config scalars (FUN_180005ca0), each `[v, 0x55-v]` (2 bytes), at:
   0x00, 0x02, 0x04, 0x06, 0x08, 0x0A, 0xA9, 0xAB, 0xAD, 0xAF, 0xB1, 0xB3, 0xB5, 0xB7, 0xB9, 0xBD, 0xBF, 0xC1;
   then 32-bit values `[b0 b1 b2 b3, chk]` (5 bytes) at 0xC3, 0xC8, 0xCD, 0xD2; then 2-byte groups at 0xD7,
   0xD9; a 4-byte group at 0xDB; 2-byte groups at 0xDF, 0xE1, 0xE3, 0xE5.
2. For DPI stage i = 0..7: DPI value `[x, y, ex, chk]` at `0x0C+4i`, then colour `[R, G, B, chk]` at
   `0x2C+4i`.
3. DPI LED: 0x4C, 0x4E, 0x50, 0x52 (2 bytes each).
4. LED bar: `[mode, R, G, B, speed, brightness, chk]` at 0xA0 (7 bytes); enable `[v, chk]` at 0xA7.
5. Keys i = 0..15: `[type, p1, p2, chk]` at `0x60+4i`.
6. Shortcut slots (FUN_180006e60): if the serialized 0x180-byte slot image differs, `3n+2` bytes at
   `0x100+0x20i`. A disabled slot (n = 0) serializes as `[00, FF]`.
7. Macro slots (FUN_180006a70): if they differ, `5n+0x21` bytes at `0x300+0x180j`.
8. Flush (FUN_180003480).

### 4.4 Value + check scheme

Every config group is `data bytes … , check` with `check = (0x55 - sum(data)) & 0xFF` (FUN_1800149f0 called
on the group). The parser ignores a group whose check fails and uses the default instead (FUN_180005840,
FUN_1800063c0). Shortcut and macro slots carry one check byte per slot (§9.4).

### 4.5 Commit / ordering

- **No commit, save or "end" command exists.** `JMMouseDeviceMessage::Write` (FUN_1400f9470) only calls
  `UsbMouseC_SaveFlashData`, and FUN_180003480 only sends the queued `08 07` packets.
- The UI calls `Write` after every single change (FUN_140118e60 is called by each handler, e.g.
  FUN_1402374d0). One exception: the sensor-sleep-time handler FUN_140231ec0 does not call it, so that change
  goes out with the next write.
- The only ordering rule is to read before writing, so that the shadow matches flash. The vendor always does a
  full read at connect (FUN_1400f8ed0 → FUN_1400fc770 → read thread FUN_1400f8520).

### 4.6 Vendor quirks to avoid copying

- Group 0xDB: the parser stores `[0x26]=flash[DB]`, `[0x27]=flash[DC]`, `[0x28]=flash[DE]` (the check byte!)
  (FUN_180005840). The writer sends `[s[0x25], s[0x25], s[0x25], chk]` (FUN_180005ca0). Both are wrong. Do not
  write 0xDB. Its meaning is **UNCONFIRMED**.
- Size probe for macro slots: the vendor's partial read (id 0x16..) reads the header at `0x31F+…` and then the
  rest from `0x30A+…` (FUN_180013c30), which does not fit together. The full read (§3.2) is correct.

---

## 5. Byte [5] |= 0x80 flag

The flag is a per-call heap byte, `*param_1` of the device object. It is OR'd into byte `[5]` (or replaces
`00` with `80`) in every builder (e.g. FUN_180015de0, FUN_180013880, FUN_18001e700, FUN_1800160c0).

- Every `UsbKeyboard_*` entry point sets it to **1**, e.g. `UsbKeyboard_LoadFlashData` @0x18000cef0 and
  `UsbKeyboard_SaveFlashData` @0x18000d0d0 (`*local_58 = 1`, `*local_50 = 1`), `UsbKeyboard_GetDeviceInfo`.
- Every `UsbMouse_*` and `UsbMouseDongle_*` entry point sets it to **0**, e.g. `UsbMouse_LoadFlashData`
  @0x1800145f0 and `UsbMouse_SaveFlashData` @0x180014830 (`*local_50 = 0`), `UsbMouse_GetDeviceInfo`,
  `UsbMouseDongle_StartPair`.

**Meaning: the 0x80 bit is the keyboard flavour of the same protocol.** It has nothing to do with wired vs.
dongle. For this mouse, through the receiver 35BB:D3FB or wired on D3FC, the bit is **never set**. HECATE uses
the same `UsbMouseC_*` calls for both PIDs (FUN_1400f8ed0).

---

## 6. Other commands

The packet bytes not listed are 00. "OK" is the transaction acceptance rule from §1.
Response offsets index the 17-byte input report.

| cmd | Name (function) | Request args | Response | Notes |
|---|---|---|---|---|
| 01 | device info (FUN_180015c90 ← `UsbMouse_GetDeviceInfo` @0x180010d00) | `[5]=08`, `[6..9]` = 4 random bytes (`srand(time)`) | `[10]`=mid, `[11]`=cid, `[12]`=? | HECATE compares `[10]`/`[11]` with the device JSON `mid`/`cid` (wireless: 5/45) when `need_check_connected` is set (FUN_1400f8ed0). Meaning of `[12]` and of the random bytes: **UNCONFIRMED** |
| 02 | SetActivedStatus (`UsbMouse_SetActivedStatus` @0x180011d40) | `[5]=01`, `[6]`=0/1 | OK and `[6]!=0` | HECATE sends 1 when the device opens (FUN_1400f8ed0) and 0 in the `Connecter_G3MPro_Upgrades` destructor (FUN_14009f580 → FUN_1400fc550). Effect **UNCONFIRMED** ("driver active") |
| 03 | online (`UsbMouse_GetOnLine` @0x180011630, FUN_180015de0) | — | `[6]!=0` = mouse online/connected | gates the flash read. Also polled by the battery logic (FUN_180002060) |
| 04 | battery (FUN_180015e90 ← `UsbMouse_GetBatteryLevel` @0x180011f00) | — | `[6]`=level %, `[7]`=charging (≠0), `[8..9]`=voltage mV, **big-endian** | level/charging: battery logic FUN_180002060/FUN_180002200 (compares with 0, 0x10, 99, 100), UI FUN_14011cba0 (clamps to 100, 0→5, charging icon if byte1≠0). mV: voltage→% table 3050…4110 in FUN_1800012f0 |
| 05 | **StartPair** (FUN_180015f60 ← `UsbMouseDongle_StartPair` @0x180010660) | `[5]=02` | — | puts the receiver into pairing mode. **Do not send** unless pairing is intended |
| 06 | GetPairingStatus (FUN_180016000 ← `UsbMouseDongle_GetPairingStatus` @0x1800109b0) | — | `[6]`=status, `[7]`=? | HECATE (FUN_1400f87c0) polls every 200 ms, at most 100 times: 3 = paired, 0 or 2 = failed, other = in progress |
| 07 | flash write | §4 | echo `08 07` | |
| 08 | flash read | §3 | `[6..15]` data | |
| 09 | ClearSetting (`UsbMouse_ClearSetting` @0x180012230) | — | OK | **factory reset**. 1 try, timeout = caller's, HECATE passes 2000 ms (FUN_1400f9af0). UI text "The device has returned to its default settings." |
| 0E | GetCurrentConfig (`UsbMouse_GetCurrentConfig` @0x1800123d0) | — | `[6]`=value | not imported by HECATE. Meaning (onboard profile index?) **UNCONFIRMED** |
| 0F | SetCurrentConfig (`UsbMouse_SetCurrentConfig` @0x180012580) | `[5]=01`, `[6]`=value | OK | not imported by HECATE. **Do not use** |
| 11 | EnterMtkMode (@0x1800134d0) | `[5]=0A`, `[6]`,`[7]` | OK | factory/test mode. **Do not use** |
| 12 | firmware version (FUN_1800160c0 ← `UsbMouse_GetMouseVersion` @0x180011300) | — | version = `[6]<<8 \| [7]` | how to display it is **UNCONFIRMED** |
| 14 / 15 | Set / Get dongle LED colour (@0x18000f760 / @0x18000f960) | set: `[5]=0A`, `[6..15]` | get: `[6..15]` | receiver LED. Not imported by HECATE |
| 16 | SetLongRangeMode (@0x180012730) | `[5]=0A`, `[6]`=0/1 | OK and `[6]`==requested | HECATE checkbox (FUN_140242e30 → FUN_1400f9730). Radio meaning **UNCONFIRMED** |
| 17 | GetLongRangeMode (@0x1800128f0) | — | `[6]`=mode | |
| 1A | SetDongleIdToMouse (@0x18000ff20) | `[5]=0A`, `[6..15]`=ID | `[6..15]` echo | pairing ID. **Do not use** |
| 1B | Set/GetSensorRaiseUpCount (@0x180013170 / @0x180013320) | `[6]`=bool | — | the "Get" variant sends the same as Set(1) (vendor bug). Not imported. **Do not use** |
| 1F / 20 | Set / Get serial number (@0x180012dd0 / @0x180012fa0) | set: `[5]=0A`, `[6..15]` | get: `[6..15]` | **Do not set** |
| 2B | dongle RSSI (@0x1800100f0) | — | `[6]`=rssi | |
| 2C / 2D | Set / Get dongle indicator LED (@0x1800102a0 / @0x180010480) | set: `[5]=0A`, `[6]`,`[7]`,`[8]`; get: `[5]=0A` | `[6..8]` | |
| 2E | StartAdjustMode (@0x1800136b0) | `[5]`=len, `[6..]`=data | — | calibration. **Do not use** |
| B3 | GetSlaveVersion (FUN_180016170 ← `UsbMouse_GetSlaveVersion` @0x180012aa0) | — | `[6]<<8 \| [7]` | not imported by HECATE. Which chip "slave" means (receiver?) is **UNCONFIRMED** |

Ready-made request packets:

```
online    08 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 4A
active=1  08 02 00 00 00 01 01 00 00 00 00 00 00 00 00 00 49
active=0  08 02 00 00 00 01 00 00 00 00 00 00 00 00 00 00 4A
battery   08 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 49
version   08 12 00 00 00 00 00 00 00 00 00 00 00 00 00 00 3B
slave ver 08 B3 00 00 00 00 00 00 00 00 00 00 00 00 00 00 9A
getlrange 08 17 00 00 00 00 00 00 00 00 00 00 00 00 00 00 36
pairstat  08 06 00 00 00 00 00 00 00 00 00 00 00 00 00 00 47
```

Vendor connect sequence (FUN_1400f8ed0):
1. `03` online.
2. (if `need_check_connected`, true for D3FB) `01` device info, then check mid/cid.
3. Start the full flash read thread (§3.2).
4. `02` active=1.
5. Start the listener (§8).

Steps 3 and 4 run concurrently, so `02` is not a precondition for reading. Whether it is needed before writing
is **UNCONFIRMED**. It was always sent earlier in the vendor's session.

---

## 7. Connection-dependent behaviour

The same commands and flash layout are used for D3FB (receiver) and D3FC (wired). The only differences in
HECATE are the `01` mid/cid check for the wireless entry (device JSON `need_check_connected`) and that the
online check (`03`) reports whether the mouse is reachable through the receiver.

---

## 8. Unsolicited input reports (listener FUN_18000ef70; HECATE handler FUN_1400f8310)

The listener reads the interface every 100 ms and passes every valid report with cmd `0A` or `B8` to the app.

- `08 0A …` — change notification. `[6]` is a bitmask:
  - 0x01: re-read id 1 (DPI stage)
  - 0x02: re-read id 2 (report rate)
  - 0x04: re-read id 0 (everything)
  - 0x08: re-read id 3 (DPI LED)
  - 0x20: re-read id 4 (LED bar)
  - 0x40: battery-related. The battery optimizer clears this bit and recomputes the level. Exact meaning
    **UNCONFIRMED**.
- `08 B8 …` — forwarded to the app. Meaning **UNCONFIRMED**.

---

## 9. Flash layout — config block 0x000–0x0FF

Field names are the JSON keys from `GetMouseDateToFile` (FUN_14011fe30), which maps JSON keys to struct
offsets. The struct-to-flash mapping comes from FUN_180005840/FUN_1800063c0 (read) and FUN_180005ca0/FUN_180006630
(write). "Default" is the value the parser uses when the check fails.

| Flash | Bytes | Struct | Name | Default | Encoding / notes |
|---|---|---|---|---|---|
| 0x00 | v,c | [0] | reportRate | 01 | §2 |
| 0x02 | v,c | [1] | maxDPI | 06 | **number of DPI stages** (UI 1..6, FUN_140236ef0) |
| 0x04 | v,c | [2] | currentDPI | 02 | active stage, 0-based. Accepted only if < maxDPI (FUN_180005840). UI FUN_1402365e0 |
| 0x06 | v,c | [3] | xSpindown | 00 | **UNCONFIRMED** |
| 0x08 | v,c | [4] | ySpindown | 00 | **UNCONFIRMED** |
| 0x0A | v,c | [5] | silenceHeight | 00 | LOD ("Silent altitude"): UI writes 1 or 2 (labels "1"/"2") (FUN_140237fd0). Unit (mm?) and meaning of 0 **UNCONFIRMED** |
| 0x0C+4i | x,y,ex,c | [0x30+6i..0x32+6i] | DPIConfig[i].xDPI/yDPI/DPIex | FF FF 00 | i = 0..7. §9.1 |
| 0x2C+4i | R,G,B,c | [0x33+6i..0x35+6i] | DPIConfig[i].Red/Green/Blue | FF FF FF | stage colour |
| 0x4C | v,c | [0x60] | DPILed.mode | 02 | |
| 0x4E | v,c | [0x61] | DPILed.brightness | 80 | |
| 0x50 | v,c | [0x62] | DPILed.breathSpeed | 03 | |
| 0x52 | v,c | [0x63] | DPILed.enable | 01 | |
| 0x60+4i | t,p1,p2,c | [0x6B+3i..] | Keys[i].type/param1/param2 | 0 | i = 0..15. Vendor defaults: type 1 = mouse button (p = 1,2,4,8,16), type 2 p = 1..3, type 7, 0 = none. Type semantics **UNCONFIRMED** |
| 0xA0 | 6+c | [0x64..0x69] | LedBar.mode, R, G, B, speed, brightness | 01 FF FF FF 08 09 | one check over 6 bytes (at 0xA6) |
| 0xA7 | v,c | [0x6A] | LedBar.enable | 01 | |
| 0xA9 | v,c | [6] | keyDebounceTime | 08 | unit (ms?) **UNCONFIRMED**. UI handler not located |
| 0xAB | v,c | [7] | motionSyncEnable | 01 | bool. UI "Motion Sync" (FUN_140238160) |
| 0xAD | v,c | [8] | allLedOffTime | 06 | UI choices 6,30,60,90,120,150,180,210 (FUN_140242ba0). Unit **UNCONFIRMED** (10 s?) |
| 0xAF | v,c | [9] | linearCorrectionEnable | 00 | bool. UI "Linear Correction" (FUN_140238110) |
| 0xB1 | v,c | [0xA] | rippleControlEnable | 00 | bool. UI "Wave control" (FUN_1402380c0) |
| 0xB3 | v,c | [0xB] | moveOffLedEnable | 00 | bool |
| 0xB5 | v,c | [0xC] | sensorCustomSleepTimeEnable | 00 | bool. UI checkbox "Peak Performance", which also enables the sleep-time combo (FUN_1402381b0) |
| 0xB7 | v,c | [0xD] | sensorSleepTime | 06 | UI choices 3,6,12,30,60,90 (FUN_140231ec0. Index 4 falls through to 90, a vendor bug). Unit **UNCONFIRMED** |
| 0xB9 | v,c | [0xE] | sensorPowerSavingModeEnable | 00 | UI "mode selection": 0 = "LP", 1 = "HP" (FUN_140237ee0) |
| 0xBD | v,c | [0xF] | sensorAngleTune | — | |
| 0xBF | v,c | [0x10] | enableSensorAngleTune | — | |
| 0xC1 | v,c | [0x11] | enableWheelToChangeDPI | — | |
| 0xC3 | 4+c | [0x14] | sensitivityPositiveX (float32 LE) | — | **UNCONFIRMED** (the JSON defaults look like raw bytes read as float) |
| 0xC8 | 4+c | [0x18] | sensitivity… (JSON key duplicated; probably PositiveY) | — | **UNCONFIRMED** |
| 0xCD | 4+c | [0x1C] | sensitivityNegativeX? | — | **UNCONFIRMED** |
| 0xD2 | 4+c | [0x20] | sensitivityNegativeY? | — | **UNCONFIRMED** |
| 0xD7 | v,c | [0x24] | ? | — | **UNCONFIRMED** |
| 0xD9 | v,c | [0x25] | ? | — | **UNCONFIRMED** |
| 0xDB | 3+c | [0x26..0x28] | ? | — | buggy in the vendor code (§4.6). Do not write |
| 0xDF | v,c | [0x29] | ? | — | **UNCONFIRMED** |
| 0xE1 | v,c | [0x2A] | sensor_fps20KEnable | — | UI checkbox "Fury Mode" (checked → 1) (FUN_140236780) |
| 0xE3 | v,c | [0x2B] | ? | — | **UNCONFIRMED** |
| 0xE5 | v,c | [0x2C] | ? | — | **UNCONFIRMED** |

`v,c` = value byte, then check `0x55-v`. `n+c` = n data bytes, then one check over them.

### 9.1 DPI value encoding (sensor 3395, max 32000)

From the UI encoder: DPI → slider position (FUN_140236030), slider → register (FUN_1402367d0), branch
"max 32000 and sensor ≠ 0xF6E".

- **DPI ≤ 26000**: `raw = DPI/50 - 1`, with the UI step 50 up to 6000 and 100 above (above 6000 the stored raw is
  always odd). `DPI = (raw+1)*50`, `raw ≤ 0x207`.
  - `xDPI = yDPI = raw & 0xFF`
  - `DPIex = ((raw>>8)<<2) | ((raw>>8)<<6)`
- **DPI > 26000**: `v = DPI/100` (261..320).
  - `xDPI = yDPI = v & 0xFF`
  - `DPIex = ((v>>8)<<2) | ((v>>8)<<6) | 0x22`
- Check: the vendor default stages are `xDPI` 7/15/31/63/127/239 with `DPIex` 0, which gives
  400/800/1600/3200/6400/12000 ✓.
- Inferred decode (**UNCONFIRMED**, no decoder found in the code):
  - X: `hi = (ex>>2)&3`, `val = x | hi<<8`, then `DPI = val*100` if `ex&0x02`, else `(val+1)*50`.
  - Y: same, using `ex>>6` for the high bits and the `0x20` flag.

### 9.2 Shortcut slot (0x100 + 0x20*i, length 3n+2) — FUN_180006d60 / FUN_180006e60

- `[0]` = n, the number of entries (2..6).
- Then n × 3 bytes `[s, v0, v1]`. In `s`: bits 7–6 = keyState (`10`→0, `01`→1, else 2), low nibble = type.
- `[3n+1]` = `chk(slot, 0, 3n+1)`.

### 9.3 Macro slot (0x300 + 0x180*j, length 5n+0x21) — FUN_180002ba0 / FUN_180002ac0

- `[0]` = name length (1..0x1E). `[1..0x1E]` = name bytes.
- `[0x1F]` = n, the number of events (2..0x46).
- n × 5 bytes from 0x20: `[s, v0, v1, delay_hi, delay_lo]`. `s` is encoded as in §9.2. `delay` is a big-endian
  u16 (unit ms? **UNCONFIRMED**).
- `[0x20+5n]` = `chk(slot, 0x1F, 0x20+5n)`, i.e. a check over the count byte and the events.

---

## 10. UNCONFIRMED summary

1. Whether the device echoes `[2..5]` (address/length) in replies to `08`/`07`.
2. Whether a rate change takes effect immediately, and identically over 2.4G and the cable. The firmware's
   reaction to `10`/`20`/`40` (2k/4k/8k), which the vendor UI hides for this model.
3. Whether `02` SetActivedStatus(1) is required before writes, and what it does.
4. `01` response byte `[12]` and the purpose of the 4 random request bytes.
5. Semantics of `0E`/`0F` (CurrentConfig), `16`/`17` (long range), `B3` (slave version), and the `08 B8`
   report.
6. Units of keyDebounceTime, allLedOffTime, sensorSleepTime, silenceHeight and macro delay.
7. Meaning of flash 0x06, 0x08, 0xBD–0xC1 details, 0xC3–0xD6 floats, 0xD7, 0xD9, 0xDB, 0xDF, 0xE3, 0xE5.
8. Key `type` semantics. The DPI decoder (only the encoder was found).
9. Physical flash size (the host image is 0x2000 bytes; 0x0000–0x1AFF is used).
