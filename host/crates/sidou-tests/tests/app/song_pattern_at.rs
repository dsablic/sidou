// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::song::{tag, SngSpec, Song};

#[test]
fn a_selected_pattern_is_private_until_its_swap_and_reference_waits_for_release() {
    let mut song = Song::device();
    let mut edit = gt2_edit::default();
    let mut reference = gt2_edit::default();
    unsafe {
        assert_eq!(
            gt2_pattern_add_at(song.ptr_mut(), 180, GT2_DEFAULT_ROWS, &mut edit),
            GT2_OK
        );
        assert_eq!(u32::from(edit.kind), GT2_EDIT_SWAP);
        assert_eq!(edit.slot, 180);
        assert_eq!(song.raw().pattern[180], 0);
        assert_eq!(
            gt2_set_order(song.ptr_mut(), 0, 0, 0, 180, &mut reference),
            GT2_ERR_PENDING
        );
        gt2_edit_apply(song.ptr_mut(), &edit);
        assert_eq!(gt2_pattern_rows(song.ptr(), 180), GT2_DEFAULT_ROWS + 1);
        let rows = std::slice::from_raw_parts(gt2_pattern_data(song.ptr(), 180), 65 * 4);
        for row in rows.as_chunks::<4>().0.iter().take(64) {
            assert_eq!(*row, [GT2_REST as u8, 0, 0, 0]);
        }
        assert_eq!(&rows[64 * 4..], &[GT2_ENDPATT as u8, 0, 0, 0]);
        assert_eq!(
            gt2_set_order(song.ptr_mut(), 0, 0, 0, 180, &mut reference),
            GT2_ERR_PENDING
        );
        gt2_edit_release(song.ptr_mut(), &edit);
        assert_eq!(
            gt2_set_order(song.ptr_mut(), 0, 0, 0, 180, &mut reference),
            GT2_OK
        );
        gt2_edit_commit(song.ptr_mut(), &reference);
        assert_eq!(*gt2_order_data(song.ptr(), 0, 0), 180);
    }
}

#[test]
fn selected_pattern_creation_refuses_ranges_and_pending_orders_and_cancels_cleanly() {
    let mut song = Song::device();
    let mut edit = gt2_edit::default();
    let before = song.bytes().to_vec();
    unsafe {
        for (pattern, rows, error) in [
            (GT2_MAX_PATTERNS, 64, GT2_ERR_RANGE),
            (0, 64, GT2_ERR_RANGE),
            (180, 0, GT2_ERR_ROWS),
            (180, GT2_MAX_ROWS + 1, GT2_ERR_ROWS),
        ] {
            assert_eq!(
                gt2_pattern_add_at(song.ptr_mut(), pattern, rows, &mut edit),
                error
            );
            assert_eq!(song.bytes(), before);
        }
        let mut order = gt2_edit::default();
        assert_eq!(
            gt2_set_order(song.ptr_mut(), 0, 0, 0, 1, &mut order),
            GT2_OK
        );
        assert_eq!(
            gt2_pattern_add_at(song.ptr_mut(), 180, 64, &mut edit),
            GT2_ERR_PENDING
        );
        gt2_edit_cancel(song.ptr_mut(), &order);
        let allocated = song.raw().used_units;
        assert_eq!(
            gt2_pattern_add_at(song.ptr_mut(), 180, 64, &mut edit),
            GT2_OK
        );
        let mut another = gt2_edit::default();
        assert_eq!(
            gt2_pattern_add_at(song.ptr_mut(), 180, 64, &mut another),
            GT2_ERR_PENDING
        );
        gt2_edit_cancel(song.ptr_mut(), &edit);
        assert_eq!(song.raw().pending, 0);
        assert_eq!(song.raw().used_units, allocated);
        assert_eq!(song.raw().pattern[180], 0);
        assert_eq!(
            gt2_pattern_add_at(song.ptr_mut(), 180, 1, &mut edit),
            GT2_OK
        );
        gt2_edit_commit(song.ptr_mut(), &edit);
        assert_eq!(gt2_pattern_rows(song.ptr(), 180), 2);
    }
}

fn check_saved_size(song: &Song) {
    assert_eq!(
        unsafe { gt2_saved_size(song.ptr()) } as usize,
        song.write(true).len()
    );
}

