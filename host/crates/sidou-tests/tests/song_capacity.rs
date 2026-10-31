// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;
use sidou_tests::song::*;

fn holes_fixture() -> Vec<u8> {
    let mut spec = SngSpec::new();
    spec.instr.clear();
    spec.patterns = (0..208)
        .map(|p| {
            let rows = if p < 40 {
                128
            } else if p == 207 {
                1
            } else {
                64
            };
            let note = if p < 40 || p == 207 { 0x60 } else { 0xbd };
            (vec![[note, 0, 0, 0]; rows], [0xff, 0, 0, 0])
        })
        .collect();
    spec.encode()
}

#[test]
fn hole_growth_is_refused_before_the_song_becomes_unsaveable() {
    let mut song = Song::device();
    song.read(&holes_fixture(), 4).unwrap();
    assert_eq!(song.write(true).len(), 64_658);
    for expected in [64_914, 65_170, 65_426] {
        let mut edit = gt2_edit::default();
        let mut pattern = 0;
        assert_eq!(
            unsafe { gt2_pattern_add(song.ptr_mut(), 128, &mut pattern, &mut edit) },
            GT2_OK
        );
        unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
        assert_eq!(song.write(true).len(), expected);
    }
    let before = song.bytes().to_vec();
    let mut edit = gt2_edit::default();
    let mut pattern = 0;
    let result = unsafe { gt2_pattern_add(song.ptr_mut(), 128, &mut pattern, &mut edit) };
    assert_eq!(
        result, GT2_ERR_SONG_FULL,
        "accepted an edit whose trailer save exceeds STORE_FILE_MAX"
    );
    assert_eq!(song.bytes(), before, "refusal changed the region");
    check_size(&song);
    assert!(song.write(true).len() <= STORE_FILE_MAX as usize);
}

unsafe extern "C" fn read_capacity_fixture(
    ctx: *mut std::ffi::c_void,
    off: u32,
    out: *mut u8,
    n: u32,
) -> u32 {
    let bytes = unsafe { &*ctx.cast::<Vec<u8>>() };
    let at = off as usize;
    let count = (n as usize).min(bytes.len().saturating_sub(at));
    if count != 0 {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr().add(at), out, count) };
    }
    count as u32
}

#[test]
fn canonical_output_not_arena_use_limits_loading() {
    let mut host = Song::host_from(&holes_fixture());
    for _ in 0..4 {
        let mut edit = gt2_edit::default();
        let mut pattern = 0;
        assert_eq!(
            unsafe { gt2_pattern_add(host.ptr_mut(), 128, &mut pattern, &mut edit) },
            GT2_OK
        );
        unsafe { gt2_edit_commit(host.ptr_mut(), &edit) };
    }
    let mut device = Song::device();
    let before = device.bytes().to_vec();
    let mut file = host.write(false);
    assert!(
        file.len() <= STORE_FILE_MAX as usize,
        "input fits; saved form does not"
    );
    let mut info = gt2_read_info::default();
    let caps = gt2_caps { max_multispeed: 4 };
    assert_eq!(
        unsafe {
            gt2_sng_read(
                device.ptr_mut(),
                Some(read_capacity_fixture),
                (&mut file as *mut Vec<u8>).cast(),
                &caps,
                &mut info,
            )
        },
        GT2_READ_SONG_FULL
    );
    assert!(unsafe { std::ffi::CStr::from_ptr(info.msg.as_ptr()) }
        .to_bytes()
        .starts_with(b"SNG: SONG FULL"));
    assert_eq!(device.bytes(), before);
}

fn check_size(song: &Song) {
    assert_eq!(
        unsafe { gt2_saved_size(song.ptr()) } as usize,
        song.write(true).len()
    );
}

