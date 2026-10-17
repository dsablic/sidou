# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# The portable live-instrument logic, front-panel mapping, register ring, MIDI parser and MIDI input,
# the MIDI clock follower, MIDI's modes (midi_route), MIDI learn, the screen font, the cell-grid renderer, the screen's views (the CHIP view among them),
# widgets and transitions, the boot sequence, the logo, the About cracktro with its credits, SCENE's
# pieces and their pixel art, the title cards, the song views' frame and the flash store. Its
# headers are included bare ("live.h"); the SID core's as "sid/sid.h".
add_library(sidou_live STATIC
    about.c
    boot.c
    bootguard.c
    chip_mix.c
    chip_ctl.c
    chip_view.c
    credits.c
    demo_song.c
    font.c
    fx_live.c
    grid.c
    learn_view.c
    keys_route.c
    live.c
    live_freq.c
    live_view.c
    master_gain.c
    logo.c
    logo_bitmap.c
    midi_clock.c
    midi_in.c
    midi_learn.c
    midi_map.c
    midi_out.c
    midi_ring.c
    midi_route.c
    midi_sync.c
    midi_rx.c
    midi_usb.c
    panel.c
    reg_ring.c
    scene.c
    scene_art.c
    scene_px.c
    scene_rmr.c
    scene_rmr_art.c
    scene_song.c
    scene_view.c
    save_key.c
    scope.c
    tail_gate.c
    screen.c
    settings.c
    sine.c
    song_view.c
    store.c
    title_card.c
    title_card_bitmap.c
    tracker_view.c
    ui_sound.c
    user_profile.c
    views.c
    widget.c)
target_include_directories(sidou_live PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
# RED MOON RITE's music hooks and its state run once a pass or a frame, not once a tile: -Os
# keeps them small in flash (M2f's code budget, docs/plans/2026-10-15-m2f-scene.md).
set_source_files_properties(scene_song.c scene_rmr.c PROPERTIES COMPILE_OPTIONS "-Os")
target_link_libraries(sidou_live PUBLIC sidou_core)
target_compile_options(sidou_live PRIVATE -Wall -Wextra -Wshadow)
if(CMAKE_CROSSCOMPILING)
    # The store runs in the main loop, never the audio path, and its polls are bounded by work
    # (STORE_POLL_WORK), not speed: built for size (about a third smaller than -O2). The
    # settings, which run a few times a second in the UI's frame, too.
    set_source_files_properties(store.c settings.c user_profile.c PROPERTIES COMPILE_OPTIONS -Os)
endif()
