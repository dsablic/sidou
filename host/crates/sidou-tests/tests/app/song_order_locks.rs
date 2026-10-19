// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Task 4 prospective order-lock regressions for cell, restart and rebuilt orderlists.

use sidou_sys::*;
use sidou_tests::song::Song;
use sidou_tests::transport::Transport;

#[derive(Clone, Copy, Debug)]
enum Edit {
    Cell,
    Restart,
    Replace,
    Insert,
    Delete,
    Controls,
    Empty,
}

fn at_boundary(restart: bool) -> (Song, Transport) {
    let mut song = Song::device();
    song.read(&sidou_tests::song_embed::sng(), SONG_MULTISPEED_MAX as u8)
        .unwrap();
    for pattern in [4, 5, 6] {
        assert_eq!(
            unsafe {
                song_lock_put(
                    song.ptr_mut(),
                    pattern,
                    0,
                    SONG_LOCK_CUTOFF,
                    pattern * 100 + 1,
                )
            },
            0
        );
    }
    let mut player = Transport::new(&mut song);
    assert!(player.play(0, if restart { 3 } else { 0 }));
    let pattern = if restart { 7 } else { 4 };
    let last = unsafe { gt2_pattern_rows(song.ptr(), pattern) } - 2;
    assert!(
        player.run_until(10_000, |player| player.sc.row_pat[0] == pattern as u8
            && player.sc.row_row[0] == last as u8)
    );
    player.pass();
    (song, player)
}

fn has_lock(player: &Transport, pattern: u8) -> bool {
    let set = &player.sc.lock_next[0][player.sc.lock_pub[0] as usize];
    set.e[..set.n as usize].iter().any(|lock| {
        lock.pattern == pattern
            && lock.row == 0
            && lock.param == SONG_LOCK_CUTOFF as u8
            && u16::from_le_bytes([lock.lo, lock.hi]) == u16::from(pattern) * 100 + 1
    })
}

fn prepare(song: &mut Song, edit: Edit) -> gt2_edit {
    let mut command = gt2_edit::default();
    let result = unsafe {
        match edit {
            Edit::Cell => gt2_set_order(song.ptr_mut(), 0, 0, 1, 6, &mut command),
            Edit::Restart => gt2_set_order(song.ptr_mut(), 0, 0, 4, 2, &mut command),
            Edit::Replace => gt2_order_replace(
                song.ptr_mut(),
                0,
                0,
                [4, 6, 5, 7].as_ptr(),
                4,
                0,
                &mut command,
            ),
            Edit::Insert => gt2_order_insert(song.ptr_mut(), 0, 0, 1, 6, &mut command),
            Edit::Delete => gt2_order_delete(song.ptr_mut(), 0, 0, 1, &mut command),
            Edit::Controls => gt2_order_replace(
                song.ptr_mut(),
                0,
                0,
                [4, GT2_TRANSUP as u8 + 1, GT2_REPEAT as u8 + 1, 6, 7].as_ptr(),
                5,
                0,
                &mut command,
            ),
            Edit::Empty => {
                gt2_order_replace(song.ptr_mut(), 0, 0, std::ptr::null(), 0, 0, &mut command)
            }
        }
    };
    assert_eq!(result, GT2_OK, "{edit:?}");
    command
}

fn submit_without_publication(player: &mut Transport, edit: &gt2_edit) {
    assert!(player.edit(edit));
    for _ in 0..10_000 {
        if player.sc.deriving == 0 {
            let last = &player.sc.cmd[(player.sc.stage.wrapping_sub(1) % SONG_CMD_RING) as usize];
            assert_eq!(last.kind, SONG_CMD_EDIT as u8);
            return;
        }
        unsafe { song_ctl_poll(&mut *player.sc, SONG_POLL_SLICE | SONG_POLL_RING) };
    }
    panic!("edit derivation did not complete");
}

#[test]
fn order_cell_restart_and_rebuild_publish_the_new_next_patterns_locks_with_the_edit() {
    for operation in [
        Edit::Cell,
        Edit::Restart,
        Edit::Replace,
        Edit::Insert,
        Edit::Delete,
        Edit::Controls,
        Edit::Empty,
    ] {
        let restart = matches!(operation, Edit::Restart);
        let (mut song, mut player) = at_boundary(restart);
        let old_next = if restart { 4 } else { 5 };
        assert!(has_lock(&player, old_next), "{operation:?}: original cache");
        let edit = prepare(&mut song, operation);
        submit_without_publication(&mut player, &edit);
        assert!(
            has_lock(&player, old_next),
            "{operation:?}: not published early"
        );
        unsafe {
            song_ctl_commit(&mut *player.sc);
            song_ctl_block(&mut *player.sc, &mut *player.chip);
        }
        assert!(
            !has_lock(&player, old_next),
            "{operation:?}: obsolete next pattern"
        );
        if !matches!(operation, Edit::Empty) {
            assert!(
                has_lock(&player, 6),
                "{operation:?}: new next pattern before release"
            );
        }
        assert!(player.sc.edits_block_max <= SONG_EDITS_PER_BLOCK);
        assert_eq!(player.sc.edit_cells + player.sc.edit_swaps, 1);
        player.pass();
        assert_eq!(song.raw().pending, 0);
    }
}

#[test]
fn order_cache_publication_reuses_the_row_sequence_refresh_handshake() {
    let (mut song, mut player) = at_boundary(false);
    let edit = prepare(&mut song, Edit::Cell);
    submit_without_publication(&mut player, &edit);
    // An unrelated voice advances between preparation and command visibility.
    player.sc.row_seq[1] = player.sc.row_seq[1].wrapping_add(1);
    let before = player.sc.applied;
    unsafe {
        song_ctl_commit(&mut *player.sc);
        song_ctl_block(&mut *player.sc, &mut *player.chip);
    }
    assert_ne!(player.sc.lock_edit_refresh, 0);
    assert_eq!(song.raw().pending, 1);
    assert!(has_lock(&player, 5));
    // A preceding shape command may acknowledge, but the order edit must still wait.
    assert!(player.sc.applied.wrapping_sub(before) <= 1);
    player.pass();
    unsafe { song_ctl_block(&mut *player.sc, &mut *player.chip) };
    assert!(has_lock(&player, 6));
    assert!(!has_lock(&player, 5));
    player.pass();
    assert_eq!(song.raw().pending, 0);
}
