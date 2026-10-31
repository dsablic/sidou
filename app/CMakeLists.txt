# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
add_library(sidou_live STATIC
    about.c
    boot.c
    bootguard.c
    chip_mix.c
    chip_spread.c
    chip_ctl.c
    chip_view.c
    credits.c
    demo_song.c
    font.c
    fx_defaults.c
    fx_live.c
    grid.c
    help_view.c
    learn_view.c
    keys_route.c
    link.c
    lab_draw.c
    lab_edit.c
    lab_song.c
    lab_sound.c
    lab_view.c
    live.c
    live_cutoff.c
    live_setup.c
    live_walk.c
    live_dice.c
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
    thump_out.c
    screen.c
    settings.c
    sid_io.c
    sid_view.c
    sine.c
    song_view.c
    song_pages.c
    song_io.c
    song_edit.c
    song_gen.c
    step_view.c
    store.c
    sysex.c
    theme.c
    title_card.c
    title_card_bitmap.c
    tracker_view.c
    ui_sound.c
    user_profile.c
    user_sound.c
    views.c
    widget.c)
target_include_directories(sidou_live PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
set_source_files_properties(scene_song.c scene_rmr.c PROPERTIES COMPILE_OPTIONS "-Os")
set_source_files_properties(lab_sound.c lab_song.c lab_edit.c user_sound.c PROPERTIES COMPILE_OPTIONS "-Os")
set_source_files_properties(link.c PROPERTIES COMPILE_OPTIONS "-Os")
target_link_libraries(sidou_live PUBLIC sidou_core)
target_compile_options(sidou_live PRIVATE -Wall -Wextra -Wshadow)
if(CMAKE_CROSSCOMPILING)
    set_source_files_properties(store.c PROPERTIES COMPILE_OPTIONS -Oz)
    set_source_files_properties(settings.c user_profile.c song_io.c sysex.c sid_io.c PROPERTIES COMPILE_OPTIONS -Os)
    set_source_files_properties(midi_route.c song_view.c PROPERTIES COMPILE_OPTIONS -Os)
    set_source_files_properties(views.c chip_view.c learn_view.c tracker_view.c sid_view.c
        PROPERTIES COMPILE_OPTIONS -Os)
    set_source_files_properties(lab_view.c lab_draw.c PROPERTIES COMPILE_OPTIONS -Oz)
    set_source_files_properties(song_pages.c step_view.c song_gen.c PROPERTIES COMPILE_OPTIONS "-Os;-fno-inline-functions;-fno-jump-tables")
    set_source_files_properties(sid_io.c song_io.c sysex.c song_view.c scene_song.c scene_rmr.c
        midi_learn.c about.c song_edit.c midi_map.c help_view.c logo.c PROPERTIES COMPILE_OPTIONS -Oz)
    set_source_files_properties(screen.c chip_ctl.c keys_route.c panel.c PROPERTIES COMPILE_OPTIONS -Os)
    set_source_files_properties(live_setup.c live_walk.c PROPERTIES COMPILE_OPTIONS -Oz)
    set_source_files_properties(scene.c PROPERTIES COMPILE_OPTIONS -Os)
    set_source_files_properties(scene_view.c PROPERTIES COMPILE_OPTIONS "-Oz")
    set_source_files_properties(scene_px.c PROPERTIES COMPILE_OPTIONS "-Oz")
    set_source_files_properties(widget.c PROPERTIES COMPILE_OPTIONS "-Oz")
    set_source_files_properties(midi_out.c PROPERTIES COMPILE_OPTIONS "-Oz")
    set_source_files_properties(song_gen.c PROPERTIES COMPILE_OPTIONS "-Oz;-fno-inline-functions;-fno-jump-tables")
    set_source_files_properties(step_view.c PROPERTIES COMPILE_OPTIONS "-Oz;-fno-inline-functions;-fno-jump-tables")
    set_source_files_properties(sid_view.c PROPERTIES COMPILE_OPTIONS "-Oz")
    set_source_files_properties(tracker_view.c PROPERTIES COMPILE_OPTIONS "-Oz")
endif()
