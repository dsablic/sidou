<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Reports for other projects

Bugs and gaps SIDOU has found in other projects, collected here to report later. **Nothing
is posted until the user has reviewed it.** Before filing, check each item against the
project's latest revision, since it may already be fixed. Then record the issue's link
here.

| # | Project | What | Found | Details | Status |
|---|---|---|---|---|---|
| 1 | FM-1 SID (`fm1-sid/` in [fm1-emu](https://github.com/joakimeriksson/fm1-emu)) | `psid_load` reads past its buffer when the header's data offset is beyond the file (`psid.c:145`); ASan repro with a 124-byte file | 2026-10-18 | [Below](#1-fm-1-sid-psid_load-out-of-bounds-read); decision record 0008 | drafted |
| 2 | fm1-emu | The decrementing register block load `{R*4} = [-R]` (`r_4_mov_r_4`) decodes but does not execute | M1b | `docs/TODO.md`, "Report fm1-emu: decrementing register block load is not executed" | not drafted |
| 3 | fm1-emu | The and-not read-modify-write on memory, `[rX+imm] &= ~rY` (`64 e8 0b ..`, `64 e8 0f ..`), does not decode | M2d Task 1 | `docs/TODO.md`, "Report fm1-emu: the and-not read-modify-write on memory is not decoded" | fixed upstream (fm1-emu #33, 2026-10-21; #35 open for the shifted form): check at the bump, then drop |
| 4 | fm1-emu | A 6-byte compare and branch (`43 ff …`), which JieLi clang emits for the 6502 prototype's dispatch, does not decode | 2026-10-18, 6502 study | decision record 0008, Risks | not drafted |
| 5 | fm1-emu | P33 and the watchdog are not modelled (`P33_CON`/`P33_DAT` read 0) | M1 | `docs/TODO.md`, "fm1-emu does not model P33 or the watchdog" | P33 modelled upstream (fm1-emu #34, 2026-10-21); the watchdog to check at the bump |
| 6 | Felucca (`firmware/src/usb.c:50-56`) | The USB DMA buffers (`ep0buf`, `ep1tx`, `ep1rx`, …) are declared `aligned(4)`, but on the FM-1 an EP1 IN buffer at 4 mod 8 is never sent (TxPktRdy stays set). Felucca's own builds happen to place them 8-aligned; a layout change would break USB-MIDI out. Fix: `aligned(8)` or more. | 2026-10-18, SIDOU's USB-out diagnosis | [Below](#6-felucca-usb-dma-buffers-need-8-byte-alignment); [usb-out-diagnosis.md](hardware-results/usb-out-diagnosis.md) | drafted |
| 7 | Felucca (`firmware/src/usb.c`, `sie_wr`, `sie_rd`) | Built with `-mcpu=r3 -mfprev1 -O2`, `usb.c` stops sending on EP1 IN on the FM-1 (enumeration and OUT keep working); with Felucca's own flags (no `-mcpu`, `-Os`) it works. At `-O2` the SIE accessors are inlined and back-to-back SIE accesses follow each other within a few cycles. Fix: `noinline` on `sie_rd`/`sie_wr`, or a read-back between accesses, so the file doesn't depend on its build flags. | 2026-10-19, SIDOU's USB-out diagnosis | [Below](#7-felucca-usbc-breaks-when-built-at--o2--mcpur3) | drafted |
| 8 | Felucca (`firmware/src/usb.c`) | Two small opt-in hooks, so a firmware built on Felucca's USB stack can use it unmodified. (1) `FELUCCA_USB_MANUFACTURER` and `FELUCCA_USB_PRODUCT` for its own string descriptors. (2) `FELUCCA_MIDI_IN_RAW(pkt)` to get every EP1 OUT event packet before `midi_in_event`'s filter, so system common and every real-time byte reach a parser of its own. Both are inert when undefined. | 2026-10-20, SIDOU adopting `usb.c` | [Below](#8-felucca-two-opt-in-hooks-in-usbc); SIDOU's patch in `platform/fm1/src/usb.c` | drafted, a proposed patch |

Items 2-5 are against the pinned `e52f4640`. The fm1-emu bump will show which are still
open: upstream has since fuzzed its decoder against the vendor objdump (#18, four decode
fixes), which may also cover item 4.

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
