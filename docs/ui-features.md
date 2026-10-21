# HECATE app — UI feature matrix for `g3mpro_upgrades` (HECATE G3M PRO 升级版)

Static analysis of HECATE 1.1.44 (`HECATE.exe`, Qt 6.8.3). Nothing was executed and no device was touched.
This document builds on [`protocol.md`](protocol.md); flash fields are referenced as "§9 @0xNN" (= protocol.md §9 table,
flash address 0xNN). Function citations are `FUN_14xxxxxxx` in HECATE.exe (decompiles in `~/hecate-re/`:
`h_ui_range.c` (0x140228000–0x140250000), `r_vies.c`, `r_keyled_macro.c`, `r_dpiwidgets.c`, `r_devcfg.c`,
`r_insertkey.c`, `h_conn.c`, `h_tables.c`, `h_keytab.c`).

Label sources: English = Qt source strings (the app has no English `.qm`); 中文 / ja / ko translations come from three
`.qm` files embedded in HECATE.exe (file offsets 25271393 = `zh`, 25017536 = `ja_JP`, 25087497 = Korean although tagged
`aa`); dump: `~/hecate-re/qm/{zh,aa}.tsv`, tool `~/hecate-re/scripts/qmdump.py`. The device manual is
`data/deviceresource/g3mpro_upgrades/manual.pdf` (text in `~/hecate-re/manual/g3mpro_upgrades.txt`).

"Write" column: **yes** = the handler calls `FUN_140118e60` → `JMMouseDeviceMessage::Write` → `SaveFlashData`
(diff write, protocol.md §4) immediately; **no** = it only edits the in-memory struct and the change goes out with the
next Write triggered by some other control; **cmd** = a direct command (not a flash write).

---

## 0. Device model facts the UI uses

| Item | Value | Source |
|---|---|---|
| Device JSON | `id=g3mpro_upgrades`, `dpi=32000`, `sensor=3395`, `support_battery=true`, no `support_20kFPS`, no `light` | `data/devices/002_g3mpro_upgrades.json`; parser FUN_140327880 (config `+0x48`=dpi, `+0x4c`=sensor, `+0x2aa`=support_battery, `+0x2ab`=support_20kFPS, `+0x2af`=support_tuningMode) |
| Mouse buttons (manual) | 1 LED, 2 left, 3 wheel button, 4 right, 5 forward, 6 back, 7/8 left/right micro-switch (silent/game slider), 9 mode switch (off/2.4G/BT), 10 DPI button | manual.pdf §1 |
| Manual spec | rate 125–1000 Hz, DPI 50–26000 (JSON/UI say 32000 — discrepancy), 550 mAh, 2.4G / BT / wired; driver works only in 2.4G or wired | manual.pdf §2, DeviceItemView string |
| Factory DPI | 400 red, 800 green, 1600 ice-blue (default stage), 3200 orange, 6400 blue, 12000 purple | manual.pdf §4 = `defaultconfiguration.json` |
| Pairing on the mouse | hold Left+Wheel+Right ~3 s; 2.4G pairing = yellow fast blink, BT = blue fast blink | manual.pdf §4 |

---

## 1. Window layout and pages (in order)

`JMMousePropertiesVies` (ctor FUN_140119770) — header + 4 tabs:

**Header:** device picture, `Device Offline!` overlay (when not connected), `User Manual` (使用说明) button
(target **UNCONFIRMED**), battery icon + `NN%` label (§6.4), and the overlay `Please wait for the data to load
completely...` (请等待数据加载...) shown until the initial full flash read finishes.

| # | Tab icon | Page class (ctor) | Tab title (EN / 中文) |
|---|---|---|---|
| 1 | `tab_mouse_key` | PropertyKeySet (FUN_1401e6b00) | `KeySet` / 按键设置 |
| 2 | `tab_mouse_dpi` | PropertyMouseDPISet (FUN_14022e450) | `Performance Settings` / 性能设置 |
| 3 | `tab_mouse_macro` | PropertyMacroMgr (FUN_140205d20) | `Macro Management` / 宏管理 |
| 4 | `tab_other` | PropertyMouseOtherSet (FUN_1402408f0) | `Built-in Settings` / 鼠标设置 |

