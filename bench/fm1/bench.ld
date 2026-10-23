/* SPDX-FileCopyrightText: 2026 Joakim Eriksson
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: BSD-2-Clause
 * Code in the XIP window; data/bss in the SRAM Felucca uses for RAM + POOL. */
MEMORY
{
    XIP (rx)  : ORIGIN = 0x02000000, LENGTH = 0x80000
    RAM (rwx) : ORIGIN = 0x01C08000, LENGTH = 0x6C000
}
SECTIONS
{
    .text : { KEEP(*(.text.crt0)) *(.text*) *(.rodata*) . = ALIGN(4); } > XIP
    .data : { . = ALIGN(4); __data_start = .; *(.data*) *(.sdata*) . = ALIGN(4); __data_end = .; } > RAM AT > XIP
    __data_load = LOADADDR(.data);
    .bss (NOLOAD) : { . = ALIGN(4); __bss_start = .; *(.bss*) *(.sbss*) *(COMMON) *(.pool*) . = ALIGN(4); __bss_end = .; } > RAM
    /DISCARD/ : { *(.comment) *(.note*) }
}
/* The .bin is .text and .data joined (bench/fm1/CMakeLists.txt); main() copies .data from
 * here. Any other section is an error, not an orphan: the link uses --orphan-handling=error. */
ASSERT(LOADADDR(.data) == ADDR(.text) + SIZEOF(.text), "bench.ld: .data must load right after .text")