#[test]
fn row_cell_accounting_work_does_not_grow_with_pattern_length() {
    let mut work = Vec::new();
    for rows in [1, 16, 64, 128] {
        let mut song = Song::device();
        let mut edit = gt2_edit::default();
        assert_eq!(
            unsafe { gt2_pattern_resize(song.ptr_mut(), 0, rows, &mut edit) },
            GT2_OK
        );
        unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
        let before = song.raw().saved_row_work;
        assert_eq!(
            unsafe { gt2_set_row(song.ptr_mut(), 0, 0, [0x60, 63, 0, 0].as_ptr(), &mut edit) },
            GT2_OK
        );
        unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
        work.push(song.raw().saved_row_work - before);
        check_size(&song);
    }
    assert!(
        work.iter().all(|&count| count == 1),
        "a cell must inspect exactly its actual previous row regardless of pattern length: {work:?}"
    );
}

#[test]
fn the_target_bench_name_cell_changes_no_variable_size_accounting() {
    let mut song = Song::device();
    let size = unsafe { gt2_saved_size(song.ptr()) };
    let edit = gt2_edit {
        kind: GT2_EDIT_CELL as u8,
        n: 4,
        bytes: *b"NAME",
        slot: GT2_SLOT_NONE as u16,
        offset: std::mem::offset_of!(gt2_song, name) as u32,
        ..Default::default()
    };
    unsafe {
        (*song.ptr_mut()).pending_loose += 1;
        gt2_edit_commit(song.ptr_mut(), &edit);
    }
    assert_eq!(unsafe { gt2_saved_size(song.ptr()) }, size);
    assert_eq!(&song.write(false)[4..8], b"NAME");
    check_size(&song);
}

#[test]
fn accepted_pattern_edits_finish_their_locks_after_a_save_freezes_new_edits() {
    for insert in [false, true] {
        let mut song = Song::device();
        for (row, value) in [(1, 10), (3, 20), (5, 30)] {
            assert_eq!(
                unsafe { song_lock_put(song.ptr_mut(), 0, row, 0, value) },
                0
            );
        }
        let mut accepted = gt2_edit::default();
        assert_eq!(
            unsafe {
                if insert {
                    gt2_pattern_insert_row(song.ptr_mut(), 0, 2, &mut accepted)
                } else {
                    gt2_pattern_resize(song.ptr_mut(), 0, 4, &mut accepted)
                }
            },
            GT2_OK
        );
        unsafe { (*song.ptr_mut()).frozen = 1 };
        let mut refused = gt2_edit::default();
        assert_eq!(
            unsafe { gt2_set_row(song.ptr_mut(), 0, 0, [0x60, 0, 0, 0].as_ptr(), &mut refused) },
            GT2_ERR_SAVING
        );
        assert_eq!(
            unsafe { song_lock_put(song.ptr_mut(), 0, 0, 0, 40) },
            -(GT2_ERR_SAVING as i32)
        );
        unsafe { gt2_edit_commit(song.ptr_mut(), &accepted) };
        assert_eq!(song.raw().frozen, 1);
        assert_eq!(song.raw().pending, 0);
        let mut locks = [song_lock::default(); 3];
        let n = unsafe { song_ext_get_locks(song.ptr(), locks.as_mut_ptr(), locks.len() as u32) };
        let rows: Vec<_> = locks[..n as usize]
            .iter()
            .map(|lock| (lock.row, lock.value))
            .collect();
        assert_eq!(
            rows,
            if insert {
                vec![(1, 10), (4, 20), (6, 30)]
            } else {
                vec![(1, 10), (3, 20)]
            }
        );
        assert_eq!(
            unsafe { gt2_pattern_insert_row(song.ptr_mut(), 0, 0, &mut refused) },
            GT2_ERR_SAVING
        );
        check_size(&song);
    }
}

