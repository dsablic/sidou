// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A main-loop editor over the real command ring, checked against independent row shifts.

use std::ffi::c_void;

use sidou_sys::*;
use sidou_tests::song::Song;
use sidou_tests::transport::Transport;

struct Binding {
    ctl: *mut song_ctl,
    refuse: bool,
    guard: i32,
    submitted: Vec<u8>,
}

unsafe extern "C" fn submit(ctx: *mut c_void, edit: *const gt2_edit, ticket: *mut u32) -> i32 {
    let binding = unsafe { &mut *ctx.cast::<Binding>() };
    if binding.refuse || unsafe { song_ctl_edit(binding.ctl, edit) } == 0 {
        return 0;
    }
    binding.submitted.push(unsafe { (*edit).kind });
    unsafe { *ticket = (*binding.ctl).edits_submitted };
    1
}

unsafe extern "C" fn completed(ctx: *mut c_void, ticket: u32) -> i32 {
    let binding = unsafe { &*ctx.cast::<Binding>() };
    i32::from(unsafe { (*binding.ctl).edits_completed }.wrapping_sub(ticket) as i32 >= 0)
}

unsafe extern "C" fn guard(ctx: *mut c_void, _: *const song_edit_request) -> i32 {
    unsafe { (*ctx.cast::<Binding>()).guard }
}

unsafe extern "C" fn submit_with_locks(
    ctx: *mut c_void,
    edit: *const gt2_edit,
    locks: *const u8,
    n: u32,
    ticket: *mut u32,
) -> i32 {
    let binding = unsafe { &mut *ctx.cast::<Binding>() };
    if binding.refuse || unsafe { song_ctl_edit_with_locks(binding.ctl, edit, locks, n) } == 0 {
        return 0;
    }
    binding.submitted.push(unsafe { (*edit).kind });
    unsafe { *ticket = (*binding.ctl).edits_submitted };
    1
}

struct Engine {
    song: Song,
    player: Transport,
    binding: Box<Binding>,
    _transport: Box<song_transport>,
    editor: song_editor,
}

impl Engine {
    fn new() -> Self {
        Self::with_song(Song::device())
    }

    fn with_song(mut song: Song) -> Self {
        let mut player = Transport::new(&mut song);
        let mut binding = Box::new(Binding {
            ctl: &mut *player.sc,
            refuse: false,
            guard: SONG_EDIT_GUARD_OK as i32,
            submitted: Vec::new(),
        });
        let transport = Box::new(song_transport {
            song: song.ptr(),
            ctx: (&mut *binding as *mut Binding).cast(),
            edit: Some(submit),
            edit_done: Some(completed),
            edit_guard: Some(guard),
            edit_with_locks: Some(submit_with_locks),
            ..Default::default()
        });
        let mut editor = song_editor::default();
        unsafe { song_edit_init(&mut editor, &*transport) };
        Self {
            song,
            player,
            binding,
            _transport: transport,
            editor,
        }
    }

    fn finish(&mut self) {
        for _ in 0..10_000 {
            if self.editor.pending == 0 {
                return;
            }
            self.player.pass();
            self.player.block_silent();
            self.player.pass();
            unsafe { song_edit_poll(&mut self.editor) };
        }
        panic!("editor did not complete: status {}", self.editor.status);
    }

    fn rows(&self) -> Vec<[u8; 4]> {
        let n = unsafe { gt2_pattern_rows(self.song.ptr(), 0) } as usize;
        let data =
            unsafe { std::slice::from_raw_parts(gt2_pattern_data(self.song.ptr(), 0), n * 4) };
        data.as_chunks::<4>().0.to_vec()
    }

    fn undo(&mut self) {
        assert_ne!(unsafe { song_edit_undo(&mut self.editor) }, 0);
        self.finish();
    }

    fn locks(&self) -> Vec<(u8, u8, u8, u16)> {
        let mut locks = vec![song_lock::default(); SONG_LOCKS_MAX as usize];
        let n = unsafe { song_ext_get_locks(self.song.ptr(), locks.as_mut_ptr(), SONG_LOCKS_MAX) };
        locks[..n as usize]
            .iter()
            .map(|lock| (lock.pattern, lock.row, lock.param, lock.value))
            .collect()
    }
}

fn commit(song: &mut Song, make: impl FnOnce(*mut gt2_song, *mut gt2_edit) -> gt2_err) {
    let mut edit = gt2_edit::default();
    assert_eq!(make(song.ptr_mut(), &mut edit), GT2_OK);
    unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
}

