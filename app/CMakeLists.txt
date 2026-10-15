# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# The portable live-instrument logic, front-panel mapping, register ring, MIDI parser and MIDI input,
# the screen font, the cell-grid renderer, and the screen's views, widgets and transitions. Its
# headers are included bare ("live.h"); the SID core's as "sid/sid.h".
add_library(sidou_live STATIC
    bootguard.c
    font.c
    fx_live.c
    grid.c
    live.c
    live_freq.c
    midi_in.c
    midi_map.c
    midi_out.c
    midi_ring.c
    midi_rx.c
    midi_usb.c
    panel.c
    reg_ring.c
    screen.c
    views.c
    widget.c)
target_include_directories(sidou_live PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(sidou_live PUBLIC sidou_core)
target_compile_options(sidou_live PRIVATE -Wall -Wextra -Wshadow)