**There is no lighting page for this model** (PropertyLedCommon is never instantiated by JMMousePropertiesVies; the
device JSON has no `light` section). Lighting is limited to per-DPI-stage colours (§5).

Every page registers for the JMHidMessage "data loaded" signal (`FUN_1403c7900`) and refreshes from the struct
(DPISet: FUN_140236460; OtherSet: FUN_140243840; KeySet: FUN_1401f4db0).

---

## 2. Feature matrix

### 2.1 Page "Performance Settings" — left column (FUN_14022e760, vtable slot 11)

| # | Control (EN / 中文) | Type | Values → stored (exact labels) | Default | Field (protocol.md §9) | Write | Gate / notes |
|---|---|---|---|---|---|---|---|
| 1 | `Report Rate Set` / 回报率设置, help: `The frequency at which the mouse reports information to the computer` / 选择鼠标向计算机报告信息的频率 | radio buttons, label `Hz` after each | `125`→08, `250`→04, `500`→02, `1000`→01, `2000`→10, `4000`→20, `8000`→40 (FUN_1402374d0 / load FUN_1402372e0) | 1000 | reportRate @0x00 | yes | buttons 2000/4000/8000 are `setVisible(false)` when id ∈ {`g3mpro_upgrades`,`g2mpro`,`g3multra`} → **only 125/250/500/1000 shown** |
| 2 | `mode selection` / 模式选择 | 2 radio | `LP`→0, `HP`→1 (FUN_140237ee0; load FUN_1402375e0 clamps to 0..1) | LP | sensorPowerSavingModeEnable @0xB9 | yes | — |
| 3 | `Silent altitude` / 静默高度 (LOD) | 2 radio | `1MM`→1, `2MM`→2 (FUN_140237fd0; load FUN_140237730: value ≥2 → 2MM else 1MM) | 1MM (flash default 0 is shown as 1MM) | silenceHeight @0x0A | yes | hidden for `g2mpro` only. Vendor bug: the status label (§2.2) shows "1"/"2" swapped after a load |
| 4 | `Wave control` / 波纹控制, help `Correct the wavy jitter` | checkbox | on→1 / off→0 (FUN_1402380c0) | off | rippleControlEnable @0xB1 | yes | — |
| 5 | `Linear Correction` / 直线修正, help `Correct the straight line trajectory` | checkbox | 1/0 (FUN_140238110) | off | linearCorrectionEnable @0xAF | yes | — |
| 6 | `Motion Sync` / 移动同步, help `Smooth correction algorithm` | checkbox | 1/0 (FUN_140238160) | on | motionSyncEnable @0xAB | yes | — |
| 7 | `Peak Performance` / 火力全开, help `The sensor LED is highlighAnglesnapping in maximum performans` (zh: 传感器和主控都进入最活跃模式，响应更快，但会增加功耗，减少使用时间) | checkbox | 1/0 (FUN_1402381b0) | off | sensorCustomSleepTimeEnable @0xB5 | yes | enables/disables combo #8 |
| 8 | (sensor sleep time, no own label) | combo | `30s`→3, `1Min`→6, `2Min`→12, `5Min`→30, `10Min`→60, `15Min`→90 (items built from seconds 30,60,120,300,600,900 in FUN_14022e760; handler FUN_140231ec0; load FUN_140237a50) → **unit = 10 s** | 1Min (6) | sensorSleepTime @0xB7 | **no** | disabled unless #7 checked. Vendor bug: index 4 falls through and stores 90 |
| 9 | `Fury Mode` / 狂暴模式, help about 20,000 fps | checkbox | Checked→1, Unchecked→0 (FUN_140236780) | — | sensor_fps20KEnable @0xE1 | yes | **only if `support_20kFPS`** (config `+0x2ab`, FUN_14022e760) → **hidden for g3mpro_upgrades** |

### 2.2 Page "Performance Settings" — right area: DPI (FUN_140231f70, vtable slot 12)

