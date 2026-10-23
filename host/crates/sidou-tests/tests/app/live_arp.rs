// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The chord arpeggio (app/live.h live_arp_set): a held chord as one voice stepping through its
//! notes at the frame rate, its gate held, the other voices free.

use sidou_sys::*;

use crate::{freq_table, Rig};

const C4: u8 = 60;
const D4: u8 = 62;
const E4: u8 = 64;
const G4: u8 = 67;

fn arp(rig: &mut Rig, field: u32, value: u32) {
    unsafe { live_arp_set(&mut *rig.live, field, value) }
}

fn turn(rig: &mut Rig, field: u32, detents: i32) {
    unsafe { live_arp_turn(&mut *rig.live, field, detents) }
}

/// On, at a step a frame (1F): the pace these tests count their ticks in (the default, 3F, is
/// off_at_power_on_and_on_every_preset_load's).
fn on(rig: &mut Rig) {
    arp(rig, LIVE_ARP_ON, 1);
    arp(rig, LIVE_ARP_RATE, 0);
}

fn hz(note: u8) -> u16 {
    freq_table()[usize::from(note)]
}

/// The gated voices.
fn gated(rig: &Rig) -> Vec<u8> {
    (0..3).filter(|&v| rig.gated(v)).collect()
}

/// The one gated voice's note after each of `ticks` ticks, by its frequency register.
fn notes(rig: &mut Rig, ticks: usize) -> Vec<u8> {
    let table = freq_table();
    (0..ticks)
        .map(|_| {
            rig.tick();
            let g = gated(rig);
            assert_eq!(g.len(), 1, "voices gated: {g:?}");
            let f = rig.voice_freq(g[0]);
            table
                .iter()
                .position(|&t| t == f)
                .expect("a note's frequency") as u8
        })
        .collect()
}

/// Each note of `cycle` held for `ticks_each` ticks, from the step after the first (which
/// the voice starts on), `rounds` times round.
fn steps(cycle: &[u8], ticks_each: usize, rounds: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for r in 0..rounds {
        for (i, &n) in cycle.iter().enumerate() {
            if r == 0 && i == 0 {
                out.extend(std::iter::repeat_n(n, ticks_each - 1));
            } else {
                out.extend(std::iter::repeat_n(n, ticks_each));
            }
        }
    }
    out
}

fn chord(rig: &mut Rig, ns: &[u8]) {
    for &n in ns {
        rig.note_on(n);
    }
}

#[test]
fn off_at_power_on_and_on_every_preset_load() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert_eq!(rig.live.arp_on, 0);
    assert_eq!(
        u32::from(rig.live.arp_rate),
        LIVE_ARP_RATE_DEFAULT,
        "3F: a held chord heard note by note, not a wobble"
    );
    assert_eq!(unsafe { live_arp_steps }[LIVE_ARP_RATE_DEFAULT as usize], 3);
    assert_eq!(rig.live.arp_order, LIVE_ARP_UP as u8);
    assert_eq!(rig.live.arp_span, 1);
    assert_eq!(rig.live.arp_voice, LIVE_ARP_NO_VOICE as u8);
    on(&mut rig);
    rig.set_preset(1);
    assert_eq!(rig.live.arp_on, 0, "the factory's preset: off");
    chord(&mut rig, &[C4, E4, G4]);
    assert_eq!(gated(&rig).len(), 3, "off: a chord on three voices");
}

#[test]
fn a_held_chord_is_one_voice_stepping_up_a_note_a_frame_with_its_gate_held() {
    let mut rig = Rig::new(SID_MODEL_6581);
    on(&mut rig);
    rig.note_on(C4);
    assert_eq!(gated(&rig).len(), 1);
    let v = gated(&rig)[0];
    assert_eq!(rig.voice_freq(v), hz(C4), "the first note at once");
    rig.note_on(E4);
    rig.note_on(G4);
    assert_eq!(gated(&rig), vec![v], "one voice for the chord");
    rig.clear();
    // A frame is two 100 Hz ticks: the note changes every second tick.
    let want = steps(&[C4, E4, G4], 2, 3);
    assert_eq!(notes(&mut rig, want.len()), want);
    // The gate never dropped: no control write for the voice, nor anything on the others.
    let ctrl = |r: u8| r % 7 == 4 && r < 21;
    assert!(
        rig.log().iter().all(|&(r, _)| !ctrl(r)),
        "a control write while the arpeggio steps: {:?}",
        rig.log()
    );
    assert!(
        rig.log()
            .iter()
            .filter(|&&(r, _)| r < 21)
            .all(|&(r, _)| r / 7 == v),
        "another voice was written"
    );
}

