// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! MIDI's modes (`app/midi_route.c`): which channel plays the instrument and which plays chip
//! A's voices as the player's jam voices, through `midi_rx` as the device runs it, with the
//! player's side recorded.

use std::ffi::c_void;

use sidou_sys::*;

use crate::Rig;

const TRS: u8 = MIDI_SRC_TRS as u8;
const C4: u8 = 60;
const E4: u8 = 64;
const G4: u8 = 67;

/// What the router asked of the player.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Ev {
    On {
        voice: u32,
        note: u32,
        instr: u32,
        legato: u32,
    },
    Off(u32),
    Bend(u32, i32),
    Filter(u32, u32, u32),
}

#[derive(Default)]
struct Player {
    log: Vec<Ev>,
    /// Refuse every call (a full command ring).
    full: bool,
}

unsafe extern "C" fn jam_on(
    ctx: *mut c_void,
    voice: u32,
    note: u32,
    instr: u32,
    legato: u32,
) -> i32 {
    let p = unsafe { &mut *ctx.cast::<Player>() };
    if p.full {
        return 0;
    }
    p.log.push(Ev::On {
        voice,
        note,
        instr,
        legato,
    });
    1
}

unsafe extern "C" fn jam_off(ctx: *mut c_void, voice: u32) -> i32 {
    let p = unsafe { &mut *ctx.cast::<Player>() };
    if p.full {
        return 0;
    }
    p.log.push(Ev::Off(voice));
    1
}

unsafe extern "C" fn jam_bend(ctx: *mut c_void, voice: u32, offset: i32) -> i32 {
    let p = unsafe { &mut *ctx.cast::<Player>() };
    if p.full {
        return 0;
    }
    p.log.push(Ev::Bend(voice, offset));
    1
}

unsafe extern "C" fn filter(ctx: *mut c_void, locks: u32, cutoff: u32, reso: u32) -> i32 {
    let p = unsafe { &mut *ctx.cast::<Player>() };
    if p.full {
        return 0;
    }
    p.log.push(Ev::Filter(locks, cutoff, reso));
    1
}

/// GoatTracker's note for a MIDI note.
fn gt(n: u8) -> u32 {
    u32::from(n) - MIDI_ROUTE_GT_FIRST
}

fn on(voice: u32, n: u8, instr: u32) -> Ev {
    Ev::On {
        voice,
        note: gt(n),
        instr,
        legato: 0,
    }
}

fn glide(voice: u32, n: u8, instr: u32) -> Ev {
    Ev::On {
        voice,
        note: gt(n),
        instr,
        legato: 1,
    }
}

/// live on a recorder, fed by midi_rx through a midi_route with a recording player.
struct Route {
    rig: Rig,
    rx: Box<midi_rx>,
    route: Box<midi_route>,
    player: Box<Player>,
}

impl Route {
    fn new() -> Self {
        let mut rig = Rig::new(SID_MODEL_6581);
        let mut rx = Box::<midi_rx>::default();
        let mut route = Box::<midi_route>::default();
        let mut player = Box::<Player>::default();
        let pp: *mut Player = &mut *player;
        let iface = midi_route_player {
            jam_on: Some(jam_on),
            jam_off: Some(jam_off),
            jam_bend: Some(jam_bend),
            filter: Some(filter),
            ctx: pp.cast(),
        };
        unsafe {
            midi_rx_init(&mut *rx, &mut *rig.live, 0);
            midi_route_init(&mut *route, &mut *rig.live, std::ptr::null_mut(), &iface);
            midi_rx_route(&mut *rx, &mut *route);
        }
        Route {
            rig,
            rx,
            route,
            player,
        }
    }

    fn with(mode: midi_mode, base: u8, edit: impl FnOnce(&mut midi_route_setup)) -> Self {
        let mut r = Route::new();
        let mut s = r.route.setup;
        s.mode = mode as u8;
        s.base = base;
        edit(&mut s);
        unsafe { midi_route_set(&mut *r.route, &s) };
        r.player.log.clear();
        r
    }

    fn trs(&mut self, bytes: &[u8]) {
        for &b in bytes {
            let e = midi_ring_entry {
                stamp: 0,
                data: u32::from(b),
                source: TRS,
                epoch: 0,
            };
            unsafe { midi_rx_entry(&mut *self.rx, &e) };
        }
    }