| # | Control | Type | Values | Default | Field | Write | Notes |
|---|---|---|---|---|---|---|---|
| 1 | `DPI Quantity` / DPI档位 | combo | `1`…`6` → stage count 1..6 (FUN_140234c70 → FUN_140236ef0; load clamps 0→1, >6→6) | 6 | maxDPI @0x02 | **no** | max **6** stages in UI (flash has 8 slots). On shrink, stages are shifted and current is clamped |
| 2 | 6 stage cards (DPIButton FUN_14030ddf0): radio "current", editable DPI value, colour button | per stage | click card → current stage (FUN_1402365e0); edit value (editingFinished → FUN_140236030, clamped 50..32000, encoded §7); colour → colour dialog (FUN_140234d50) | stage 2 (1600) | currentDPI @0x04; DPIConfig @0x0C+4i; colour @0x2C+4i | yes (all three) | only `maxDPI` cards visible |
| 3 | `Adjustable parameters` / 可调整参数 slider, end labels `Slow` / 慢 and `Quick` / 快 | DPISlider (FUN_1402f5250) | range 0..499 for this model (§7.2) | — | DPIConfig of current stage | drag: no (FUN_140235020 → FUN_1402367d0); release: yes (custom signal FUN_1403cc8b0 → FUN_140235060; signal meaning inferred) | — |
| 4 | `Restore Default` / 恢复默认 button | button | — | — | — | — | created then `setVisible(false)` → **not shown** |
| 5 | Status line: `Rport Rate` `1000` `Hz` / `LP` `mode` / `Silent Altitude` `1` `MM` | labels | mirrors #2.1-1..3 | — | — | — | display only |
| 6 | Help text `The DPI sensitivity of the mouse can be adjusted in the driver settings...` | label | — | — | — | — | — |

### 2.3 Page "Built-in Settings" (FUN_140240a20; load FUN_140243840)

| # | Section / control (EN / 中文) | Type | Values → stored | Default | Field / command | Write | Notes |
|---|---|---|---|---|---|---|---|
| 1 | `Sleep Mode` / 休眠模式 → `Mouse sleep time after stopping` / 放停后鼠标休眠时间 | combo | `1Min`→6, `5Min`→30, `10Min`→60, `15Min`→90, `20Min`→120, `25Min`→150, `30Min`→180, `35Min`→210 (seconds table 60…2100 at DAT_1419e2758; handler FUN_140242ba0) → **unit = 10 s** | 1Min (6) | allLedOffTime @0xAD (JSON name says "LED off", UI says mouse sleep) | yes | load bug: 210 never re-selected (checks wrong offset) |
| 2 | `Other` / 其它 → `Windows Mouse Property` + button `Mouse properties` / 鼠标属性 | button | runs `control main.cpl,0` (FUN_140242c90) | — | — | — | Windows-only; skip or map to desktop settings |
| 3 | `Advanced Setup` / 高级设置 → `Long-distance mode` / 远距离模式 (help: longer range, more current) | checkbox | checked → `SetLongRangeMode(1)`, unchecked → `(0)` (FUN_140242e30 → FUN_1400f9730) | from device (`GetLongRangeMode` on every data load) | cmd 0x16 / 0x17 (protocol.md §6) | cmd | — |
| 4 | `Set Default` / 恢复默认 → button `Reset Device` / 恢复出厂设置 | button | factory reset (§6.1) | — | cmd 0x09 | cmd | **no confirmation dialog** |
| 5 | `Device Info` / 设备信息 → `Receiver Firmware Version` / 接收器固件版本, `Mouse Firmware Version` / 鼠标固件版本 | labels | §6.5 | — | 0x12 + USB bcdDevice | — | receiver row shown only when versions differ and a config check passes (FUN_1400bd1f0, likely "wireless"; **UNCONFIRMED**) |
| 6 | `Operation and use` / 操作使用 → `Instructions` / 说明书 + button `Product Specification` / 产品规格 | button | opens `https://www.edifier.com/product/support-789.html` (FUN_140243010) | — | — | — | — |

### 2.4 Page "KeySet" and "Macro Management" — see §3 and §4.

### 2.5 Flash fields with **no UI** for this model

keyDebounceTime @0xA9 (default 8), moveOffLedEnable @0xB3, DPILed.* @0x4C–0x52, LedBar.* @0xA0–0xA8,
sensorAngleTune/enableSensorAngleTune @0xBD/0xBF, enableWheelToChangeDPI @0xC1, xSpindown/ySpindown @0x06/0x08,
sensitivity floats @0xC3–0xD6, Keys[5..15], shortcut/macro slots 5..15. A search for UI writes to these struct
offsets in all decompiled UI ranges found none. They are only touched by factory reset and by the JSON profile
import/export (`GetMouseDateToFile` FUN_14011fe30 / export FUN_14011d8a0; the app has a PC-side profile UI —
contexts `CfgView`: Apply/Delete/Duplicate/Edit — whose mouse usage is **UNCONFIRMED**).