#[test]
fn rejected_submission_is_retained_and_history_waits_for_release_not_just_the_isr_ack() {
    let mut engine = Engine::new();
    let original = engine.rows();
    let cell = [GT2_FIRSTNOTE as u8 + 48, 1, 0, 0];
    engine.binding.refuse = true;
    assert_ne!(
        unsafe { song_edit_row(&mut engine.editor, 0, 2, cell.as_ptr()) },
        0
    );
    let pending = engine.song.raw().pending;
    for _ in 0..4 {
        unsafe { song_edit_poll(&mut engine.editor) };
        assert_eq!(engine.song.raw().pending, pending, "do not prepare twice");
    }
    assert_eq!(engine.rows(), original);
    assert_eq!(engine.editor.can_undo, 0);
    let mut preview = [0; 4];
    assert_ne!(
        unsafe { song_edit_read_row(&engine.editor, 0, 2, preview.as_mut_ptr()) },
        0
    );
    assert_eq!(preview, cell);
    engine.binding.refuse = false;
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_eq!(engine.rows(), original, "even idle edits wait for the ISR");
    unsafe { song_ctl_commit(&mut *engine.player.sc) };
    engine.player.block_silent();
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_eq!(
        engine.editor.revision, 0,
        "an ISR ack is not a released edit"
    );
    assert_eq!(engine.editor.can_undo, 0);
    engine.finish();
    let mut expected = original.clone();
    expected[2] = cell;
    assert_eq!(engine.rows(), expected);
    assert_eq!(engine.editor.revision, 1);
    engine.undo();
    assert_eq!(engine.rows(), original);
    engine.undo();
    assert_eq!(engine.rows(), expected);
    assert_eq!(engine.player.sc.edit_cells, 3);
    assert_eq!(engine.player.sc.edit_swaps, 0);
}

#[test]
fn fixed_length_shifts_and_both_history_directions_restore_lost_rows_and_all_their_locks() {
    for rows in [1, 2, GT2_DEFAULT_ROWS, GT2_MAX_ROWS] {
        for cursor in [0, rows - 1] {
            for insert in [false, true] {
                let mut engine = Engine::new();
                commit(&mut engine.song, |s, e| unsafe {
                    gt2_pattern_resize(s, 0, rows, e)
                });
                for row in 0..rows {
                    let cell = [GT2_FIRSTNOTE as u8 + (row % 60) as u8, 1, 0, row as u8];
                    commit(&mut engine.song, |s, e| unsafe {
                        gt2_set_row(s, 0, row, cell.as_ptr(), e)
                    });
                }
                let lost = if insert { rows - 1 } else { cursor };
                for param in 0..SONG_LOCK_PARAMS {
                    assert_eq!(
                        unsafe {
                            song_lock_put(
                                engine.song.ptr_mut(),
                                0,
                                lost,
                                param,
                                song_lock_max(param),
                            )
                        },
                        0
                    );
                }
                let before = engine.rows();
                let locks = engine.locks();
                let mut after = before.clone();
                if insert {
                    after[cursor as usize..rows as usize].rotate_right(1);
                    after[cursor as usize] = [GT2_REST as u8, 0, 0, 0];
                } else {
                    after[cursor as usize..rows as usize].rotate_left(1);
                    after[rows as usize - 1] = [GT2_REST as u8, 0, 0, 0];
                }
                assert_ne!(
                    unsafe { song_edit_shift(&mut engine.editor, 0, cursor, i32::from(insert)) },
                    0
                );
                engine.finish();
                assert_eq!(engine.rows(), after);
                assert!(engine.locks().is_empty());
                engine.undo();
                assert_eq!(
                    engine.rows(),
                    before,
                    "rows={rows}, cursor={cursor}, insert={insert}"
                );
                assert_eq!(engine.locks(), locks);
                engine.undo();
                assert_eq!(engine.rows(), after);
                assert!(engine.locks().is_empty());
                engine.undo();
                assert_eq!(engine.rows(), before);
                assert!(engine
                    .binding
                    .submitted
                    .iter()
                    .all(|&kind| kind == GT2_EDIT_SWAP as u8));
                assert_eq!(engine.player.sc.edit_swaps, 4);
            }
        }
    }
}