    fn tick(&mut self) {
        unsafe { midi_route_tick(&mut *self.route) };
    }

    fn take(&mut self) -> Vec<Ev> {
        std::mem::take(&mut self.player.log)
    }

    /// The instrument's notes held.
    fn held(&self) -> Vec<u8> {
        self.rig
            .live
            .voice
            .iter()
            .filter(|v| v.gate != 0)
            .map(|v| v.note)
            .collect()
    }
}

#[test]
fn omni_is_the_default_and_every_channel_plays_the_instrument() {
    let mut r = Route::new();
    assert_eq!(r.route.setup.mode, MIDI_MODE_OMNI as u8);
    for ch in 0..16u8 {
        r.trs(&[0x90 | ch, C4, 100]);
        assert_eq!(r.held(), [C4], "channel {}", ch + 1);
        r.trs(&[0x80 | ch, C4, 0]);
    }
    assert!(r.take().is_empty());
}

#[test]
fn live_plays_channel_n_alone() {
    let mut r = Route::with(MIDI_MODE_LIVE, 4, |_| {});
    for ch in 0..16u8 {
        r.trs(&[0x90 | ch, C4, 100]);
        let want: &[u8] = if ch == 4 { &[C4] } else { &[] };
        assert_eq!(r.held(), want, "channel {}", ch + 1);
        r.trs(&[0x80 | ch, C4, 0]);
        r.trs(&[0xB0 | ch, 74, 0]);
        let cutoff = r.rig.live.cutoff;
        assert_eq!(cutoff == 0, ch == 4, "channel {}: the CC map", ch + 1);
        r.trs(&[0xB0 | ch, 74, 127]);
    }
    assert!(r.take().is_empty());
}

#[test]
fn whole_chip_b_plays_channel_n_on_the_instrument() {
    let mut r = Route::with(MIDI_MODE_WHOLE_B, 0, |_| {});
    r.trs(&[0x90, C4, 100, E4, 100, G4, 100]);
    assert_eq!(r.held(), [C4, E4, G4]);
    r.trs(&[0x91, C4, 100]);
    assert_eq!(r.held(), [C4, E4, G4]);
    assert!(r.take().is_empty());
}

#[test]
fn multitimbral_plays_chip_as_voices_on_n_to_n_plus_2_and_the_instrument_on_n_plus_3() {
    let mut r = Route::with(MIDI_MODE_MULTI, 2, |_| {});
    r.trs(&[0x92, C4, 100, 0x93, E4, 100, 0x94, G4, 100]);
    assert_eq!(r.take(), [on(0, C4, 1), on(1, E4, 1), on(2, G4, 1)]);
    assert_eq!(
        r.held(),
        [] as [u8; 0],
        "chip A's channels do not play the instrument"
    );
    r.trs(&[0x95, 72, 100]);
    assert_eq!(r.held(), [72], "N + 3 plays the instrument");
    assert!(r.take().is_empty());
    for ch in [0u8, 1, 6, 15] {
        r.trs(&[0x90 | ch, C4, 100]);
    }
    assert!(r.take().is_empty(), "other channels are ignored");
    assert_eq!(r.held(), [72]);
    r.trs(&[0x83, E4, 0]);
    assert_eq!(r.take(), [Ev::Off(1)]);
}

#[test]
fn the_base_channel_wraps_round_sixteen() {
    let mut r = Route::with(MIDI_MODE_MULTI, 14, |_| {});
    r.trs(&[0x9E, C4, 1, 0x9F, C4, 1, 0x90, C4, 1, 0x91, E4, 1]);
    assert_eq!(r.take(), [on(0, C4, 1), on(1, C4, 1), on(2, C4, 1)]);
    assert_eq!(r.held(), [E4]);
}

#[test]
fn a_jam_channel_is_mono_with_the_last_note_held_sounding() {
    let mut r = Route::with(MIDI_MODE_MULTI, 0, |_| {});
    r.trs(&[0x90, C4, 100, E4, 100, G4, 100]);
    assert_eq!(r.take(), [on(0, C4, 1), on(0, E4, 1), on(0, G4, 1)]);
    r.trs(&[0x80, E4, 0]);
    assert!(
        r.take().is_empty(),
        "a note held under the sounding one goes quietly"
    );
    r.trs(&[0x80, G4, 0]);
    assert_eq!(r.take(), [on(0, C4, 1)], "back to the note held before");
    r.trs(&[0x80, C4, 0]);
    assert_eq!(r.take(), [Ev::Off(0)]);
    r.trs(&[0x80, C4, 0]);
    assert!(r.take().is_empty());
}

