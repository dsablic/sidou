// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;
use sidou_tests::song::Song;
use std::ffi::c_void;

const INSTRUMENT: u32 = 2;

unsafe fn copy_out(bytes: &[u8], at: usize, out: *mut u8, len: u32) -> usize {
    let n = bytes.len().saturating_sub(at).min(len as usize);
    if n != 0 {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr().add(at), out, n) };
    }
    n
}

unsafe extern "C" fn read(ctx: *mut c_void, at: u32, out: *mut u8, len: u32) -> u32 {
    let bytes = unsafe { &*ctx.cast::<Vec<u8>>() };
    unsafe { copy_out(bytes, at as usize, out, len) as u32 }
}

fn fixture() -> Vec<u8> {
    let mut bytes = b"GTI5".to_vec();
    let mut instr = [0; 25];
    instr[0] = 0x12;
    instr[2..6].copy_from_slice(&[10, 20, 30, 40]);
    bytes.extend(instr);
    bytes.extend([3, 0xf1, 0x21, 0xff, 41, 0x80, 11]);
    bytes.extend([2, 0x88, 0xff, 0, 20]);
    bytes.extend([2, 0x90, 0xff, 0, 0]);
    bytes.extend([2, 0, 1, 9, 2]);
    bytes
}

fn import(song: &mut Song, bytes: &mut Vec<u8>) -> gt2_err {
    unsafe {
        gt2_ins_import(
            song.ptr_mut(),
            INSTRUMENT,
            Some(read),
            (bytes as *mut Vec<u8>).cast(),
            bytes.len() as u32,
        )
    }
}

fn snapshot(song: &Song) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(song.ptr().cast::<u8>(), SONG_REGION_BYTES as usize) }
        .to_vec()
}

fn import_refused_leaving_the_song_untouched(song: &mut Song, bytes: &mut Vec<u8>) -> gt2_err {
    let before = snapshot(song);
    let err = import(song, bytes);
    assert_ne!(err, GT2_OK);
    assert_eq!(snapshot(song), before, "a refused import changed the song");
    err
}

fn export_begin(out: &mut gt2_ins_export, song: &Song) -> gt2_err {
    unsafe { gt2_ins_export_begin(out, song.ptr(), INSTRUMENT) }
}

fn export_read(out: &mut gt2_ins_export, at: usize, buf: &mut [u8]) -> usize {
    unsafe {
        gt2_ins_export_read(
            (out as *mut gt2_ins_export).cast(),
            at as u32,
            buf.as_mut_ptr(),
            buf.len() as u32,
        ) as usize
    }
}

#[test]
fn export_refuses_a_sequence_that_runs_off_the_last_table_row() {
    let mut song = Song::device();
    unsafe {
        let s = &mut *song.ptr_mut();
        s.instr[0][GT2_INS_WAVE as usize] = 1;
        s.ltable[GT2_WTBL as usize].fill(0x21);
        let mut descriptor = gt2_ins_export::default();
        assert_eq!(gt2_ins_export_begin(&mut descriptor, s, 1), GT2_ERR_RANGE);
        s.ltable[GT2_WTBL as usize][254] = 0xff;
        assert_eq!(gt2_ins_export_begin(&mut descriptor, s, 1), GT2_OK);
    }
}

#[test]
fn relocates_jumps_and_wave_speed_commands_without_touching_old_tables() {
    let mut song = Song::device();
    unsafe {
        let s = &mut *song.ptr_mut();
        for t in 0..4 {
            s.ltable[t][0] = 0xff;
            s.rtable[t][0] = 0;
        }
        gt2_saved_recount(s);
    }
    let revision = unsafe { (*song.ptr()).revision };
    assert_eq!(import(&mut song, &mut fixture()), GT2_OK);
    unsafe {
        let s = &*song.ptr();
        assert_eq!(&s.instr[1][2..6], &[2, 2, 2, 2]);
        assert_eq!(s.rtable[0][1], 3);
        assert_eq!(s.rtable[0][3], 3);
        assert_eq!(s.rtable[1][2], 2);
        assert_eq!(s.ltable[0][0], 0xff);
        assert_eq!(s.revision, revision + 1);
        let size = gt2_saved_size(s);
        gt2_saved_recount(song.ptr_mut());
        assert_eq!(gt2_saved_size(song.ptr()), size);
    }
}

