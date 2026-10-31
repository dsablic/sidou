// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! DICE by the lane's role (app/song_gen.c, 1.5): chords on an ARP lane, through shared wavetable
//! chord programs the batch writer adds once; the kit's kick and snare time-shared with the bass
//! on a BASS+DRUMS lane; the role set with PRESETS while EDIT + SCL is held, kept in the SCNE
//! chunk as an older SIDOU reads it; both played by GoatTracker's own player as by SIDOU's.

use sidou_sys::*;
use sidou_tests::song::{tag, SngSpec, Song as Model};

use crate::song_view::{open_tracker, Song};
use crate::step_view::{
    open_step, popup, reference_calls, set_dice_all, settle, sidou_calls, snapshot, C3, SCALES,
};

const R: [u8; 4] = [GT2_REST as u8, 0, 0, 0];
const LEAD: u8 = 1;
const KICK: u8 = 3;
const SNARE: u8 = 4;
const DRUMS: usize = GT2_KIT_DRUMS as usize;
/// The lead's rows: a gated pulse at the note, then the table stops.
const LEAD_ROWS: [(u8, u8); 2] = [(0x41, 0x00), (0xff, 0x00)];

fn kit_instr(d: usize) -> [u8; 25] {
    // SAFETY: gt2_kit_drum is 25 bytes, an instrument's.
    unsafe { std::mem::transmute::<gt2_kit_drum, [u8; 25]>(gt2_kit_instr[d]) }
}

/// Three 16-row patterns, a pulse lead (instrument 1, its first frame pulse: the chords keep it)
/// and a bass (2); with `kit` the SID drum kit as instruments 3-10, its rows first, as a NEW song
/// holds it; `used` wave rows taken before the lead's (filler that is not a chord).
pub(crate) fn song(kit: bool, used: usize) -> Model {
    let mut s = SngSpec::new();
    let (wave, pulse) = unsafe {
        (
            (&raw const gt2_kit_wave).read(),
            (&raw const gt2_kit_pulse).read(),
        )
    };
    let mut w: Vec<(u8, u8)> = Vec::new();
    if kit {
        w.extend(wave[0].iter().copied().zip(wave[1].iter().copied()));
    }
    w.extend(std::iter::repeat_n((0x41, 0x0c), used));
    let lead = w.len() as u8 + 1;
    w.extend(LEAD_ROWS);
    let p: Vec<(u8, u8)> = pulse[0]
        .iter()
        .copied()
        .zip(pulse[1].iter().copied())
        .collect();
    let mut ins = [[0u8; 25]; 2];
    ins[0][..9].copy_from_slice(&[0x19, 0xa8, lead, 1, 0, 0, 0, 2, 0x41]);
    ins[1][..9].copy_from_slice(&[0x12, 0x98, lead, 1, 0, 0, 0, 2, 0x41]);
    s.instr = ins.to_vec();
    if kit {
        s.instr.extend((0..DRUMS).map(kit_instr));
    }
    s.tables = vec![w, p, vec![], vec![]];
    s.patterns = (0..3).map(|_| (vec![R; 16], [0xff, 0, 0, 0])).collect();
    let mut m = Model::host();
    m.read(&s.encode(), 0).unwrap();
    m
}

fn set_role(s: &mut Song, lane: usize, role: u32) {
    let mut scene = song_scene {
        count: 3,
        ..Default::default()
    };
    scene.role[lane] = role as u8;
    assert_eq!(
        unsafe { song_ext_set_scene(s.ui.v.song.song.cast_mut(), &scene) },
        0
    );
}

fn stepping(m: Model) -> Song {
    let mut s = Song::playing(Some(m));
    open_step(&mut s);
    s
}

fn pattern(s: &Song, p: u32) -> Vec<[u8; 4]> {
    crate::step_view::pattern(s, p)
}

/// The wavetable's rows in use, (left, right) from row 1.
fn wave(s: &Song) -> Vec<(u8, u8)> {
    let song = unsafe { &*s.ui.v.song.song };
    let t = GT2_WTBL as usize;
    let mut n = GT2_TABLE_ROWS as usize;
    while n > 0 && song.ltable[t][n - 1] | song.rtable[t][n - 1] == 0 {
        n -= 1;
    }
    (0..n)
        .map(|r| (song.ltable[t][r], song.rtable[t][r]))
        .collect()
}

