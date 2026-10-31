// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::song::Song;
use sidou_tests::song_embed;
use sidou_tests::transport::Transport;

use crate::{Rig, NOISE_PERC};

fn sidou_loop() -> Song {
    Song::host_from(&song_embed::sng())
}

fn playing(song: &mut Song) -> Box<song_ctl> {
    let mut sc = Box::<song_ctl>::default();
    sc.song = song.ptr_mut();
    sc.state = SONG_PLAYING as u8;
    sc
}

const ROW: u32 = GT2_EV_ROW;
const NOTE: u32 = GT2_EV_NOTE;
const ON: u32 = GT2_EV_GATE_ON;

fn ev(call: u16, ch: u32, flags: u32, row: u8, note: u8, instr: u8) -> song_event {
    song_event {
        frame: call,
        flags: (flags | ch << 5) as u8,
        row,
        note,
        instr,
        ..Default::default()
    }
}

struct Ears {
    ss: Box<scene_song>,
    rig: Rig,
}

impl Ears {
    fn new() -> Ears {
        let mut ss = Box::<scene_song>::default();
        unsafe { scene_song_init(&mut *ss) };
        Ears {
            ss,
            rig: Rig::new(SID_MODEL_6581),
        }
    }

    fn feed(&mut self, sc: &song_ctl, evs: &[song_event]) {
        unsafe { scene_song_events(&mut *self.ss, sc, evs.as_ptr(), evs.len() as u32) };
    }

    fn frame(&mut self) -> scene_song_frame_out {
        let mut out = scene_song_frame_out::default();
        unsafe { scene_song_frame(&mut *self.ss, &*self.rig.live, &mut out) };
        out
    }
}

fn row_call(n: u16, row: u8, notes: [Option<(u8, u8)>; 3]) -> Vec<song_event> {
    (0..3)
        .map(|ch| match notes[ch as usize] {
            Some((note, instr)) => ev(n, ch, ROW | NOTE | ON, row, note, instr),
            None => ev(n, ch, ROW, row, 0, 0),
        })
        .collect()
}

const LEAD: u8 = 2;
const BASS: u8 = 1;
const KICK: u8 = 3;
const SNARE: u8 = 4;
const HAT: u8 = 5;

#[test]
fn roles_and_drums_are_guessed_from_the_instruments_and_the_pitches() {
    let mut song = sidou_loop();
    let sc = playing(&mut song);
    let mut e = Ears::new();
    let drums = [KICK, HAT, SNARE, HAT];
    let mut evs = Vec::new();
    for (k, d) in drums.iter().enumerate() {
        evs.extend(row_call(
            k as u16,
            k as u8,
            [Some((52, LEAD)), Some((21, BASS)), Some((48, *d))],
        ));
    }
    e.feed(&sc, &evs);
    let out = e.frame();
    assert_eq!(out.n, 4);
    let hits: Vec<u8> = out.beat[..4].iter().map(|b| b.hits).collect();
    let arp = SCENE_RMR_ARP as u8;
    assert_eq!(
        hits,
        [
            SCENE_RMR_KICK as u8 | arp,
            SCENE_RMR_HAT as u8 | arp,
            SCENE_RMR_SNARE as u8 | arp,
            SCENE_RMR_HAT as u8 | arp
        ],
        "the lead is the arp, the noise instruments the drums by their pitch"
    );
    assert!(
        out.beat[..4]
            .iter()
            .all(|b| b.bass != SCENE_RMR_BASS_NONE as u8),
        "the bass"
    );
    let place: Vec<(u8, u8, u8)> = out.beat[..4].iter().map(|b| (b.row, b.s, b.bar)).collect();
    assert_eq!(
        place,
        [(1, 0, 0), (1, 1, 0), (1, 2, 0), (1, 3, 0)],
        "rows 0-3 of bar 0, 4 rows a beat"
    );
    assert_eq!(out.sounding, 1, "the gates are on");
    let of = |p: u32| unsafe { scene_song_drum_of_pitch(p) };
    assert_eq!(
        (of(99), of(100), of(105), of(106)),
        (
            SONG_DRUM_KICK,
            SONG_DRUM_SNARE,
            SONG_DRUM_SNARE,
            SONG_DRUM_HAT
        ),
        "the pitch thresholds"
    );
}

