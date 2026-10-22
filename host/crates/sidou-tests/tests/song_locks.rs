// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Parameter locks (M2c Task 7): the LOCK chunk's records kept sorted, carried through the
//! trailer and through the pattern edits that move rows, and played: a locked row's register
//! holds the lock's value after each of its calls, the player itself plays as without locks,
//! and the song's own value is back at the next row; the character's locks reach the hook.

use std::ffi::c_void;

use sidou_sys::*;
use sidou_tests::song::*;
use sidou_tests::transport::*;

const R: [u8; 4] = [0xbd, 0, 0, 0];

fn row(note: u8, instr: u8) -> [u8; 4] {
    [note, instr, 0, 0]
}

/// Three channels of 8-row patterns (6 ticks a row), a pulse instrument.
fn sng() -> Vec<u8> {
    let p0 = vec![row(0x60 + 36, 1), R, R, R, row(0x60 + 40, 0), R, R, R];
    let p1 = vec![row(0x60 + 24, 1), R, R, R, R, R, R, R];
    let p2 = vec![row(0x60 + 19, 1), R, R, R, R, R, R, R];
    let mut s = SngSpec::new();
    s.orders = vec![vec![(vec![0], 0), (vec![1], 0), (vec![2], 0)]];
    let mut ins = [0u8; 25];
    ins[..9].copy_from_slice(&[0x09, 0xa0, 1, 1, 0, 0, 0, 2, 0x09]);
    s.instr = vec![ins];
    s.tables = vec![
        vec![(0x41, 0x00), (0xff, 0x00)],
        vec![(0x88, 0x00), (0xff, 0x00)],
        vec![],
        vec![],
    ];
    s.patterns = vec![p0, p1, p2]
        .into_iter()
        .map(|p| (p, [0xff, 0, 0, 0]))
        .collect();
    s.encode()
}

fn song() -> Song {
    let mut s = Song::host();
    s.read(&sng(), 0).unwrap();
    s
}

fn locks(s: &Song) -> Vec<(u8, u8, u8, u16)> {
    let mut v = vec![song_lock::default(); SONG_LOCKS_MAX as usize];
    let n = unsafe { song_ext_get_locks(s.ptr(), v.as_mut_ptr(), v.len() as u32) } as usize;
    v[..n]
        .iter()
        .map(|l| (l.pattern, l.row, l.param, l.value))
        .collect()
}

fn put(s: &mut Song, p: u32, r: u32, param: u32, v: u32) {
    assert_eq!(unsafe { song_lock_put(s.ptr_mut(), p, r, param, v) }, 0);
}

#[test]
fn locks_stay_sorted_and_round_trip_through_the_trailer() {
    let mut s = song();
    put(&mut s, 2, 5, SONG_LOCK_PULSE, 3000);
    put(&mut s, 0, 3, SONG_LOCK_CUTOFF, 900);
    put(&mut s, 0, 3, SONG_LOCK_RESONANCE, 12);
    put(&mut s, 0, 1, SONG_LOCK_AGE, 100);
    put(&mut s, 0, 3, SONG_LOCK_CUTOFF, 1000); // a new value, not a new lock
    let want = vec![
        (0, 1, SONG_LOCK_AGE as u8, 100),
        (0, 3, SONG_LOCK_CUTOFF as u8, 1000),
        (0, 3, SONG_LOCK_RESONANCE as u8, 12),
        (2, 5, SONG_LOCK_PULSE as u8, 3000),
    ];
    assert_eq!(locks(&s), want);
    assert_eq!(
        unsafe { song_lock_put(s.ptr_mut(), 0, 0, SONG_LOCK_CUTOFF, 2048) },
        -1
    );
    assert_eq!(
        unsafe { song_lock_put(s.ptr_mut(), 0, 0, SONG_LOCK_PARAMS, 0) },
        -1
    );
    let mut n = 0u32;
    let rec = unsafe { song_locks_row(s.ptr(), 0, 3, &mut n) };
    assert_eq!(n, 2);
    assert!(!rec.is_null());
    let file = s.write(true);
    let mut back = Song::host();
    back.read(&file, 0).unwrap();
    assert_eq!(locks(&back), want, "through the trailer");
    assert_eq!(
        unsafe { song_lock_remove(s.ptr_mut(), 0, 3, SONG_LOCK_CUTOFF) },
        1
    );
    assert_eq!(
        unsafe { song_lock_remove(s.ptr_mut(), 0, 3, SONG_LOCK_CUTOFF) },
        0
    );
    assert_eq!(locks(&s).len(), 3);
}

