/* SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-only
 * SIDOU memory map: Felucca's firmware/app.ld, without the second core's .c1_text, and with
 * RAMTEXT at 48 KB instead of Felucca's 24 KB (RAM and the debug area moved up, POOL starts
 * higher; docs/hardware.md, "Code in SRAM").
 * XIP    : code and constants, executed in place; app.bin offset 0 = 0x02000120.
 * RAMTEXT: .ram_text, code that runs from SRAM: the audio path (no I-cache misses on flash
 *          in the audio interrupt) and code that runs while XIP is off (the flash driver).
 *          0x01C0C000..0x01C0E000 stays free for RAM-run debug (crt0.S writes there).
 * RAM    : .data/.bss, what the interrupts touch and small variables.
 * POOL   : big zero-initialised buffers (section .pool).
 * NOINIT : survives resets (the crash record, fm1_irq.h).
 * Stacks with 256-byte guard bands below each. 0x01C7FD50.. is reserved: boot info, the
 * boot-state mailbox and the vector table (0x01C7FE00). */
MEMORY
{
    XIP     (rx)  : ORIGIN = 0x02000120, LENGTH = 0x8DFBC
    RAMTEXT (rwx) : ORIGIN = 0x01C00000, LENGTH = 48K
    RAM     (rwx) : ORIGIN = 0x01C0E000, LENGTH = 72K
    POOL    (rw)  : ORIGIN = 0x01C20000, LENGTH = 0x54000
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
     * from SRAM. No direct call may cross between this code and flash code: SRAM is at the
     * edge of the pi32v2's branch reach (about 4 MB) from XIP. So a function the audio path
     * calls directly and these rules miss (a new non-inlined envelope or oscillator helper,
     * say) usually fails the link with "relocation truncated to fit" (add its section here),
     * but may link where the two happen to lie in reach. The guard is the emulator tests
     * (emu-gate tests/ram_audio.rs): they check the linker's relocations for any crossing
     * (the firmware links with --emit-relocs), and that the ALNK0 interrupt runs only from
     * here, which also covers code entered through an absolute address (the vector, a future
     * callback). Code shared with flash callers is static inline, or they call through a
     * pointer (audio.c far_drain).
     * Constant tables stay in .rodata in flash (XIP data reads go through the D-cache), except
     * those this code reads: their definitions put them in .data (RAM), and ram_audio.rs fails
     * on any address this code loads from XIP. The pulldown rows, read through a pointer, stay
     * in flash. docs/hardware.md, "Code in SRAM", has the rules. */
    .ram_text : {
        _rt_start = .;
        KEEP(*(.ram_text*))
        *(.text.isr_alnk0)                          /* hal/fm1_isr.S: the IRQ entry */
        *(.text.fm1_alnk0_irq)                      /* main.c */
        *(.text.audio_fill)                         /* audio.c */
        *(.text.reg_ring_drain .text.reg_ring_pop .text.reg_ring_chip)   /* app/reg_ring.c */
        *(.text.sid_render .text.sid_write)         /* core/sid/sid.c (sid_svf_set is inline) */
        *(.text.sid_set_gate_reset)                 /* sid.c, from reg_ring_drain */
        *(.text.sid_read .text.read_osc3 .text.clock_voices .text.fold_owed)    /* sid.c */
        *(.text.sid_osc_advance_slow .text.sid_osc_cycle .text.sid_osc_run_combined)   /* sid_osc.h */
        *(.text.sid_osc_bulk_commit .text.sid_osc_bulk_limit .text.sid_osc_quiet_step)  /* sid_osc.h, */
        *(.text.sid_osc_pull_msb)                                                /* when not inlined */
        *(.text.sid_noise_clock .text.sid_noise_fade .text.sid_noise_settle_deferred)  /* sid_osc.h */
        *(.text.sid_env_advance_slow .text.sid_env_run .text.sid_env_settle)    /* sid_env.h */
        /* The effect slots (core/fx/fx_slots.c): the render, the settings, the switch path
         * (slot_start, slot_copy; distinctive names, so no other file's function matches),
         * every algorithm tick and LFO the compiler kept out of line, and the inits slot_start
         * still runs (the cheap ones; sid_slots_init, in flash, builds the others' templates).
         * The patterns for functions the compiler inlines today stay as a safety net. The two
         * reverbs' ticks. live's way in: the drain's pseudo-register handler (app/fx_live.c). */
        *(.text.sid_slots_process .text.sid_slots_set .text.slot_start .text.step_switch .text.slot_copy)
        *(.text.dist_tick_any .text.mod_tick_any .text.fx_tick_any)
        *(.text.sid_fx_*_tick .text.sid_fx_*_lfo)
        *(.text.sid_fx_crush_init .text.sid_fx_noise_init .text.sid_fx_chorus_init)
        *(.text.sid_fx_flanger_init .text.sid_fx_tremolo_init .text.sid_fx_wow_init)
        *(.text.fx_live_pseudo .text.fx_live_apply)
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
    /* The core's shared voice DAC tables (core/sid/sid_dac.c, 17,408 bytes per chip model)
     * are big zero-initialised buffers: POOL, not RAM. The pattern depends on CMake's object
     * naming (sid_dac.c.obj); an assert below catches the tables falling back into .bss. */
    .bss (NOLOAD) : {
        _bss_start = .;
        *(EXCLUDE_FILE(*sid_dac.c.obj) .bss*) *(.sbss*) *(COMMON)
        . = ALIGN(4);
        _bss_end = .;
    } > RAM
    .pool (NOLOAD) : {
        _pool_start = .;
        *(.pool*)
        _pool_dac_start = .;
        *sid_dac.c.obj(.bss*)
        _pool_dac_end = .;
        . = ALIGN(4);
        _pool_end = .;
    } > POOL
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
ASSERT(_rt_end <= 0x01C0C000, "app.ld: .ram_text runs into the RAM-run debug area")
ASSERT(SIZEOF(.ram_text) <= 48K, "app.ld: .ram_text is over RAMTEXT's 48 KB")
ASSERT(ADDR(.ram_text) == ORIGIN(RAMTEXT), "app.ld: .ram_text must start at RAMTEXT")
ASSERT(_pool_dac_end - _pool_dac_start >= 2 * 17408, "app.ld: the DAC tables (sid_dac.c's .bss) are not in POOL: does *sid_dac.c.obj still match its object?")
/* The region lengths already keep each section inside its region; these guard the constants
 * above against an edit that moves a region or a stack into its neighbour. */
ASSERT(ORIGIN(RAM) + LENGTH(RAM) <= ORIGIN(POOL), "app.ld: RAM overlaps POOL")
ASSERT(ORIGIN(POOL) + LENGTH(POOL) <= _guard0, "app.ld: POOL overlaps the user stack's guard band")
ASSERT(_ustack_top <= _guard1 && _sstack_top <= ORIGIN(NOINIT), "app.ld: the stacks overlap")
ASSERT(ORIGIN(NOINIT) + LENGTH(NOINIT) <= 0x01C7FD50, "app.ld: NOINIT overlaps the reserved top of RAM")