#[test]
fn legato_changes_the_note_without_a_new_attack() {
    let mut r = Route::with(MIDI_MODE_MULTI, 0, |s| s.legato = 1);
    r.trs(&[
        0x90, C4, 100, E4, 100, 0x80, E4, 0, 0x80, C4, 0, 0x90, G4, 100,
    ]);
    assert_eq!(
        r.take(),
        [
            on(0, C4, 1),
            glide(0, E4, 1),
            glide(0, C4, 1),
            Ev::Off(0),
            on(0, G4, 1)
        ]
    );
}

#[test]
fn notes_outside_goattrackers_range_are_ignored() {
    let mut r = Route::with(MIDI_MODE_MULTI, 0, |_| {});
    r.trs(&[0x90, 11, 100, 108, 100, 127, 100, 12, 100, 107, 100]);
    assert_eq!(r.take(), [on(0, 12, 1), on(0, 107, 1)]);
    assert_eq!(gt(107), 95);
}

#[test]
fn a_program_change_picks_the_channels_goattracker_instrument() {
    let mut r = Route::with(MIDI_MODE_MULTI, 0, |_| {});
    r.trs(&[0xC1, 4, 0xC0, 62, 0xC2, 63]);
    r.trs(&[0x90, C4, 1, 0x91, C4, 1, 0x92, C4, 1]);
    assert_eq!(r.take(), [on(0, C4, 63), on(1, C4, 5), on(2, C4, 1)]);
    assert_eq!(
        r.rig.live.preset, 0,
        "the instrument's preset is not chip A's channels'"
    );
    r.trs(&[0xC3, 2]);
    assert_eq!(r.rig.live.preset, 2, "N + 3's program is the instrument's");
}

#[test]
fn each_jam_channel_bends_its_voice_by_its_own_range_at_the_tick() {
    let mut r = Route::with(MIDI_MODE_MULTI, 0, |_| {});
    r.trs(&[0x90, C4, 1, 0x91, E4, 1]);
    r.take();
    r.trs(&[0xE0, 0x7F, 0x7F, 0xE1, 0x00, 0x00, 0xE1, 0x00, 0x20]);
    assert!(r.take().is_empty(), "bends wait for the tick, coalesced");
    r.tick();
    assert_eq!(
        r.take(),
        [Ev::Bend(0, 8191 * 2), Ev::Bend(1, (0x1000 - 8192) * 2)]
    );
    r.tick();
    assert!(r.take().is_empty(), "nothing changed");
    // RPN 0 on channel 2 only: 12 semitones.
    r.trs(&[0xB1, 101, 0, 0xB1, 100, 0, 0xB1, 6, 12]);
    r.tick();
    assert_eq!(r.take(), [Ev::Bend(1, (0x1000 - 8192) * 12)]);
    r.trs(&[0xB1, 96, 0]);
    r.tick();
    assert_eq!(r.take(), [Ev::Bend(1, (0x1000 - 8192) * 13)]);
    r.trs(&[0xB0, 121, 0]);
    r.tick();
    assert_eq!(
        r.take(),
        [Ev::Bend(0, 0)],
        "CC 121 centres the channel's bend"
    );
}

#[test]
fn a_bend_the_player_refuses_goes_at_the_next_tick() {
    let mut r = Route::with(MIDI_MODE_MULTI, 0, |_| {});
    r.trs(&[0xE2, 0x00, 0x00]);
    r.player.full = true;
    r.tick();
    r.player.full = false;
    r.tick();
    assert_eq!(r.take(), [Ev::Bend(2, -8192 * 2)]);
}

#[test]
fn a_note_the_player_refuses_is_dropped_and_counted() {
    let mut r = Route::with(MIDI_MODE_MULTI, 0, |_| {});
    r.player.full = true;
    r.trs(&[0x90, C4, 1]);
    r.player.full = false;
    assert_eq!(r.route.dropped, 1);
    r.trs(&[0x80, C4, 0]);
    assert!(r.take().is_empty(), "nothing sounds to release");
}

