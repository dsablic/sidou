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