/// EDIT + SCL with a fixed seed: the toast it raised, and its edits written.
fn roll(s: &mut Song, seed: u32) -> Option<String> {
    s.ui.v.song.gen_.seed = seed;
    s.ui.v.s.now = 1000;
    s.ui.v.s.frame = 25;
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.press(PANEL_BTN_SCL);
    s.ui.hold(PANEL_BTN_EDIT, false);
    let toast = s.ui.toast();
    settle(&mut s.ui);
    toast
}

fn chord_rows(c: u32, first: u32) -> Vec<(u8, u8)> {
    let (mut l, mut r) = ([0u8; 5], [0u8; 5]);
    let n = unsafe { song_gen_chord_rows(c, first, l.as_mut_ptr(), r.as_mut_ptr()) } as usize;
    (0..n).map(|i| (l[i], r[i])).collect()
}

const CHORD_TONES: [&[u8]; 4] = [&[0, 4, 7], &[0, 3, 7], &[0, 5, 7], &[0, 4, 7, 10]];

#[test]
fn the_chord_programs_are_plain_wavetable_rows_that_keep_the_waveform() {
    assert_eq!(
        chord_rows(SONG_GEN_MAJ, 5),
        [(0, 0), (0, 4), (0, 7), (0xff, 5)]
    );
    assert_eq!(
        chord_rows(SONG_GEN_MIN, 9),
        [(0, 0), (0, 3), (0, 7), (0xff, 9)]
    );
    assert_eq!(
        chord_rows(SONG_GEN_SUS4, 1),
        [(0, 0), (0, 5), (0, 7), (0xff, 1)]
    );
    assert_eq!(
        chord_rows(SONG_GEN_7, 200),
        [(0, 0), (0, 4), (0, 7), (0, 10), (0xff, 200)]
    );
    let names: Vec<String> = (0..SONG_GEN_CHORDS)
        .map(|c| {
            unsafe { std::ffi::CStr::from_ptr(song_gen_chord_name(c)) }
                .to_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(names, ["MAJ", "MIN", "SUS4", "7"]);
}

/// An ARP lane's roll: the four programs written once after the table's last row, each hit a
/// note, the lane's instrument and 8XX at a chord whose tones are all in the lane's scale; the
/// rests carry no 8XX. The same seed writes the same cells.
#[test]
fn dice_on_an_arp_lane_writes_chords_in_the_scale_and_the_programs_once() {
    let mut s = stepping(song(false, 0));
    set_role(&mut s, 0, SONG_ROLE_ARP);
    set_dice_all(&mut s, 3, 3, 0, 1); // BUSY, major
    assert_eq!(roll(&mut s, 7).as_deref(), Some("NEW CHORDS"));
    assert_eq!(s.ui.v.song.gen_.run, 0, "the run is over");
    let mut want = LEAD_ROWS.to_vec();
    for (c, at) in [
        (SONG_GEN_MAJ, 3),
        (SONG_GEN_MIN, 7),
        (SONG_GEN_SUS4, 11),
        (SONG_GEN_7, 15),
    ] {
        want.extend(chord_rows(c, at));
    }
    assert_eq!(wave(&s), want, "the programs after the lead's rows");
    let starts = [3u8, 7, 11, 15];
    let p = pattern(&s, 0);
    let mut kinds = std::collections::BTreeSet::new();
    for (r, c) in p.iter().enumerate() {
        if c[0] >= GT2_FIRSTNOTE as u8 && c[0] <= GT2_LASTNOTE as u8 {
            assert_eq!(
                (c[1], c[2]),
                (LEAD, 8),
                "row {r}: the lane's instrument and 8XX"
            );
            let k = starts
                .iter()
                .position(|&x| x == c[3])
                .expect("a chord's first row");
            kinds.insert(k);
            for t in CHORD_TONES[k] {
                let d = (c[0] + t - C3) % 12;
                assert!(
                    SCALES[3].contains(&d),
                    "row {r}: {k} on {} leaves the scale",
                    c[0]
                );
            }
        } else {
            assert_eq!(c[2], 0, "row {r}: a rest has no chord");
        }
    }
    assert!(kinds.len() >= 2, "more than one chord: {kinds:?}");
    assert_eq!(
        p,
        [
            [0x89, 1, 8, 3],
            [0xbe, 0, 0, 0],
            [0x97, 1, 8, 3],
            [0xbe, 0, 0, 0],
            [0x8d, 1, 8, 7],
            [0x86, 1, 8, 11],
            [0x86, 1, 8, 11],
            [0xbe, 0, 0, 0],
            [0xbd, 0, 0, 0],
            [0xbd, 0, 0, 0],
            [0x92, 1, 8, 11],
            [0xbe, 0, 0, 0],
            [0x84, 1, 8, 11],
            [0xbe, 0, 0, 0],
            [0x90, 1, 8, 11],
            [0x89, 1, 8, 3],
        ],
        "seed 7's cells"
    );
    // A second roll finds the programs by their rows: none is written again.
    let before = wave(&s);
    assert_eq!(roll(&mut s, 99).as_deref(), Some("NEW CHORDS"));
    assert_eq!(wave(&s), before);
    assert!(pattern(&s, 0).iter().any(|c| c[2] == 8));
}

/// A wavetable with no room for the programs refuses the roll with TABLE FULL and leaves the
/// pattern and the table as they were.
#[test]
fn an_arp_roll_with_no_room_for_the_programs_says_table_full_and_writes_nothing() {
    let mut s = stepping(song(false, 250));
    set_role(&mut s, 0, SONG_ROLE_ARP);
    let (rows, cells) = (wave(&s), pattern(&s, 0));
    assert_eq!(roll(&mut s, 7).as_deref(), Some("TABLE FULL"));
    assert_eq!(s.ui.v.song.gen_.run, 0);
    assert_eq!(wave(&s), rows);
    assert_eq!(pattern(&s, 0), cells);
}

/// GoatTracker's own player plays a rolled chord lane as SIDOU's does, call for call, and the
/// chord is heard: three or four pitches a note, a frame each.
#[test]
fn goattrackers_player_plays_a_chord_lane_as_sidous_does() {
    let mut s = stepping(song(false, 0));
    set_role(&mut s, 0, SONG_ROLE_ARP);
    set_dice_all(&mut s, 4, 3, 0, 0); // a chord on every row
    roll(&mut s, 7);
    let m = snapshot(&s);
    let sng = m.write(false);
    let (ours, theirs) = (sidou_calls(&m, 300), reference_calls(&sng, 300));
    assert_eq!(ours, theirs);
    let mut freq = std::collections::BTreeSet::new();
    let (mut lo, mut hi) = (0u8, 0u8);
    for c in &theirs[..60] {
        for &(reg, v) in c {
            match reg {
                0 => lo = v,
                1 => hi = v,
                _ => {}
            }
        }
        freq.insert(u16::from(hi) << 8 | u16::from(lo));
    }
    assert!(freq.len() >= 4, "the chords' tones: {freq:?}");
}

/// TRACKER says the chord on the cursor's row: CHORD, its root and its kind.
#[test]
fn tracker_names_the_chord_a_row_points_at() {
    let mut s = Song::playing(Some(song(false, 0)));
    open_tracker(&mut s.ui);
    set_role(&mut s, 0, SONG_ROLE_ARP);
    set_dice_all(&mut s, 3, 3, 0, 1);
    assert_eq!(roll(&mut s, 7).as_deref(), Some("NEW CHORDS"));
    s.ui.turn(PANEL_ENC_SELECT, 4); // row 4: A-3, the minor program
    settle(&mut s.ui);
    assert_eq!(pattern(&s, 0)[4], [0x8d, 1, 8, 7]);
    let say = s.ui.small(TRACKER_VIEW_SAY_ROW as usize, 0, 20);
    assert_eq!(say, "V1 8 07 CHORD A MIN ");
}

/// BASS+DRUMS: the kit's kick and snare on the rhythm's strong rows (STRAIGHT: kick on beats 1
/// and 3, snare on 2 and 4 of each bar of 16 rows), the lane's instrument between in the scale;
/// pattern cells only.
#[test]
fn dice_on_a_bass_and_drums_lane_puts_the_kit_on_the_strong_beats_and_the_bass_between() {
    // rhythm: (its kicks, its snares) by row of the bar
    let rhythms: [(u32, &[usize], &[usize]); 5] = [
        (0, &[0, 8], &[4, 12]),
        (1, &[0, 8], &[4, 12]),
        (2, &[0, 3, 8, 11], &[6, 14]),
        (3, &[0, 4, 8, 12], &[]),
        (4, &[0, 7, 10], &[4, 12]),
    ];
    for (rhythm, kicks, snares) in rhythms {
        let mut s = stepping(song(true, 0));
        set_role(&mut s, 0, SONG_ROLE_BASS_DRUMS);
        set_dice_all(&mut s, 3, 0, rhythm, 3); // BUSY, minor pentatonic, BASS
        let rows = wave(&s);
        assert_eq!(roll(&mut s, 7).as_deref(), Some("NEW PATTERN"));
        assert_eq!(wave(&s), rows, "no table writes");
        let p = pattern(&s, 0);
        let mut bass = 0;
        for (r, c) in p.iter().enumerate() {
            let note = c[0] >= GT2_FIRSTNOTE as u8 && c[0] <= GT2_LASTNOTE as u8;
            if kicks.contains(&r) {
                assert_eq!(*c, [C3, KICK, 0, 0], "rhythm {rhythm}: row {r} the kick");
            } else if snares.contains(&r) {
                assert_eq!(*c, [C3, SNARE, 0, 0], "rhythm {rhythm}: row {r} the snare");
            } else if note {
                assert_eq!(c[1], LEAD, "rhythm {rhythm}: row {r} the lane's instrument");
                let d = (c[0] + 24 - C3) % 12;
                assert!(
                    SCALES[0].contains(&d),
                    "rhythm {rhythm}: row {r} in the scale"
                );
                assert!(c[0] < C3 + 12, "rhythm {rhythm}: row {r} BASS's octaves");
                bass += 1;
            } else {
                assert_ne!(c[1], KICK);
            }
        }
        assert!(bass > 0, "rhythm {rhythm}: a bass between");
        // GoatTracker plays it as SIDOU does.
        let m = snapshot(&s);
        let sng = m.write(false);
        assert_eq!(
            sidou_calls(&m, 200),
            reference_calls(&sng, 200),
            "rhythm {rhythm}"
        );
    }
}

/// A BASS+DRUMS roll in a song without the kit says so and writes nothing.
#[test]
fn a_bass_and_drums_roll_without_the_kit_says_no_drum_kit() {
    let mut s = stepping(song(false, 0));
    set_role(&mut s, 0, SONG_ROLE_BASS_DRUMS);
    let cells = pattern(&s, 0);
    assert_eq!(roll(&mut s, 7).as_deref(), Some("NO DRUM KIT"));
    assert_eq!(s.ui.v.song.gen_.run, 0);
    assert_eq!(pattern(&s, 0), cells);
    // the kit found by its bytes, wherever it is
    let k = song(true, 0);
    assert_eq!(unsafe { gt2_kit_find(k.ptr(), GT2_KIT_KICK) }, 3);
    assert_eq!(unsafe { gt2_kit_find(k.ptr(), GT2_KIT_SNARE) }, 4);
    assert_eq!(
        unsafe { gt2_kit_find(song(false, 0).ptr(), GT2_KIT_KICK) },
        0
    );
}

/// PRESETS with EDIT + SCL held: the selected lane's role, in the frame (small, PRE ROLE) and
/// the popup (large); the turn spoils the roll; the SCNE chunk keeps it, the others AUTO.
#[test]
fn presets_with_edit_and_scl_held_sets_the_lanes_role_into_the_scene_chunk() {
    let mut s = stepping(song(true, 0));
    s.ui.turn(PANEL_ENC_SELECT, 1); // V2
    let before = pattern(&s, 1);
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.down(PANEL_BTN_SCL);
    settle(&mut s.ui);
    let top = STEP_VIEW_HELD_ROW as usize;
    assert_eq!(s.ui.small(top + 9, 19, 8), "PRE ROLE");
    assert_eq!(s.ui.small(top + 10, 19, 10), "AUTO      ");
    s.ui.turn(PANEL_ENC_PRESETS, 3);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(top + 10, 19, 10), "BASS+DRUMS");
    assert_eq!(
        popup(&s, PANEL_ENC_PRESETS, true),
        Some(("ROLE".into(), "BASS+DRUMS".into(), 3, 6))
    );
    s.ui.turn(PANEL_ENC_PRESETS, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(top + 10, 19, 10), "ARP       ");
    s.ui.turn(PANEL_ENC_PRESETS, 9);
    settle(&mut s.ui);
    assert_eq!(
        s.ui.small(top + 10, 19, 10),
        "NONE      ",
        "it stops at the end"
    );
    s.ui.turn(PANEL_ENC_PRESETS, -3);
    // While an edit is in flight (a roll writing its cells) the chunk waits: WAITING, not FULL.
    let song = s.ui.v.song.song.cast_mut();
    unsafe { (*song).pending = 1 };
    s.ui.turn(PANEL_ENC_PRESETS, 1);
    assert_eq!(s.ui.toast().as_deref(), Some("WAITING"));
    s.ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(
        s.ui.toast().as_deref(),
        Some("WAITING"),
        "DICE's settings too"
    );
    unsafe { (*song).pending = 0 };
    s.ui.turn(PANEL_ENC_K1, -1);
    s.ui.up(PANEL_BTN_SCL);
    s.ui.hold(PANEL_BTN_EDIT, false);
    settle(&mut s.ui);
    assert_eq!(pattern(&s, 1), before, "a turn: no roll");
    assert_eq!(
        s.ui.v.step.instr[1], 1,
        "PRESETS left the lane's instrument"
    );
    let chunk = snapshot(&s).ext_find(tag(b"SCNE")).unwrap();
    assert_eq!(chunk, [1, 0, 3, 0, SONG_ROLE_BASS_DRUMS as u8, 0]);
    // and the roll now is the lane's
    assert_eq!(roll(&mut s, 7).as_deref(), Some("NEW PATTERN"));
    assert_eq!(pattern(&s, 1)[0][1], KICK);
}

/// The chunk with BASS+DRUMS saved and read back; as an older SIDOU reads it (1.0 and 1.1's
/// reader is this one: a role is a byte it does not check): the roles and the drum table after
/// them as written, and RED MOON RITE hears the lane's kick and snare as drums and the rest as the
/// bass.
#[test]
fn the_scene_chunk_keeps_bass_and_drums_through_a_save_and_an_older_reader() {
    let mut m = song(true, 0);
    let mut v = song_scene {
        count: 3,
        drums: 1,
        ..Default::default()
    };
    v.role[..3].copy_from_slice(&[SONG_ROLE_BASS_DRUMS as u8, SONG_ROLE_ARP as u8, 0]);
    v.drum[0] = [5, SONG_DRUM_HAT as u8];
    assert_eq!(unsafe { song_ext_set_scene(m.ptr_mut(), &v) }, 0);
    let file = m.write(true);
    let mut back = Model::host();
    back.read(&file, 0).unwrap();
    assert_eq!(
        back.ext_find(tag(b"SCNE")).unwrap(),
        [
            1,
            0,
            3,
            SONG_ROLE_BASS_DRUMS as u8,
            SONG_ROLE_ARP as u8,
            0,
            1,
            5,
            3
        ]
    );
    let mut got = song_scene::default();
    assert_eq!(unsafe { song_ext_get_scene(back.ptr(), &mut got) }, 1);
    assert_eq!(
        (got.count, got.role[0], got.drums, got.drum[0]),
        (3, 6, 1, [5, 3])
    );
}