#[test]
fn selected_sparse_pattern_matches_an_independent_canonical_file_when_referenced() {
    for pattern in [3, 180, GT2_MAX_PATTERNS - 1] {
        for rows in [1, GT2_DEFAULT_ROWS, GT2_MAX_ROWS] {
            let mut song = Song::device();
            let original = song.write(true);
            let mut edit = gt2_edit::default();
            assert_eq!(
                unsafe { gt2_pattern_add_at(song.ptr_mut(), pattern, rows, &mut edit) },
                GT2_OK
            );
            assert_eq!(edit.old_span, 0);
            assert_eq!(edit.bytes[0] as u32, SONG_LOCKS_REMOVE);
            assert_eq!(
                song.write(true),
                original,
                "private construction must not publish content"
            );
            unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
            assert_eq!(
                song.write(true),
                original,
                "an unnamed empty pattern does not extend the saved prefix"
            );
            check_saved_size(&song);
            assert_eq!(
                unsafe { gt2_set_order(song.ptr_mut(), 0, 0, 0, pattern as u8, &mut edit) },
                GT2_OK
            );
            unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
            let mut expected = SngSpec::new();
            expected.instr.clear();
            expected.orders[0][0].0 = vec![pattern as u8];
            expected.patterns.resize(
                pattern as usize + 1,
                (
                    vec![[GT2_REST as u8, 0, 0, 0]; GT2_DEFAULT_ROWS as usize],
                    [GT2_ENDPATT as u8, 0, 0, 0],
                ),
            );
            expected.patterns[pattern as usize].0 = vec![[GT2_REST as u8, 0, 0, 0]; rows as usize];
            assert_eq!(song.write(false), expected.encode());
            check_saved_size(&song);
        }
    }
}

fn selected_pattern_near_cap(slack: usize, last: u32) -> Song {
    let mut spec = SngSpec::new();
    spec.instr.clear();
    spec.patterns = (0..208)
        .map(|p| {
            let rows = if p < 40 {
                128
            } else if p == last {
                1
            } else {
                64
            };
            let note = if p < 40 || p == last { 0x60 } else { 0xbd };
            (vec![[note, 0, 0, 0]; rows], [0xff, 0, 0, 0])
        })
        .collect();
    let mut song = Song::device();
    song.read(&spec.encode(), 4).unwrap();
    let payload = vec![1; GT2_SAVED_MAX as usize - slack - song.write(true).len() - 8];
    assert_eq!(song.ext_set(tag(b"CATA"), &payload), 0);
    assert_eq!(song.write(true).len(), GT2_SAVED_MAX as usize - slack);
    check_saved_size(&song);
    song
}

#[test]
fn selected_pattern_creation_caps_growth_and_cancellation_returns_its_reservation() {
    let mut song = selected_pattern_near_cap(0, 207);
    let before = song.bytes().to_vec();
    let mut edit = gt2_edit::default();
    assert_eq!(
        unsafe { gt2_pattern_add_at(song.ptr_mut(), 180, 128, &mut edit) },
        GT2_ERR_SONG_FULL
    );
    assert_eq!(song.bytes(), before);

    let mut song = selected_pattern_near_cap(256, 207);
    let revision = song.raw().revision;
    assert_eq!(
        unsafe { gt2_pattern_add_at(song.ptr_mut(), 180, 128, &mut edit) },
        GT2_OK
    );
    assert_eq!(edit.reserved, 256);
    let mut second = gt2_edit::default();
    let before = song.bytes().to_vec();
    assert_eq!(
        unsafe { gt2_pattern_add_at(song.ptr_mut(), 181, 128, &mut second) },
        GT2_ERR_SONG_FULL
    );
    assert_eq!(song.bytes(), before);
    unsafe { gt2_edit_cancel(song.ptr_mut(), &edit) };
    assert_eq!(song.raw().saved_growth, 0);
    assert_eq!(song.raw().revision, revision);
    assert_eq!(
        unsafe { gt2_pattern_add_at(song.ptr_mut(), 181, 128, &mut second) },
        GT2_OK
    );
    assert_eq!(second.reserved, 256);
    unsafe { gt2_edit_commit(song.ptr_mut(), &second) };
    assert_eq!(song.write(true).len(), GT2_SAVED_MAX as usize);
    assert_eq!(song.raw().pending, 0);
    check_saved_size(&song);
}