#[test]
fn malformed_truncated_and_full_files_preserve_every_song_byte() {
    let mut song = Song::device();
    let valid = fixture();
    for n in 0..valid.len() {
        import_refused_leaving_the_song_untouched(&mut song, &mut valid[..n].to_vec());
    }
    let mut invalid = valid.clone();
    invalid[33] = 99;
    import_refused_leaving_the_song_untouched(&mut song, &mut invalid);
    unsafe {
        (*song.ptr_mut()).ltable[0][254] = 1;
        gt2_saved_recount(song.ptr_mut());
    }
    assert_eq!(
        import_refused_leaving_the_song_untouched(&mut song, &mut fixture()),
        GT2_ERR_FULL
    );
}

#[test]
fn refuses_save_freeze_and_pending_edits() {
    let mut song = Song::device();
    unsafe { (*song.ptr_mut()).frozen = 1 };
    assert_eq!(
        import_refused_leaving_the_song_untouched(&mut song, &mut fixture()),
        GT2_ERR_SAVING
    );
    unsafe {
        (*song.ptr_mut()).frozen = 0;
        (*song.ptr_mut()).pending_loose = 1;
    }
    assert_eq!(
        import_refused_leaving_the_song_untouched(&mut song, &mut fixture()),
        GT2_ERR_PENDING
    );
}

#[test]
fn protects_zero_speed_rows_referenced_by_patterns_and_wave_commands() {
    let mut song = Song::device();
    unsafe {
        let s = &mut *song.ptr_mut();
        s.instr[0][5] = 60;
        s.ltable[0][0] = 0xf4;
        s.rtable[0][0] = 80;
        s.ltable[0][1] = 0xff;
        let mut edit = gt2_edit::default();
        let row = [GT2_REST as u8, 0, 1, 90];
        assert_eq!(gt2_set_row(s, 0, 0, row.as_ptr(), &mut edit), GT2_OK);
        gt2_edit_commit(s, &edit);
        gt2_saved_recount(s);
    }
    assert_eq!(import(&mut song, &mut fixture()), GT2_OK);
    unsafe {
        assert_eq!((*song.ptr()).instr[1][5], 91);
        assert_eq!((*song.ptr()).rtable[0][2], 92);
        assert_eq!((*song.ptr()).ltable[3][89], 0);
    }
}

#[test]
fn indexed_export_replays_identically_at_every_packet_size() {
    let mut song = Song::device();
    assert_eq!(import(&mut song, &mut fixture()), GT2_OK);
    let mut out = gt2_ins_export::default();
    assert_eq!(export_begin(&mut out, &song), GT2_OK);
    assert!(size_of::<gt2_ins_export>() <= 150);
    let mut expected = vec![0; out.length as usize];
    assert_eq!(export_read(&mut out, 0, &mut expected), out.length as usize);
    assert_eq!(&expected[..4], b"GTI5");
    for packet in 1..=expected.len() + 1 {
        let mut actual = Vec::<u8>::new();
        loop {
            let mut chunk = vec![0; packet];
            let n = export_read(&mut out, actual.len(), &mut chunk);
            if n == 0 {
                break;
            }
            actual.extend(&chunk[..n]);
        }
        assert_eq!(actual, expected);
    }
    let mut other = Song::device();
    assert_eq!(import(&mut other, &mut expected), GT2_OK);
    assert_eq!(other.raw().rtable[0][0], 2);
}

#[test]
fn owned_import_keeps_freeze_and_save_cap_refusal_is_atomic() {
    let mut song = Song::device();
    unsafe { (*song.ptr_mut()).frozen = 1 };
    let mut bytes = fixture();
    assert_eq!(
        unsafe {
            gt2_ins_import_owned(
                song.ptr_mut(),
                INSTRUMENT,
                Some(read),
                (&mut bytes as *mut Vec<u8>).cast(),
                bytes.len() as u32,
            )
        },
        GT2_OK
    );
    assert_eq!(song.raw().frozen, 1);
    unsafe {
        (*song.ptr_mut()).frozen = 0;
        (*song.ptr_mut()).saved_bytes = GT2_SAVED_MAX;
    }
    assert_eq!(
        import_refused_leaving_the_song_untouched(&mut song, &mut bytes),
        GT2_ERR_SONG_FULL
    );
}

