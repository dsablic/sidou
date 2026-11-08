<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Optimist's Bluetooth LE stack (from fm1-base's `bt/ble`)

- **Source:** <https://github.com/charlesvestal/fm1-base>, `bt/ble/`, commit `698e2cb`
  (2026-11-05), the first whose felucca-bsp pin was `f878800`, the one SIDOU vendored then.
  SIDOU's BSP is `7e09895c` now (`platform/fm1/VENDORED.md`): the radio headers are the same
  bytes in both, and fm1-base's `bt/ble/` is unchanged to `b0976f2` (2026-11-07, pinning
  felucca-bsp `5b0879b`), so the copy stands. The stack is Optimist's (<https://github.com/w0ts/optimist>), by
  Henri David.
- **Licence:** `GPL-3.0-only OR MPL-2.0` (each file's header; offered under either licence,
  w0ts/optimist#55). SIDOU takes it under GPL-3.0-only, which its firmware image already is.
- **Local changes:** none. All 35 files byte for byte, the host-only ones (`ble_hw_stub.c`,
  `ble_diag.c`) too, so an update is a copy of the directory.
- **The radio drivers** it drives are felucca-bsp's (`platform/fm1/bsp/chip/fm1_ble.h`,
  `fm1_ble_rf.h`, `fm1_rf_cal.h`, `fm1_rf_regs.h`, `ble_rf_tables_v15.h`), from the same commit as
  the rest of the BSP. `fm1_ble_rf.h` includes `ble_rf_tables.h` (fm1-base's build copies the
  table under that name); SIDOU's `platform/fm1/ble_rf_tables.h` includes the v15 table instead.
- **How SIDOU builds it** (`-DSIDOU_BLE=ON`, `platform/fm1/CMakeLists.txt`): the host side in
  `platform/fm1/ble_stack_unit.c` at `-Oz`, the driver in SIDOU's glue `platform/fm1/ble_radio.c`,
  its own unit at `-Os` with `FM1_IRQ_DEFS 0`, so `main.c` keeps the BSP's `fm1_irq.h`
  definitions (the design notes, "Bluetooth").

## Updating

Copy fm1-base's `bt/ble/` at the commit whose felucca-bsp pin SIDOU uses, and the radio headers
from that felucca-bsp commit; update the commits here and in `LICENSING.md`.
