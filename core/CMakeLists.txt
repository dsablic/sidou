# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
add_library(sidou_core STATIC
    sid/sid.c
    sid/sid_filter.c
    fx/fx.c
    fx/fx_slots.c
    fx/fx_reverb.c
    fx/fx_plate.c)
target_include_directories(sidou_core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
# Float results must be identical on host and pi32v2 so their outputs can be compared.
target_compile_options(sidou_core PUBLIC -ffp-contract=off)
target_compile_options(sidou_core PRIVATE -Wall -Wextra -Wshadow)