#[test]
fn invalid_jumps_command_references_and_extra_bytes_are_atomic() {
    let mut song = Song::device();
    for (index, value) in [(35, 200), (35, 12), (33, 42), (37, 0xff), (29, 255)] {
        let mut bytes = fixture();
        bytes[index] = value;
        import_refused_leaving_the_song_untouched(&mut song, &mut bytes);
    }
    let mut bytes = fixture();
    bytes.push(0);
    import_refused_leaving_the_song_untouched(&mut song, &mut bytes);
}

#[test]
fn relocates_wave_pulse_filter_and_funktempo_operands() {
    for (command, pointer, relocated) in
        [(0xf8, 11, 2), (0xf9, 20, 1), (0xfa, 30, 1), (0xfe, 41, 2)]
    {
        let mut song = Song::device();
        let mut bytes = fixture();
        bytes[30] = command;
        bytes[33] = pointer;
        assert_eq!(import(&mut song, &mut bytes), GT2_OK);
        assert_eq!(song.raw().rtable[0][0], relocated);
    }
}

#[test]
fn import_updates_only_affected_save_accounting() {
    let mut song = Song::device();
    unsafe {
        let mut edit = gt2_edit::default();
        let row = [GT2_REST as u8, 7, 0, 0];
        assert_eq!(
            gt2_set_row(song.ptr_mut(), 0, 0, row.as_ptr(), &mut edit),
            GT2_OK
        );
        gt2_edit_commit(song.ptr_mut(), &edit);
    }
    let work = song.raw().saved_row_work;
    let refs = song.raw().saved_instr_base;
    assert_eq!(import(&mut song, &mut fixture()), GT2_OK);
    assert_eq!(song.raw().saved_row_work, work);
    assert_eq!(song.raw().saved_instr_base, refs);
    let size = unsafe { gt2_saved_size(song.ptr()) };
    let tables = song.raw().saved_table_bits;
    let instruments = song.raw().saved_instr_defs;
    unsafe { gt2_saved_recount(song.ptr_mut()) };
    assert_eq!(unsafe { gt2_saved_size(song.ptr()) }, size);
    assert_eq!(song.raw().saved_table_bits, tables);
    assert_eq!(song.raw().saved_instr_defs, instruments);
}

struct CountedSource {
    bytes: Vec<u8>,
    seen: Vec<bool>,
    read_bytes: u32,
}

unsafe extern "C" fn counted_read(ctx: *mut c_void, at: u32, out: *mut u8, len: u32) -> u32 {
    let source = unsafe { &mut *ctx.cast::<CountedSource>() };
    let at = at as usize;
    let n = unsafe { copy_out(&source.bytes, at, out, len) };
    source.seen[at..at + n].fill(true);
    source.read_bytes += n as u32;
    n as u32
}

#[test]
fn stepped_import_preflights_every_byte_then_replays_in_small_slices() {
    let mut song = Song::device();
    let bytes = fixture();
    let mut source = CountedSource {
        seen: vec![false; bytes.len()],
        bytes: bytes.clone(),
        read_bytes: 0,
    };
    let mut state = gt2_ins_import_state::default();
    assert!(size_of::<gt2_ins_import_state>() <= 150);
    unsafe { (*song.ptr_mut()).frozen = 1 };
    let before = snapshot(&song);
    assert_eq!(
        unsafe {
            gt2_ins_import_begin(
                &mut state,
                song.ptr_mut(),
                INSTRUMENT,
                Some(counted_read),
                (&mut source as *mut CountedSource).cast(),
                bytes.len() as u32,
                1,
            )
        },
        GT2_OK
    );
    assert_eq!(snapshot(&song), before);
    let mut steps = 0;
    loop {
        source.read_bytes = 0;
        let result = unsafe { gt2_ins_import_step(&mut state) };
        assert!(source.read_bytes <= 24);
        if snapshot(&song) != before {
            assert!(source.seen.iter().all(|seen| *seen));
        }
        steps += 1;
        assert!(steps < 5_000);
        assert!(result >= 0, "import failed: {}", state.error);
        if result == 1 {
            break;
        }
    }
    assert!(steps > 1);
    assert_eq!(source.bytes, bytes);
    assert_eq!(song.raw().frozen, 1);
    assert_eq!(&song.raw().instr[1][2..6], &[1, 1, 1, 1]);
    let done = snapshot(&song);
    assert_eq!(unsafe { gt2_ins_import_step(&mut state) }, 1);
    assert_eq!(snapshot(&song), done);
}