#[test]
fn saving_full_invalid_and_missing_hooks_refuse_without_model_changes_or_lost_history() {
    let mut engine = Engine::new();
    let cell = [GT2_KEYOFF as u8, 0, 0, 0];
    assert_ne!(
        unsafe { song_edit_row(&mut engine.editor, 0, 0, cell.as_ptr()) },
        0
    );
    engine.finish();
    for (guard, status) in [
        (SONG_EDIT_GUARD_SAVING, SONG_EDIT_SAVING),
        (SONG_EDIT_GUARD_FULL, SONG_EDIT_FULL),
    ] {
        engine.binding.guard = guard as i32;
        let before = engine.song.bytes().to_vec();
        assert_eq!(unsafe { song_edit_shift(&mut engine.editor, 0, 0, 1) }, 0);
        assert_eq!(engine.editor.status, status as u8);
        assert_eq!(engine.song.bytes(), before);
        assert_ne!(engine.editor.can_undo, 0);
    }
    engine.binding.guard = SONG_EDIT_GUARD_OK as i32;
    let before = engine.song.bytes().to_vec();
    assert_eq!(
        unsafe { song_edit_row(&mut engine.editor, 0, u32::MAX, cell.as_ptr()) },
        0
    );
    assert_eq!(
        unsafe { song_edit_shift(&mut engine.editor, GT2_MAX_PATTERNS, 0, 0) },
        0
    );
    assert_eq!(engine.song.bytes(), before);
    engine.undo();
    assert_eq!(engine.rows()[0][0], GT2_REST as u8);
    let transport = song_transport {
        song: engine.song.ptr(),
        ..Default::default()
    };
    let mut unbound = song_editor::default();
    unsafe { song_edit_init(&mut unbound, &transport) };
    let before = engine.song.bytes().to_vec();
    assert_eq!(unsafe { song_edit_shift(&mut unbound, 0, 0, 1) }, 0);
    assert_eq!(unbound.status, SONG_EDIT_UNAVAILABLE as u8);
    assert_eq!(engine.song.bytes(), before);
}

#[test]
fn a_backpressured_swap_keeps_its_private_copy_and_busy_edits_do_not_replace_it() {
    let mut engine = Engine::new();
    engine.binding.refuse = true;
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 0, 0, 1) }, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    let allocated = engine.song.raw().used_units;
    let span = engine.editor.edit.span;
    for _ in 0..8 {
        unsafe { song_edit_poll(&mut engine.editor) };
        assert_eq!(engine.song.raw().used_units, allocated);
        assert_eq!(engine.editor.edit.span, span);
        assert_eq!(unsafe { song_edit_shift(&mut engine.editor, 0, 1, 0) }, 0);
    }
    engine.binding.refuse = false;
    engine.finish();
    assert_eq!(engine.binding.submitted, vec![GT2_EDIT_SWAP as u8]);
    assert_eq!(engine.song.raw().pending, 0);
    assert!(engine.song.raw().used_units < allocated);
}

#[test]
fn a_fragmented_rebuild_compacts_one_acknowledged_swap_at_a_time_then_retries() {
    let mut song = Song::new(unsafe { gt2_region_min() });
    commit(&mut song, |s, e| unsafe {
        gt2_pattern_resize(s, 0, GT2_MAX_ROWS, e)
    });
    loop {
        let mut edit = gt2_edit::default();
        let mut pattern = 0;
        let result = unsafe { gt2_pattern_add(song.ptr_mut(), 2, &mut pattern, &mut edit) };
        if result != GT2_OK {
            assert_eq!(result, GT2_ERR_FULL);
            break;
        }
        unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
    }
    commit(&mut song, |s, e| unsafe {
        gt2_pattern_resize(s, 1, GT2_DEFAULT_ROWS, e)
    });
    assert!(unsafe { gt2_arena_largest_hole(song.ptr()) } < GT2_PATTERN_MAX_BYTES);
    let mut engine = Engine::with_song(song);
    let before = engine.rows();
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 0, 0, 1) }, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_eq!(engine.editor.status, SONG_EDIT_TIDYING as u8);
    assert_eq!(engine.editor.can_undo, 0);
    engine.finish();
    assert_eq!(engine.rows(), before);
    assert!(
        engine.binding.submitted.len() > 1,
        "compaction must precede the user's swap"
    );
    assert!(engine
        .binding
        .submitted
        .iter()
        .all(|&kind| kind == GT2_EDIT_SWAP as u8));
    assert_eq!(engine.editor.revision, 1, "compaction is not an undo step");
    assert_eq!(engine.song.raw().pending, 0);
    assert!(engine.player.sc.edits_block_max <= SONG_EDITS_PER_BLOCK);
}

