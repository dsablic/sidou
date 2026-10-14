/* SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-only
 * SIDOU memory map: Felucca's firmware/app.ld, without the second core's .c1_text.
 * XIP    : code and constants, executed in place; app.bin offset 0 = 0x02000120.
 * RAMTEXT: .ram_text, code that runs while XIP is off (the flash driver).
 *          0x01C06000..0x01C08000 stays free for RAM-run debug (crt0.S writes there).
 * RAM    : .data/.bss, what the interrupts touch and small variables.
 * POOL   : big zero-initialised buffers (section .pool).
 * NOINIT : survives resets (the crash record, fm1_irq.h).
 * Stacks with 256-byte guard bands below each. 0x01C7FD50.. is reserved: boot info, the
 * boot-state mailbox and the vector table (0x01C7FE00). */
MEMORY
{
    XIP     (rx)  : ORIGIN = 0x02000120, LENGTH = 0x8DFBC
    RAMTEXT (rwx) : ORIGIN = 0x01C00000, LENGTH = 0x6000
    RAM     (rwx) : ORIGIN = 0x01C08000, LENGTH = 72K
    POOL    (rw)  : ORIGIN = 0x01C1A000, LENGTH = 0x5A000
    NOINIT  (rw)  : ORIGIN = 0x01C7C000, LENGTH = 0x3D50
}
SECTIONS
{
    .text : {
        KEEP(*(.text.crt0)) *(.text*) *(.rodata*)
        . = ALIGN(4);
        __init_array_start = .; KEEP(*(.init_array*)) __init_array_end = .;
        _etext = .;
    } > XIP
    .ram_text : { _rt_start = .; KEEP(*(.ram_text*)) . = ALIGN(4); _rt_end = .; } > RAMTEXT AT > XIP
    _rt_load = LOADADDR(.ram_text);
    .data : ALIGN(4) { _data_start = .; *(.data*) *(.sdata*) . = ALIGN(4); _data_end = .; } > RAM AT > XIP
    _data_load = LOADADDR(.data);
    .bss (NOLOAD) : { _bss_start = .; *(.bss*) *(.sbss*) *(COMMON) . = ALIGN(4); _bss_end = .; } > RAM
    .pool (NOLOAD) : { _pool_start = .; *(.pool*) . = ALIGN(4); _pool_end = .; } > POOL
    .noinit (NOLOAD) : { *(.noinit*) } > NOINIT
    _guard0 = 0x01C74000; _ustack_lo = 0x01C74100; _ustack_top = 0x01C7A000;
    _guard1 = 0x01C7A000; _sstack_lo = 0x01C7A100; _sstack_top = 0x01C7C000;
    /DISCARD/ : { *(.comment) *(.note*) }
}
/* sidou_app.bin is .text, .ram_text and .data joined (CMakeLists.txt); fm1_cstart copies the
 * last two from there. Any other section is an error: the link uses --orphan-handling=error. */
ASSERT(_start == ORIGIN(XIP), "app.ld: crt0's _start must be the image's first byte (the SPL enters there)")
ASSERT(LOADADDR(.ram_text) == ADDR(.text) + SIZEOF(.text), "app.ld: .ram_text must load right after .text")
ASSERT(LOADADDR(.data) == LOADADDR(.ram_text) + SIZEOF(.ram_text), "app.ld: .data must load right after .ram_text")
ASSERT(_data_start == ADDR(.data) && _data_end == ADDR(.data) + SIZEOF(.data), "app.ld: _data_* must bound .data")
ASSERT(_rt_end <= 0x01C06000, "app.ld: .ram_text runs into the RAM-run debug area")
/* The region lengths already keep each section inside its region; these guard the constants
 * above against an edit that moves a region or a stack into its neighbour. */
ASSERT(ORIGIN(RAM) + LENGTH(RAM) <= ORIGIN(POOL), "app.ld: RAM overlaps POOL")
ASSERT(ORIGIN(POOL) + LENGTH(POOL) <= _guard0, "app.ld: POOL overlaps the user stack's guard band")
ASSERT(_ustack_top <= _guard1 && _sstack_top <= ORIGIN(NOINIT), "app.ld: the stacks overlap")
ASSERT(ORIGIN(NOINIT) + LENGTH(NOINIT) <= 0x01C7FD50, "app.ld: NOINIT overlaps the reserved top of RAM")
