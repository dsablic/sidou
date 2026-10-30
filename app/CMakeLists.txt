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
    lab_draw.c
    lab_edit.c
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
# RED MOON RITE's music hooks and its state run once a pass or a frame, not once a tile: -Os
# keeps them small in flash (M2f's code budget, https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2f-scene.md).
set_source_files_properties(scene_song.c scene_rmr.c PROPERTIES COMPILE_OPTIONS "-Os")
# LAB's sound and its undo (docs/plans/2026-10-25-lab-plan.md): checked, compared and recorded once
# a turn or a load.
set_source_files_properties(lab_sound.c lab_edit.c user_sound.c PROPERTIES COMPILE_OPTIONS "-Os")
target_link_libraries(sidou_live PUBLIC sidou_core)
target_compile_options(sidou_live PRIVATE -Wall -Wextra -Wshadow)
if(CMAKE_CROSSCOMPILING)
    # The store runs in the main loop, never the audio path, and its polls are bounded by work
    # (STORE_POLL_WORK), not speed: built for size (about a third smaller than -O2). The
    # settings, which run a few times a second in the UI's frame, too, and a tune's loading, a
    # part a pass (sid_io.c). The store itself at -Oz,
    # another 1,562 bytes (docs/plans/2026-10-24-flash-space.md): its gates bound work, not time.
    set_source_files_properties(store.c PROPERTIES COMPILE_OPTIONS -Oz)
    set_source_files_properties(settings.c user_profile.c song_io.c sysex.c sid_io.c PROPERTIES COMPILE_OPTIONS -Os)
    # Cold routing and song-view controls: each file's size build keeps every emulator gate.
    set_source_files_properties(midi_route.c song_view.c PROPERTIES COMPILE_OPTIONS -Os)
    # The views' compose and input code runs once a frame or a key in the main loop, each
    # view's worst pass tied to POLL_INSTRUCTIONS (emu-gate screen.rs): the size build keeps
    # every view's gate (the flash win-back before M3, https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-19-m3-sid-files.md).
    # Not live_view.c: LIVE's frame is the instrument's, nearest its noise-chord gate.
    set_source_files_properties(views.c chip_view.c learn_view.c tracker_view.c sid_view.c
        PROPERTIES COMPILE_OPTIONS -Os)
    # LAB's view and its drawings (docs/plans/2026-10-25-lab-plan.md): its compose, a turn and its input, once a
    # frame or a detent, inside the LAB flash stage.
    set_source_files_properties(lab_view.c lab_draw.c PROPERTIES COMPILE_OPTIONS -Oz)
    # These editors run bounded main-loop work. Keep their rendering and edit helpers outlined
    # rather than duplicating them into the page/input paths.
    set_source_files_properties(song_pages.c step_view.c song_gen.c PROPERTIES COMPILE_OPTIONS "-Os;-fno-inline-functions;-fno-jump-tables")
    # The flash win-back before 1.0 (docs/plans/2026-10-24-flash-space.md): files whose gates bound
    # work or sit well clear of their budgets, built for size. Left out on purpose, each measured:
    # live.c (the input's instruction count: ui_encoder over CHIP_PICK_INPUT 400, K1 over POPUP_INPUT
    # 250) and title_card.c (THE CHIP's worst ui_poll 79,349 against 79,000).
    # boot.c, sid_character.c and sid_dac.c stay as they were: with LAB in, at -Os they moved the
    # thump stage over its 31 a sample, the boot timeline (LIVE chip B's golden) and a tune's pause.
    set_source_files_properties(sid_io.c song_io.c sysex.c song_view.c scene_song.c scene_rmr.c
        midi_learn.c about.c song_edit.c midi_map.c help_view.c logo.c PROPERTIES COMPILE_OPTIONS -Oz)
    set_source_files_properties(screen.c chip_ctl.c keys_route.c panel.c PROPERTIES COMPILE_OPTIONS -Os)
    # live's cold half (1.1's flash win-back, docs/plans/2026-10-29-1.1-design.md "Flash"): the
    # presets, the parameters and the sound, once a turn, a CC or a load. live.c keeps the input's
    # paths the budgets bound (K1's cutoff, a slot's pick) and the tick at the default. The knob
    # walks (live_walk.c) are as cold: a detent or a CC.
    set_source_files_properties(live_setup.c live_walk.c PROPERTIES COMPILE_OPTIONS -Oz)
    # SCENE's pieces run once a frame, inside the scene budgets (emu-gate scene_budget.rs, screen.rs).
    set_source_files_properties(scene.c PROPERTIES COMPILE_OPTIONS -Os)
    # 1.5's win-back (docs/plans/2026-10-30-1.5-design.md "Flash"): files built for size one by one,
    # each kept only while its budget gate holds with room. Left out on purpose, each measured:
    # views.c (-Oz saves 880 bytes but K1's popup input reaches 250 of POPUP_INPUT 250), chip_view.c
    # (240 bytes; CHIP K1 on DIST reaches 245 of CHIP_INPUT 245), grid.c (3,360 bytes, but the tile
    # renderer's frame grows 15 %: THE CHIP's ui_poll 79,774 over 79,000), live_view.c and live.c
    # (the instrument's frame and input), reg_ring.c (the audio interrupt's pops).
    set_source_files_properties(sid_view.c PROPERTIES COMPILE_OPTIONS "-Oz")
endif()