#[test]
fn an_edit_while_playing_matches_the_expected_model_from_the_same_player_state() {
    let mut song = Song::device();
    song.read(&sidou_tests::song_embed::sng(), SONG_MULTISPEED_MAX as u8)
        .unwrap();
    let mut expected_song = Song::device();
    expected_song.copy_from(&song);
    let mut engine = Engine::with_song(song);
    let mut expected = Transport::new(&mut expected_song);
    assert!(engine.player.play(0, 0));
    assert!(expected.play(0, 0));
    for _ in 0..2000 {
        engine.player.pass();
        expected.pass();
        let mut actual_audio = [0i16; SONG_BLOCK as usize];
        let mut expected_audio = actual_audio;
        engine.player.block(&mut actual_audio);
        expected.block(&mut expected_audio);
        assert_eq!(actual_audio, expected_audio);
        if engine.player.calls() >= 12 && engine.player.sc.spare_wait == 0 {
            break;
        }
    }
    assert!(engine.player.calls() >= 12);
    let cell = [GT2_KEYOFF as u8, 0, 0, 0];
    assert_ne!(
        unsafe { song_edit_row(&mut engine.editor, 0, 8, cell.as_ptr()) },
        0
    );
    assert_ne!(
        engine.editor.sent, 0,
        "this edit only narrows the existing shape"
    );
    commit(&mut expected_song, |s, e| unsafe {
        gt2_set_row(s, 0, 8, cell.as_ptr(), e)
    });
    for _ in 0..500 {
        engine.player.pass();
        expected.pass();
        unsafe { song_edit_poll(&mut engine.editor) };
        let mut actual_audio = [0i16; SONG_BLOCK as usize];
        let mut expected_audio = actual_audio;
        engine.player.block(&mut actual_audio);
        expected.block(&mut expected_audio);
        assert_eq!(actual_audio, expected_audio);
        assert_eq!(engine.player.sc.shadow, expected.sc.shadow);
    }
    assert_eq!(engine.editor.pending, 0);
    assert_eq!(engine.player.sc.edit_cells, 1);
}

#[test]
fn a_shift_publishes_its_post_edit_lock_cache_in_the_same_block_as_the_pattern_swap() {
    let mut song = Song::device();
    song.read(&sidou_tests::song_embed::sng(), SONG_MULTISPEED_MAX as u8)
        .unwrap();
    assert_eq!(
        unsafe { song_lock_put(song.ptr_mut(), 4, 1, SONG_LOCK_CUTOFF, 1234) },
        0
    );
    let mut engine = Engine::with_song(song);
    assert!(engine.player.play(0, 0));
    assert!(engine
        .player
        .run_until(2000, |player| player.sc.row_seq[0] != 0
            && player.sc.row_pat[0] == 4
            && player.sc.row_row[0] == 0));
    engine.player.pass();
    let cache = &engine.player.sc.lock_next[0][engine.player.sc.lock_pub[0] as usize];
    assert!(cache.e[..cache.n as usize].iter().any(|lock| lock.row == 1));
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 4, 1, 1) }, 0);
    assert_eq!(
        engine.editor.prepared, 0,
        "input retains a rebuild without allocating"
    );
    assert_eq!(engine.editor.sent, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_ne!(engine.editor.sent, 0);
    unsafe { song_ctl_commit(&mut *engine.player.sc) };
    engine.player.block_silent();
    // Check before the main loop gets to release the swap: the ISR may start row 1 now.
    let cache = &engine.player.sc.lock_next[0][engine.player.sc.lock_pub[0] as usize];
    assert!(
        !cache.e[..cache.n as usize].iter().any(|lock| lock.row == 1),
        "the empty inserted row must never see the old row 1 lock"
    );
    engine.finish();
    let mut seen = [false; 2];
    for _ in 0..3000 {
        engine.player.pass();
        let calls = engine.player.calls();
        engine.player.block_silent();
        if calls == engine.player.calls() || engine.player.sc.row_pat[0] != 4 {
            continue;
        }
        match engine.player.sc.row_row[0] {
            1 => {
                assert_ne!(engine.player.chip.fc, 1234);
                seen[0] = true;
            }
            2 => {
                assert_eq!(engine.player.chip.fc, 1234);
                seen[1] = true;
                break;
            }
            _ => {}
        }
    }
    assert_eq!(seen, [true, true]);
}

#[test]
fn undo_restores_discarded_locks_to_the_isr_cache_before_the_release_pass() {
    let mut song = Song::device();
    song.read(&sidou_tests::song_embed::sng(), SONG_MULTISPEED_MAX as u8)
        .unwrap();
    assert_eq!(
        unsafe { song_lock_put(song.ptr_mut(), 4, 1, SONG_LOCK_CUTOFF, 1234) },
        0
    );
    let mut engine = Engine::with_song(song);
    assert!(engine.player.play(0, 0));
    assert!(engine
        .player
        .run_until(2000, |player| player.sc.row_seq[0] != 0
            && player.sc.row_pat[0] == 4
            && player.sc.row_row[0] == 0));
    engine.player.pass();
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 4, 1, 0) }, 0);
    engine.finish();
    assert!(engine.locks().is_empty());
    assert_ne!(unsafe { song_edit_undo(&mut engine.editor) }, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_ne!(engine.editor.sent, 0);
    unsafe { song_ctl_commit(&mut *engine.player.sc) };
    engine.player.block_silent();
    assert!(
        engine.locks().is_empty(),
        "the extension move is still main-loop work"
    );
    let cache = &engine.player.sc.lock_next[0][engine.player.sc.lock_pub[0] as usize];
    assert!(cache.e[..cache.n as usize]
        .iter()
        .any(|lock| lock.pattern == 4
            && lock.row == 1
            && lock.param == SONG_LOCK_CUTOFF as u8
            && u16::from_le_bytes([lock.lo, lock.hi]) == 1234));
    engine.finish();
    assert_eq!(engine.locks(), [(4, 1, SONG_LOCK_CUTOFF as u8, 1234)]);
}