#[test]
fn overlapping_row_writes_account_the_actual_predecessor_and_cancel_independently() {
    let mut song = Song::device();
    let mut initial = gt2_edit::default();
    assert_eq!(
        unsafe {
            gt2_set_row(
                song.ptr_mut(),
                0,
                0,
                [0x60, 63, 0, 0].as_ptr(),
                &mut initial,
            )
        },
        GT2_OK
    );
    unsafe { gt2_edit_commit(song.ptr_mut(), &initial) };
    let mut edits = Vec::new();
    for cell in [
        [0xbd, 0, 0, 0],
        [0x60, 7, 0, 0],
        [0xbd, 0, 0, 0],
        [0x60, 31, 0, 0],
    ] {
        let mut edit = gt2_edit::default();
        assert_eq!(
            unsafe { gt2_set_row(song.ptr_mut(), 0, 0, cell.as_ptr(), &mut edit) },
            GT2_OK
        );
        edits.push(edit);
    }
    unsafe { gt2_edit_cancel(song.ptr_mut(), &edits.remove(1)) };
    let work = song.raw().saved_row_work;
    for edit in &edits {
        unsafe { gt2_edit_apply(song.ptr_mut(), edit) };
    }
    for edit in edits.iter().rev() {
        unsafe { gt2_edit_release(song.ptr_mut(), edit) };
    }
    assert_eq!(song.raw().saved_row_work - work, 3);
    assert_eq!(song.raw().saved_pattern_nonempty[0], 1);
    assert_eq!(unsafe { gt2_saved_instruments(song.ptr()) }, 31);
    check_size(&song);
    let mut clear = gt2_edit::default();
    assert_eq!(
        unsafe { gt2_set_row(song.ptr_mut(), 0, 0, [0xbd, 0, 0, 0].as_ptr(), &mut clear) },
        GT2_OK
    );
    unsafe { gt2_edit_commit(song.ptr_mut(), &clear) };
    assert_eq!(song.raw().saved_pattern_nonempty[0], 0);
    assert_eq!(unsafe { gt2_saved_instruments(song.ptr()) }, 0);
    check_size(&song);
}

#[test]
fn independent_instrument_counter_writers_remain_exact_across_u16_wraparound() {
    let mut song = Song::device();
    for cycle in 0..=u16::MAX as u32 + 1 {
        let mut edit = gt2_edit::default();
        let mut pattern = 0;
        assert_eq!(
            unsafe { gt2_pattern_add(song.ptr_mut(), 1, &mut pattern, &mut edit) },
            GT2_OK
        );
        assert_eq!(pattern, 3);
        unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
        assert_eq!(
            unsafe {
                gt2_set_row(
                    song.ptr_mut(),
                    pattern,
                    0,
                    [0x60, 63, 0, 0].as_ptr(),
                    &mut edit,
                )
            },
            GT2_OK
        );
        unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
        assert_eq!(
            unsafe { gt2_saved_instruments(song.ptr()) },
            63,
            "cycle {cycle}"
        );
        assert_eq!(
            unsafe { gt2_pattern_remove(song.ptr_mut(), pattern, &mut edit) },
            GT2_OK
        );
        unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
        assert_eq!(
            unsafe { gt2_saved_instruments(song.ptr()) },
            0,
            "cycle {cycle}"
        );
    }
    check_size(&song);
}

fn trailing_removal_fixture(saved_bytes: usize) -> Song {
    let mut spec = SngSpec::new();
    spec.patterns = (0..208)
        .map(|p| {
            let rows = if p < 40 {
                128
            } else if p >= 206 {
                1
            } else {
                64
            };
            let note = if p < 40 || p == 206 { 0x60 } else { 0xbd };
            (vec![[note, 0, 0, 0]; rows], [0xff, 0, 0, 0])
        })
        .collect();
    let mut song = Song::device();
    song.read(&spec.encode(), 4).unwrap();
    let payload = vec![1u8; saved_bytes - song.write(true).len() - 8];
    assert_eq!(
        unsafe {
            song_ext_set(
                song.ptr_mut(),
                tag(b"TEST"),
                payload.as_ptr(),
                payload.len() as u32,
            )
        },
        0
    );
    assert_eq!(song.write(true).len(), saved_bytes);
    check_size(&song);
    song
}

