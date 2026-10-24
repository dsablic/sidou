/* SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-only
 * SIDOU memory map: Felucca's firmware/app.ld, without the second core's .c1_text, and with
 * RAMTEXT at 48 KB instead of Felucca's 24 KB (RAM and the debug area moved up, POOL starts
 * higher; docs/hardware.md, "Code in SRAM"), and the RAM/POOL boundary 8 KiB down from
 * Felucca's 0x01C20000 for the song's 64 KiB content region (M2c Task 6).
 * XIP    : code and constants, executed in place; app.bin offset 0 = 0x02000120.
 * RAMTEXT: .ram_text, code that runs from SRAM: the audio path (no I-cache misses on flash
 *          in the audio interrupt) and code that runs while XIP is off (the flash driver).
 *          0x01C0C000..0x01C0E000, Felucca's RAM-run debug area, stays out of RAMTEXT and
 *          unused but for crt0.S's boot words, as in Felucca's layout. A release candidate of
 *          v0.4.0 ran RAMTEXT up to RAM; nothing showed that SRAM failing (a device report
 *          blamed on it turned out to be a misread view), but nothing proved it safe on the
 *          device either, so code stays below until a device check does.
 * RAM    : .data/.bss, what the interrupts touch and small variables.
 * POOL   : big zero-initialised buffers (section .pool): first the song's content region
 *          (__song_region, 64 KiB, 4 KiB aligned: M3's 6502 memory overlays it), then the rest.
 * NOINIT : survives resets (the crash record, fm1_irq.h).
 * Stacks with 256-byte guard bands below each. The user stack is 8 KiB, three times the
 * deepest use the emulator tests measure (2,700 bytes: every test's machine checks both stacks
 * stay within half, host/crates/emu-gate/tests/common/mod.rs), and POOL ends at its guard band;
 * the system stack (the interrupts') is 7,936 bytes, the deepest 1,820.
 * 0x01C7FD50.. is reserved: boot info, the boot-state mailbox and the vector table (0x01C7FE00). */