#[test]
fn full_cancels_an_unsubmitted_copy_but_cannot_revoke_an_accepted_edit() {
    let mut engine = Engine::new();
    let allocated = engine.song.raw().used_units;
    engine.binding.refuse = true;
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 0, 0, 1) }, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_ne!(engine.editor.prepared, 0);
    engine.binding.guard = SONG_EDIT_GUARD_SAVING as i32;
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_ne!(engine.editor.pending, 0, "saving holds an unsubmitted edit");
    assert!(engine.binding.submitted.is_empty());
    engine.binding.guard = SONG_EDIT_GUARD_FULL as i32;
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_eq!(engine.editor.pending, 0);
    assert_eq!(engine.editor.prepared, 0);
    assert_eq!(engine.editor.status, SONG_EDIT_FULL as u8);
    assert_eq!(engine.song.raw().used_units, allocated);
    assert_eq!(engine.song.raw().pending, 0);
    assert_eq!(engine.editor.can_undo, 0);

    engine.binding.refuse = false;
    engine.binding.guard = SONG_EDIT_GUARD_OK as i32;
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 0, 0, 1) }, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_ne!(engine.editor.sent, 0);
    engine.binding.guard = SONG_EDIT_GUARD_FULL as i32;
    engine.finish();
    assert_eq!(engine.editor.status, SONG_EDIT_READY as u8);
    assert_eq!(engine.editor.revision, 1);
    assert_ne!(engine.editor.can_undo, 0);
    engine.binding.guard = SONG_EDIT_GUARD_OK as i32;
    engine.undo();
}

#[test]
fn a_wrapped_row_event_counter_does_not_masquerade_as_an_unstarted_channel() {
    let mut song = Song::device();
    song.read(&sidou_tests::song_embed::sng(), SONG_MULTISPEED_MAX as u8)
        .unwrap();
    assert_eq!(
        unsafe { song_lock_put(song.ptr_mut(), 4, 3, SONG_LOCK_CUTOFF, 1234) },
        0
    );
    let mut engine = Engine::with_song(song);
    assert!(engine.player.play(0, 0));
    assert!(engine
        .player
        .run_until(2000, |player| player.sc.row_pat[0] == 4
            && player.sc.row_row[0] == 1));
    engine.player.pass();
    engine.player.sc.row_seq[0] = 0;
    engine.player.sc.row_seen[0] = 0;
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 4, 2, 0) }, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_ne!(engine.editor.sent, 0);
    unsafe { song_ctl_commit(&mut *engine.player.sc) };
    engine.player.block_silent();
    let cache = &engine.player.sc.lock_next[0][engine.player.sc.lock_pub[0] as usize];
    assert!(cache.e[..cache.n as usize]
        .iter()
        .any(|lock| lock.pattern == 4
            && lock.row == 2
            && lock.param == SONG_LOCK_CUTOFF as u8
            && u16::from_le_bytes([lock.lo, lock.hi]) == 1234));
    engine.finish();
}

#[test]
fn a_row_advanced_before_command_visibility_rebuilds_the_reserved_cache_before_applying() {
    let mut song = Song::device();
    song.read(&sidou_tests::song_embed::sng(), SONG_MULTISPEED_MAX as u8)
        .unwrap();
    assert_eq!(
        unsafe { song_lock_put(song.ptr_mut(), 4, 3, SONG_LOCK_CUTOFF, 1234) },
        0
    );
    let mut engine = Engine::with_song(song);
    assert!(engine.player.play(0, 0));
    assert!(engine
        .player
        .run_until(2000, |player| player.sc.row_seq[0] != 0
            && player.sc.row_pat[0] == 4
            && player.sc.row_row[0] == 0));
    engine.player.pass();
    let old_span = engine.song.raw().pattern[4];
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 4, 2, 0) }, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_ne!(engine.editor.sent, 0);
    // An interrupt just before the command's commit started the next row.
    engine.player.sc.row_row[0] = 1;
    engine.player.sc.row_seq[0] = engine.player.sc.row_seq[0].wrapping_add(1);
    unsafe {
        song_ctl_commit(&mut *engine.player.sc);
        song_ctl_block(&mut *engine.player.sc, &mut *engine.player.chip);
    }
    assert_eq!(
        engine.song.raw().pattern[4],
        old_span,
        "the edit waits for a cache resolved from the new position"
    );
    engine.player.pass();
    unsafe { song_ctl_block(&mut *engine.player.sc, &mut *engine.player.chip) };
    assert_ne!(engine.song.raw().pattern[4], old_span);
    let cache = &engine.player.sc.lock_next[0][engine.player.sc.lock_pub[0] as usize];
    assert!(cache.e[..cache.n as usize]
        .iter()
        .any(|lock| lock.pattern == 4
            && lock.row == 2
            && lock.param == SONG_LOCK_CUTOFF as u8
            && u16::from_le_bytes([lock.lo, lock.hi]) == 1234));
    engine.finish();
}