#[test]
fn selected_sparse_pattern_reference_refuses_canonical_prefix_growth_at_the_cap() {
    let mut song = selected_pattern_near_cap(0, 206);
    let mut edit = gt2_edit::default();
    assert_eq!(song.raw().pattern[207], 0);
    assert_eq!(
        unsafe { gt2_pattern_add_at(song.ptr_mut(), 207, 64, &mut edit) },
        GT2_OK
    );
    assert_eq!(edit.reserved, 0);
    unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
    let before = song.bytes().to_vec();
    assert_eq!(
        unsafe { gt2_set_order(song.ptr_mut(), 0, 0, 0, 207, &mut edit) },
        GT2_ERR_SONG_FULL
    );
    assert_eq!(song.bytes(), before);
    assert_eq!(song.write(true).len(), GT2_SAVED_MAX as usize);
    check_saved_size(&song);
}

#[test]
fn selected_pattern_creation_obeys_freeze_and_finishes_preaccepted_lock_cleanup() {
    let mut song = Song::device();
    for (pattern, row) in [(0, 0), (180, 0), (180, 5)] {
        assert_eq!(
            unsafe { song_lock_put(song.ptr_mut(), pattern, row, 0, 123) },
            0
        );
    }
    let mut edit = gt2_edit::default();
    let revision = song.raw().revision;
    let locks = song.ext_chunks();
    assert_eq!(
        unsafe { gt2_pattern_add_at(song.ptr_mut(), 180, 64, &mut edit) },
        GT2_OK
    );
    unsafe { gt2_edit_cancel(song.ptr_mut(), &edit) };
    assert_eq!(song.ext_chunks(), locks);
    assert_eq!(song.raw().revision, revision);
    assert_eq!(
        unsafe { gt2_pattern_add_at(song.ptr_mut(), 180, 64, &mut edit) },
        GT2_OK
    );
    unsafe { (*song.ptr_mut()).frozen = 1 };
    let before = song.bytes().to_vec();
    for (pattern, rows) in [(0, 64), (GT2_MAX_PATTERNS, 0), (181, 64)] {
        assert_eq!(
            unsafe { gt2_pattern_add_at(song.ptr_mut(), pattern, rows, &mut gt2_edit::default()) },
            GT2_ERR_SAVING
        );
        assert_eq!(song.bytes(), before);
    }
    unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
    assert_eq!(song.raw().frozen, 1);
    assert_eq!(song.raw().pending, 0);
    assert!(song.raw().revision > revision);
    let mut locks = [song_lock::default(); 3];
    assert_eq!(
        unsafe { song_ext_get_locks(song.ptr(), locks.as_mut_ptr(), 3) },
        1
    );
    assert_eq!(locks[0].pattern, 0);
    check_saved_size(&song);
}

#[test]
fn selected_pattern_creation_waits_for_a_pending_prefix_trim() {
    let mut spec = SngSpec::new();
    spec.patterns = vec![(vec![[GT2_REST as u8, 0, 0, 0]; 64], [0xff, 0, 0, 0]); 208];
    spec.patterns[206] = (vec![[0x60, 0, 0, 0]], [0xff, 0, 0, 0]);
    for cancel in [false, true] {
        let mut song = Song::device();
        song.read(&spec.encode(), 4).unwrap();
        assert_eq!(song.raw().pattern[207], 0);
        let mut removal = gt2_edit::default();
        assert_eq!(
            unsafe { gt2_pattern_remove(song.ptr_mut(), 206, &mut removal) },
            GT2_OK
        );
        for applied in [false, true] {
            if applied {
                unsafe { gt2_edit_apply(song.ptr_mut(), &removal) };
            }
            let before = song.bytes().to_vec();
            assert_eq!(
                unsafe { gt2_order_pattern_check(song.ptr(), 0, 0, 0, 207, 0) },
                GT2_ERR_PENDING
            );
            assert_eq!(
                unsafe { gt2_pattern_add_at(song.ptr_mut(), 207, 64, &mut gt2_edit::default()) },
                GT2_ERR_PENDING
            );
            assert_eq!(song.bytes(), before);
            if cancel {
                break;
            }
        }
        unsafe {
            if cancel {
                gt2_edit_cancel(song.ptr_mut(), &removal)
            } else {
                gt2_edit_release(song.ptr_mut(), &removal)
            }
        };
        let mut edit = gt2_edit::default();
        assert_eq!(
            unsafe { gt2_pattern_add_at(song.ptr_mut(), 207, 64, &mut edit) },
            GT2_OK
        );
        unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
        check_saved_size(&song);
    }
}