#[test]
fn the_scne_chunk_names_the_roles_and_the_drums() {
    let mut song = sidou_loop();
    let mut v = song_scene {
        piece: SCENE_RMR as u8,
        count: 3,
        drums: 2,
        ..Default::default()
    };
    v.role[..3].copy_from_slice(&[
        SONG_ROLE_DRUMS as u8,
        SONG_ROLE_BASS as u8,
        SONG_ROLE_LEAD as u8,
    ]);
    v.drum[0] = [2, SONG_DRUM_HAT as u8];
    v.drum[1] = [1, SONG_DRUM_KICK as u8];
    assert_eq!(unsafe { song_ext_set_scene(song.ptr_mut(), &v) }, 0);
    let mut back = song_scene::default();
    assert_eq!(unsafe { song_ext_get_scene(song.ptr(), &mut back) }, 1);
    assert_eq!(
        (back.drums, back.drum[0], back.drum[1]),
        (2, [2, 3], [1, 1])
    );
    let sc = playing(&mut song);
    let mut e = Ears::new();
    e.feed(
        &sc,
        &row_call(0, 0, [Some((40, 2)), Some((60, LEAD)), Some((30, BASS))]),
    );
    e.feed(&sc, &row_call(1, 1, [Some((40, 1)), None, None]));
    e.feed(&sc, &row_call(2, 2, [Some((40, SNARE)), None, None]));
    let out = e.frame();
    let hits: Vec<u8> = out.beat[..3].iter().map(|b| b.hits).collect();
    assert_eq!(
        hits,
        [
            SCENE_RMR_HAT as u8 | SCENE_RMR_ARP as u8,
            SCENE_RMR_KICK as u8,
            SCENE_RMR_SNARE as u8
        ],
        "the table's drums, the lead an arp, an instrument not in the table by its pitch"
    );
    assert_ne!(
        out.beat[0].bass, SCENE_RMR_BASS_NONE as u8,
        "voice 2 is the bass"
    );
    let a_chunk_without_the_drum_table = [1u8, 0, 3, 1, 2, 3];
    song.ext_set(
        u32::from_le_bytes(*b"SCNE"),
        &a_chunk_without_the_drum_table,
    );
    assert_eq!(unsafe { song_ext_get_scene(song.ptr(), &mut back) }, 1);
    assert_eq!(
        (back.count, back.drums),
        (3, 0),
        "no drum table reads as no drums"
    );
}

#[test]
fn a_bass_and_drums_voice_is_the_kits_drums_and_the_bass() {
    let mut song = crate::song_gen_roles::song(true, 0);
    let mut v = song_scene {
        count: 3,
        ..Default::default()
    };
    v.role[0] = SONG_ROLE_BASS_DRUMS as u8;
    v.role[1] = SONG_ROLE_NONE as u8;
    v.role[2] = SONG_ROLE_NONE as u8;
    assert_eq!(unsafe { song_ext_set_scene(song.ptr_mut(), &v) }, 0);
    let sc = playing(&mut song);
    let mut e = Ears::new();
    e.feed(&sc, &row_call(0, 0, [Some((36, 3)), None, None]));
    e.feed(&sc, &row_call(1, 1, [Some((24, 1)), None, None]));
    e.feed(&sc, &row_call(2, 2, [Some((36, 4)), None, None]));
    let out = e.frame();
    let got: Vec<(u8, bool)> = out.beat[..3]
        .iter()
        .map(|b| (b.hits, b.bass != SCENE_RMR_BASS_NONE as u8))
        .collect();
    assert_eq!(
        got,
        [
            (SCENE_RMR_KICK as u8, false),
            (0, true),
            (SCENE_RMR_SNARE as u8, false)
        ]
    );
}