---

## 3. Key remapping (PropertyKeySet)

### 3.1 Buttons offered

Only **5** buttons are clickable on the mouse picture (FUN_1401ed1d0, image buttons `left`, `right`, `wheel`, `back`,
`forword`, button-group ids 0..4; the click lambdas set the selected index `+0x3a0` = 0..4):

| UI button | Keys[i] | Vendor default (type,p1,p2) |
|---|---|---|
| Left (left_*.png) | 0 | (1,1,1) Left Key |
| Right | 1 | (1,2,2) Right Key |
| Wheel | 2 | (1,4,4) Wheel Key |
| Back | 3 | (1,8,8) Back Key |
| Forward | 4 | (1,16,16) Forward Key |

Not remappable in the vendor UI: Keys[5] = (2,1,1) DPI Loop (very likely the DPI button), Keys[8] = (7,0,0) report
key, Keys[9] = (2,2,2) DPI+, Keys[10] = (2,3,3) DPI−; the physical meaning of indexes 5–15 is **UNCONFIRMED**.

Hint text: `Please click the mouse button on the screen to start the settings.`
Rule (FUN_1401f6510, FUN_1401ee830): if the clicked key is the **only** key with (type 1, p1 1) among all 16, editing is
refused with `Among all the buttons, at least one of them has the main function!` (所有按键中，至少要包含一个主键！).

### 3.2 Function categories (combo, FUN_1401e77a0) and encoding

Assignment dispatcher FUN_1401f0e30(category) writes `Keys[i] = (type, p1, p2)` and **always calls Write** (yes).
Label shown under the button = the sub-item label.

| Combo item (EN / 中文) | Sub-items (EN / 中文) | Keys[i] (type, p1, p2) | Handler |
|---|---|---|---|
| `Mousekey` / 鼠标按键 | `Left Key` 主键, `Right Key` 次键, `Wheel Key` 中键, `Back Key` 后退键, `Forward Key` 前进键 | (1, 1/2/4/8/16, 0) — table at 0x140bd2408 | FUN_1401f6ed0 |
| `Report Key` / 切换报告率 | (selecting the item assigns immediately) label `report key` | (7, 0, 0) — on-mouse report-rate cycling | FUN_1401ebdd0 → FUN_1401f8390 |
| `Fire Key` / 火力键 | `Number of key presses` 按键次数 slider `0`–`3`; `Interval time` 间隔时间 slider + edit `10`–`255` (regex validator); hint `When the count is set to 0, pressing the button will continuously send messages...` | (4, interval 10..255, count 0..3) — interval unit **UNCONFIRMED** (no unit label; likely ms) | FUN_1401f7100 |
| `Combination Key` / 组合键 | modifier radio `Close` 关闭 / `Shift` / `Ctrl` / `Alt` / `Win` + key input box (`Please enter the key in the input box.`) | `Close` → (0,0,0) = **disabled**; otherwise (5,0,0) + shortcut slot **i** (§3.3) | FUN_1401ec000 (asm: r8=0 for id 0, r8=5 otherwise) → FUN_1401f8540 / FUN_1401f7360 |
| `Scroll UP Or Down` / 上滚/下滚 | `Scroll UP` 上滚, `Scroll Down` 下滚 | (0x0B, 1/2, 0) | FUN_1401f8620 |
| `Scroll Left Or Right` / 左滚/右滚 | `Scroll Left` 左滚, `Scroll Right` 右滚 | (3, 1/2, 0) | FUN_1401f6fb0 |
| `DPI Key` / DPI按键 | `DPI Loop` DPI循环, `DPI+`, `DPI-` | (2, 1/2/3, 0) | FUN_1401f6ce0 |
| `Multimedia Function` / 快捷键 | 18 items (below) | (5,0,0) + shortcut slot **i** with 2 consumer events (§3.3) | FUN_1401f7360 (combo index 7 branch) |
| `Macro` / 宏 | radio list of macros from the Macro page; `The macro list is empty...` / `Please select a macro.` | (6, i, loopMode) + macro slot **i** (§3.4) | FUN_1401f5a60 → FUN_1401f81c0 |