#[test]
fn the_orders_down_up_down_and_as_played() {
    for (order, played, want) in [
        (LIVE_ARP_UP, [E4, C4, G4], vec![C4, E4, G4]),
        (LIVE_ARP_DOWN, [E4, C4, G4], vec![G4, E4, C4]),
        (LIVE_ARP_UP_DOWN, [E4, C4, G4], vec![C4, E4, G4, E4]),
        (LIVE_ARP_PLAYED, [E4, C4, G4], vec![E4, C4, G4]),
    ] {
        let mut rig = Rig::new(SID_MODEL_6581);
        on(&mut rig);
        arp(&mut rig, LIVE_ARP_ORDER, order);
        chord(&mut rig, &played);
        // The first note was the only one held at its note-on: the cycle as it is now from the
        // next step on.
        rig.tick();
        let got = notes(&mut rig, 2 * want.len() * 2);
        let mut cycle = Vec::new();
        let start = want
            .iter()
            .position(|&n| n == got[0])
            .expect("a note of the cycle");
        for k in 0..want.len() * 2 {
            let n = want[(start + k) % want.len()];
            cycle.extend([n, n]);
        }
        assert_eq!(got, cycle, "order {order}");
    }
}

#[test]
fn the_rate_is_frames_a_step() {
    for (rate, frames) in [(0u32, 1usize), (1, 2), (2, 3), (3, 4), (4, 6), (5, 8)] {
        assert_eq!(unsafe { live_arp_steps }[rate as usize], frames as u8);
        let mut rig = Rig::new(SID_MODEL_6581);
        on(&mut rig);
        arp(&mut rig, LIVE_ARP_RATE, rate);
        chord(&mut rig, &[C4, E4, G4]);
        let want = steps(&[C4, E4, G4], frames * 2, 2);
        assert_eq!(notes(&mut rig, want.len()), want, "rate {rate}");
    }
    let mut rig = Rig::new(SID_MODEL_6581);
    arp(&mut rig, LIVE_ARP_RATE, 99);
    assert_eq!(u32::from(rig.live.arp_rate), LIVE_ARP_RATES - 1, "clamped");
    turn(&mut rig, LIVE_ARP_RATE, -100);
    assert_eq!(rig.live.arp_rate, 0);
    turn(&mut rig, LIVE_ARP_RATE, 2);
    assert_eq!(rig.live.arp_rate, 2);
}

#[test]
fn a_new_key_joins_the_cycle_and_a_released_one_leaves_it() {
    let mut rig = Rig::new(SID_MODEL_6581);
    on(&mut rig);
    chord(&mut rig, &[C4, G4]);
    assert_eq!(notes(&mut rig, 3), vec![C4, G4, G4]);
    // E4 down while G4 sounds: the next step is the one after G4 in C4 E4 G4, C4.
    rig.note_on(E4);
    assert_eq!(notes(&mut rig, 6), vec![C4, C4, E4, E4, G4, G4]);
    // A note below them all joins in its place.
    rig.note_on(D4);
    assert_eq!(notes(&mut rig, 8), vec![C4, C4, D4, D4, E4, E4, G4, G4]);
    // E4 up while C4 sounds: C4 D4 G4.
    assert_eq!(notes(&mut rig, 1), vec![C4]);
    rig.note_off(E4);
    assert_eq!(notes(&mut rig, 6), vec![C4, D4, D4, G4, G4, C4]);
    // The sounding note up: the one after it next.
    rig.note_off(C4);
    assert_eq!(notes(&mut rig, 4), vec![C4, D4, D4, G4]);
    // A note held twice is held once.
    rig.note_on(G4);
    assert_eq!(rig.live.arp_n, 2);
}

