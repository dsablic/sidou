<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Reports for other projects

Bugs and gaps SIDOU has found in other projects, collected here to report later. **Nothing
is posted until the user has reviewed it.** Before filing, check each item against the
project's latest revision, since it may already be fixed. Then record the issue's link
here.

| # | Project | What | Found | Details | Status |
|---|---|---|---|---|---|
| 1 | FM-1 SID (`fm1-sid/` in [fm1-emu](https://github.com/joakimeriksson/fm1-emu)) | `psid_load` reads past its buffer when the header's data offset is beyond the file (`psid.c:145`); ASan repro with a 124-byte file | 2026-10-18 | [Below](#1-fm-1-sid-psid_load-out-of-bounds-read); decision record 0008 | **filed 2026-10-23: [fm1-emu#63](https://github.com/joakimeriksson/fm1-emu/issues/63); fixed upstream the same day (PR #68); pin `c1c045fc` (2026-10-23) contains it** |
| 2 | fm1-emu | The decrementing register block load `{R*4} = [-R]` (`r_4_mov_r_4`) decodes but does not execute | M1b | `docs/TODO.md`, "Report fm1-emu: decrementing register block load is not executed" | **filed 2026-10-23: [fm1-emu#64](https://github.com/joakimeriksson/fm1-emu/issues/64); fixed upstream the same day (PR #67); verified at the pin `c1c045fc` (2026-10-23): `41 eb 0a 00` runs, and `step_block_load_dec` is dropped** |
| 3 | fm1-emu | The and-not read-modify-write on memory, `[rX+imm] &= ~rY` (`64 e8 0b ..`, `64 e8 0f ..`), does not decode | M2d Task 1 | `docs/TODO.md`, "A gate that decodes every instruction of the image" | fixed upstream, nothing to file: #33 (the source register is bits 24-27, so r8-r15 decode; `64 e8 5f bc`, `[r11+92] &= ~r12`, faults at `7ca5b96c` and runs at `db8d472e`), and #36 for the pair-head `R = R & ~(1 << R)` (`94 f1 ..`) with every register |
| 4 | fm1-emu | A 6-byte compare and branch (`43 ff …`), which JieLi clang emits for the 6502 prototype's dispatch, does not decode | 2026-10-18, 6502 study | decision record 0008, Risks | runs in the emulator, nothing to file: fm1-core's interpreter executes it (taken and not taken) at `7ca5b96c` and `db8d472e`, through its raw decoder; SIDOU's image has four (`sid_fx_echo_tick`, `sid_character_filter_job_step`), which the gate runs. Only fm1-isa's table (`decode_bytes`) still rejects it, as the vendor objdump does |
| 5 | fm1-emu | P33 and the watchdog are not modelled (`P33_CON`/`P33_DAT` read 0) | M1 | `docs/TODO.md`, "fm1-emu does not model the watchdog" | **filed 2026-10-23: [fm1-emu#65](https://github.com/joakimeriksson/fm1-emu/issues/65)** |
| 6 | Felucca (`firmware/src/usb.c:50-56`) | The USB DMA buffers (`ep0buf`, `ep1tx`, `ep1rx`, …) are declared `aligned(4)`, but on the FM-1 an EP1 IN buffer at 4 mod 8 is never sent (TxPktRdy stays set). Felucca's own builds happen to place them 8-aligned; a layout change would break USB-MIDI out. Fix: `aligned(8)` or more. | 2026-10-18, SIDOU's USB-out diagnosis | [Below](#6-felucca-usb-dma-buffers-need-8-byte-alignment); [usb-out-diagnosis.md](hardware-results/usb-out-diagnosis.md) | **filed 2026-10-23: [Felucca#293](https://github.com/hugelton/Felucca/issues/293)** |
| 7 | Felucca (`firmware/src/usb.c`, `sie_wr`, `sie_rd`) | Built with `-mcpu=r3 -mfprev1 -O2`, `usb.c` stops sending on EP1 IN on the FM-1 (enumeration and OUT keep working); with Felucca's own flags (no `-mcpu`, `-Os`) it works. At `-O2` the SIE accessors are inlined and back-to-back SIE accesses follow each other within a few cycles. Fix: `noinline` on `sie_rd`/`sie_wr`, or a read-back between accesses, so the file doesn't depend on its build flags. | 2026-10-19, SIDOU's USB-out diagnosis | [Below](#7-felucca-usbc-breaks-when-built-at--o2--mcpur3) | **filed 2026-10-23: [Felucca#294](https://github.com/hugelton/Felucca/issues/294)** |
| 8 | Felucca (`firmware/src/usb.c`) | Two small opt-in hooks, so a firmware built on Felucca's USB stack can use it unmodified. (1) `FELUCCA_USB_MANUFACTURER` and `FELUCCA_USB_PRODUCT` for its own string descriptors. (2) `FELUCCA_MIDI_IN_RAW(pkt)` to get every EP1 OUT event packet before `midi_in_event`'s filter, so system common and every real-time byte reach a parser of its own. Both are inert when undefined. | 2026-10-20, SIDOU adopting `usb.c` | [Below](#8-felucca-two-opt-in-hooks-in-usbc); SIDOU's patch in `platform/fm1/src/usb.c` | **filed 2026-10-23: [Felucca#295](https://github.com/hugelton/Felucca/issues/295)** |
| 9 | fm1-emu (`crates/fm1-web`, `web/`) | Feature request: keep the emulated FM-1's SPI flash across page loads in the web player. The wasm exports no way to read or write the flash, so every visit boots with empty flash: firmwares that store settings (SIDOU's first-run tour and saved songs, Felucca-family settings) start over each time. | 2026-10-22, SIDOU's web demo | [Below](#9-fm1-emu-persist-the-flash-in-the-web-player) | already upstream, nothing to file: 1adbd85 (2026-10-18, after the old pin) keeps the flash per package in IndexedDB with export and import; SIDOU's demo has it since the bump to `db8d472e` |

**Filed 2026-10-23** (after the user's review), each linked in the table. **Rechecked before filing:** Felucca 6 and 7
(one issue) and 8 (a pull request carrying all three), FM-1 SID 1, fm1-emu 2 and 5's watchdog.
**Nothing to file:** 3 and 4 (fixed or working upstream) and 9 (already upstream). Items 2-5 were
found against `e52f4640` and rechecked at the pin, `db8d472e` (2026-10-22), each instruction
stepped in a scratch crate at `7ca5b96c` and at `db8d472e`.

**Offers rather than bugs** (also shown to the user first): SIDOU's USB host model for
fm1-emu's tests (`docs/TODO.md`), and macOS support for the FM-1's ROM-boot recovery
(`tools/fm1-unbrick-mac`) for MvaveFM1Unbricker / jl-uboot-tool. Later: FM-1 Live Link (the roadmap's companion module) for the other FM-1 firmwares.

## 1. FM-1 SID: psid_load out-of-bounds read

**Title:** fm1-sid: psid_load reads past the buffer when the header's data offset is beyond
the file

`psid_load` (`fm1-sid/player/psid.c`, at 7ca5b96) takes `dataOffset` from the header and
uses it without checking it against the file length `n`:

- with `load == 0` it reads `b[off]` and `b[off + 1]` for the load address (line 145);
- `len = n - off` wraps when `off > n`, and the clamp to `65536 - load` still lets
  `memcpy` read up to 64 KB past the buffer.

**Repro:** a 124-byte PSID v2 file with `dataOffset = 0x2000` and `loadAddress = 0`, all
other fields zero, loaded by a small host harness built with `-fsanitize=address`:

```
50534944 0002 2000 0000 1000 1003 0001 0001 00000000  (then zeros to 124 bytes)
```

```
ERROR: AddressSanitizer: heap-buffer-overflow
READ of size 1 ... in psid_load psid.c:145
```

SIDOU's fixtures (added after filing) include a second file that reaches the same read with a
valid data offset: 124 bytes (`dataOffset = 0x7C`, `loadAddress = 0`, songs 1, start song 1, the
rest zero) leaves no byte for the load address, so `b[off]` and `b[off + 1]` are past the end. SIDOU
refuses both (and its fuzz cuts files at every length of the header), in
`host/crates/sidou-tests/tests/psid_file.rs` (`fm1_sids_bounds_bug_files_are_refused`).

On the FM-1 this is an out-of-bounds read of flash or RAM when a damaged or crafted `.sid`
is packed in. It's probably harmless in practice, but cheap to close.

**Suggested fix:** before using the offset,

```c
if (off < 0x76 || off > n)
    return -1;
if (!p->info.load) {
    if (n - off < 2)
        return -1;
    ...
}
```

then compute `len = n - off` only after those checks.

Thanks for FM-1 SID; it's a very useful reference for the FM-1 community.

## 2. fm1-emu: the decrementing register block load does not execute

**Title:** fm1-core: `r_4_mov_r_4` (`{R*4} = [-R]`) decodes but `Cpu::step` returns Unsupported

fm1-isa decodes the decrementing block load `r_4_mov_r_4` (`{R*4} = [-R]`, mask `0x0804fff0`,
match `0x0000eb40`), but fm1-core executes only the `[R+]` and `[R++]` block moves, so stepping
it stops with `Unsupported { name: "r_4_mov_r_4" }` (still so at `db8d472e`).

**Repro:** JieLi clang 4.0.1 at `-mcpu=r3 -O2` emits `41 eb 0a 00` (`{r1, r3} = [-r1]`) when it
loads the previous element of a two-float array, e.g.

```c
float interpolate(const float *t, unsigned i, float f) {
    return t[i - 1] + (t[i] - t[i - 1]) * f;
}
```

**Semantics we use in our tests' workaround, which matches the device and a bit-exact host
comparison:** the registers load in ascending order from `base - 4 * count`, and the base
register is not written back. SIDOU's `step_block_load_dec` (in our emulator tests'
`common/mod.rs`) implemented it; fixed upstream by PR #67, so it was dropped at the pin `c1c045fc`.

Thanks for fm1-emu: SIDOU's whole test gate runs on it.

## 5. fm1-emu: a watchdog on the P33 model

**Title:** fm1-soc: P33 could drive a watchdog and PR_PWR resets

Since #34 the P33 interface (`P33_CON` 0x13E08, `P33_DAT` 0x13E0C) keeps what is written, but
nothing acts on it (`fm1-soc/src/p33.rs`: "no watchdog, no reset through PR_PWR"). Firmwares
arm and feed the watchdog through it (Felucca's `fm1_sys.h`: `fm1_wdt_arm`, `fm1_wdt_feed`,
`fm1_reboot`), so in the emulator a firmware that stops feeding it runs on, and a reboot request
does nothing.

**Request:** model the watchdog on P33: when armed and not fed within its period, reset the
emulated chip as the device does; and a reset through `PR_PWR`. That would let firmware test
suites check that a hang recovers and that the main loop always feeds the watchdog. Happy to
help test it against SIDOU's boot and boot-guard tests.

## 6. Felucca: USB DMA buffers need 8-byte alignment

**Title:** usb.c: the EP DMA buffers need 8-byte alignment, but are declared aligned(4)

`firmware/src/usb.c` (at `a53ecf4`, 1.6.1) declares the endpoint buffers
`__attribute__((aligned(4)))` (lines 50-56: `ep0buf`, `ep1tx`, `ep1rx`, `ep2tx`, `ep3tx`,
`ep3rx`). On the FM-1, an EP1 IN buffer at an address that is 4 mod 8 is never sent:
`TXCSR1` keeps TxPktRdy set, and the host sees nothing on EP1 IN, while EP1 OUT and
enumeration keep working.

Felucca's own builds happen to put `ep1tx` on an 8-byte boundary, so Felucca works. Any
change to the variables in front of it can move it to 4 mod 8 and silently break USB-MIDI
out and the update protocol's replies. SIDOU found this when its own layout put `ep1tx` at
4 mod 8 (the counters showed TxPktRdy never clearing).

**Suggested fix:** `aligned(8)` on the DMA buffers (or 64, one packet, to be safe); the
same for the update loader, which builds from the same file.

## 7. Felucca: usb.c breaks when built at -O2 -mcpu=r3

**Title:** usb.c depends on its build flags: built with `-mcpu=r3 -O2`, EP1 IN stops on the
FM-1

SIDOU builds its firmware with `-mcpu=r3 -mfprev1 -O2`. Built that way, Felucca's
`firmware/src/usb.c` (at `a53ecf4`, unmodified) enumerates and receives on EP1 OUT, but EP1
IN stops after the first packet or two: the installer's identity request (cmd 0x11) gets
no reply. The same source, in the same image, compiled as `tools/build.py` compiles
Felucca (no `-mcpu`, `-Os`) answers every time. Results on one unit, 10 handshakes each:
0/10 with SIDOU's flags, 10/10 with Felucca's, the rest of the image unchanged. Only the
flags of the translation unit that includes `usb.c` matter.

The generated code has no unusual instructions on the USB path with `r3`. The SFR accesses
are the same forms and there is the same `csync` after each CON1 write. The difference is
inlining: at `-O2`, `sie_wr` and `sie_rd` are inlined into `usb_poll`, so consecutive SIE
accesses (INDEX then TXCSR1, TXCSR1's read then its write, the EP count/address then
TxPktRdy) run a few cycles apart. At `-Os` each is a call. Our guess, not proven: the SIE
sets the done bit (CON1 bit 15) before it has really latched the access, so a following
access issued that soon hits stale state.

Felucca's own build hides this, but a different compiler version or flag choice would
bring it back.

**Suggested fix (either):**

- `__attribute__((noinline))` on `sie_wr` and `sie_rd`, which keeps the spacing Felucca's
  build has today; or
- a read-back of CON1 (or a few cycles' delay) after the done bit, before the next access.

We can test a patch on our unit with SIDOU's flags if that helps.

## 8. Felucca: two opt-in hooks in usb.c

**Title:** usb.c: opt-in hooks for the USB strings and raw MIDI in, so other firmwares can
use it unmodified

SIDOU now builds Felucca's `firmware/src/usb.c` (at `a53ecf4`) into its app as it is. The
same file also builds the update loader. Two things kept it from being usable without
edits. Each is fixed by a hook that is inert when its macro is undefined. Felucca's
firmware and its loader build byte-identical with the patch applied: we checked the
loader's `loader.bin`.

1. **The string descriptors.** `STR1` ("Hügelton Instruments") and `STR2` ("Felucca") are
   fixed, so a firmware that is not Felucca presents as Felucca. The patch: an includer
   may define `FELUCCA_USB_MANUFACTURER` and `FELUCCA_USB_PRODUCT`, each the bytes of a
   whole string descriptor (`bLength, 3, UTF-16LE…`), much as `FELUCCA_USB_PID` already
   overrides the PID. The loader keeps "Felucca Update".
2. **Raw MIDI in.** `midi_in_event` keeps channel messages and F8/FA/FB/FC for Felucca's
   engine. It drops system common (song position, song select, tune request) and the
   other real-time bytes. A firmware with its own MIDI parser needs all of them. The patch:
   if the includer defines `FELUCCA_MIDI_IN_RAW(pkt)`, `midi_in_event` calls it first with
   every event packet, then goes on unchanged.

```c
/* SIDOU hook (upstream candidate): an includer's own manufacturer and product strings, each
 * the bytes of a whole string descriptor (bLength, 3, UTF-16LE), for a firmware built on this
 * stack that is not Felucca. Undefined, Felucca's. */
#ifdef FELUCCA_USB_MANUFACTURER
static const uint8_t STR1[] = {FELUCCA_USB_MANUFACTURER};
#else
static const uint8_t STR1[] = {42, 3, 'H', 0, 0xFC, 0, /* … as now … */};
#endif
#ifdef FELUCCA_LOADER
static const uint8_t STR2[] = {30, 3, 'F', 0, /* "Felucca Update", as now */};
#elif defined(FELUCCA_USB_PRODUCT)
static const uint8_t STR2[] = {FELUCCA_USB_PRODUCT};
#else
static const uint8_t STR2[] = {16, 3, 'F', 0, /* "Felucca", as now */};
#endif

static void midi_in_event(uint32_t pkt)
{
    uint32_t cin = pkt & 15u, st = (pkt >> 8) & 0xFFu;
#ifdef FELUCCA_MIDI_IN_RAW
    /* every event packet as it came, to an includer that parses MIDI itself, before the
     * filter below, which goes on as it is */
    FELUCCA_MIDI_IN_RAW(pkt);
#endif
    /* … as now … */
}
```

Both hooks would let SIDOU, and any other firmware built on the stack, take `usb.c`
byte for byte. That way fixes like items 6 and 7 reach every firmware.

## 9. fm1-emu: persist the flash in the web player

**Title:** web: keep the emulated flash between visits (export and import it)

The web player boots a package with `fm1_boot_fwsc`, which builds the machine's SPI flash from
the package, and `web/main.js` starts it again on every page load. Firmware that keeps data in
flash (settings, saved songs, a "first run done" record) therefore starts from nothing on each
visit; SIDOU's demo shows its first-run tour every time, and a song saved in it is lost on
reload. The wasm has no export for the flash (the machine is a static inside `fm1-web`), so a
page cannot keep it.

**Proposal:** two small exports in `crates/fm1-web/src/lib.rs`, in the style of the others:

```rust
/// A pointer to the emulated SPI flash (FLASH_SIZE bytes), for the page to copy out.
#[no_mangle] pub extern "C" fn fm1_flash() -> *const u8;
/// Replace the flash with `len` bytes at `ptr` (from fm1_alloc), before or just after boot.
#[no_mangle] pub extern "C" fn fm1_flash_load(ptr: *mut u8, len: u32);
```

and in the worklet a `flash` message that posts the bytes back (on request, or when the flash
changed), which `main.js` can keep in IndexedDB (it already has an `idb()` helper for
firmwares) and send in with `boot`. The page would save when it hides or unloads and every
few seconds while the flash changed, and offer a "reset" that clears the kept copy. Where the
package's own flash image would overwrite the kept bytes (the app and loader regions), the
kept copy should be applied around it, not over it, or a new firmware version would run
against the old image.