#[test]
fn cutoff_and_resonance_on_chip_as_channels_lock_its_filter_until_cc_121() {
    let mut r = Route::with(MIDI_MODE_MULTI, 0, |_| {});
    let cutoff = r.rig.live.cutoff;
    r.trs(&[0xB1, 74, 127, 0xB2, 71, 64]);
    assert_eq!(r.rig.live.cutoff, cutoff, "not the instrument's filter");
    r.tick();
    assert_eq!(r.take(), [Ev::Filter(3, 2047, 8)]);
    r.trs(&[0xB0, 74, 0]);
    r.tick();
    assert_eq!(r.take(), [Ev::Filter(3, 0, 8)]);
    r.trs(&[0xB2, 121, 0]);
    r.tick();
    assert_eq!(r.take(), [Ev::Filter(0, 0, 8)], "given back to the song");
    r.trs(&[0xB3, 74, 0]);
    assert_eq!(r.rig.live.cutoff, 0, "N + 3's CC 74 is the instrument's");
    r.tick();
    assert!(r.take().is_empty());
}

#[test]
fn a_change_of_mode_releases_the_jam_and_gives_the_filter_back() {
    let mut r = Route::with(MIDI_MODE_MULTI, 0, |_| {});
    r.trs(&[0x90, C4, 1, 0x92, E4, 1, 0xB0, 74, 10, 0x93, G4, 1]);
    r.tick();
    r.take();
    assert_eq!(r.held(), [G4]);
    let mut s = r.route.setup;
    s.mode = MIDI_MODE_LIVE as u8;
    unsafe { midi_route_set(&mut *r.route, &s) };
    assert_eq!(r.take(), [Ev::Off(0), Ev::Off(2)]);
    assert_eq!(r.held(), [] as [u8; 0], "the instrument's notes too");
    r.tick();
    assert_eq!(r.take(), [Ev::Filter(0, 161, 0)]);
    unsafe { midi_route_set(&mut *r.route, &s) };
    r.tick();
    assert!(r.take().is_empty(), "the same setup changes nothing");
}

#[test]
fn all_notes_off_releases_the_channels_voices_only() {
    let mut r = Route::with(MIDI_MODE_MULTI, 0, |_| {});
    r.trs(&[0x90, C4, 1, 0x91, E4, 1, 0xB1, 123, 0, 0xB0, 120, 0]);
    assert_eq!(
        r.take(),
        [on(0, C4, 1), on(1, E4, 1), Ev::Off(1), Ev::Off(0)]
    );
}

#[test]
fn lost_input_releases_every_jam_note() {
    let mut r = Route::with(MIDI_MODE_MULTI, 0, |_| {});
    r.trs(&[0x90, C4, 1, 0x92, E4, 1, 0xE0, 0, 0]);
    r.take();
    let e = midi_ring_entry {
        stamp: 0,
        data: 0xFE,
        source: TRS,
        epoch: 1,
    };
    unsafe { midi_rx_entry(&mut *r.rx, &e) };
    assert_eq!(r.take(), [Ev::Off(0), Ev::Off(2)]);
    r.tick();
    assert!(
        r.take().is_empty(),
        "the bend never went, and is centred now"
    );
}

#[test]
fn whole_chip_a_plays_three_voices_and_steals_the_oldest() {
    let mut r = Route::with(MIDI_MODE_WHOLE_A, 0, |_| {});
    r.trs(&[0xC0, 9]);
    r.trs(&[0x90, C4, 1, E4, 1, G4, 1, 72, 1]);
    assert_eq!(
        r.take(),
        [on(0, C4, 10), on(1, E4, 10), on(2, G4, 10), on(0, 72, 10)]
    );
    r.trs(&[0x80, E4, 0, 0x90, 74, 1, 0x90, G4, 1]);
    assert_eq!(r.take(), [Ev::Off(1), on(1, 74, 10), on(2, G4, 10)]);
    r.trs(&[0x91, C4, 1, 0x92, C4, 1]);
    assert!(r.take().is_empty(), "N + 1 and N + 2 are nobody's");
    r.trs(&[0x93, C4, 1]);
    assert_eq!(r.held(), [C4], "N + 3 plays the instrument");
}

#[test]
fn whole_chip_a_bends_every_voice_by_channel_n() {
    let mut r = Route::with(MIDI_MODE_WHOLE_A, 0, |_| {});
    r.trs(&[0xE0, 0x00, 0x60]);
    r.tick();
    let o = (0x3000 - 8192) * 2;
    assert_eq!(r.take(), [Ev::Bend(0, o), Ev::Bend(1, o), Ev::Bend(2, o)]);
}