#[test]
fn a_reserved_edit_cache_does_not_delay_another_channels_character_row_locks() {
    unsafe extern "C" fn character(ctx: *mut c_void, field: u32, value: i32) {
        unsafe { &mut *ctx.cast::<Vec<(u32, i32)>>() }.push((field, value));
    }

    let mut song = Song::device();
    song.read(&sidou_tests::song_embed::sng(), SONG_MULTISPEED_MAX as u8)
        .unwrap();
    assert_eq!(
        unsafe { song_lock_put(song.ptr_mut(), 0, 1, SONG_LOCK_AGE, 91) },
        0
    );
    let mut engine = Engine::with_song(song);
    let mut told = Vec::<(u32, i32)>::new();
    let hooks = song_chip_hooks {
        chip_a_character: Some(character),
        ctx: (&mut told as *mut Vec<(u32, i32)>).cast(),
        ..Default::default()
    };
    unsafe { song_ctl_set_hooks(&mut *engine.player.sc, &hooks) };
    assert!(engine.player.play(0, 0));
    assert!(engine
        .player
        .run_until(2000, |player| player.sc.row_seq[0] != 0
            && player.sc.row_pat[0] == 4
            && player.sc.row_row[0] == 0
            && player.sc.row_pat[1] == 0
            && player.sc.row_row[1] == 0));
    engine.player.pass();
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 4, 2, 1) }, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_ne!(engine.editor.sent, 0);

    // The unrelated channel starts its locked row before the edit becomes visible.
    engine.player.sc.row_row[1] = 1;
    engine.player.sc.row_seq[1] = engine.player.sc.row_seq[1].wrapping_add(1);
    unsafe {
        song_ctl_commit(&mut *engine.player.sc);
        song_ctl_block(&mut *engine.player.sc, &mut *engine.player.chip);
    }
    assert_ne!(engine.player.sc.lock_edit_refresh, 0);
    engine.player.pass();
    assert_ne!(
        engine.player.sc.lock_edit_seq, 0,
        "the shift still awaits the ISR"
    );
    assert_eq!(
        told,
        [(0, 91)],
        "the character lock must reach the hook this pass"
    );

    engine.player.sc.row_row[1] = 2;
    engine.player.sc.row_seq[1] = engine.player.sc.row_seq[1].wrapping_add(1);
    engine.player.pass();
    assert_eq!(
        told,
        [(0, 91), (0, -1)],
        "release must not wait for the shift either"
    );
    engine.finish();
}

fn cap_fixture(instrument: u8) -> Song {
    let mut spec = sidou_tests::song::SngSpec::new();
    spec.orders = vec![vec![(vec![207; if instrument == 0 { 100 } else { 80 }], 0); 3]; 32];
    spec.patterns = (0..208)
        .map(|_| {
            (
                vec![[GT2_REST as u8, 0, 0, 0]; 64],
                [GT2_ENDPATT as u8, 0, 0, 0],
            )
        })
        .collect();
    spec.patterns[207].0[0] = [GT2_FIRSTNOTE as u8, 1, 0, 0];
    if instrument != 0 {
        spec.patterns[207].0[63] = [GT2_FIRSTNOTE as u8, instrument, 0, 0];
    }
    let mut song = Song::device();
    song.read(&spec.encode(), SONG_MULTISPEED_MAX as u8)
        .unwrap();
    song
}

fn pad_to_cap(song: &mut Song, slack: u32) {
    let tag = sidou_tests::song::tag(b"CAPT");
    unsafe { song_ext_remove(song.ptr_mut(), tag) };
    let before = unsafe { gt2_saved_size(song.ptr()) };
    let trailer = unsafe { gt2_saved_trailer_size(song.raw().ext_used) };
    let size = (1..SONG_EXT_BYTES - song.raw().ext_used - SONG_CHUNK_HEADER)
        .find(|&n| {
            before - trailer
                + unsafe { gt2_saved_trailer_size(song.raw().ext_used + SONG_CHUNK_HEADER + n) }
                == GT2_SAVED_MAX - slack
        })
        .expect("padding reaches requested canonical length");
    assert_eq!(song.ext_set(tag, &vec![1; size as usize]), 0);
    assert_eq!(song.write(true).len(), (GT2_SAVED_MAX - slack) as usize);
    assert_eq!(unsafe { gt2_saved_size(song.ptr()) }, GT2_SAVED_MAX - slack);
}