fn activate_pattern_207(song: &mut Song, kind: u32, edit: &mut gt2_edit) -> gt2_err {
    unsafe {
        match kind {
            0 => gt2_set_row(song.ptr_mut(), 207, 0, [0x60, 0, 0, 0].as_ptr(), edit),
            1 => gt2_set_order(song.ptr_mut(), 0, 0, 0, 207, edit),
            2 => gt2_order_insert(song.ptr_mut(), 0, 0, 0, 207, edit),
            _ => unreachable!(),
        }
    }
}

#[test]
fn trailing_removal_blocks_queued_higher_row_and_order_activation_until_release_or_cancel() {
    for kind in 0..3 {
        for cancel in [false, true] {
            let mut song = trailing_removal_fixture(if kind == 2 { 65_526 } else { 65_527 });
            let mut removal = gt2_edit::default();
            assert_eq!(
                unsafe { gt2_pattern_remove(song.ptr_mut(), 206, &mut removal) },
                GT2_OK
            );
            assert_eq!(removal.reserved, 0);
            let before = song.bytes().to_vec();
            let mut activation = gt2_edit::default();
            assert_eq!(
                activate_pattern_207(&mut song, kind, &mut activation),
                GT2_ERR_PENDING,
                "kind {kind} cancel {cancel}"
            );
            assert_eq!(song.bytes(), before);
            if cancel {
                unsafe { gt2_edit_cancel(song.ptr_mut(), &removal) };
                assert_eq!(
                    activate_pattern_207(&mut song, kind, &mut activation),
                    GT2_OK
                );
                unsafe { gt2_edit_commit(song.ptr_mut(), &activation) };
                assert_eq!(song.write(true).len(), GT2_SAVED_MAX as usize);
            } else {
                unsafe { gt2_edit_apply(song.ptr_mut(), &removal) };
                let before = song.bytes().to_vec();
                assert_eq!(
                    activate_pattern_207(&mut song, kind, &mut activation),
                    GT2_ERR_PENDING
                );
                assert_eq!(song.bytes(), before);
                unsafe { gt2_edit_release(song.ptr_mut(), &removal) };
                let before = song.bytes().to_vec();
                assert_eq!(
                    activate_pattern_207(&mut song, kind, &mut activation),
                    GT2_ERR_SONG_FULL
                );
                assert_eq!(song.bytes(), before);
            }
            check_size(&song);
            assert!(song.write(true).len() <= GT2_SAVED_MAX as usize);
        }
    }
}

#[test]
fn queued_higher_activation_blocks_zero_reservation_removal_in_reverse_order() {
    for kind in 0..3 {
        let mut song = trailing_removal_fixture(if kind == 2 { 65_526 } else { 65_527 });
        let mut activation = gt2_edit::default();
        assert_eq!(
            activate_pattern_207(&mut song, kind, &mut activation),
            GT2_OK
        );
        let before = song.bytes().to_vec();
        let mut removal = gt2_edit::default();
        assert_eq!(
            unsafe { gt2_pattern_remove(song.ptr_mut(), 206, &mut removal) },
            GT2_ERR_PENDING,
            "kind {kind}"
        );
        assert_eq!(song.bytes(), before);
        unsafe { gt2_edit_cancel(song.ptr_mut(), &activation) };
        assert_eq!(
            unsafe { gt2_pattern_remove(song.ptr_mut(), 206, &mut removal) },
            GT2_OK
        );
        unsafe { gt2_edit_commit(song.ptr_mut(), &removal) };
        check_size(&song);
        assert!(song.write(true).len() <= GT2_SAVED_MAX as usize);
    }
}

