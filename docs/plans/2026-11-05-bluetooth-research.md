<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Bluetooth MIDI (and Wi-Fi + Ableton Link) on the FM-1: research

**Status:** proposed, 2026-11-05. A study, not a change: nothing here touches the firmware, and
nothing reaches `main` without the owner's decisions (the end of this document). No device, radio
or MIDI port was used. Third-party code was cloned into `build/ext/` (git-ignored) and built
there; none of it is vendored.

Tags: **[measured]** built or run here, numbers read from the output; **[verified]** read in
the source or docs at the commit named; **[upstream]** an upstream's own measurement, not
repeated here; **[estimated]** reasoning from the above, with the basis stated.

## The answer in short

- **Go, for BLE MIDI only (peripheral, no pairing), as an opt-in build option first, after the
  felucca-bsp move.** Bluetooth audio (A2DP): **no-go** for SIDOU (size, RAM, latency, CPU).
- The licence is no obstacle. The CPU is no obstacle: the stack's interrupts take about 0.02 % of
  the core, sit below the audio interrupt and survived a 2.6 ms audio interrupt in the
  emulator. **Size and the power set-up are the obstacles:** about 29-33 KB of flash against
  25,072 B free under the ceiling, about 1.5 KB of DMA RAM plus 5-8 KB of other RAM against 2,780 B
  usable, and 36-44 B of RAMTEXT against 16 B. The radio also needs the supply rails that
  felucca-bsp's `fm1_sys.h` raises at boot, which SIDOU's Felucca 1.6.1 HAL does not do.
- **Wi-Fi + Ableton Link: below Bluetooth.** It does not work end to end on a real FM-1 yet
  (DHCP never gets an answer), it needs the BLE radio bring-up anyway, and it adds about 28 KB.
  The Link peer itself is finished and tested on a host and is the part worth having later.

## Sources, as checked (2026-11-04)