Types 9 and 10 exist in the dispatcher (FUN_1401f8470 → (9,0,0); FUN_1401f84d0 → (10,p,0)) but no UI path reaches them
for this page (**UNCONFIRMED** meaning).

Multimedia items (UI order → HID consumer usage, table FUN_14000bc10, entries `{2, lo, hi}`):
`Player` 多媒体 0x0183 · `Play/Pause` 开始/暂停 0x00CD · `Next Song` 下一曲 0x00B5 · `Previous Song` 上一曲 0x00B6 ·
`Stop Playing` 停止播放 0x00B7 · `Mute` 静音 0x00E2 · `Volume Up` 音量加 0x00E9 · `Volume Down` 音量减 0x00EA ·
`Mail` 邮件 0x018A · `Calculator` 计算器 0x0192 · `My Computer` 我的电脑 0x0194 · `Home` 主页 0x0223 ·
`Search` 搜索 0x0221 · `Forward Webpage` 网页前进 0x0225 · `Back Webpage` 网页后退 0x0224 · `Stop Webpage` 网页停止 0x0226 ·
`Refresh` 刷新 0x0227 · `Favorites` 收藏夹 0x022A.

### 3.3 Shortcut slot use (type 5)

Keys[i] of type 5 always uses **shortcut slot i** (flash `0x100 + 0x20*i`, struct `0x9C + 0x34*i`; FUN_1401f7360 indexes
the slot with the selected key index). Event entries: struct `+4+8k` = keyState (0 down, 1 up), `+5` = type, `+6/+7` =
code lo/hi; serialized per protocol.md §9.2 as `[state-bits|type, lo, hi]` (state 0 → 0x80, 1 → 0x40).
Event types seen: **0 = modifier bitmask** (LCtrl 0x01, LShift 0x02, LAlt 0x04, LWin 0x08; table FUN_14000bab0),
**1 = keyboard HID usage** (page 7; ~132-key table FUN_14000c240, e.g. Esc 0x29, F1 0x3A),
**2 = consumer usage**, **4 = mouse button mask** (macros only).

- Multimedia: n = 2 → `[02, 0x82, lo, hi, 0x42, lo, hi, chk]`; e.g. Volume Up = `02 82 E9 00 42 E9 00 BD`.
- Combination: [modifier down] + captured key events (downs in order, then ups in reverse) + [modifier up]; n ≤ 6.
  Exact capture order and the "modifier with empty key box" path (which leaves n = 1, an invalid slot) are
  **UNCONFIRMED**.

### 3.4 Macro slot use (type 6)

Keys[i] = (6, **i**, loopMode) and the selected macro (0x250-byte struct) is copied into **macro slot i** (flash
`0x300 + 0x180*i`, struct `0x3DC + 0x250*i`) by FUN_1401f5a60 / FUN_140215870, then Write. So each of the 5 buttons has its
own macro slot; the macro library itself lives only on the PC. `loopMode` (p2) values (MacroMgr FUN_140209a90…e80,
load mapping at r_keyled_macro.c:21003):

| p2 | UI radio (`Type of cycle` / 循环选择) |
|---|---|
| 0xFE | `Loop until the button is released` / 循环直到按键松开 |
| 0xFF | `Loop until any key is pressed` / 循环直到任意按键按下 |
| 0xFD | `Loop until this button is pressed` / 循环直至此按键按下 |
| 1..250 (0xFA) | `Cycle-index` / 播放次数 + count edit, label `count` / 次 (0 is forced to 1) |

---

## 4. Macro editor (PropertyMacroMgr, FUN_140205d20 / FUN_140206630 / FUN_14020a5f0)

- Macro list buttons: `new` 新增, `import` 导入, `export` 导出, `rename` 重命名, `delete` 删除; event table columns `Key` /
  `Action` / `Delay`; event buttons `Up to top` 上移到顶, `move up` 上移, `move down` 下移, `down to bottom` 下移到底部,
  `delete`, `remove all` 清空.