#[test]
fn stepped_import_late_refusal_is_atomic_and_terminal() {
    let mut song = Song::device();
    let mut bytes = fixture();
    bytes[45] = 31;
    let before = snapshot(&song);
    let mut state = gt2_ins_import_state::default();
    assert_eq!(
        unsafe {
            gt2_ins_import_begin(
                &mut state,
                song.ptr_mut(),
                INSTRUMENT,
                Some(read),
                (&mut bytes as *mut Vec<u8>).cast(),
                bytes.len() as u32,
                0,
            )
        },
        GT2_OK
    );
    let mut result = 0;
    for _ in 0..5_000 {
        result = unsafe { gt2_ins_import_step(&mut state) };
        assert_eq!(snapshot(&song), before);
        if result != 0 {
            break;
        }
    }
    assert_eq!(result, -1);
    assert_eq!(state.error, GT2_ERR_RANGE);
    assert_eq!(unsafe { gt2_ins_import_step(&mut state) }, -1);
}

#[test]
fn export_retains_original_bases_and_unrelocated_snapshot_bytes() {
    let mut song = Song::device();
    let expected = fixture();
    unsafe {
        let s = &mut *song.ptr_mut();
        s.instr[1].copy_from_slice(&expected[4..29]);
        let mut at = 29;
        for t in 0..4 {
            let rows = expected[at] as usize;
            let base = s.instr[1][2 + t] as usize - 1;
            at += 1;
            s.ltable[t][base..base + rows].copy_from_slice(&expected[at..at + rows]);
            at += rows;
            s.rtable[t][base..base + rows].copy_from_slice(&expected[at..at + rows]);
            at += rows;
        }
    }
    let before = snapshot(&song);
    let mut out = gt2_ins_export::default();
    assert_eq!(export_begin(&mut out, &song), GT2_OK);
    let mut actual = vec![0; out.length as usize];
    assert_eq!(export_read(&mut out, 0, &mut actual), out.length as usize);
    assert_eq!(actual, expected);
    assert_eq!(snapshot(&song), before);
}

#[test]
fn export_checks_commands_in_wavetable_rows_added_by_another_command() {
    let mut song = Song::device();
    unsafe {
        let s = &mut *song.ptr_mut();
        s.instr[1][2..6].copy_from_slice(&[10, 20, 30, 40]);
        s.ltable[0][9..14].copy_from_slice(&[0xf8, 0xff, 0xf9, 0xf1, 0xff]);
        s.rtable[0][9..14].copy_from_slice(&[13, 0, 22, 42, 0]);
        s.ltable[1][19..23].copy_from_slice(&[0x88, 0xff, 0x88, 0xff]);
        s.ltable[2][29..31].copy_from_slice(&[0x90, 0xff]);
        s.ltable[3][39..42].copy_from_slice(&[0, 1, 2]);
    }
    let mut out = gt2_ins_export::default();
    assert_eq!(export_begin(&mut out, &song), GT2_OK);
    assert_eq!(out.rows, [5, 4, 2, 3]);
    unsafe { (*song.ptr_mut()).rtable[0][12] = 39 };
    assert_eq!(export_begin(&mut out, &song), GT2_ERR_RANGE);
}

#[test]
fn export_refuses_unrepresentable_dependencies_and_invalid_graphs() {
    let mut song = Song::device();
    assert_eq!(import(&mut song, &mut fixture()), GT2_OK);
    let mut out = gt2_ins_export::default();
    unsafe {
        (*song.ptr_mut()).instr[1][5] = 2;
        (*song.ptr_mut()).rtable[0][0] = 1;
    }
    assert_eq!(export_begin(&mut out, &song), GT2_ERR_RANGE);
    unsafe {
        (*song.ptr_mut()).instr[1][5] = 1;
        (*song.ptr_mut()).rtable[0][2] = 3;
    }
    assert_eq!(export_begin(&mut out, &song), GT2_ERR_RANGE);
}