#[test]
fn missing_pattern_order_preflight_refuses_the_cap_without_creating_or_dirtying() {
    for insert in [0, 1] {
        let song = selected_pattern_near_cap(0, 206);
        let before = song.bytes().to_vec();
        assert_eq!(
            unsafe { gt2_order_pattern_check(song.ptr(), 0, 0, 0, 207, insert) },
            GT2_ERR_SONG_FULL
        );
        assert_eq!(song.bytes(), before);
        assert_eq!(song.raw().pattern[207], 0);
    }
}

#[test]
fn missing_pattern_order_preflight_matches_authoritative_checks_at_the_exact_cap() {
    for insert in [0, 1] {
        let growth = 261 + insert as usize;
        let mut song = selected_pattern_near_cap(growth, 206);
        let before = song.bytes().to_vec();
        for _ in 0..2 {
            assert_eq!(
                unsafe { gt2_order_pattern_check(song.ptr(), 0, 0, 0, 207, insert) },
                GT2_OK
            );
            assert_eq!(
                song.bytes(),
                before,
                "preflight must not allocate, reserve or dirty"
            );
        }
        let mut edit = gt2_edit::default();
        assert_eq!(
            unsafe { gt2_pattern_add_at(song.ptr_mut(), 207, 64, &mut edit) },
            GT2_OK
        );
        unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
        let before = song.bytes().to_vec();
        assert_eq!(
            unsafe { gt2_order_pattern_check(song.ptr(), 0, 0, 0, 207, insert) },
            GT2_OK
        );
        assert_eq!(song.bytes(), before);
        let result = unsafe {
            if insert != 0 {
                gt2_order_insert(song.ptr_mut(), 0, 0, 0, 207, &mut edit)
            } else {
                gt2_set_order(song.ptr_mut(), 0, 0, 0, 207, &mut edit)
            }
        };
        assert_eq!(result, GT2_OK);
        assert_eq!(edit.reserved as usize, growth);
        unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
        assert_eq!(song.write(true).len(), GT2_SAVED_MAX as usize);
        check_saved_size(&song);
    }
}

#[test]
fn missing_pattern_order_preflight_observes_pending_growth_and_creation() {
    let mut song = selected_pattern_near_cap(261, 206);
    let mut pending = gt2_edit::default();
    assert_eq!(
        unsafe { gt2_set_instr(song.ptr_mut(), 1, 0, 1, &mut pending) },
        GT2_OK
    );
    let before = song.bytes().to_vec();
    assert_eq!(
        unsafe { gt2_order_pattern_check(song.ptr(), 0, 0, 0, 207, 0) },
        GT2_ERR_SONG_FULL
    );
    assert_eq!(song.bytes(), before);
    unsafe { gt2_edit_cancel(song.ptr_mut(), &pending) };
    assert_eq!(
        unsafe { gt2_order_pattern_check(song.ptr(), 0, 0, 0, 207, 0) },
        GT2_OK
    );
    assert_eq!(
        unsafe { gt2_pattern_add_at(song.ptr_mut(), 207, 64, &mut pending) },
        GT2_OK
    );
    for applied in [false, true] {
        if applied {
            unsafe { gt2_edit_apply(song.ptr_mut(), &pending) };
        }
        let before = song.bytes().to_vec();
        assert_eq!(
            unsafe { gt2_order_pattern_check(song.ptr(), 0, 0, 0, 207, 0) },
            GT2_ERR_PENDING
        );
        assert_eq!(song.bytes(), before);
    }
    unsafe { gt2_edit_release(song.ptr_mut(), &pending) };
    assert_eq!(
        unsafe { gt2_order_pattern_check(song.ptr(), 0, 0, 0, 207, 0) },
        GT2_OK
    );
}