#[test]
fn the_fill_is_the_last_beat_of_every_fourth_bar() {
    let mut song = sidou_loop();
    let sc = playing(&mut song);
    let mut e = Ears::new();
    let mut r = Box::<scene_rmr>::default();
    unsafe { scene_rmr_init(&mut *r) };
    let mut spun_at = None;
    let mut disarmed_at = None;
    for n in 0..5 * 16u16 {
        let row = (n % 16) as u8;
        let kick = row.is_multiple_of(4).then_some((48, KICK));
        e.feed(&sc, &row_call(n, row, [None, None, kick]));
        let out = e.frame();
        assert_eq!(out.n, 1);
        let b = out.beat[0];
        assert_eq!((b.s, b.bar), (row, ((n / 16) % 4) as u8), "call {n}");
        unsafe {
            scene_rmr_step(&mut *r, &b);
            scene_rmr_frame(&mut *r, 1000, 1);
        }
        if r.spin != 0 && spun_at.is_none() {
            spun_at = Some(n);
        }
        if r.sword != 0 && disarmed_at.is_none() {
            disarmed_at = Some(n);
        }
    }
    assert_eq!(
        spun_at,
        Some(3 * 16 + 12),
        "the spin starts on bar 4's last beat"
    );
    assert_eq!(disarmed_at, Some(3 * 16 + 15), "and lands four rows on");
    let mut song = sidou_loop();
    let m = song_midi {
        rows_per_beat: 2,
        count: 0,
        ..Default::default()
    };
    assert_eq!(unsafe { song_ext_set_midi(song.ptr_mut(), &m) }, 0);
    let sc = playing(&mut song);
    let mut e = Ears::new();
    for n in 0..10u16 {
        e.feed(&sc, &row_call(n, n as u8, [None, None, None]));
    }
    let out = e.frame();
    let place: Vec<(u8, u8)> = out.beat[..10].iter().map(|b| (b.s, b.bar)).collect();
    assert_eq!(
        place,
        [
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 0),
            (4, 0),
            (5, 0),
            (6, 0),
            (7, 0),
            (0, 1),
            (1, 1)
        ],
        "two rows a beat: a bar is 8 rows"
    );
    assert!(out.beat[..10].iter().all(|b| b.rpb == 2));
}

#[test]
fn a_muted_drum_voice_plays_no_part() {
    let mut song = sidou_loop();
    let mut sc = playing(&mut song);
    let mut e = Ears::new();
    let bar = |n: u16| {
        row_call(
            n,
            n as u8,
            [Some((52, LEAD)), Some((21, BASS)), Some((48, KICK))],
        )
    };
    e.feed(&sc, &bar(0));
    let out = e.frame();
    assert_eq!(out.beat[0].hits, (SCENE_RMR_KICK | SCENE_RMR_ARP) as u8);
    sc.mute = 1 << 2;
    e.feed(&sc, &bar(1));
    let out = e.frame();
    assert_eq!(
        out.beat[0].hits, SCENE_RMR_ARP as u8,
        "no kick from a muted voice"
    );
    assert_eq!(out.beat[0].s, 1, "its rows still count");
    sc.mute = 1 << 2;
    sc.taken = 0b011;
    e.feed(&sc, &bar(2));
    let out = e.frame();
    assert_eq!(
        (out.beat[0].hits, out.sounding),
        (0, 0),
        "the keys' voices count as muted too: nothing sounds"
    );
}

#[test]
fn the_bass_is_high_above_the_middle_of_its_range() {
    let mut song = sidou_loop();
    let sc = playing(&mut song);
    let mut e = Ears::new();
    for (n, note) in [21u8, 33, 28, 21].iter().enumerate() {
        e.feed(
            &sc,
            &row_call(n as u16, n as u8, [None, Some((*note, BASS)), None]),
        );
    }
    let out = e.frame();
    let bass: Vec<u8> = out.beat[..4].iter().map(|b| b.bass).collect();
    let (lo, hi) = (SCENE_RMR_BASS_LOW as u8, SCENE_RMR_BASS_HIGH as u8);
    assert_eq!(
        bass,
        [lo, hi, hi, lo],
        "MIDI 33 alone is under SCENE_SONG_BASS_SPLIT, then against the range 33..45"
    );
}