- Name: `Macro Name` 宏名; allowed `^[a-zA-Z0-9一-龥\s_-]+$` (FUN_140214ae0) with error `Input contains special
  characters!...`; input box limited by `LengthValidator(20)` (FUN_140302bb0; length measure **UNCONFIRMED**); flash name
  field max 30 bytes (protocol.md §9.3); byte encoding of the name: the Windows ANSI code page of the PC that wrote it (live: Big5/CP950 names "切回去:D", "切螢幕:D"; GBK fails), so keep names as raw bytes and decode by trying UTF-8, then Big5.
- `Delay option` 延时选择: `Automatic insertion delay` 自动插入延时 (record real gaps) or `Default delay` 默认延时 with edit
  default `50` and unit label `ms` → **macro delay unit = ms** (stored u16 BE, so ≤ 65535; UI range limit **UNCONFIRMED**).
- `Insert event` 插入事件 combo: `Left Key`, `Right Key`, `Wheel Key`, `Back Key`, `Forward Key` (type 4, mask 1/2/4/8/16,
  inserted as down + up), `Key Dowm` 按键按下, `Key Up` 按键弹起 (keyboard key via `InsertKeyDialog` "Please press a key").
  Recorded keyboard events use the key table (type 1, HID usage).
- Event fields: keyState 0 = down (`Down` 按下) / 1 = up (`Up` 抬起), type, 16-bit code, delay.
- **Max 70 events** (append refused when count ≥ 0x46, FUN_14020f2e0).
- Loop modes: §3.4.

---

## 5. Lighting

- No lighting page, no DPI-LED mode/brightness/breath controls, no LED-bar controls, no `moveOffLedEnable` control.
- Only lighting control: **per-DPI-stage colour** (stage card colour button → colour dialog FUN_1400864c0
  (`ColorSetQDialog`): `Color selection` 颜色选择, `Custom colors` 自定义颜色, `Cancel`; the context also has
  `HECATE Lighting selection`, usage unverified); FUN_140234d50 → R,G,B into DPIConfig[i].Red/Green/Blue
  @0x2C+4i, Write yes). Defaults: red, green, (0,255,150), (255,100,0), blue, (255,0,255).
- `allLedOffTime` @0xAD is presented as the mouse sleep timer (§2.3-1), not as an LED option.
- DPI-LED defaults from flash/JSON (mode 2, brightness 128, breathSpeed 3, enable 0) and LED bar (mode 1, speed 7,
  brightness 9, enable 0, colour 255,0,255) are never shown; their mode lists/labels are not in the mouse UI.

---

## 6. Device actions

### 6.1 Factory reset
Button `Reset Device` (OtherSet) → FUN_140242e80: `ClearSetting` (cmd 0x09, 1 try, 2000 ms) → full re-read
(ReadFlash id 0) → info box title `Hint` / text `The device has returned to its default settings.` (该设备已恢复到默认设置) /
button `OK`. **No confirmation before reset** (the "Are you sure you want to restore the factory settings?" string belongs
to the headset page `PropertyDeviceInfo`).

### 6.2 Pairing ("RepairDongle")
`JMMouseDeviceMessage::RepairDongle` (FUN_1400fc730): `StartPair` (cmd 0x05) then `MouseReadRepairDongleThread`
(FUN_1400f87c0) polls `GetPairingStatus` (0x06) every 200 ms, up to 100 times: `resp[6]==3` → success, 0 or 2 → fail.
Called for wireless connect-infos by the connector (FUN_14009fb70). The UI strings live in `UpgradeCenter`: `Dongle
Pairing` 接收器配对, `Start Pairing` 开始配对, `The pairing process is about to start. Please make sure that the mouse is in
pairing mode.`, `Start pairing...`, `Pairing success!`, `Pairing failed, please make sure both the device and the dongle
are properly connected.` Which button triggers FUN_14009fb70 is **UNCONFIRMED** (inferred: "Start Pairing").

### 6.3 Long-distance mode — §2.3-3 (cmd 0x16/0x17, applied immediately, not a flash field).

### 6.4 Battery display
- Source: `GetBatteryLevel` (0x04) into `JMMouseDeviceMessage+0x22`: byte0 = level, byte1 = charging (FUN_1400f96e0),
  polled by the device-list refresh (FUN_140323620) when connected; refresh interval **UNCONFIRMED**.