#[test]
fn the_gt2_parts_crc_rule_drops_the_locks() {
    let mut s = song();
    put(&mut s, 0, 3, SONG_LOCK_CUTOFF, 900);
    let mut file = s.write(true);
    file[4] ^= 0x20; // the song's name: the GT2 part changed, its CRC no longer matches
    let mut back = Song::host();
    let info = back.read(&file, 0).unwrap();
    assert_eq!(info.msg, "SNG: LOCKS DROPPED");
    assert!(locks(&back).is_empty());
}

/// The song's LOCK chunk's records (SONG_LOCK_BYTES each), to damage them in place.
fn lock_records(s: &mut Song) -> &mut [u8] {
    let mut len = 0u32;
    let d = unsafe { song_ext_find(s.ptr(), u32::from_le_bytes(*b"LOCK"), &mut len) };
    assert!(!d.is_null(), "no LOCK chunk");
    unsafe { std::slice::from_raw_parts_mut(d.cast_mut().add(3), len as usize - 3) }
}

unsafe extern "C" fn read_bytes(ctx: *mut c_void, at: u32, buf: *mut u8, len: u32) -> u32 {
    let bytes = unsafe { &*ctx.cast::<Vec<u8>>() };
    let n = bytes.len().saturating_sub(at as usize).min(len as usize);
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr().add(at as usize), buf, n) };
    n as u32
}

/// The file through the streamed reader (gt2_stream, as the device loads from the store):
/// its status and message.
fn stream_load(s: &mut Song, file: &[u8]) -> (i32, String) {
    let mut w = gt2_stream::default();
    let bytes = file.to_vec();
    let caps = gt2_caps { max_multispeed: 0 };
    unsafe {
        gt2_stream_read_begin(
            &mut w,
            s.ptr_mut(),
            Some(read_bytes),
            (&bytes as *const Vec<u8>).cast_mut().cast(),
            &caps,
        )
    };
    for _ in 0..100_000 {
        let status = unsafe { gt2_stream_read_step(&mut w) };
        if status != 0 {
            let msg = w.info.msg.iter().take_while(|&&c| c != 0);
            return (status, msg.map(|&c| c as u8 as char).collect());
        }
    }
    panic!("the reader did not finish")
}

/// A LOCK chunk whose CRCs hold but whose records are out of range or out of order is dropped
/// when the file is read, by both readers, with the message (review 3.6): a filter-mode lock of
/// 8 would set $18's bit 7 (voice 3 off), and unsorted records hide locks from the row search.
#[test]
fn a_lock_chunk_out_of_range_or_out_of_order_is_dropped_when_read() {
    let base = || {
        let mut s = song();
        put(&mut s, 0, 1, SONG_LOCK_FILTER_MODE, 2);
        put(&mut s, 0, 3, SONG_LOCK_CUTOFF, 900);
        put(&mut s, 1, 2, SONG_LOCK_PULSE, 3000);
        s
    };
    let good = base().write(true);
    let mut back = Song::host();
    assert_eq!(back.read(&good, 0).unwrap().msg, "");
    assert_eq!(locks(&back).len(), 3, "a good chunk stays");
    assert_eq!(stream_load(&mut Song::host(), &good), (1, String::new()));

    let damage: [(&str, fn(&mut [u8])); 6] = [
        ("a filter mode of 8", |r| r[3] = 8),
        ("a cutoff past 2047", |r| {
            r[5 + 3..5 + 5].copy_from_slice(&2048u16.to_le_bytes())
        }),
        ("a parameter past the last", |r| {
            r[2] = SONG_LOCK_PARAMS as u8
        }),
        ("a row past GT2_MAX_ROWS", |r| r[1] = GT2_MAX_ROWS as u8),
        ("a pattern past GT2_MAX_PATTERNS", |r| {
            r[10] = GT2_MAX_PATTERNS as u8
        }),
        ("two records out of order", |r| {
            let (a, b) = r.split_at_mut(5);
            a.swap_with_slice(&mut b[..5]);
        }),
    ];
    for (what, f) in damage {
        let mut s = base();
        f(lock_records(&mut s));
        let file = s.write(true);
        let mut back = Song::host();
        let info = back.read(&file, 0).unwrap();
        assert_eq!(info.msg, "SNG: LOCKS DROPPED", "{what}");
        assert!(locks(&back).is_empty(), "{what}: the chunk kept");
        let mut streamed = Song::host();
        assert_eq!(
            stream_load(&mut streamed, &file),
            (1, "SNG: LOCKS DROPPED".to_string()),
            "{what}: streamed"
        );
        assert!(locks(&streamed).is_empty(), "{what}: streamed, kept");
        assert_eq!(
            streamed.write(true),
            back.write(true),
            "{what}: both readers alike"
        );
    }
}