#[test]
fn accounted_size_matches_writes_through_random_edits_and_cancellations() {
    let mut rng = Rng(0xca9a_c170);
    for seed in 0..20 {
        let mut song = Song::device();
        song.read(&random_canonical(&mut rng, seed % 3 == 0).encode(), 4)
            .unwrap();
        check_size(&song);
        for step in 0..300 {
            let mut edit = gt2_edit::default();
            let p = rng.below(12);
            let result = unsafe {
                match rng.below(9) {
                    0 => gt2_pattern_resize(song.ptr_mut(), p, 1 + rng.below(128), &mut edit),
                    1 => gt2_pattern_insert_row(song.ptr_mut(), p, rng.below(8), &mut edit),
                    2 => gt2_pattern_delete_row(song.ptr_mut(), p, rng.below(8), &mut edit),
                    3 => {
                        let cell = [
                            if rng.chance(2) { 0x60 } else { 0xbd },
                            rng.below(64) as u8,
                            0,
                            0,
                        ];
                        gt2_set_row(song.ptr_mut(), p, rng.below(8), cell.as_ptr(), &mut edit)
                    }
                    4 => gt2_set_instr(
                        song.ptr_mut(),
                        1 + rng.below(63),
                        rng.below(25),
                        rng.below(2) as u8,
                        &mut edit,
                    ),
                    5 => gt2_set_table(
                        song.ptr_mut(),
                        rng.below(4),
                        rng.below(255),
                        rng.below(2),
                        rng.below(2) as u8,
                        &mut edit,
                    ),
                    6 => gt2_order_insert(song.ptr_mut(), 0, rng.below(3), 0, 0, &mut edit),
                    7 => gt2_order_delete(song.ptr_mut(), 0, rng.below(3), 0, &mut edit),
                    _ => gt2_pattern_remove(song.ptr_mut(), p, &mut edit),
                }
            };
            if result == GT2_OK {
                if rng.chance(4) {
                    unsafe { gt2_edit_cancel(song.ptr_mut(), &edit) };
                } else {
                    unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
                }
            }
            assert!(
                song.write(true).len() <= GT2_SAVED_MAX as usize,
                "seed {seed} step {step}"
            );
            check_size(&song);
        }
    }
}

#[test]
fn highest_pattern_removal_frees_capacity_even_at_the_exact_limit() {
    let mut song = Song::device();
    song.read(&holes_fixture(), 4).unwrap();
    let payload = vec![1u8; (GT2_SAVED_MAX as usize - song.write(true).len()) - 8];
    assert_eq!(
        unsafe {
            song_ext_set(
                song.ptr_mut(),
                tag(b"TEST"),
                payload.as_ptr(),
                payload.len() as u32,
            )
        },
        0
    );
    assert_eq!(song.write(true).len(), GT2_SAVED_MAX as usize);
    let mut edit = gt2_edit::default();
    assert_eq!(
        unsafe { gt2_pattern_remove(song.ptr_mut(), 207, &mut edit) },
        GT2_OK
    );
    unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
    check_size(&song);
    assert!(song.write(true).len() < GT2_SAVED_MAX as usize);
}

#[test]
fn pending_growth_is_reserved_and_cancellation_returns_the_reservation() {
    let mut song = Song::device();
    song.read(&holes_fixture(), 4).unwrap();
    let mut edits = Vec::new();
    for _ in 0..3 {
        let mut edit = gt2_edit::default();
        let mut pattern = 0;
        assert_eq!(
            unsafe { gt2_pattern_add(song.ptr_mut(), 128, &mut pattern, &mut edit) },
            GT2_OK
        );
        edits.push(edit);
    }
    let mut rejected = gt2_edit::default();
    let mut pattern = 0;
    let before = song.bytes().to_vec();
    assert_eq!(
        unsafe { gt2_pattern_add(song.ptr_mut(), 128, &mut pattern, &mut rejected) },
        GT2_ERR_SONG_FULL
    );
    assert_eq!(song.bytes(), before);
    let cancelled = edits.pop().unwrap();
    unsafe { gt2_edit_cancel(song.ptr_mut(), &cancelled) };
    assert_eq!(
        unsafe { gt2_pattern_add(song.ptr_mut(), 128, &mut pattern, &mut rejected) },
        GT2_OK
    );
    edits.push(rejected);
    for edit in edits {
        unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
    }
    check_size(&song);
    assert_eq!(song.write(true).len(), 65_426);
}