#[test]
fn missing_pattern_order_preflight_includes_auxiliary_lock_cleanup_padding() {
    let mut spec = SngSpec::new();
    spec.instr.clear();
    spec.orders[0][0].0 = vec![0; 111];
    spec.patterns = (0..208)
        .map(|p| {
            let count = if p < 42 {
                128
            } else if p == 206 {
                1
            } else {
                64
            };
            let note = if p < 40 || p == 206 { 0x60 } else { 0xbd };
            (vec![[note, 0, 0, 0]; count], [0xff, 0, 0, 0])
        })
        .collect();
    let mut song = Song::device();
    song.read(&spec.encode(), 4).unwrap();
    for pattern in [0, 207] {
        assert_eq!(
            unsafe { song_lock_put(song.ptr_mut(), pattern, 0, 0, 123) },
            0
        );
    }
    assert_eq!(song.ext_set(tag(b"CPAD"), &[1; 224]), 0);
    assert_eq!(song.raw().ext_used, 253);
    assert_eq!(song.write(true).len(), GT2_SAVED_MAX as usize - 264);
    let before = song.bytes().to_vec();
    for insert in [0, 1] {
        assert_eq!(
            unsafe { gt2_order_pattern_check(song.ptr(), 0, 0, 0, 207, insert) },
            GT2_ERR_SONG_FULL
        );
        assert_eq!(song.bytes(), before);
    }
}

#[test]
fn order_pattern_preflight_validates_ranges_grammar_pending_orders_and_freeze() {
    let mut song = Song::device();
    for (subtune, channel, pos, pattern, insert) in [
        (1, 0, 0, 180, 0),
        (0, 3, 0, 180, 0),
        (0, 0, 1, 180, 0),
        (0, 0, 2, 180, 1),
        (0, 0, 0, GT2_MAX_PATTERNS, 0),
        (0, 0, 0, u32::MAX, 1),
    ] {
        let before = song.bytes().to_vec();
        assert_eq!(
            unsafe { gt2_order_pattern_check(song.ptr(), subtune, channel, pos, pattern, insert) },
            GT2_ERR_RANGE
        );
        assert_eq!(song.bytes(), before);
    }
    let mut pending = gt2_edit::default();
    assert_eq!(
        unsafe { gt2_set_order(song.ptr_mut(), 0, 1, 0, 1, &mut pending) },
        GT2_OK
    );
    let before = song.bytes().to_vec();
    assert_eq!(
        unsafe { gt2_order_pattern_check(song.ptr(), 0, 0, 0, 180, 0) },
        GT2_ERR_PENDING
    );
    assert_eq!(
        unsafe { gt2_order_pattern_check(song.ptr(), 0, 1, 0, 0, 0) },
        GT2_ERR_PENDING
    );
    assert_eq!(song.bytes(), before);
    unsafe { gt2_edit_cancel(song.ptr_mut(), &pending) };
    unsafe { (*song.ptr_mut()).frozen = 1 };
    let before = song.bytes().to_vec();
    assert_eq!(
        unsafe { gt2_order_pattern_check(song.ptr(), u32::MAX, 0, 0, u32::MAX, 0) },
        GT2_ERR_SAVING
    );
    assert_eq!(song.bytes(), before);
    for (entries, pos, insert, expected) in [
        (vec![0; GT2_MAX_ORDER as usize], 0, 1, GT2_ERR_ORDER_FULL),
        (vec![0xe1, 0xe2, 0], 2, 0, GT2_ERR_ORDER_RULE),
        (vec![0xe1, 0xe2, 0], 3, 1, GT2_ERR_ORDER_RULE),
    ] {
        let mut spec = SngSpec::new();
        spec.orders[0][0].0 = entries;
        let mut song = Song::device();
        song.read(&spec.encode(), 4).unwrap();
        let before = song.bytes().to_vec();
        assert_eq!(
            unsafe { gt2_order_pattern_check(song.ptr(), 0, 0, pos, 180, insert) },
            expected
        );
        assert_eq!(song.bytes(), before);
    }
}
