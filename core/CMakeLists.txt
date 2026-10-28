# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
add_library(sidou_core STATIC
    sid/sid.c
    sid/sid_shadow.c
    sid/sid_character.c
    sid/sid_dac.c
    sid/sid_filter.c
    fx/fx.c
    fx/fx_slots.c
    fx/fx_slots_init.c
    fx/fx_reverb.c
    fx/fx_plate.c
    fx/fx_limit.c
    song/gt2_song.c
    song/gt2_sng.c
    song/gt2_stream.c
    song/gt2_ins.c
    song/gt2_kit.c
    song/song_ext.c
    psid/psid_file.c
    psid/psid_env.c
    psid/psid_ctl.c
    psid/psid_ctl_isr.c
    player/gt2_play.c
    player/gt2_shape.c
    player/song_ctl.c
    cpu6502/cpu6502.c)
target_include_directories(sidou_core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
# The song player's frame path and song_ctl run once a frame (a call), not once a sample: -Os
# keeps their SRAM small (platform/fm1/app.ld; M2c Task 6), and song_ctl -Oz (112 bytes less
# than -Os: RAMTEXT must end below Felucca's debug area, app.ld; the player at -Oz would save
# 336 but puts a fast-forward's slice over SONG_SLICE_INSTRUCTIONS, emu-gate song.rs). No jump
# tables: a switch's table would be read from flash (.rodata) by the audio interrupt.
set_source_files_properties(player/gt2_play.c PROPERTIES COMPILE_OPTIONS "-Os;-fno-jump-tables")
set_source_files_properties(player/song_ctl.c PROPERTIES COMPILE_OPTIONS "-Oz;-fno-jump-tables")
set_source_files_properties(song/gt2_stream.c song/gt2_ins.c PROPERTIES COMPILE_OPTIONS -Os)
# The whole-file .sng reader (the boot reads its song in the LCD's reset wait) and its helpers:
# boot and main-loop work, never the interrupt.
set_source_files_properties(song/gt2_sng.c PROPERTIES COMPILE_OPTIONS -Os)
# Save/edit bookkeeping and extension lookups are main-loop work; their O(1) IRQ edit
# application stays in .ram_text unchanged.
set_source_files_properties(song/gt2_song.c song/song_ext.c PROPERTIES COMPILE_OPTIONS "-Os;-fno-jump-tables")
# Shape analysis is bounded main-loop work, outside the interrupt's player frame path; the
# size build costs more an analysis unit, which SONG_SLICE_UNITS (song_ctl.h) accounts for.
set_source_files_properties(player/gt2_shape.c PROPERTIES COMPILE_OPTIONS "-Os;-fno-inline-functions;-fno-jump-tables")
# The 6502 (M3; cpu6502.h) runs a tune's frames in the main loop, from flash: built for size,
# with the I/O page's accesses out of line (CPU6502_SMALL), about 7 % more instructions a
# 6502 cycle than -O2 with them inline and some 2,900 bytes smaller. Its switch keeps its
# jump table (flash .rodata; the 6502 never runs in the audio interrupt).
set_source_files_properties(cpu6502/cpu6502.c PROPERTIES
    COMPILE_OPTIONS -Os COMPILE_DEFINITIONS CPU6502_SMALL)
# The PSID environment (M3; psid_env.h) runs with the 6502 in the main loop, from flash, at a
# call's start and return and on the I/O page's accesses: built for size.
set_source_files_properties(psid/psid_env.c PROPERTIES COMPILE_OPTIONS -Os)
# The .sid player (M3; psid_ctl.h): its main half runs once a pass in the main loop, from
# flash, built for size; its interrupt half (psid_ctl_block) places a tune's writes in the audio
# interrupt, from SRAM (platform/fm1/app.ld, Task 9), built for speed as the render's callers
# are, with no jump tables (a table would be read from flash by the interrupt).
set_source_files_properties(psid/psid_ctl.c PROPERTIES COMPILE_OPTIONS -Os)
set_source_files_properties(psid/psid_ctl_isr.c PROPERTIES COMPILE_OPTIONS "-O2;-fno-jump-tables")
# The voice shadow (M3; sid_shadow.h) runs with the 6502 in the main loop, from flash: built for
# size, and in one section, so that none of the voice code it instantiates from the headers
# (sid_osc.h's and sid_env.h's out-of-line helpers) has a name app.ld's .ram_text patterns take
# into SRAM.
set_source_files_properties(sid/sid_shadow.c PROPERTIES COMPILE_OPTIONS "-Os;-fno-function-sections")
# The character jobs and the DAC tables run in the main loop (and the boot); built for size, they
# keep emu-gate character.rs and the goldens. Not -Oz: the thump's stage costs 32.14 a sample
# against its 31, and a filter build's slice goes over REG_RING_PREPARE_SLICE.
if(CMAKE_CROSSCOMPILING)
    set_source_files_properties(sid/sid_character.c sid/sid_dac.c fx/fx_slots_init.c PROPERTIES COMPILE_OPTIONS -Os)
endif()
# Float results must be identical on host and pi32v2 so their outputs can be compared.
target_compile_options(sidou_core PUBLIC -ffp-contract=off)
target_compile_options(sidou_core PRIVATE -Wall -Wextra -Wshadow)

# Combined-waveform profiles (gen/grid.h), per model. SIDOU_ALL_COMBINED_PROFILES, ON (the
# default since M1c Task 3), compiles reSIDfp's WEAK, AVERAGE and STRONG 6581 tables, 92,160
# bytes, which the chip profiles (gen/profiles.h) need: the 6581 R2 plays WEAK, the worn
# 6581 STRONG. OFF keeps AVERAGE only (30,720 bytes), and those profiles play AVERAGE.
# SIDOU_ALL_COMBINED_PROFILES_8580 is also ON: the expert layer selects all three sampled
# strengths on either model. gen/pulldown.h stores them as shared, packed blocks.
option(SIDOU_ALL_COMBINED_PROFILES "Compile all three 6581 combined-waveform profiles" ON)
if(SIDOU_ALL_COMBINED_PROFILES)
    target_compile_definitions(sidou_core PUBLIC SIDOU_ALL_COMBINED_PROFILES)
endif()
option(SIDOU_ALL_COMBINED_PROFILES_8580 "Compile all three 8580 combined-waveform profiles" ON)
if(SIDOU_ALL_COMBINED_PROFILES_8580)
    target_compile_definitions(sidou_core PUBLIC SIDOU_ALL_COMBINED_PROFILES_8580)
endif()