MEMORY
{
    XIP     (rx)  : ORIGIN = 0x02000120, LENGTH = 0x8DFBC
    RAMTEXT (rwx) : ORIGIN = 0x01C00000, LENGTH = 48K
    RAM     (rwx) : ORIGIN = 0x01C0E000, LENGTH = 64K
    POOL    (rw)  : ORIGIN = 0x01C1E000, LENGTH = 0x59F00
    NOINIT  (rw)  : ORIGIN = 0x01C7C000, LENGTH = 0x3D50
}
SECTIONS
{
    /* crt0 alone first: the SPL enters the image's first byte. */
    .entry : { KEEP(*(.text.crt0)) . = ALIGN(16); } > XIP
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
        *(.text.audio_fill .text.audio_ramp)        /* audio.c */
        *(.text.chip_mix .text.mix_past)                /* app/chip_mix.c: chip B into chip A */
        *(.text.tail_gate_run .text.tail_gate_also .text.tail_busy)          /* app/tail_gate.c: the tail muted after notes */
        *(.text.chip_spread_side .text.chip_spread_apply .text.spread_out .text.chip_spread_glide_step) /* app/chip_spread.c: SPREAD */
        *(.text.master_gain_apply .text.master_scaled)         /* app/master_gain.c: MASTER's gain */
        *(.text.thump_out_take .text.thump_out_drop .text.thump_out_apply .text.thump_add .text.thump_run) /* app/thump_out.c: the chips' thump */
        *(.text.reg_ring_drain .text.reg_ring_pop .text.reg_ring_chip .text.reg_ring_chip_of)   /* app/reg_ring.c */
        *(.text.sid_render .text.sid_render_tap .text.sid_write .text.grit)   /* core/sid/sid.c (sid_svf_set is inline) */
        *(.text.render_6581 .text.render_8580)     /* sid.c, from sid_render_tap; and with */
        *(.text.render_6581_bleed .text.render_8580_bleed)  /* the bleed (sid.h) */
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
        *(.text.sid_slots_dry .text.slot_dry .text.slots_run .text.slots_bypass)
        *(.text.sid_slots_process_wide .text.sid_slots_process_float)
        *(.text.sid_slots_process .text.sid_slots_set .text.slot_start .text.step_switch .text.slot_copy)
        *(.text.dist_tick_any .text.mod_tick_any .text.fx_tick_any)
        *(.text.sid_fx_*_tick .text.sid_fx_*_lfo)
        *(.text.sid_fx_limit_round)                 /* core/fx/fx_limit.c: the soft limiter */
        *(.text.sid_fx_crush_init .text.sid_fx_noise_init .text.sid_fx_chorus_init)
        *(.text.sid_fx_flanger_init .text.sid_fx_tremolo_init .text.sid_fx_wow_init)
        *(.text.fx_live_pseudo .text.fx_live_apply)
        /* The LIVE view's tap (app/scope.c): the tapped render, its points (called through a
         * pointer from sid_render_tap: no branch check sees it) and the output's peaks. */
        *(.text.scope_render .text.scope_tap_point .text.scope_peaks)
        /* The song (M2c Task 6): the player's frame path, the whole of core/player/gt2_play.c
         * (its analysis is gt2_shape.c, in flash); song_ctl's interrupt side (its main-loop
         * side stays in flash and reaches the player through pointers); the edit it applies
         * (core/song/gt2_song.c). song_ctl_chip_swapped is called through reg_ring_chips'
         * swap hook. */
        *gt2_play.c.obj(.text .text.*)
        *(.text.song_ctl_block .text.song_ctl_due .text.song_ctl_call .text.song_ctl_block_end)
        *(.text.song_ctl_chip_swapped .text.song_sink .text.song_rewrite .text.song_gate_off)
        *(.text.song_set_gate_reset .text.song_apply .text.song_voices .text.song_handover)
        *(.text.song_push_events)
        *(.text.song_lock_row .text.song_lock_apply)
        *(.text.render_a .text.render_part)         /* audio.c: chip A split at the call */
        *(.text.gt2_edit_apply)
        . = ALIGN(16);
        _rt_end = .;
    } > RAMTEXT AT > XIP
    _rt_load = LOADADDR(.ram_text);
    /* Lookup constants need 16-byte alignment: keep padding inside the packed sections. */
    .text (LOADADDR(.ram_text) + SIZEOF(.ram_text)) : {
        *(.text*) *(.rodata*)
        KEEP(*(.sidou_options))   /* build_options.c: the record of the image's build options */
        . = ALIGN(16);
        __init_array_start = .; KEEP(*(.init_array*)) __init_array_end = .;
        _etext = .;
    } > XIP
    .data : ALIGN(4) { _data_start = .; *(.data*) *(.sdata*) . = ALIGN(4); _data_end = .; } > RAM AT > XIP
    _data_load = LOADADDR(.data);
    /* POOL's rules come before .bss's: the first rule that matches an input section places it,
     * and these take a few .bss sections out of the RAM region (the regions are separate, so
     * the order of the two statements moves nothing else). */
    .pool (NOLOAD) : {
        /* The song's region and its transport first, outside the zeroed range (_pool_start
         * on): platform/fm1/audio.c sets them up in the LCD's reset wait. */
        . = ALIGN(4096);
        __song_region = .;
        *(.pool.song_region)
        __song_region_end = .;
        *(.pool.song_noinit)
        . = ALIGN(4);
        _pool_start = .;
        /* USB audio out's ring from usb.c (midi.c, -fdata-sections), 2 KiB: zeroed with POOL,
         * as .bss would have it. (Its smaller buffers are in the compiler's merged globals,
         * which no rule can name: they stay in .bss.) */
        *midi.c.obj(.bss.ua_ring)
        *(.pool*)
        _pool_dac_start = .;
        *sid_dac.c.obj(.bss*)
        _pool_dac_end = .;
        . = ALIGN(4);
        _pool_end = .;
    } > POOL
    /* The core's shared voice DAC tables (core/sid/sid_dac.c, 17,408 bytes per chip model)
     * are big zero-initialised buffers: POOL, not RAM. The pattern depends on CMake's object
     * naming (sid_dac.c.obj); an assert below catches the tables falling back into .bss. */
    .bss (NOLOAD) : {
        _bss_start = .;
        /* usb.c's USB DMA buffers (midi.c is built with -fdata-sections): usb.c declares them
         * aligned(4), and on the FM-1 an EP1 IN buffer at 4 mod 8 sends nothing
         * (docs/hardware-results/usb-out-diagnosis.md). EP1 IN's on a 64-byte boundary, as on
         * the device test, the other two right after it on 8-byte ones, packed: RAM is tight.
         * EP4 IN's (USB audio, ep4tx) is in the compiler's merged globals, which no rule can
         * name: emu-gate tests/usb.rs checks it lands on an 8-byte boundary. */
        . = ALIGN(64); *(.bss.ep1tx)
        . = ALIGN(8); *(.bss.ep0buf)
        . = ALIGN(8); *(.bss.ep1rx)
        *(EXCLUDE_FILE(*sid_dac.c.obj) .bss*) *(.sbss*) *(COMMON)
        . = ALIGN(4);
        _bss_end = .;
    } > RAM
    /* The song transport's interrupt-side state, not zeroed (platform/fm1/audio.c). */
    .song_ram (NOLOAD) : ALIGN(4) { *(.song_noinit) . = ALIGN(4); } > RAM
    .noinit (NOLOAD) : { *(.noinit*) } > NOINIT
    _guard0 = 0x01C77F00; _ustack_lo = 0x01C78000; _ustack_top = 0x01C7A000;
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
ASSERT(_rt_end <= 0x01C0C000, "app.ld: .ram_text runs into Felucca's RAM-run debug area (0x01C0C000..0x01C0E000): code stays out of it until a device check shows it is the app's")
ASSERT(SIZEOF(.ram_text) <= 48K, "app.ld: .ram_text is over RAMTEXT's 48 KB")
ASSERT(ADDR(.ram_text) == ORIGIN(RAMTEXT), "app.ld: .ram_text must start at RAMTEXT")
ASSERT(__song_region == ORIGIN(POOL) && __song_region % 4096 == 0, "app.ld: the song's region must start POOL, 4 KiB aligned")
ASSERT(__song_region_end - __song_region == 65536, "app.ld: the song's region (audio.c song_region) must be 64 KiB")
ASSERT(_pool_dac_end - _pool_dac_start >= 2 * 17408, "app.ld: the DAC tables (sid_dac.c's .bss) are not in POOL: does *sid_dac.c.obj still match its object?")
/* The region lengths already keep each section inside its region; these guard the constants
 * above against an edit that moves a region or a stack into its neighbour. */
ASSERT(ORIGIN(RAMTEXT) + LENGTH(RAMTEXT) <= 0x01C0C000, "app.ld: RAMTEXT overlaps the RAM-run debug area")
ASSERT(0x01C0E000 <= ORIGIN(RAM), "app.ld: RAM overlaps the RAM-run debug area")
ASSERT(ORIGIN(RAM) + LENGTH(RAM) <= ORIGIN(POOL), "app.ld: RAM overlaps POOL")
ASSERT(ORIGIN(POOL) + LENGTH(POOL) <= _guard0, "app.ld: POOL overlaps the user stack's guard band")
ASSERT(_ustack_top <= _guard1 && _sstack_top <= ORIGIN(NOINIT), "app.ld: the stacks overlap")
ASSERT(ORIGIN(NOINIT) + LENGTH(NOINIT) <= 0x01C7FD50, "app.ld: NOINIT overlaps the reserved top of RAM")
