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
    psid/psid_lift.c
    player/gt2_play.c
    player/gt2_shape.c
    player/song_ctl.c
    cpu6502/cpu6502.c)
target_include_directories(sidou_core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
set_source_files_properties(player/gt2_play.c PROPERTIES COMPILE_OPTIONS "-Os;-fno-jump-tables")
set_source_files_properties(player/song_ctl.c PROPERTIES COMPILE_OPTIONS "-Oz;-fno-jump-tables")
set_source_files_properties(song/gt2_ins.c PROPERTIES COMPILE_OPTIONS -Os)
set_source_files_properties(song/gt2_stream.c PROPERTIES COMPILE_OPTIONS -Oz)
set_source_files_properties(song/gt2_sng.c PROPERTIES COMPILE_OPTIONS -Os)
set_source_files_properties(song/gt2_song.c PROPERTIES COMPILE_OPTIONS "-Os;-fno-jump-tables")
set_source_files_properties(song/song_ext.c PROPERTIES COMPILE_OPTIONS "-Oz;-fno-jump-tables")
set_source_files_properties(player/gt2_shape.c PROPERTIES COMPILE_OPTIONS "-Os;-fno-inline-functions;-fno-jump-tables")
set_source_files_properties(cpu6502/cpu6502.c PROPERTIES
    COMPILE_OPTIONS -Os COMPILE_DEFINITIONS CPU6502_SMALL)
set_source_files_properties(psid/psid_env.c PROPERTIES COMPILE_OPTIONS -Os)
set_source_files_properties(psid/psid_file.c PROPERTIES COMPILE_OPTIONS -Oz)
set_source_files_properties(psid/psid_ctl.c PROPERTIES COMPILE_OPTIONS -Os)
set_source_files_properties(psid/psid_ctl_isr.c PROPERTIES COMPILE_OPTIONS "-O2;-fno-jump-tables")
set_source_files_properties(psid/psid_lift.c PROPERTIES COMPILE_OPTIONS -Oz)
set_source_files_properties(sid/sid_shadow.c PROPERTIES COMPILE_OPTIONS "-Os;-fno-function-sections")
target_compile_options(sidou_core PUBLIC -ffp-contract=off)
target_compile_options(sidou_core PRIVATE -Wall -Wextra -Wshadow)

option(SIDOU_ALL_COMBINED_PROFILES "Compile all three 6581 combined-waveform profiles" ON)
if(SIDOU_ALL_COMBINED_PROFILES)
    target_compile_definitions(sidou_core PUBLIC SIDOU_ALL_COMBINED_PROFILES)
endif()
option(SIDOU_ALL_COMBINED_PROFILES_8580 "Compile all three 8580 combined-waveform profiles" ON)
if(SIDOU_ALL_COMBINED_PROFILES_8580)
    target_compile_definitions(sidou_core PUBLIC SIDOU_ALL_COMBINED_PROFILES_8580)
endif()