#[test]
fn the_span_repeats_the_chord_an_octave_up() {
    let mut rig = Rig::new(SID_MODEL_6581);
    on(&mut rig);
    arp(&mut rig, LIVE_ARP_SPAN, 2);
    chord(&mut rig, &[C4, E4, G4]);
    let want = steps(&[C4, E4, G4, C4 + 12, E4 + 12, G4 + 12], 2, 2);
    assert_eq!(notes(&mut rig, want.len()), want);
    let mut rig = Rig::new(SID_MODEL_6581);
    on(&mut rig);
    arp(&mut rig, LIVE_ARP_SPAN, 2);
    arp(&mut rig, LIVE_ARP_ORDER, LIVE_ARP_DOWN);
    rig.note_on(C4);
    assert_eq!(notes(&mut rig, 4), vec![C4 + 12, C4, C4, C4 + 12]);
    arp(&mut rig, LIVE_ARP_SPAN, 9);
    assert_eq!(u32::from(rig.live.arp_span), LIVE_ARP_SPAN_MAX, "clamped");
    arp(&mut rig, LIVE_ARP_SPAN, 0);
    assert_eq!(rig.live.arp_span, 1, "clamped");
}

#[test]
fn letting_go_of_every_key_releases_the_voice_and_the_steps_stop() {
    let mut rig = Rig::new(SID_MODEL_6581);
    on(&mut rig);
    chord(&mut rig, &[C4, E4, G4]);
    let v = gated(&rig)[0];
    notes(&mut rig, 3);
    rig.note_off(C4);
    rig.note_off(E4);
    assert_eq!(gated(&rig), vec![v], "a key still held");
    rig.note_off(G4);
    assert!(gated(&rig).is_empty(), "the release");
    assert_eq!(rig.live.arp_voice, LIVE_ARP_NO_VOICE as u8);
    rig.clear();
    for _ in 0..10 {
        rig.tick();
    }
    assert!(
        rig.log().iter().all(|&(r, _)| r >= 21),
        "the voice moved after the release: {:?}",
        rig.log()
    );
    // A new chord starts again from its first note.
    rig.note_on(E4);
    assert_eq!(rig.voice_freq(gated(&rig)[0]), hz(E4));
    unsafe { live_all_notes_off(&mut *rig.live) };
    assert!(gated(&rig).is_empty());
    assert_eq!(rig.live.arp_n, 0);
}

#[test]
fn turning_it_on_takes_a_held_chord_into_one_voice_and_off_gives_it_back() {
    let mut rig = Rig::new(SID_MODEL_6581);
    chord(&mut rig, &[C4, E4, G4]);
    assert_eq!(gated(&rig).len(), 3);
    let first = (0..3u8)
        .find(|&v| rig.voice_freq(v) == hz(C4))
        .expect("C4's voice");
    on(&mut rig);
    assert_eq!(
        gated(&rig),
        vec![first],
        "the oldest note's voice keeps its gate"
    );
    assert_eq!(notes(&mut rig, 5), vec![C4, E4, E4, G4, G4]);
    arp(&mut rig, LIVE_ARP_ON, 0);
    assert_eq!(gated(&rig).len(), 3, "each held note on a voice again");
    let mut held: Vec<u16> = (0..3).map(|v| rig.voice_freq(v)).collect();
    held.sort_unstable();
    assert_eq!(held, vec![hz(C4), hz(E4), hz(G4)]);
}

#[test]
fn an_order_or_span_change_keeps_the_sounding_note() {
    let mut rig = Rig::new(SID_MODEL_6581);
    on(&mut rig);
    chord(&mut rig, &[C4, E4, G4]);
    assert_eq!(notes(&mut rig, 3), vec![C4, E4, E4]);
    arp(&mut rig, LIVE_ARP_ORDER, LIVE_ARP_DOWN);
    assert_eq!(notes(&mut rig, 4), vec![C4, C4, G4, G4]);
    turn(&mut rig, LIVE_ARP_ORDER, 9);
    assert_eq!(u32::from(rig.live.arp_order), LIVE_ARP_PLAYED, "clamped");
}

#[test]
fn the_settings_are_kept_with_the_presets_edits() {
    let mut rig = Rig::new(SID_MODEL_6581);
    on(&mut rig);
    arp(&mut rig, LIVE_ARP_RATE, 3);
    arp(&mut rig, LIVE_ARP_ORDER, LIVE_ARP_UP_DOWN);
    arp(&mut rig, LIVE_ARP_SPAN, 2);
    assert_eq!(
        unsafe { live_preset_edited(&*rig.live) },
        1,
        "an edit: the `*`"
    );
    unsafe { live_recall_preset(&mut *rig.live, 1) };
    assert_eq!(rig.live.arp_on, 0, "PWM BASS's own: off");
    assert_eq!(unsafe { live_preset_edited(&*rig.live) }, 0);
    unsafe { live_recall_preset(&mut *rig.live, 0) };
    assert_eq!(
        (
            rig.live.arp_on,
            rig.live.arp_rate,
            rig.live.arp_order,
            rig.live.arp_span
        ),
        (1, 3, LIVE_ARP_UP_DOWN as u8, 2),
        "SAW LEAD's edits back"
    );
    // A revert, or a program change, loads the factory's: off.
    unsafe { live_preset_step(&mut *rig.live, 0, 1) };
    assert_eq!(
        (rig.live.arp_on, u32::from(rig.live.arp_rate)),
        (0, LIVE_ARP_RATE_DEFAULT)
    );
    assert_eq!(
        unsafe { live_preset_edited(&*rig.live) },
        0,
        "the default rate is no edit"
    );
}