#[test]
fn sidou_loop_through_the_transport_plays_the_duel() {
    let mut song = sidou_loop();
    let mut t = Transport::new(&mut song);
    assert!(t.play(0, 0));
    assert!(t.run_until(2000, |t| t.state() == SONG_PLAYING));
    let mut e = Ears::new();
    let mut beats = Vec::new();
    while t.calls() < 8 * 16 * 6 + 2 {
        t.pass();
        t.block_silent();
        let evs = t.events();
        e.feed(&t.sc, &evs);
        let out = e.frame();
        beats.extend_from_slice(&out.beat[..out.n as usize]);
    }
    let rows: Vec<&scene_rmr_beat> = beats.iter().filter(|b| b.row != 0).collect();
    assert!(rows.len() >= 8 * 16, "{} rows", rows.len());
    let drum_pattern_of_bars_1_to_3 = "K.H.S.H.K.K.S.HH";
    for (k, c) in drum_pattern_of_bars_1_to_3.chars().enumerate() {
        let b = rows[k];
        let drum = b.hits & (SCENE_RMR_KICK | SCENE_RMR_SNARE | SCENE_RMR_HAT) as u8;
        let expect = match c {
            'K' => SCENE_RMR_KICK,
            'S' => SCENE_RMR_SNARE,
            'H' => SCENE_RMR_HAT,
            _ => 0,
        } as u8;
        assert_eq!(drum, expect, "row {k}");
        assert_eq!((b.s, b.bar), (k as u8, 0));
    }
    let arps: Vec<u8> = rows[..16]
        .iter()
        .filter(|b| b.hits & SCENE_RMR_ARP as u8 != 0)
        .map(|b| b.s)
        .collect();
    assert_eq!(arps, [0, 4, 6, 8, 10, 12, 14], "the lead's arp rows");
    assert!(
        rows[3 * 16 + 12].bar == 3 && rows[3 * 16 + 12].s == 12,
        "bar 4 is bar 3 of the count: its last beat the fill"
    );
    assert_eq!(e.ss.lost, 0);
}

#[test]
fn with_no_song_the_instrument_plays_the_parts_on_a_free_clock() {
    let mut e = Ears::new();
    let out = e.frame();
    assert_eq!((out.n, out.sounding), (0, 0));
    e.rig.note_on(45);
    e.rig.note_on(64);
    let out = e.frame();
    assert_eq!(out.n, 1);
    assert_eq!(
        out.beat[0].hits, SCENE_RMR_ARP as u8,
        "the higher note the arp"
    );
    assert_ne!(
        out.beat[0].bass, SCENE_RMR_BASS_NONE as u8,
        "the lower the bass"
    );
    assert_eq!(out.sounding, 1);
    let mut rows = 0;
    for _ in 0..3 * SCENE_SONG_FREE_FRAMES {
        let out = e.frame();
        rows += out.beat[..out.n as usize]
            .iter()
            .filter(|b| b.row != 0)
            .count();
    }
    assert_eq!(
        rows, 3,
        "a row every SCENE_SONG_FREE_FRAMES while a note is held"
    );
    e.rig.note_off(45);
    e.rig.note_off(64);
    e.rig.set_preset(NOISE_PERC);
    e.rig.note_on(60);
    let out = e.frame();
    assert_eq!(
        out.beat[..out.n as usize]
            .iter()
            .map(|b| b.hits)
            .fold(0, |a, h| a | h),
        SCENE_RMR_KICK as u8,
        "a noise note under SCENE_SONG_KICK_BELOW is a kick"
    );
}