| What | Where | Commit |
|---|---|---|
| Optimist (the stack's author's tree, Henri David) | `github.com/w0ts/optimist`, `main` | `1fed5103` |
| fm1-base (the stack as a platform option, Charles Vestal) | `github.com/charlesvestal/fm1-base` | `2d8ed74` |
| felucca-bsp (radio drivers, as fm1-base pins it) | `github.com/hugelton/felucca-bsp` | `773b0b9` (upstream `main` is one commit on, `9670b5d`, a test fix) |
| CTL-1 | `github.com/charlesvestal/fm1-ctl` | `772b90f` |
| Optimist's emulator fork, with a BLE engine model | `github.com/w0ts/fm1-emulator` | `8265d46` |
| fm1-emu (SIDOU's emulator) | `github.com/joakimeriksson/fm1-emu` | `9de1ef0` (v0.4.0) |
| SIDOU 1.6.0 | this branch's base, `e3512196`, built here | |

**Correction to the brief [verified]:** CTL-1 ships Bluetooth on **JieLi's** libraries, not the
own stack: `app/fm1.toml` sets `ble = true` and `ble_central = true` but no `ble_stack`, and
fm1-base's default is `"jieli"` (`tools/build.py` line 1192). In fm1-base only
`examples/ble_own` uses the own stack. The firmware that ships it is **Optimist** (release flavours
"synths-USB-BTA-BTM" and "drum machine": BLE MIDI and Bluetooth audio), which calls itself early
access (VERSION 0.2).

## 1. Licence

**Every file a BLE-MIDI-only build needs is GPL-3.0 compatible [verified].**

| Files | Licence | Role |
|---|---|---|
| The stack, `bt/ble/*` in fm1-base (58 files with `bt/br`), `firmware/src/ble/*` in Optimist | `GPL-3.0-only OR MPL-2.0` | link layer, host, GATT, SMP, BLE-MIDI, the WL82 driver |
| felucca-bsp `chip/fm1_ble.h`, `fm1_ble_rf.h` (and `fm1_bredr.h`, A2DP only) | `GPL-3.0-only OR MPL-2.0` | register access, the radio's start-up runner |
| felucca-bsp `chip/ble_rf_tables_v15.h` | Apache-2.0 (JieLi / M-VAVE, captured from V15) | the radio's start-up program |
| felucca-bsp `chip/fm1_rf_cal.h` | Apache-2.0 (JieLi's SDK, rewritten by Charles Vestal) | calibration at every start |
| felucca-bsp `chip/fm1_rf_regs.h` | MPL-2.0 | register names |
| A2DP only: AOSP SBC encoder (12 files), micro-ecc (11 files) | Apache-2.0; BSD-2-Clause | not needed for MIDI |
| fm1-base's glue (`device/bt_own*.c`, `radio.c`) | MPL-2.0 | written against fm1-base's `plat.h`: SIDOU writes its own glue, so none of it is taken |

- SIDOU's image is already GPL-3.0-only (Felucca's HAL and `usb.c`, LICENSING.md). The stack
  can be taken under its **GPL-3.0-only** branch, which adds no obligation beyond the GPL the
  image already has. The MPL-2.0 branch carries no Exhibit B, so MPL is GPL-compatible too
  (MPL-2.0 section 3.3) if a file ever must stay MPL (felucca-bsp's `fm1_rf_regs.h` is MPL-2.0 only:
  that is fine, as felucca-bsp's other MPL files will be after the move).
- Apache-2.0 is GPL-3.0 compatible (one-way, which is the way SIDOU needs).
- **What SIDOU must ship:** `LICENSES/MPL-2.0.txt` (REUSE requires a text for every licence an SPDX
  expression names, so the `OR MPL-2.0` headers need it even when SIDOU uses the GPL branch);
  `LICENSES/Apache-2.0.txt` (already there, for the SDK packaging files); felucca-bsp's `NOTICE`
  contents for its two Apache files (Apache-2.0 section 4(d)); a LICENSING.md table row per
  component with commit, licence, "linked into the firmware"; CREDITS.md lines for Henri David
  (the stack), Charles Vestal (`fm1_rf_cal.h`, the felucca-bsp integration), JieLi and M-VAVE (the
  captured tables). Files stay byte for byte with their own SPDX headers, so `reuse lint` needs no
  `REUSE.toml` entry for them.
- **Risks, not blockers:** the stack is not clean-room (Optimist's LICENSING.md: written from the
  specification and from fact sheets read from JieLi's binaries and M-VAVE's V15 by a separate
  reader); felucca-bsp's LICENSING.md says the AC79 SDK's Apache-2.0 declaration "has not been
  confirmed with JieLi". SIDOU already carries the second risk for `uboot.boot` and `cfg_tool.bin`.

## 2. Size

### Measured: fm1-base's template, own stack

fm1-base `2d8ed74` with felucca-bsp `773b0b9`, built with fm1-base's `tools/build.py` inside SIDOU's
build image (JieLi clang 4.0.1, fm1-base's flags: `-Os`, one unity build); section sizes from
`objdump -h`, symbols from `nm -S` **[measured]**. The template was copied five times with only
`fm1.toml` changed (no touch-key or app change).

| Build | Image | `.text` | `.ram_text` | `.data` | `.bss` | pool | `.noinit` |
|---|---:|---:|---:|---:|---:|---:|---:|
| no radio | 84,440 | 81,180 | 3,132 | 128 | 27,716 | 69,384 | 484 |
| BLE MIDI (`ble_stack = "own"`) | 125,060 | 118,984 | 3,564 | 2,512 | 34,916 | 69,384 | 1,836 |
| + pairing (`ble_pairing`) | 128,320 | 122,244 | 3,564 | 2,512 | 35,348 | 69,384 | 1,836 |
| + pairing + central (`ble_central`) | 143,440 | 136,920 | 3,564 | 2,956 | 37,224 | 69,384 | 1,836 |
| + pairing + A2DP (`bt_audio`) | 164,964 | 158,664 | 3,564 | 2,736 | 55,856 | 78,984 | 1,836 |

Deltas over "no radio":

| | Flash | RAM (`.data` + `.bss`) | RAMTEXT | Pool | `.noinit` |
|---|---:|---:|---:|---:|---:|
| BLE MIDI | **+40,620** | **+9,584** | +432 | 0 | +1,352 |
| + pairing | +43,880 | +10,016 | +432 | 0 | +1,352 |
| + pairing + central | +59,000 | +12,340 | +432 | 0 | +1,352 |
| + pairing + A2DP | +80,524 | +30,748 | +432 | +9,600 | +1,352 |

What is in BLE MIDI's delta **[measured, from the symbols]**:

- **RAMTEXT:** the radio itself needs only `fm1_rf_burst_run`, **36 B** (the crystal ramp, which
  must run from SRAM with interrupts off; `build.py` refuses a build where it is not). The rest is
  fm1-base's glue: `fl_uid_ram` 262 B (reading the flash's unique ID for the address) and
  `shim_delay_us` 134 B.
- **RAM:** the baseband block `bb` **1,508 B** (the engine reads and writes it by DMA), the link
  layer `bll` 1,316 B, the PLL table 972 B, the MIDI rings and the L2CAP ring (about 3.5 KB in
  512/408/256-byte rings), and `ble_dg` **2,380 B**: the diagnostics' counters, which Optimist
  removed upstream on 2026-10-29 (`6b1dca8d`) and fm1-base's copy still has (`BLE_DIAG` defaults
  to 1; fm1-base's `main_fm1.c` reads `ble_dg`, so a `BLE_DIAG 0` build does not compile there).
- **`.noinit`:** `blog`, fm1-base's radio log (1,304 B), glue.
- **Flash:** the radio's start-up program `ble_rf_prog` 5,795 B (packed), `hw_rx_service`
  5,212 B, `fm1_ble_rf_exec` 2,966 B, the event interrupt 2,558 B, `ble_start` 2,304 B, the
  advertising receive 2,076 B, the RX interrupt 1,644 B; the rest is the host, ATT and BLE-MIDI.

### Optimist's own measurements [upstream]

Optimist measures every builder item on its own firmware (`tools/builder/costs.json`,
2026-10-31; and on fm1-base's configuration, `costs-fm1base.json`, 2026-11-02):

| Item | Flash | RAM | RAMTEXT | Pool |
|---|---:|---:|---:|---:|
| `BLE` (MIDI, peripheral) | 28,680 (fm1-base config: 36,516) | 6,412 (8,648) | 44 (32) | 0 |
| `BLE_BOND` (pairing) | +3,836 | +456 | 0 | 0 |
| `BLE_CENTRAL` level 1 / 3 | +19,864 / +12,156 | +2,172 / +1,480 | 0 | 0 |
| `BT_AUDIO` (A2DP) | +43,952 | +6,208 | +32 | +32,080 |

Optimist's 28.7 KB / 6.4 KB is the stack without the diagnostics and without fm1-base's glue.

### Fixed RAM regions the radio needs [verified]

- **None at a fixed address.** The baseband RAM block "is ordinary SRAM, anywhere the firmware
  chooses" (BLE-HW-FACTS.md section 1: its start and end go into `0x2FC84` / `0x2FCBC`).
- **But it must be reachable by the engine's DMA.** Optimist's Wi-Fi work found that a Wi-Fi
  transmit buffer in the pool (`0x01C20000` and up) "goes out as garbage with no error"
  (WIFI-CLAUDE-LIMITS.md section 4). No one has tried the BLE block there; the safe reading is to
  keep `bb` (1,508 B) in SIDOU's RAM region, as fm1-base and Optimist do.
- The radio's stored trims in the stock VM (flash `0x0E8000`) are not needed: felucca-bsp's
  `fm1_rf_cal.h` calibrates at every start. SIDOU never writes `0x0E8000` anyway (docs/hardware.md,
  "Flash map"). The bond and the address (about 40 B) go in SIDOU's own store.

### Estimate for SIDOU's BLE MIDI [estimated]

From Optimist's 28.7 KB / 6.4 KB (no diagnostics) and fm1-base's 40.6 KB / 9.6 KB (with them and
its glue): **flash 29-33 KB** (the stack plus SIDOU's glue, its SETTINGS row and popup, about
1-4 KB), **RAM 6.5-8 KB**, of which 1.5 KB must be DMA-reachable, **RAMTEXT 36-44 B**. Pairing adds
about 3.3-3.8 KB and 0.5 KB.

## 3. CPU and timing

**How it runs [verified]:** no tasks, no allocation. The whole stack runs in the radio's two
interrupts, IRQ 45 (event) and IRQ 29 (RX), both at priority 2 on core 0, never nested in each
other, below SIDOU's audio (ALNK0, 3) and TIMER5 (4); it talks to the main loop through
single-producer rings. The engine does every radio deadline in hardware (advertising, anchors,
hopping, ack and retransmission). Software deadlines: state 7 programmed within the transmit
window after a CONNECT_IND (at least 1.25 ms), a TX refill and the instants within one interval
(at least 7.5 ms). The handlers run from flash. A flash write with interrupts off (SIDOU's erases
hold up to 35 ms) lets the engine run alone: missed events are the central's retransmissions,
well inside the supervision timeout (1 s at the parameters asked for).

**Measured in the emulator [measured; emulator model, not hardware].** fm1-emu has no radio
model (no BLE or Wi-Fi peripheral in `fm1-soc`). Optimist's fork of simonjohansson's emulator,
`w0ts/fm1-emulator`, models the BLE engine from the fact sheet, runs a virtual central and
profiles the interrupts. The fm1-base template with the own stack, its touch keys off (the
emulator lacks that ADC channel), a central that scans 20 adverts, connects at **7.5 ms**,
discovers, subscribes and sends MIDI for 3 s, at 360 MHz:

| Run | Audio interrupt, max | BLE RX (29) max | BLE event (45) max | Link |
|---|---:|---:|---:|---|
| the template's own audio | 46.7 us | 3,923 instr (10.9 us), mean 257 | 258 instr (0.7 us), mean 181 | 566 events, 0 retransmissions, every MIDI write acknowledged |
| a spin added to the render, 1.41 ms a half (SIDOU's worst block is 345,600 instructions: 1.44 ms at the budget's 240 MHz) | 1,412 us | 10.9 us | 0.7 us | the same; BLE waited up to 1.41 ms for the audio, 128 times |
| a spin of 2.61 ms a half (44 % duty, far over SIDOU's budget) | 2,607 us | 10.9 us | 0.7 us | the same: 540 events, 0 retransmissions |

- **The stack's load is about 0.02 % of core 0** (133 events a second each way at 7.5 ms, about
  440 instructions an event, unencrypted). The audio is never delayed by it: lower priority, and
  in the emulator no interrupt nests.
- **Pairing's cost [measured]:** the stack's AES-128 (on-the-fly round keys, one S-box) takes
  **5,065 instructions a block** on pi32v2 (1,000 blocks, JieLi clang `-Os -mcpu=r3`, counted in
  the same emulator). CCM costs 7 blocks for a MIDI notification (35,000 instructions, about
  0.1 ms at 360 MHz) and 35 for a full 251-octet PDU (177,000, 0.49 ms). That is in the BLE
  interrupt, so it never touches the audio, but it does hold the main loop: an encrypted SysEx
  flood both ways would take about 13 % of the core and up to 0.5 ms out of a main-loop pass,
  against SIDOU's 0.5 ms MIDI latency and `POLL_INSTRUCTIONS` gates [estimated]. **A first SIDOU
  build without pairing** (macOS and iOS connect to an unpaired BLE-MIDI device; fm1-base and
  Optimist both say so) avoids it.
- **The CONNECT_IND deadline.** A connection whose CONNECT_IND is handled after the transmit window
  fails ("no answer in the first 6 events"), and the central retries. Measured once in the
  emulator: a central that connected 1.5 ms after the radio started, while fm1-base's boot still
  had the interrupts off, saw state 7 programmed 5.4 ms late and the link failed. SIDOU's worst
  audio block at the real 360 MHz is about 0.96 ms (below the 1.25 ms window) [estimated], but its
  boot has long interrupts-off waits (the LCD's 150 ms and 120 ms) and its erases hold up to 35 ms:
  **start the radio after the boot's waits**, and expect a rare first connection to need the
  central's retry.
- **The radio's start-up [verified, fm1-base `device/radio.c`, `main_fm1.c`]:** the trim draws
  about 500 ms of high current; at the boot loader's supply levels flash fetches came back
  corrupted and CTL-1 crashed 0.6 s into most starts. fm1-base therefore (1) raises the rails at
  every boot, before anything (`fm1_vdc14_raise(7)`, `fm1_sysvdd_raise(15)`: 1.60 V and 1.38 V, then
  `fm1_power_stock`), and (2) holds the app's render for the first second of a radio start. SIDOU's
  audio path reads nothing from flash, but its TIMER5 tick and main loop run from flash, and
  **SIDOU's HAL (Felucca 1.6.1 `fm1_sys.h`) has no rail raise**. That raise is felucca-bsp's, and it
  changes every SIDOU boot, not only Bluetooth's: a device check with the owner's yes.

**The second core [estimated].** Not needed: the stack is 0.02 % of a core. Not usable either: IRQ
45 and 29 are delivered to core 0 (BLE-HW-FACTS.md section 10), and in decision 0010's design core
1 only runs jobs inside the audio interrupt. One interaction matters: Optimist found every BLE plus
dual-core boot crashing within 2-60 s until it used the stock order (rails first, BLE started
before the audio, core 1's jobs 400 ms after BLE: DUAL-CORE.md section 3). If SIDOU does both
0010 and Bluetooth, the start order must follow that, and fm1-emu cannot prove it.

## 4. Fit: what SIDOU would give up or move

SIDOU 1.6.0 as built here **[measured]**: image 542,928 B (ceiling 568,000: **25,072 free**; the slot
581,564: 38,636), RAMTEXT 49,136 of 49,152 (**16 free**), RAM region 57,636 of 65,536 (7,900 free), POOL
363,056 of 368,384 (5,328 free), NOINIT 80 of 15,696 (**15,616 free**). emu-gate requires each of RAM
and POOL to keep `REGION_FREE_MIN` 5,120 and both together `FREE_RAM_MIN` 8,976: **usable RAM 2,780,
POOL 208**. NOINIT is in no gate.

**Flash: short by about 4-8 KB.** Options, best first:

| # | Option | Bytes | Cost and risk |
|---|---|---:|---|
| F1 | Part of the 13,564 B kept above the ceiling "for felucca-bsp and later" | up to 13,564 | Owner's decision; the felucca-bsp move itself may need it. No work |
| F2 | `-Oz` for the remaining cold `-Os` files (flash-space plan, candidate 7) | about 6,800 | Gates move (screen, song); one file at a time with its gate |
| F3 | Packed strings (candidate 11) | 2,000-3,000 | A decoder in every text path |
| F4 | A separate **SIDOU-BT** package without RED MOON RITE's art (`RUNS`, 34,538 B, and its motion) | about 36,000-41,700 | Product decision; SCENE loses a vignette in that package only |
| F5 | SIDOU-BT without the 8580's WEAK/STRONG combined profiles (`SIDOU_ALL_COMBINED_PROFILES_8580=OFF`, an existing option) | 19,216 | Product decision; that package's 8580 sounds differ |

The felucca-bsp move's own effect is unknown until it is built (its `usb.c` grew MSC, host and UAC
options, behind flags) [estimated: neutral to a few KB].

**RAM: short by about 4-6 KB; NOINIT solves it.** Keep the DMA block (1,508 B) and the rings the
audio or USB side touches in the RAM region (about 1.5-2.5 KB, inside its 2,780 usable). Put the
rest of the stack's `.bss` (link layer, host, ATT, L2CAP ring: about 4-6 KB) in NOINIT with an
`app.ld` input-section rule on the stack's object file, the way `sid_dac.c.obj`'s `.bss` already goes
to POOL, and zero it in SIDOU's glue before `ble_start`. That keeps the vendored files byte for byte.
NOINIT's DMA reach is unknown, so nothing DMA goes there.

**RAMTEXT: short by 20-28 B.** Decision 0010 already needs about 200 B from 1.7's performance pass.
Options there (docs/hardware.md "Code in SRAM"): `-Os` for `core/fx` (about 3.3 KB, cycle cost
unmeasured), or RAMTEXT above the debug area (8 KB, needs a device check).

**A separate published variant is the realistic first form.** CTL-1 already publishes two
packages (`ctl-X.Y.fwsc` with Bluetooth, `ctl-usb-X.Y.fwsc` without), and fm1-base's
`tools/build.py --no-ble` exists for exactly that. Proposed: `sidou-X.Y.fwsc` (as now) and
`sidou-bt-X.Y.fwsc` (BLE MIDI; F4 or F5 if F1 and F2 do not cover it). Both built and gated by the
same merge check: emu-gate runs twice, and the BT package's goldens may differ where its features
do. That doubles the gate's time, which `sidou-fast-iterations` cares about.

## 5. Integration path

**Upstream of record [verified].** There is no single stable upstream yet:

- Optimist (`firmware/src/ble/`, `firmware/hal/fm1_ble*.h`) is the author's tree and moves daily
  (three driver commits after the licence change of 2026-10-26; diagnostics removed 2026-10-29).
- fm1-base imported the stack at Optimist `3274ce5` (2026-10-26) into `bt/` and changed it since
  (`ble_att.c`, three `bt/br` files; it keeps `ble_diag`). Its copy and Optimist's differ in 980
  diff lines.
- felucca-bsp holds the radio drivers and tables in `chip/` (imported from Optimist `3274ce5`, then
  changed), but not the stack itself.
- Neither calls it stable: Optimist is early access, fm1-base "tried on one FM-1".
- The pair that is built and tested together is **fm1-base `bt/` with the felucca-bsp commit
  fm1-base pins**; its 33 host tests pass here on macOS **[measured]** (`tests/bt/run.sh`, with
  UBSan). That is the pair SIDOU would vendor, once on felucca-bsp: `chip/fm1_ble.h`,
  `fm1_ble_rf.h`, `ble_rf_tables_v15.h`, `fm1_rf_cal.h`, `fm1_rf_regs.h` with the rest of
  felucca-bsp, and `bt/ble/` byte for byte with fm1-base's flags (`-Os`, the unity wrapper
  `ble_stack.c`, `BLE_API` static). Taking Optimist's tree instead would mean a second copy of the
  radio drivers beside felucca-bsp's.

**What SIDOU writes [estimated]:** about the shape of fm1-base's `device/bt_own.c` (13.7 KB of
source), against SIDOU's own platform:

- the hooks the stack asks for (`ble_app_midi_peek/pop/in`, `ble_app_state`; `ble_app_bond` only
  with pairing), two rings, the address (random static, from the flash ID or kept in the store);
- the radio's start after the boot's waits, the rail raise (felucca-bsp), a short render hold at
  start, and the safety net (a boot after a radio start that hung leaves the radio off: SIDOU's
  `bootguard` already counts boots);
- **routing:** a third port beside TRS and USB in `midi_rx` (its own `midi_map_port`, as `trs_map`
  and `usb_map`), the clock and transport into `midi_sync` as a source, MIDI out to BLE as a choice
  in `midi_out`; no bridging between the ports (Optimist and fm1-base do none either);
- **SysEx and Live Link:** the stack carries SysEx across packets both ways, and Chrome's Web MIDI
  sees a BLE-MIDI device once the computer connected it, so Live Link and SysEx uploads could run
  over BLE without a protocol change [estimated, untested]: Live Link's frames (48-byte payloads, rate-limited by `LINK_RATE`) are small next
  to a 7.5 ms link's 133 events a second of up to 244 bytes each, but a `.syx` upload is slower than USB, and the store's
  erase holds (35 ms) are longer than a connection interval;
- SETTINGS: BLUETOOTH OFF / ON (off by default, so the radio never starts unless asked, as
  Optimist), with the popup rule for its encoder; the name `SIDOU-XXXX` in the advertising.

**What depends on the felucca-bsp move:** the drivers and tables (felucca-bsp's `chip/`), the rail
raise (`fm1_sys.h`), and the vendoring rule itself (one HAL, not Felucca 1.6.1's and felucca-bsp's
side by side). The move waits on felucca-bsp PR #21 (ROADMAP.md, Platform). So Bluetooth comes after
the move.

## 6. Emulator and tests

- **fm1-emu cannot test the radio [verified]**: no BLE engine, no Wi-Fi MAC. SIDOU's emu-gate can
  still gate a BLE build's sizes, its boot with the radio off, its RAM placement, and the audio and
  main-loop budgets with a stub driver (the stack ships `ble_hw_stub.c`, a driver that never calls
  back).
- **w0ts/fm1-emulator models the engine** (from the fact sheet and stock V15; it "proves the
  software, not the radio") with virtual centrals and peripherals and the interrupt profiler used
  above. **SIDOU 1.6.0 does not boot in it** [measured]: it stops in `input_buttons_at_power_on`
  (1.5 billion instructions, nothing on the LCD), a difference in the two emulators' key matrix.
  Getting SIDOU to run there, or the engine model into fm1-emu (both MIT/GPL, Joakim Eriksson's is
  MIT; the fork is GPL-3.0), is the work that makes step 3 below provable.
- **Host tests:** the stack's own (`tests/bt/`, 33 binaries, a fake engine built from the FM-1's
  measurements, a simulated Mac-like central) run as they are; SIDOU adds its glue's tests in
  `sidou-tests` (rings, routing, stuck notes when the link drops, the settings row) with the stub
  driver.

## 7. What players get

- **Wireless MIDI from an iPad, iPhone or Mac** (Audio MIDI Setup, GarageBand, AUM, Cubasis, Live):
  play SIDOU from a sequencer or a phone app, record its keys and knobs into a DAW, follow a DAW's
  clock, without a cable. That is the main value, and the one the M-VAVE firmware had (V15
  advertises "FM-1_BLE").
- **A BLE keyboard or controller as central** (the FM-1 connects out): costs 12-20 KB more (Optimist
  `BLE_CENTRAL` level 3 / 1) and a devices page. A later step, if asked for.
- **Bluetooth audio to headphones (A2DP): not for SIDOU.** 36.6 KB of flash and 20.7 KB of RAM over
  pairing, 9.6 KB of pool [measured]; the SBC encoder and the BR/EDR signalling at priority 5, above
  the audio (Optimist `ce77e4fb`); the device's own output muted while it streams; 150-300 ms from key
  to ear (fm1-base README); and SIDOU's 44,117.6 Hz would need resampling to 44.1 kHz.
- **Other FM-1 firmwares with Bluetooth MIDI [verified]:** stock M-VAVE V15 (JieLi), Optimist (own
  stack, with central and A2DP), CTL-1 and other fm1-base apps (JieLi's by default, the own stack as
  an option). SIDOU would be the first SID firmware with it.

## 8. Wi-Fi and Ableton Link (Optimist's own Wi-Fi)

The owner's second question (2026-11-04). Optimist `main` at `1fed5103` and its branch `link`.

**Licence [verified]:** `firmware/src/wifi/` (station, AP, DHCP, IP layer, crypto) is `GPL-3.0-only
OR MPL-2.0`; `firmware/hal/fm1_wifi.h` is the same, its register values JieLi's facts (Apache-2.0);
`fm1_wifi_wlc.h` (the coexistence controller's 565 set-up words) is Apache-2.0 with JieLi's
copyright. `firmware/src/link/` is a C port of Ableton Link's sync part, taken under the library's
GPL-2.0-or-later as **GPL-3.0-only**: compatible with SIDOU's GPL-3.0-only image. "Not a clean room"
applies here more than to Bluetooth (written after reading JieLi's libraries as LLVM IR and running
them in the emulator). Ableton's rules for showing the Link name or logo were not checked: an owner
check before any SIDOU text says "Ableton Link".

**Maturity [verified, Optimist's own docs, not tried here]:** not working end to end on a real FM-1.
Receive, scan, join and group decryption work on the device; "DHCP and a Link session not through
yet" (`tools/builder/registry.py`, the `WIFI_LINK` item, experimental, off in every profile). The
open fault is transmit: large protected frames reach the access point garbled, so DHCP never gets
an answer (WIFI-LINK-HANDOVER.md, 2026-10-27/29), and the FM-1 is heard about 20 dB weak at 3-4 m,
joins succeeding about 6 times in 8 (WIFI-CLAUDE-LIMITS.md). MODE AP is host-tested only. The Link
peer is the mature part: 151 host checks, and within 0.003 ms of phase of Ableton's own library in
Live's session.

**Size [upstream, hand measurements, `costs-all` not run]:** `WIFI_LINK` (station, IP, Link)
**+27,700 B flash, +1,944 B RAM, +3,792 B pool, 0 RAMTEXT** on Optimist (on fm1-base's configuration
34,924 / 7,220 / 0); MODE AP +6,168-8,700 more. The Link peer alone is about 6 KB of code and a
1,888-byte node. Wi-Fi DMA buffers must be in main RAM (the pool fails), and its receive buffer is
1-2.4 KB.

**Bluetooth and Wi-Fi together:** fm1-base's "not both" is about **JieLi's** libraries (WiFi alone
about 300 KB of flash and 100 KB of RAM there). Optimist's own Wi-Fi is the opposite: it **needs**
the BLE radio's bring-up (the same start-up program is JieLi's Wi-Fi quick start, 33,357 of 34,020
writes the same) and is 27.7 KB on top of it. So for SIDOU, Wi-Fi + Link is BLE's ~29-33 KB plus
~28 KB: about 57-61 KB of flash, which needs F4 and F5 together or more. Whether BLE MIDI and Wi-Fi
can run at the same moment on the one front end is not answered upstream ("for the driver to
answer", LINK-DESIGN.md).

**CPU and audio [estimated, from the design]:** receive is polled from the main loop (interrupt
52 "never latched"), an early filter drops a busy network's chatter at about 20 instructions a
frame, and `lk_poll` runs from the main loop with no floats. Nothing runs in the audio interrupt.
Because receive is polled, a Link measurement's arrival stamp is late by up to a main-loop pass on
one side, which moves the shared beat by half of that (LINK-DESIGN.md section 2.4): with SIDOU's
0.5 ms passes, up to about 0.25 ms of phase, inaudible for a clock.

**What players would get:** tempo, beat phase and start / stop shared with Live, iOS Link apps and
other Link devices on the same network, with no cable and no MIDI clock jitter; and, with MODE AP, a
gig network with no router. **Not** Live Link or the site over Wi-Fi: the IP layer is UDP only (no
TCP, no DNS, no mDNS) and a browser cannot speak raw UDP, so that would need a TCP/WebSocket stack
of lwIP's size.

**Ranking: below Bluetooth MIDI.** It does not work on hardware yet, it depends on the BLE radio
work, it costs twice the flash, and its value (Link sync) overlaps what BLE MIDI clock and USB MIDI
clock already give. Revisit when Optimist reports a Link session on a real FM-1, after BLE MIDI.

## Recommended plan

| Step | What | Proof | Size |
|---|---|---|:-:|
| 0 | The owner's decisions below | | |
| 1 | **The felucca-bsp move** (already on the roadmap, waits on PR #21), with the rail raise examined as its own item | emu-gate; a device check of the boot with the owner's yes | L |
| 2 | **Measurement spike in SIDOU:** a `SIDOU_BLE` CMake option (off), fm1-base's `bt/ble/` and felucca-bsp's radio files from `build/ext` (not committed), SIDOU's glue against `ble_hw_stub.c`, the `app.ld` placement (DMA block in RAM, the rest in NOINIT), the stack at its flags. Measures SIDOU's real flash, RAM and RAMTEXT deltas and runs emu-gate's budgets with BLE built in and the radio off | fm1-emu, host tests | M |
| 3 | **SIDOU in the BLE emulator:** make SIDOU boot in w0ts/fm1-emulator (or bring its engine model to fm1-emu), then a virtual central plays notes into LIVE and MIDI comes back, with SIDOU's real two-SID audio interrupt profiled as in section 3 | emulator | M |
| 4 | **The room:** F1/F2 (or the BT package with F4/F5), RAMTEXT with 0010's pass | emu-gate | M |
| 5 | **BLE MIDI in, out and clock**, no pairing, SETTINGS row and popup, docs (README, demo page, cheat sheet, `fmhub.json`, LICENSING, CREDITS, design notes), behind the option or as `sidou-bt` | emu-gate, host tests | M |
| 6 | **Device test** with the owner's yes for each step: radio start, a Mac and an iPad connecting, an hour of two SIDs with BLE MIDI in, an erase during a link | device | S |
| 7 | Later, if asked: pairing (+3.8 KB, AES in the interrupt measured above), the central role (+12-20 KB), Live Link over BLE | | M |

## Decisions for the owner

1. **Priority and form:** BLE MIDI after 1.7 and after the felucca-bsp move, first as a build
   option, then as the main image or a second `sidou-bt` package?
2. **Flash:** may part of the 13,564 B above the ceiling go to Bluetooth (F1)? If not, which feature
   does a `sidou-bt` package leave out (F4: RED MOON RITE's art, about 36-42 KB; F5: the 8580's
   WEAK/STRONG profiles, 19.2 KB)?
3. **RAM:** the stack's non-DMA state in NOINIT (a new placement rule, not a budget raise)?
4. **The rail raise** (felucca-bsp `fm1_sys.h`, 1.60 V / 1.38 V as M-VAVE's firmware sets them) for
   every SIDOU boot: accept it with the felucca-bsp move, with a device check?
5. **Pairing:** start without it (macOS and iOS connect anyway; no AES in the interrupt)?
6. **Upstream:** vendor the stack from fm1-base `bt/` at the commit whose felucca-bsp pin SIDOU uses;
   put "would the stack live in felucca-bsp itself?" in `docs/upstream-reports.md` for the next batch.
7. **A2DP** dropped for SIDOU, and **Wi-Fi + Link** parked until Optimist has a Link session on a real
   FM-1?

## Not done here

- Nothing ran on an FM-1, so every timing above is the emulator's model or an estimate.
- SIDOU with the stack linked was not built (step 2); the SIDOU numbers in section 2 are estimates
  from fm1-base's and Optimist's builds.
- The fm1-base builds ran the template without its touch keys in the emulator only (the emulator
  lacks that ADC channel); the size table is the template as it is.
- Encrypted links were not run in the emulator (the virtual central has no pairing step); the AES cost
  is a separate count.
- Optimist's Wi-Fi was not built or run; its numbers are its own.