#[test]
fn its_ccs_switch_it_and_set_the_rate_order_and_span() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.cc(LIVE_CC_ARP, 64);
    assert_eq!(rig.live.arp_on, 1);
    rig.cc(LIVE_CC_ARP, 63);
    assert_eq!(rig.live.arp_on, 0);
    rig.cc(LIVE_CC_ARP, 127);
    for (value, rate) in [(0u8, 0u8), (14, 0), (15, 1), (127, 8)] {
        rig.cc(LIVE_CC_ARP_RATE, value);
        assert_eq!(rig.live.arp_rate, rate, "CC {value}");
    }
    rig.cc(LIVE_CC_ARP_ORDER, 127);
    assert_eq!(u32::from(rig.live.arp_order), LIVE_ARP_PLAYED);
    rig.cc(LIVE_CC_ARP_ORDER, 32);
    assert_eq!(u32::from(rig.live.arp_order), LIVE_ARP_DOWN);
    rig.cc(LIVE_CC_ARP_SPAN, 64);
    assert_eq!(rig.live.arp_span, 2);
    rig.cc(LIVE_CC_ARP_SPAN, 0);
    assert_eq!(rig.live.arp_span, 1);
    // None is a map row's CC.
    for cc in LIVE_CC_ARP..=LIVE_CC_ARP_SPAN {
        assert!(unsafe { midi_map_for_cc(cc as u8) }.is_null(), "CC {cc}");
    }
}

#[test]
fn a_clock_rate_steps_on_the_midi_clock_and_on_frames_without_one() {
    let mut rig = Rig::new(SID_MODEL_6581);
    on(&mut rig);
    arp(&mut rig, LIVE_ARP_RATE, LIVE_ARP_RATE_CLOCK + 1); // a 16th: 6 clocks
    assert_eq!(
        unsafe { live_arp_steps }[(LIVE_ARP_RATE_CLOCK + 1) as usize],
        6
    );
    chord(&mut rig, &[C4, E4, G4]);
    // No clock: a clock a frame, 6 frames a step.
    assert_eq!(
        notes(&mut rig, 24),
        steps(&[C4, E4, G4], 12, 1)[..24].to_vec()
    );
    // With a clock: a step every sixth clock, whatever the ticks.
    unsafe { live_arp_realtime(&mut *rig.live, 0xFA) };
    let mut got = Vec::new();
    for _ in 0..18 {
        unsafe { live_arp_realtime(&mut *rig.live, 0xF8) };
        got.push(notes(&mut rig, 1)[0]);
    }
    let sounding = got[0];
    let changes: Vec<usize> = (1..got.len()).filter(|&i| got[i] != got[i - 1]).collect();
    assert_eq!(changes, vec![5, 11, 17], "{got:?} from {sounding}");
    // The clock gone (300 ms): frames again.
    for _ in 0..LIVE_ARP_CLOCK_TICKS {
        rig.tick();
    }
    let got = notes(&mut rig, 24);
    let changes = (1..got.len()).filter(|&i| got[i] != got[i - 1]).count();
    assert_eq!(changes, 2, "a step every 12 ticks: {got:?}");
}

#[test]
fn the_sync_lead_steps_its_synced_pair() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.set_preset(2); // SYNC LEAD: voice 1 synced to voice 3, which sets the pitch
    on(&mut rig);
    chord(&mut rig, &[C4, E4, G4]);
    let mut got = Vec::new();
    for _ in 0..6 {
        rig.tick();
        got.push(rig.voice_freq(2));
    }
    assert_eq!(
        got,
        vec![hz(C4), hz(E4), hz(E4), hz(G4), hz(G4), hz(C4)],
        "voice 3 carries the pitch"
    );
}
