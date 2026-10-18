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
| 3 | fm1-emu | The and-not read-modify-write on memory, `[rX+imm] &= ~rY` (`64 e8 0b ..`, `64 e8 0f ..`), does not decode | M2d Task 1 | `docs/TODO.md`, "Report fm1-emu: the and-not read-modify-write on memory is not decoded" | not drafted |
| 4 | fm1-emu | A 6-byte compare and branch (`43 ff …`), which JieLi clang emits for the 6502 prototype's dispatch, does not decode | 2026-10-18, 6502 study | decision record 0008, Risks | not drafted |
| 5 | fm1-emu | P33 and the watchdog are not modelled (`P33_CON`/`P33_DAT` read 0) | M1 | `docs/TODO.md`, "fm1-emu does not model P33 or the watchdog" | feature request, not drafted |

Items 2-5 are against the pinned `e52f4640`. The fm1-emu bump will show which are still
open.

**Offers rather than bugs** (also shown to the user first): SIDOU's USB host model for
fm1-emu's tests (`docs/TODO.md`), and macOS support for the FM-1's ROM-boot recovery
(`tools/fm1-unbrick-mac`) for MvaveFM1Unbricker / jl-uboot-tool.

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