#[test]
fn pattern_edits_carry_their_locks() {
    let mut s = song();
    put(&mut s, 0, 2, SONG_LOCK_CUTOFF, 100);
    put(&mut s, 0, 5, SONG_LOCK_CUTOFF, 500);
    put(&mut s, 0, 7, SONG_LOCK_PULSE, 700);
    put(&mut s, 1, 5, SONG_LOCK_CUTOFF, 900);
    let p = s.ptr_mut();
    let edit = |f: &dyn Fn(*mut gt2_song, &mut gt2_edit) -> u32| {
        let mut e = gt2_edit::default();
        assert_eq!(f(p, &mut e), GT2_OK);
        unsafe { gt2_edit_commit(p, &e) };
    };
    edit(&|s, e| unsafe { gt2_pattern_insert_row(s, 0, 3, e) });
    assert_eq!(
        locks(&s),
        vec![
            (0, 2, 0, 100),
            (0, 6, 0, 500),
            (0, 8, 4, 700),
            (1, 5, 0, 900)
        ],
        "a row inserted at 3: the locks after it move down"
    );
    edit(&|s, e| unsafe { gt2_pattern_delete_row(s, 0, 2, e) });
    assert_eq!(
        locks(&s),
        vec![(0, 5, 0, 500), (0, 7, 4, 700), (1, 5, 0, 900)],
        "row 2 deleted: its lock goes, the others move up"
    );
    edit(&|s, e| unsafe { gt2_pattern_resize(s, 0, 6, e) });
    assert_eq!(
        locks(&s),
        vec![(0, 5, 0, 500), (1, 5, 0, 900)],
        "cut to 6 rows"
    );
    // an edit never applied moves nothing
    let mut e = gt2_edit::default();
    assert_eq!(unsafe { gt2_pattern_insert_row(p, 0, 0, &mut e) }, GT2_OK);
    unsafe { gt2_edit_cancel(p, &e) };
    assert_eq!(locks(&s), vec![(0, 5, 0, 500), (1, 5, 0, 900)], "a cancel");
}

/// The transport through `calls` calls: after each call, the chip's cutoff and voice 1's
/// pulse width, and the row channel 1 plays.
fn play(s: &mut Song, calls: u32) -> (Transport, Vec<(u32, u32, u8, u8)>) {
    let mut t = Transport::new(s);
    assert!(t.play(0, 0));
    let mut out = Vec::new();
    while t.calls() < calls {
        t.pass();
        let c = t.calls();
        t.block_silent();
        if t.calls() != c {
            out.push((
                t.chip.fc,
                t.chip.osc[0].pw,
                t.sc.row_pat[0],
                t.sc.row_row[0],
            ));
        }
    }
    (t, out)
}

#[test]
fn a_locked_row_holds_its_values_and_the_song_returns_after() {
    let mut plain = song();
    let (t0, base) = play(&mut plain, 300);
    let mut s = song();
    put(&mut s, 0, 3, SONG_LOCK_CUTOFF, 1234);
    put(&mut s, 0, 3, SONG_LOCK_PULSE, 0x321);
    put(&mut s, 0, 6, SONG_LOCK_PULSE, 0x777);
    let (t, got) = play(&mut s, 300);
    let mut locked = 0;
    for (k, (&(fc, pw, pat, r), &(fc0, pw0, _, _))) in got.iter().zip(&base).enumerate() {
        match (pat, r) {
            (0, 3) => {
                assert_eq!((fc, pw), (1234, 0x321), "call {k}: row 3 locked");
                locked += 1;
            }
            (0, 6) => {
                assert_eq!((fc, pw), (fc0, 0x777), "call {k}: row 6's pulse");
                locked += 1;
            }
            _ => assert_eq!((fc, pw), (fc0, pw0), "call {k}: row {r}, the song's own"),
        }
    }
    assert!(locked > 20, "{locked} locked calls");
    // the player itself is the same as without locks
    assert!(player_state(&t.sc.player) == player_state(&t0.sc.player));
}