#[test]
fn authoritative_model_capacity_and_freeze_errors_are_shown_without_a_guard_approximation() {
    let mut song = cap_fixture(0);
    pad_to_cap(&mut song, 0);
    let mut engine = Engine::with_song(song);
    let before = engine.song.bytes().to_vec();
    let row = [GT2_FIRSTNOTE as u8, 63, 0, 0];
    assert_eq!(
        unsafe { song_edit_row(&mut engine.editor, 207, 1, row.as_ptr()) },
        0
    );
    assert_eq!(engine.editor.error, GT2_ERR_SONG_FULL);
    assert_eq!(engine.editor.status, SONG_EDIT_FULL as u8);
    assert_eq!(engine.song.bytes(), before);
    unsafe { (*engine.song.ptr_mut()).frozen = 1 };
    assert_ne!(
        unsafe { song_edit_row(&mut engine.editor, 207, 1, row.as_ptr()) },
        0
    );
    assert_eq!(engine.editor.status, SONG_EDIT_SAVING as u8);
    assert_eq!(engine.editor.prepared, 0);
    unsafe { (*engine.song.ptr_mut()).frozen = 0 };
    engine.finish();
    assert_eq!(engine.editor.status, SONG_EDIT_FULL as u8);
}

#[test]
fn a_prepared_edit_drains_after_save_freezes_the_model() {
    let mut engine = Engine::new();
    engine.binding.refuse = true;
    let row = [GT2_FIRSTNOTE as u8, 1, 0, 0];
    assert_ne!(
        unsafe { song_edit_row(&mut engine.editor, 0, 0, row.as_ptr()) },
        0
    );
    assert_eq!(engine.song.raw().pending, 1);
    let mut storage = crate::store::fresh(81, 16);
    storage.maintain();
    let mut io = Box::<song_io>::default();
    let mut stream = Box::<gt2_stream>::default();
    unsafe {
        song_io_init(
            &mut *io,
            &mut *engine.player.sc,
            &mut *storage.st,
            std::ptr::null_mut(),
            (&mut *stream as *mut gt2_stream).cast(),
            std::ptr::null(),
        );
        assert_eq!(song_io_save(&mut *io, c"DRAIN".as_ptr(), 0), 1);
        song_io_poll(&mut *io, 1);
    }
    assert_eq!(io.state, SONG_IO_SAVE_WAIT as u8);
    engine.binding.guard = SONG_EDIT_GUARD_SAVING as i32;
    engine.binding.refuse = false;
    unsafe {
        (*engine.song.ptr_mut()).frozen = 1;
        song_edit_poll(&mut engine.editor);
    }
    assert_ne!(
        engine.editor.sent, 0,
        "SAVE_WAIT must be able to drain model.pending"
    );
    engine.finish();
    assert_eq!(engine.song.raw().pending, 0);
    assert_eq!(engine.rows()[0], row);
    assert_eq!(engine.song.raw().frozen, 1);
    assert_eq!(engine.song.raw().saved_growth, 0);
    unsafe { song_io_poll(&mut *io, 2) };
    assert_eq!(
        io.state, SONG_IO_WRITE as u8,
        "SAVE_WAIT is no longer waiting on the editor"
    );
}

#[test]
fn shifted_undo_preflights_the_restored_instrument_and_lock_trailer_before_allocating() {
    for (insert, restore_locks) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut song = cap_fixture(if restore_locks { 0 } else { 63 });
        if restore_locks {
            for param in 0..SONG_LOCK_PARAMS {
                assert_eq!(
                    unsafe { song_lock_put(song.ptr_mut(), 207, 63, param, song_lock_max(param)) },
                    0
                );
            }
        }
        let mut engine = Engine::with_song(song);
        assert_ne!(
            unsafe {
                song_edit_shift(
                    &mut engine.editor,
                    207,
                    if insert { 0 } else { 63 },
                    i32::from(insert),
                )
            },
            0
        );
        engine.finish();
        pad_to_cap(&mut engine.song, 0);
        let before = engine.song.bytes().to_vec();
        assert_ne!(unsafe { song_edit_undo(&mut engine.editor) }, 0);
        unsafe { song_edit_poll(&mut engine.editor) };
        assert_eq!(
            engine.editor.error, GT2_ERR_SONG_FULL,
            "insert={insert}, locks={restore_locks}"
        );
        assert_eq!(engine.editor.status, SONG_EDIT_FULL as u8);
        assert_eq!(engine.editor.pending, 0);
        assert_eq!(engine.song.bytes(), before);
        assert_eq!(engine.editor.undone, 0);
        assert_eq!(engine.song.raw().saved_growth, 0);
    }
}