#[test]
fn chord_plays_the_stored_chord_over_the_three_voices() {
    let mut r = Route::with(MIDI_MODE_WHOLE_A, 0, |s| {
        s.option = MIDI_WHOLE_CHORD as u8;
        s.chord = [3, 10];
    });
    r.trs(&[0x90, C4, 1]);
    assert_eq!(
        r.take(),
        [on(0, C4, 1), on(1, C4 + 3, 1), on(2, C4 + 10, 1)]
    );
    r.trs(&[0x90, E4, 1]);
    assert_eq!(
        r.take(),
        [on(0, E4, 1), on(1, E4 + 3, 1), on(2, E4 + 10, 1)]
    );
    r.trs(&[0x80, E4, 0]);
    assert_eq!(
        r.take(),
        [on(0, C4, 1), on(1, C4 + 3, 1), on(2, C4 + 10, 1)]
    );
    r.trs(&[0x80, C4, 0]);
    assert_eq!(r.take(), [Ev::Off(0), Ev::Off(1), Ev::Off(2)]);
    // Past the top a voice is left silent.
    r.trs(&[0x90, 100, 1]);
    assert_eq!(r.take(), [on(0, 100, 1), on(1, 103, 1)]);
}

#[test]
fn arp_cycles_the_held_notes_on_voice_1_every_n_frames() {
    let mut r = Route::with(MIDI_MODE_WHOLE_A, 0, |s| {
        s.option = MIDI_WHOLE_ARP as u8;
        s.arp_rate = 2;
    });
    r.trs(&[0x90, C4, 1, E4, 1, G4, 1]);
    assert_eq!(r.take(), [on(0, C4, 1)], "the first note at once");
    let mut steps = Vec::new();
    for t in 0..12 {
        r.tick();
        for e in r.take() {
            steps.push((t, e));
        }
    }
    // A step every 2 frames, 4 ticks.
    assert_eq!(
        steps,
        [(3, on(0, E4, 1)), (7, on(0, G4, 1)), (11, on(0, C4, 1))]
    );
    r.trs(&[0x80, E4, 0, 0x80, C4, 0]);
    for _ in 0..4 {
        r.tick();
    }
    assert_eq!(r.take(), [on(0, G4, 1)]);
    r.trs(&[0x80, G4, 0]);
    assert_eq!(r.take(), [Ev::Off(0)]);
    for _ in 0..8 {
        r.tick();
    }
    assert!(r.take().is_empty());
}

#[test]
fn arp_with_rate_0_steps_every_16th_of_the_clock() {
    let mut r = Route::with(MIDI_MODE_WHOLE_A, 0, |s| {
        s.option = MIDI_WHOLE_ARP as u8;
        s.arp_rate = 0;
    });
    r.trs(&[0x90, C4, 1, E4, 1]);
    r.take();
    for _ in 0..10 {
        r.tick();
    }
    assert!(r.take().is_empty(), "no clock, no step");
    let mut at = Vec::new();
    for k in 0..12 {
        r.trs(&[0xF8]);
        if !r.take().is_empty() {
            at.push(k);
        }
    }
    assert_eq!(at, [5, 11]);
}

#[test]
fn chip_a_belongs_to_the_player_in_multitimbral_and_whole_chip() {
    for (mode, owns) in [
        (MIDI_MODE_LIVE, 0),
        (MIDI_MODE_MULTI, 1),
        (MIDI_MODE_WHOLE_A, 1),
        (MIDI_MODE_WHOLE_B, 1),
        (MIDI_MODE_OMNI, 0),
    ] {
        assert_eq!(unsafe { midi_route_owns_chip_a(mode) }, owns, "mode {mode}");
    }
}

#[test]
fn every_mode_and_option_has_a_name_and_a_doc() {
    for m in 0..MIDI_MODE_COUNT {
        let name = unsafe { std::ffi::CStr::from_ptr(midi_route_mode_name(m)) };
        assert!(!name.is_empty() && name.to_bytes().len() <= 9);
        let doc = unsafe { std::ffi::CStr::from_ptr(midi_route_mode_doc(m)) };
        assert!(!doc.is_empty());
    }
    for o in 0..3 {
        let name = unsafe { std::ffi::CStr::from_ptr(midi_route_option_name(o)) };
        assert!(!name.is_empty() && name.to_bytes().len() <= 9);
    }
}