static mut TOLD: Vec<(u32, i32)> = Vec::new();

unsafe extern "C" fn told(_ctx: *mut c_void, field: u32, value: i32) {
    unsafe { (*std::ptr::addr_of_mut!(TOLD)).push((field, value)) };
}

#[test]
fn a_character_lock_reaches_the_hook_and_goes_after_its_row() {
    let mut s = song();
    put(&mut s, 0, 2, SONG_LOCK_THUMP, 90);
    let mut t = Transport::new(&mut s);
    let hooks = song_chip_hooks {
        chip_a_song: None,
        chip_a_keys: None,
        chip_a_character: Some(told),
        ctx: std::ptr::null_mut(),
    };
    unsafe { song_ctl_set_hooks(&mut *t.sc, &hooks) };
    assert!(t.play(0, 0));
    let until = 60; // past row 3 of pattern 0 (6 ticks a row)
    while t.calls() < until {
        t.pass();
        t.block_silent();
    }
    let got = unsafe { (*std::ptr::addr_of!(TOLD)).clone() };
    let f = SONG_LOCK_THUMP - SONG_LOCK_AGE;
    assert_eq!(got, vec![(f, 90), (f, -1)]);
}

/// Ergonomics review E06 (1), M2 review 3.4: at a pattern's last row the look-ahead holds row 0
/// of the pattern and of the next; with all five filter and pulse locks on both, the next
/// pattern's row 0 plays all five of its own (eight slots lost its ROUTE and PULSE).
#[test]
fn a_pattern_boundary_with_five_locks_on_both_first_rows_keeps_all_ten() {
    let p0 = vec![row(0x60 + 36, 1), R, R, R, R, R, R, R];
    let p1 = vec![row(0x60 + 24, 1), R, R, R, R, R, R, R];
    let p2 = vec![row(0x60 + 19, 1), R, R, R, R, R, R, R];
    let mut spec = SngSpec::new();
    spec.orders = vec![vec![(vec![0, 1], 0), (vec![2], 0), (vec![2], 0)]];
    let mut ins = [0u8; 25];
    ins[..9].copy_from_slice(&[0x09, 0xa0, 1, 1, 0, 0, 0, 2, 0x09]);
    spec.instr = vec![ins];
    spec.tables = vec![
        vec![(0x41, 0x00), (0xff, 0x00)],
        vec![(0x88, 0x00), (0xff, 0x00)],
        vec![],
        vec![],
    ];
    spec.patterns = vec![p0, p1, p2]
        .into_iter()
        .map(|p| (p, [0xff, 0, 0, 0]))
        .collect();
    let mut s = Song::host();
    s.read(&spec.encode(), 0).unwrap();
    for (p, v) in [
        (0u32, [100u32, 3, 1, 1, 0x111]),
        (1, [1500, 9, 2, 0, 0x555]),
    ] {
        for (param, value) in [
            SONG_LOCK_CUTOFF,
            SONG_LOCK_RESONANCE,
            SONG_LOCK_FILTER_MODE,
            SONG_LOCK_ROUTE,
            SONG_LOCK_PULSE,
        ]
        .into_iter()
        .zip(v)
        {
            put(&mut s, p, 0, param, value);
        }
    }
    let mut t = Transport::new(&mut s);
    assert!(t.play(0, 0));
    let mut seen = 0;
    while t.calls() < 200 {
        t.pass();
        let c = t.calls();
        t.block_silent();
        if t.calls() != c && t.sc.row_pat[0] == 1 && t.sc.row_row[0] == 0 {
            assert_eq!(t.sc.lock_n[0], 5, "pattern 1's row 0: its five locks");
            assert_eq!(t.chip.fc, 1500);
            assert_eq!(t.chip.osc[0].pw, 0x555, "its PULSE");
            assert_eq!(
                t.chip.routing & 1,
                0,
                "its ROUTE: voice 1 out of the filter"
            );
            assert_eq!(
                (t.chip.res, t.chip.mode_vol >> 4 & 7),
                (9, 2),
                "its RESONANCE and MODE"
            );
            seen += 1;
        }
    }
    assert!(seen > 3, "{seen} calls on pattern 1's row 0");
}