#[test]
fn accepted_undo_restores_all_locks_before_completion_even_during_a_save_freeze() {
    let mut engine = Engine::new();
    for param in 0..SONG_LOCK_PARAMS {
        assert_eq!(
            unsafe { song_lock_put(engine.song.ptr_mut(), 0, 63, param, song_lock_max(param)) },
            0
        );
    }
    let locks = engine.locks();
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 0, 0, 1) }, 0);
    engine.finish();
    engine.binding.refuse = true;
    assert_ne!(unsafe { song_edit_undo(&mut engine.editor) }, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_ne!(engine.editor.prepared, 0);
    let reserved = engine.editor.edit.reserved;
    assert!(reserved > 0);
    unsafe { (*engine.song.ptr_mut()).frozen = 1 };
    engine.binding.guard = SONG_EDIT_GUARD_SAVING as i32;
    engine.binding.refuse = false;
    engine.finish();
    assert_eq!(engine.locks(), locks);
    assert_eq!(engine.song.raw().saved_growth, 0);
    assert_eq!(engine.song.raw().frozen, 1);
    assert_eq!(
        unsafe { gt2_saved_size(engine.song.ptr()) } as usize,
        engine.song.write(true).len()
    );
}

#[test]
fn busy_cancels_a_prepared_undo_and_returns_its_complete_growth_reservation() {
    let mut song = cap_fixture(63);
    for param in 0..SONG_LOCK_PARAMS {
        assert_eq!(
            unsafe { song_lock_put(song.ptr_mut(), 207, 63, param, song_lock_max(param)) },
            0
        );
    }
    let mut engine = Engine::with_song(song);
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 207, 0, 1) }, 0);
    engine.finish();
    let saved = unsafe { gt2_saved_size(engine.song.ptr()) };
    let used = engine.song.raw().used_units;
    engine.binding.refuse = true;
    assert_ne!(unsafe { song_edit_undo(&mut engine.editor) }, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    assert!(engine.editor.edit.reserved >= 1550 + 9 * SONG_LOCK_BYTES as u16);
    assert_eq!(
        engine.song.raw().saved_growth,
        u32::from(engine.editor.edit.reserved)
    );
    engine.binding.guard = SONG_EDIT_GUARD_BUSY as i32;
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_eq!(engine.editor.status, SONG_EDIT_BUSY as u8);
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(song_edit_message(&engine.editor)) },
        c"SONG BUSY"
    );
    assert_eq!(engine.editor.pending, 0);
    assert_eq!(engine.editor.undone, 0);
    assert_eq!(engine.song.raw().pending, 0);
    assert_eq!(engine.song.raw().used_units, used);
    assert_eq!(engine.song.raw().saved_growth, 0);
    assert_eq!(unsafe { gt2_saved_size(engine.song.ptr()) }, saved);
    engine.binding.refuse = false;
    engine.binding.guard = SONG_EDIT_GUARD_OK as i32;
    engine.undo();
    assert_eq!(
        unsafe { gt2_saved_size(engine.song.ptr()) } as usize,
        engine.song.write(true).len()
    );
    assert_eq!(engine.song.raw().saved_growth, 0);
}

#[test]
fn undo_reserves_trailer_padding_growth_not_just_the_restored_record_bytes() {
    let mut engine = Engine::new();
    for row in [0, 63] {
        assert_eq!(
            unsafe { song_lock_put(engine.song.ptr_mut(), 0, row, SONG_LOCK_CUTOFF, 100) },
            0
        );
    }
    assert_eq!(
        engine
            .song
            .ext_set(sidou_tests::song::tag(b"CAPT"), &[1; 219]),
        0
    );
    assert_eq!(engine.song.raw().ext_used, 248);
    let original = engine.song.write(true).len();
    assert_ne!(unsafe { song_edit_shift(&mut engine.editor, 0, 0, 1) }, 0);
    engine.finish();
    assert_eq!(engine.song.raw().ext_used, 243);
    assert_eq!(engine.song.write(true).len() + 14, original);
    engine.binding.refuse = true;
    assert_ne!(unsafe { song_edit_undo(&mut engine.editor) }, 0);
    unsafe { song_edit_poll(&mut engine.editor) };
    assert_eq!(engine.editor.edit.reserved, 14);
    assert_eq!(engine.song.raw().saved_growth, 14);
    engine.binding.refuse = false;
    engine.finish();
    assert_eq!(engine.song.write(true).len(), original);
    assert_eq!(
        unsafe { gt2_saved_size(engine.song.ptr()) } as usize,
        original
    );
    assert_eq!(engine.song.raw().saved_growth, 0);
}
