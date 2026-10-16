# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
add_library(sidou_core STATIC
    sid/sid.c
    sid/sid_character.c
    sid/sid_dac.c
    sid/sid_filter.c
    fx/fx.c
    fx/fx_slots.c
    fx/fx_reverb.c
    fx/fx_plate.c
    song/gt2_song.c
    song/gt2_sng.c
    song/song_ext.c)
target_include_directories(sidou_core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
# Float results must be identical on host and pi32v2 so their outputs can be compared.
target_compile_options(sidou_core PUBLIC -ffp-contract=off)
target_compile_options(sidou_core PRIVATE -Wall -Wextra -Wshadow)

# Combined-waveform profiles (gen/grid.h), per model. SIDOU_ALL_COMBINED_PROFILES, ON (the
# default since M1c Task 3), compiles reSIDfp's WEAK, AVERAGE and STRONG 6581 tables, 92,160
# bytes, which the chip profiles (gen/profiles.h) need: the 6581 R2 plays WEAK, the worn
# 6581 STRONG. OFF keeps AVERAGE only (30,720 bytes), and those profiles play AVERAGE.
# SIDOU_ALL_COMBINED_PROFILES_8580, OFF, does the same for the 8580, whose one profile (the
# R5) plays AVERAGE: ON would add 61,440 bytes no profile reads. The host tests build both.
option(SIDOU_ALL_COMBINED_PROFILES "Compile all three 6581 combined-waveform profiles" ON)
if(SIDOU_ALL_COMBINED_PROFILES)
    target_compile_definitions(sidou_core PUBLIC SIDOU_ALL_COMBINED_PROFILES)
endif()
option(SIDOU_ALL_COMBINED_PROFILES_8580 "Compile all three 8580 combined-waveform profiles" OFF)
if(SIDOU_ALL_COMBINED_PROFILES_8580)
    target_compile_definitions(sidou_core PUBLIC SIDOU_ALL_COMBINED_PROFILES_8580)
endif()