/// Ergonomics review E06 (2), M2 review 3.5: changing a lock in place while a row edit is
/// pending is refused as adding one is, so the value never lands on rows about to move.
#[test]
fn changing_a_lock_while_an_edit_is_pending_waits() {
    let mut s = song();
    put(&mut s, 0, 5, SONG_LOCK_CUTOFF, 500);
    let p = s.ptr_mut();
    let mut e = gt2_edit::default();
    assert_eq!(unsafe { gt2_pattern_insert_row(p, 0, 3, &mut e) }, GT2_OK);
    let pending = -(GT2_ERR_PENDING as i32);
    assert_eq!(
        unsafe { song_lock_put(p, 0, 5, SONG_LOCK_CUTOFF, 600) },
        pending,
        "in place"
    );
    assert_eq!(
        unsafe { song_lock_put(p, 0, 6, SONG_LOCK_CUTOFF, 600) },
        pending,
        "added"
    );
    assert_eq!(
        unsafe { song_lock_put(p, 0, 5, SONG_LOCK_CUTOFF, 500) },
        0,
        "the same value: nothing to change"
    );
    unsafe { gt2_edit_commit(p, &e) };
    assert_eq!(
        locks(&s),
        vec![(0, 6, 0, 500)],
        "the lock moved with its row"
    );
    put(&mut s, 0, 6, SONG_LOCK_CUTOFF, 600);
    assert_eq!(locks(&s), vec![(0, 6, 0, 600)]);
}

/// Ergonomics review E06 (3), M2 review 3.7: a row that starts while an edit's look-ahead is
/// in flight (lock_edit_seq) is resolved once it lands, not marked seen and skipped: the next
/// row's lock still plays.
#[test]
fn a_row_that_starts_during_an_edits_look_ahead_is_resolved_after_it() {
    let mut s = song();
    put(&mut s, 0, 4, SONG_LOCK_PULSE, 0x777);
    let mut t = Transport::new(&mut s);
    assert!(t.play(0, 0));
    let mut guard = 0;
    while !(t.sc.row_pat[0] == 0 && t.sc.row_row[0] == 2) {
        t.pass();
        t.block_silent();
        guard += 1;
        assert!(guard < 10_000);
    }
    t.pass();
    // An edit's look-ahead in flight (as between poll_applied and poll_locks on the device):
    // row 3 starts and is polled meanwhile.
    t.sc.lock_edit_seq = t.sc.applied.wrapping_add(1000);
    while t.sc.row_row[0] != 3 {
        t.block_silent();
    }
    t.pass();
    t.sc.lock_edit_seq = 0;
    let mut locked = 0;
    while t.sc.row_row[0] != 5 {
        t.pass();
        let c = t.calls();
        t.block_silent();
        if t.calls() != c && t.sc.row_row[0] == 4 {
            assert_eq!(t.chip.osc[0].pw, 0x777, "row 4's lock");
            locked += 1;
        }
    }
    assert!(locked > 0);
}

/// Ergonomics review E08 (M2 review 2.3): chip A swapped (a profile change) on a locked row:
/// the new chip gets the row's locked cutoff and pulse with the player's registers, not the
/// song's own until the next call.
#[test]
fn a_chip_swapped_on_a_locked_row_gets_the_locked_registers_at_once() {
    let mut s = song();
    put(&mut s, 0, 3, SONG_LOCK_CUTOFF, 1234);
    put(&mut s, 0, 3, SONG_LOCK_PULSE, 0x321);
    let mut t = Transport::new(&mut s);
    assert!(t.play(0, 0));
    let mut guard = 0;
    while !(t.sc.row_pat[0] == 0 && t.sc.row_row[0] == 3 && t.chip.fc == 1234) {
        t.pass();
        t.block_silent();
        guard += 1;
        assert!(guard < 10_000);
    }
    let mut fresh = Box::<sid>::default();
    unsafe {
        sid_init(&mut *fresh, SID_MODEL_6581, SID_CLOCK_PAL, SAMPLE_RATE);
        song_ctl_chip_swapped((&mut *t.sc as *mut song_ctl).cast(), 0, &mut *fresh);
    }
    assert_eq!(fresh.fc, 1234, "the locked cutoff");
    assert_eq!(fresh.osc[0].pw, 0x321, "the locked pulse");
    assert_eq!(fresh.mode_vol, t.chip.mode_vol);
    for v in 1..3 {
        assert_eq!(fresh.osc[v].pw, t.chip.osc[v].pw, "voice {v}: the song's");
    }
}
