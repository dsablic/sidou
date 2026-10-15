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
    fx/fx_plate.c)
target_include_directories(sidou_core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
# Float results must be identical on host and pi32v2 so their outputs can be compared.
target_compile_options(sidou_core PUBLIC -ffp-contract=off)
target_compile_options(sidou_core PRIVATE -Wall -Wextra -Wshadow)

# Combined-waveform profiles (gen/grid.h): ON (the default since M1c Task 3) compiles
# reSIDfp's WEAK, AVERAGE and STRONG, 92,160 bytes of tables per model, which the chip
# profiles (gen/profiles.h) need: the 6581 R2 plays WEAK, the worn 6581 STRONG. OFF keeps
# AVERAGE only (30,720 bytes per model), and those profiles play AVERAGE.
option(SIDOU_ALL_COMBINED_PROFILES "Compile all three combined-waveform profiles" ON)
if(SIDOU_ALL_COMBINED_PROFILES)
    target_compile_definitions(sidou_core PUBLIC SIDOU_ALL_COMBINED_PROFILES)
endif()
