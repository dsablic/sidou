# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# The portable live-instrument logic, front-panel mapping, register ring and MIDI parser. Its headers are included bare
# ("live.h"); the SID core's as "sid/sid.h".
add_library(sidou_live STATIC
    live.c
    live_freq.c
    midi_in.c
    panel.c
    reg_ring.c)
target_include_directories(sidou_live PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(sidou_live PUBLIC sidou_core)
target_compile_options(sidou_live PRIVATE -Wall -Wextra -Wshadow)
