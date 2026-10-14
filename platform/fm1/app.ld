/* SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-only
 * SIDOU memory map: Felucca's firmware/app.ld, without the second core's .c1_text.
 * XIP    : code and constants, executed in place; app.bin offset 0 = 0x02000120.
 * RAMTEXT: .ram_text, code that runs from SRAM: the audio path (no I-cache misses on flash
 *          in the audio interrupt) and code that runs while XIP is off (the flash driver).
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
    /* crt0 alone first: the SPL enters the image's first byte. */
    .entry : { KEEP(*(.text.crt0)) . = ALIGN(4); } > XIP
    /* .ram_text before .text: the first rule that matches an input section places it, and
     * .text takes every other .text.*. It loads right after .entry (AT > XIP), and
     * fm1_cstart copies it to SRAM before anything calls into it.
     *
     * The audio path, one section per function (the device build uses -ffunction-sections):
     * the ALNK0 entry and its C body, the render, the register-ring drain, and what they call.
     * No attributes in the portable C: these rules are the only place that says what runs
     * from SRAM. A function the audio interrupt reaches and these rules miss runs from flash;
     * the emulator test (emu-gate tests/ram_audio.rs) then fails and names it, so a new
     * helper, for example a non-inlined envelope or oscillator function, must be added here.
     * No direct call may cross between this code and flash code: the pi32v2's call reach
     * (about 4 MB) does not span SRAM to XIP. Share code by static inline, or call through a
     * pointer (audio.c far_drain); tests/ram_audio.rs checks the disassembly.
     * Constant tables stay in .rodata in flash (XIP data reads go through the D-cache), unless
     * a measurement shows one is hot. docs/hardware.md, "Code in SRAM", has the rules. */
    .ram_text : {
        _rt_start = .;
        KEEP(*(.ram_text*))
        *(.text.isr_alnk0)                          /* hal/fm1_isr.S: the IRQ entry */
        *(.text.fm1_alnk0_irq)                      /* main.c */
        *(.text.audio_fill)                         /* audio.c */
        *(.text.reg_ring_drain .text.reg_ring_pop .text.reg_ring_chip)   /* app/reg_ring.c */
        *(.text.sid_render .text.sid_write)         /* core/sid/sid.c (sid_svf_set is inline) */
        . = ALIGN(4);
        _rt_end = .;
    } > RAMTEXT AT > XIP
    _rt_load = LOADADDR(.ram_text);
    .text : {
        *(.text*) *(.rodata*)
        . = ALIGN(4);
        __init_array_start = .; KEEP(*(.init_array*)) __init_array_end = .;
        _etext = .;
    } > XIP
    .data : ALIGN(4) { _data_start = .; *(.data*) *(.sdata*) . = ALIGN(4); _data_end = .; } > RAM AT > XIP
    _data_load = LOADADDR(.data);
    .bss (NOLOAD) : { _bss_start = .; *(.bss*) *(.sbss*) *(COMMON) . = ALIGN(4); _bss_end = .; } > RAM
    .pool (NOLOAD) : { _pool_start = .; *(.pool*) . = ALIGN(4); _pool_end = .; } > POOL
    .noinit (NOLOAD) : { *(.noinit*) } > NOINIT
    _guard0 = 0x01C74000; _ustack_lo = 0x01C74100; _ustack_top = 0x01C7A000;
    _guard1 = 0x01C7A000; _sstack_lo = 0x01C7A100; _sstack_top = 0x01C7C000;
    /DISCARD/ : { *(.comment) *(.note*) }
}
/* sidou_app.bin is .entry, .ram_text, .text and .data joined (CMakeLists.txt); fm1_cstart
 * copies .ram_text and .data from there. Any other section is an error: the link uses
 * --orphan-handling=error. */
ASSERT(_start == ORIGIN(XIP), "app.ld: crt0's _start must be the image's first byte (the SPL enters there)")
ASSERT(ADDR(.entry) == ORIGIN(XIP), "app.ld: .entry must be the image's start")
ASSERT(LOADADDR(.ram_text) == ADDR(.entry) + SIZEOF(.entry), "app.ld: .ram_text must load right after .entry")
ASSERT(ADDR(.text) == LOADADDR(.ram_text) + SIZEOF(.ram_text), "app.ld: .text must follow .ram_text's image")
ASSERT(LOADADDR(.data) == ADDR(.text) + SIZEOF(.text), "app.ld: .data must load right after .text")
ASSERT(_data_start == ADDR(.data) && _data_end == ADDR(.data) + SIZEOF(.data), "app.ld: _data_* must bound .data")
ASSERT(_rt_end <= 0x01C06000, "app.ld: .ram_text runs into the RAM-run debug area")
ASSERT(SIZEOF(.ram_text) <= 24K, "app.ld: .ram_text is over RAMTEXT's 24 KB")
ASSERT(ADDR(.ram_text) == ORIGIN(RAMTEXT), "app.ld: .ram_text must start at RAMTEXT")
/* The region lengths already keep each section inside its region; these guard the constants
 * above against an edit that moves a region or a stack into its neighbour. */
ASSERT(ORIGIN(RAM) + LENGTH(RAM) <= ORIGIN(POOL), "app.ld: RAM overlaps POOL")
ASSERT(ORIGIN(POOL) + LENGTH(POOL) <= _guard0, "app.ld: POOL overlaps the user stack's guard band")
ASSERT(_ustack_top <= _guard1 && _sstack_top <= ORIGIN(NOINIT), "app.ld: the stacks overlap")
ASSERT(ORIGIN(NOINIT) + LENGTH(NOINIT) <= 0x01C7FD50, "app.ld: NOINIT overlaps the reserved top of RAM")