/// A setup's fields, to compare.
fn fields(s: &midi_route_setup) -> [u8; 8] {
    [
        s.mode,
        s.base,
        s.rows_per_beat,
        s.legato,
        s.option,
        s.chord[0],
        s.chord[1],
        s.arp_rate,
    ]
}

fn setup(mode: midi_mode, base: u8) -> midi_route_setup {
    let mut s = midi_route_setup::default();
    unsafe { midi_route_setup_default(&mut s) };
    s.mode = mode as u8;
    s.base = base;
    s
}

#[test]
fn the_midi_chunk_round_trips_every_mode() {
    let dflt = setup(MIDI_MODE_OMNI, 0);
    for mode in [
        MIDI_MODE_LIVE,
        MIDI_MODE_MULTI,
        MIDI_MODE_WHOLE_A,
        MIDI_MODE_WHOLE_B,
        MIDI_MODE_OMNI,
    ] {
        let mut s = setup(mode, if mode == MIDI_MODE_OMNI { 0 } else { 13 });
        s.rows_per_beat = 3;
        s.legato = 1;
        s.option = MIDI_WHOLE_ARP as u8;
        s.chord = [5, 9];
        s.arp_rate = 0;
        let mut song = sidou_tests::song::Song::device();
        assert_eq!(unsafe { midi_route_setup_to_song(song.ptr_mut(), &s) }, 0);
        let mut back = midi_route_setup::default();
        assert_eq!(
            unsafe { midi_route_setup_from_song(song.ptr(), &dflt, &mut back) },
            1
        );
        assert_eq!(fields(&back), fields(&s), "mode {mode}");
        let mut m = song_midi::default();
        assert_eq!(unsafe { song_ext_get_midi(song.ptr(), &mut m) }, 1);
        let want: &[u8] = match mode {
            MIDI_MODE_MULTI => &[13, 14, 15, 0],
            MIDI_MODE_WHOLE_A => &[13, 13, 13, 0],
            MIDI_MODE_OMNI => &[],
            _ => &[13],
        };
        assert_eq!(
            &m.channel[..usize::from(m.count)],
            want,
            "mode {mode}: each voice's channel"
        );
    }
}

#[test]
fn a_song_without_the_chunk_or_its_options_takes_the_default() {
    let mut dflt = setup(MIDI_MODE_LIVE, 7);
    dflt.chord = [3, 7];
    let mut song = sidou_tests::song::Song::device();
    let mut back = midi_route_setup::default();
    assert_eq!(
        unsafe { midi_route_setup_from_song(song.ptr(), &dflt, &mut back) },
        0
    );
    assert_eq!(fields(&back), fields(&dflt));
    // An M2c chunk: version, mode, rows a beat, the channels, no options.
    let chunk = [1u8, MIDI_MODE_MULTI as u8, 6, 1, 2];
    assert_eq!(
        unsafe {
            song_ext_set(
                song.ptr_mut(),
                sidou_tests::song::TAG_MIDI,
                chunk.as_ptr(),
                chunk.len() as u32,
            )
        },
        0
    );
    assert_eq!(
        unsafe { midi_route_setup_from_song(song.ptr(), &dflt, &mut back) },
        1
    );
    let mut want = dflt;
    want.mode = MIDI_MODE_MULTI as u8;
    want.base = 2;
    want.rows_per_beat = 6;
    assert_eq!(fields(&back), fields(&want));
    // An unknown mode and out-of-range options are the default's, clamped.
    let chunk = [1u8, 9, 0, 0, 1, 7, 99, 200, 250];
    unsafe {
        song_ext_set(
            song.ptr_mut(),
            sidou_tests::song::TAG_MIDI,
            chunk.as_ptr(),
            chunk.len() as u32,
        )
    };
    unsafe { midi_route_setup_from_song(song.ptr(), &dflt, &mut back) };
    assert_eq!(back.mode, dflt.mode);
    assert_eq!(back.legato, 1);
    assert_eq!(back.option, MIDI_WHOLE_POLY as u8);
    assert_eq!(back.chord, [MIDI_ROUTE_CHORD_MAX as u8; 2]);
    assert_eq!(back.arp_rate, MIDI_ROUTE_ARP_MAX as u8);
}