#[test]
fn freeze_and_revision_cover_same_size_changes_and_unchanged_capture() {
    let mut song = Song::device();
    let revision = song.raw().revision;
    let mut edit = gt2_edit::default();
    let cell = [0x60, 0, 0, 0];
    assert_eq!(
        unsafe { gt2_set_row(song.ptr_mut(), 0, 0, cell.as_ptr(), &mut edit) },
        GT2_OK
    );
    unsafe { gt2_edit_cancel(song.ptr_mut(), &edit) };
    assert_eq!(song.raw().revision, revision);
    assert_eq!(
        unsafe { gt2_set_row(song.ptr_mut(), 0, 0, cell.as_ptr(), &mut edit) },
        GT2_OK
    );
    unsafe { gt2_edit_commit(song.ptr_mut(), &edit) };
    assert_ne!(song.raw().revision, revision);
    let options = gt2_options::default();
    assert_eq!(unsafe { song_ext_set_options(song.ptr_mut(), &options) }, 0);
    let revision = song.raw().revision;
    assert_eq!(unsafe { song_ext_set_options(song.ptr_mut(), &options) }, 0);
    assert_eq!(song.raw().revision, revision);
    unsafe { (*song.ptr_mut()).frozen = 1 };
    let before = song.bytes().to_vec();
    assert_eq!(
        unsafe { gt2_set_row(song.ptr_mut(), 0, 0, cell.as_ptr(), &mut edit) },
        GT2_ERR_SAVING
    );
    assert_eq!(
        unsafe { gt2_set_instr(song.ptr_mut(), 1, 0, 1, &mut edit) },
        GT2_ERR_SAVING
    );
    assert_eq!(
        unsafe { gt2_set_table(song.ptr_mut(), 0, 0, 0, 1, &mut edit) },
        GT2_ERR_SAVING
    );
    assert_eq!(
        unsafe { gt2_pattern_resize(song.ptr_mut(), 0, 1, &mut edit) },
        GT2_ERR_SAVING
    );
    assert_eq!(
        unsafe { song_lock_put(song.ptr_mut(), 0, 0, 0, 10) },
        -(GT2_ERR_SAVING as i32)
    );
    assert_eq!(
        unsafe { song_ext_set_options(song.ptr_mut(), &options) },
        -(GT2_ERR_SAVING as i32)
    );
    assert_eq!(song.bytes(), before);
}

#[test]
fn first_lock_failure_is_atomic_and_trailer_padding_is_accounted() {
    let mut song = Song::device();
    song.read(&holes_fixture(), 4).unwrap();
    let payload = vec![1u8; (GT2_SAVED_MAX as usize - song.write(true).len()) - 8];
    assert_eq!(
        unsafe {
            song_ext_set(
                song.ptr_mut(),
                tag(b"TEST"),
                payload.as_ptr(),
                payload.len() as u32,
            )
        },
        0
    );
    let before = song.bytes().to_vec();
    assert_eq!(
        unsafe { song_lock_put(song.ptr_mut(), 0, 0, 0, 10) },
        -(GT2_ERR_SONG_FULL as i32)
    );
    assert_eq!(song.bytes(), before);
    let mut small = Song::device();
    for len in 1..1024 {
        let payload = vec![1u8; len];
        assert_eq!(
            unsafe { song_ext_set(small.ptr_mut(), tag(b"TEST"), payload.as_ptr(), len as u32) },
            0
        );
        check_size(&small);
    }
}