- Mouse page (FUN_14011cba0): level clamped to ≤100; 0 is shown as 5; text `NN%`; icon: charging (byte1 ≠ 0) →
  `charging`; else 100 → 100%, 80–99 → 80%, 60–79 → 60%, 20–59 → 40%, 0–19 → 20%.
- Device list (FUN_140323620): same clamps; charging icon only when byte1 == 1; <1 shown as 5.
- Firmware upgrade refuses below 20 % (`Battery level is below 20%, upgrade cannot be performed...`).

### 6.5 Firmware versions
Mouse: cmd 0x12 → `v{hi}.0{lo}` if lo < 16, else `v{hi}.{lo}` (decimal) (FUN_140243840, strings at 0x140bee670…).
Receiver: USB `HIDD_ATTRIBUTES.VersionNumber` (bcdDevice) of the opened HID device (FUN_1400fa0c0), same format.

### 6.6 Connection handling
Connector (FUN_14009f6f0 / FUN_14009f9a0): `OpenDevice` (online 0x03, then 0x01 mid/cid check for the wireless PID,
then full read, `SetActivedStatus(1)`, listener) is retried every **1 s** while offline and repeated every **5 s** while
connected; `SetActivedStatus(0)` on teardown (FUN_14009f580). Device-list states (DeviceItemView): `Device sleep` 设备休眠,
`Dongle not connect to mouse` 未连接到鼠标, `Device offline`, `Device Connecting`; hint: driver only works in 2.4G or wired
mode (蓝牙模式不可使用驱动).

### 6.7 Firmware upgrade (describe only — do not implement)
`UpgradeCenter` / `UpgradeItem`: `Firmware Upgrades`, `Current Version` / `New Version`, `Upgrade`, `Click to upgrade
the Dongle firmware`, `The device firmware version is too old`, battery ≥ 20 % required, warnings to use a motherboard
USB port, `Upgrade successful` / `Upgrade failed`. Uses `UsbUpgrade_*` in HIDUsb.dll and downloaded firmware files.

---

## 7. DPI

### 7.1 Decoder (UI load path FUN_140235080 / FUN_140235930, branch dpi = 32000 and sensor ≠ 0xF6E)
```
raw = ((DPIex & 0xC0) >> 6) << 8 | xDPI
DPI = (DPIex & 0x03) ? raw * 100 : raw * 50 + 50
```
(The vendor decoder uses the **y** high bits 6–7 together with the x low byte; the encoder writes x == y.)
Encoder = protocol.md §9.1. Round trip checked for 50, 400, 800, 6000, 6100, 12000, 26000, 26100, 32000; 6050 rounds
to 6000.

### 7.2 Slider for this model
`setRange(0, 499)` (FUN_140231f70, dpi 32000 / sensor ≠ 3950), max label `32000`, ends `Slow`/`Quick`. Position ↔ DPI:
- p 0–239: DPI = (⌊p/2⌋+1)·50 → 50…6000, step 50 (two positions per step)
- p 240–439: DPI = 100·p − 17900 → 6100…26000, step 100
- p 440–499: DPI = (p − 179)·100 → 26100…32000, step 100

Stored → position (FUN_140235930): flags clear: `p = raw<120 ? raw*2 : (raw-119)/2 + 239`; flags set: `p = raw + 179`.
Typed values are clamped to 50…32000 (FUN_140236030).

### 7.3 Stages
Max **6** stages in the UI (`DPI Quantity` 1–6; flash has 8 slots — slots 6/7 only ever hold defaults). Current stage is
0-based and must be < count.

---

## 8. UNCONFIRMED summary

1. `User Manual` header button target; meaning of the receiver-row condition FUN_1400bd1f0.
2. Physical meaning of Keys[5..15]; types 9/10; Fire Key interval unit; exact combination-key event order and the
   modifier-only path.
3. Macro name length measure (LengthValidator(20)) and byte encoding; UI limit for delay values.
4. Which UI element triggers RepairDongle (FUN_14009fb70); battery poll interval.
5. Whether the PC-side profile system (CfgView, JSON via FUN_14011fe30 / FUN_14011d8a0) is reachable for this mouse.
6. keyDebounceTime unit (no UI found).
7. The DPISlider custom signal (FUN_1403cc8b0) being "slider released".
