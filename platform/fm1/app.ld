/* SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-only */
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
    .entry : { KEEP(*(.text.crt0)) . = ALIGN(16); } > XIP
    .ram_text : {
        _rt_start = .;
        KEEP(*(.ram_text*))
        *(.text.isr_alnk0)
        *(.text.fm1_alnk0_irq)
        *(.text.audio_fill .text.audio_ramp)
        *(.text.chip_mix .text.mix_past)
        *(.text.tail_gate_run .text.tail_gate_also .text.tail_busy)
        *(.text.chip_spread_side .text.chip_spread_apply .text.spread_out .text.chip_spread_glide_step)
        *(.text.master_gain_apply .text.master_scaled)
        *(.text.thump_out_take .text.thump_out_drop .text.thump_out_apply .text.thump_add .text.thump_run)
        *(.text.reg_ring_drain .text.reg_ring_pop .text.reg_ring_chip .text.reg_ring_chip_of)
        *(.text.sid_render .text.sid_render_tap .text.sid_write .text.grit)
        *(.text.render_6581 .text.render_8580)
        *(.text.render_6581_bleed .text.render_8580_bleed)
        *(.text.sid_set_gate_reset)
        *(.text.sid_advance)
        *(.text.sid_read .text.read_osc3 .text.clock_voices .text.fold_owed)
        *(.text.sid_osc_advance_slow .text.sid_osc_cycle .text.sid_osc_run_combined)
        *(.text.sid_osc_bulk_commit .text.sid_osc_bulk_limit .text.sid_osc_quiet_step)
        *(.text.sid_osc_pull_msb)
        *(.text.sid_noise_clock .text.sid_noise_fade .text.sid_noise_settle_deferred)
        *(.text.sid_env_advance_slow .text.sid_env_run .text.sid_env_settle)
        *(.text.sid_slots_dry .text.slot_dry .text.slots_run .text.slots_bypass)
        *(.text.sid_slots_process_wide .text.sid_slots_process_float)
        *(.text.sid_slots_process .text.sid_slots_set .text.slot_start .text.step_switch)
        *(.text.slot_copy .text.slot_clear)
        *(.text.dist_tick_any .text.mod_tick_any .text.fx_tick_any)
        *(.text.sid_fx_*_tick .text.sid_fx_*_lfo)
        *(.text.sid_fx_limit_round)
        *(.text.fx_live_pseudo .text.fx_live_apply)
        *(.text.scope_render .text.scope_tap_point .text.scope_peaks)
        *gt2_play.c.obj(.text .text.*)
        *(.text.song_ctl_block .text.song_ctl_due .text.song_ctl_call .text.song_ctl_block_end)
        *(.text.song_ctl_chip_swapped .text.song_sink .text.song_rewrite .text.song_gate_off)
        *(.text.song_set_gate_reset .text.song_apply .text.song_voices .text.song_handover)
        *(.text.song_push_events)
        *(.text.song_lock_row .text.song_lock_apply .text.song_draft .text.song_held)
        *(.text.render_a .text.render_part .text.play_a)
        *(.text.gt2_edit_apply)
        *(.text.psid_ctl_block .text.psid_isr_chip .text.psid_isr_ramp .text.psid_isr_clear)
        *(.text.psid_swapped .text.psid_rewrite)
        . = ALIGN(16);
        _rt_end = .;
    } > RAMTEXT AT > XIP
    _rt_load = LOADADDR(.ram_text);
    .text (LOADADDR(.ram_text) + SIZEOF(.ram_text)) : {
        *(.text*) *(.rodata*)
        KEEP(*(.sidou_options))
        . = ALIGN(16);
        __init_array_start = .; KEEP(*(.init_array*)) __init_array_end = .;
        _etext = .;
    } > XIP
    .data : ALIGN(4) { _data_start = .; *(.data*) *(.sdata*) . = ALIGN(4); _data_end = .; } > RAM AT > XIP
    _data_load = LOADADDR(.data);
    .pool (NOLOAD) : {
        . = ALIGN(4096);
        __song_region = .;
        *(.pool.song_region)
        __song_region_end = .;
        *(.pool.song_noinit)
        . = ALIGN(4);
        _pool_start = .;
        *midi.c.obj(.bss.ua_ring)
        *(.pool*)
        _pool_dac_start = .;
        *sid_dac.c.obj(.bss*)
        _pool_dac_end = .;
        . = ALIGN(4);
        _pool_end = .;
    } > POOL
    .bss (NOLOAD) : {
        _bss_start = .;
        . = ALIGN(64); *(.bss.ep1tx)
        . = ALIGN(8); *(.bss.ep0buf)
        . = ALIGN(8); *(.bss.ep1rx)
        *(EXCLUDE_FILE(*sid_dac.c.obj) .bss*) *(.sbss*) *(COMMON)
        . = ALIGN(4);
        _bss_end = .;
    } > RAM
    .song_ram (NOLOAD) : ALIGN(4) { *(.song_noinit) . = ALIGN(4); } > RAM
    .noinit (NOLOAD) : { *(.noinit*) } > NOINIT
    _guard0 = 0x01C77F00; _ustack_lo = 0x01C78000; _ustack_top = 0x01C7A000;
    _guard1 = 0x01C7A000; _sstack_lo = 0x01C7A100; _sstack_top = 0x01C7C000;
    /DISCARD/ : { *(.comment) *(.note*) }
}
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
ASSERT(ORIGIN(RAMTEXT) + LENGTH(RAMTEXT) <= 0x01C0C000, "app.ld: RAMTEXT overlaps the RAM-run debug area")
ASSERT(0x01C0E000 <= ORIGIN(RAM), "app.ld: RAM overlaps the RAM-run debug area")
ASSERT(ORIGIN(RAM) + LENGTH(RAM) <= ORIGIN(POOL), "app.ld: RAM overlaps POOL")
ASSERT(ORIGIN(POOL) + LENGTH(POOL) <= _guard0, "app.ld: POOL overlaps the user stack's guard band")
ASSERT(_ustack_top <= _guard1 && _sstack_top <= ORIGIN(NOINIT), "app.ld: the stacks overlap")
ASSERT(ORIGIN(NOINIT) + LENGTH(NOINIT) <= 0x01C7FD50, "app.ld: NOINIT overlaps the reserved top of RAM")
