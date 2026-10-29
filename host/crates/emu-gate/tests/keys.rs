// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: which chip the keys play (M2b Task 4, app/keys_route.h). With no song the keys
//! play chip A as in M1 (the other gates hold that: every golden and host comparison). A song
//! on chip A is M2c's (PLAY, tests/song.rs); here the flag it sets, `keys_router.song`
//! (platform/fm1/main.c), is set in emulated RAM with no song playing, and the transport leaves
//! a flag it did not set alone: the keys then play chip B, which holds the keys' setup.

mod common;

use fm1_core::Bus;

use common::{build_dir, pitch, record, Firmware, Machine};
use sidou_sys::{ilp32, SID_MODEL_6581, SID_MODEL_8580};

/// The A4 key (Felucca's id), ALGORITHM (encoder 1: on LIVE's TONE page the chip's profile, the
/// 6581 R4AR at boot, the 8580 R5 one step on).
const A4_KEY: u8 = 30;
const ALGORITHM: u8 = 1;

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, fw);
    m.run_ms(300);
    m
}

fn song(m: &mut Machine, fw: &Firmware, on: bool) {
    m.soc.write8(
        fw.symbol("keys_router") + ilp32::keys_route::song as u32,
        u8::from(on),
    );
}

fn ui_live(m: &mut Machine, fw: &Firmware) -> u32 {
    m.soc.read32(fw.symbol("ui") + ilp32::views::l as u32)
}

fn keys_chip(m: &mut Machine, fw: &Firmware) -> u8 {
    let l = ui_live(m, fw);
    m.soc.read8(l + ilp32::live::keys_chip as u32)
}

/// The sid chip x renders.
fn sid_of(m: &mut Machine, fw: &Firmware, x: usize) -> u32 {
    let base = fw.chips();
    let active = m
        .soc
        .read8(base + (ilp32::reg_ring_chips::active + x) as u32);
    base + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32
}

/// Voice 1..3's registers as chip x has them: frequency, control's gate, attack, sustain.
fn voices(m: &mut Machine, fw: &Firmware, x: usize) -> Vec<(u32, u8, u8, u8)> {
    let s = sid_of(m, fw, x);
    (0..3usize)
        .map(|v| {
            let o = s + (ilp32::sid::osc + v * ilp32::sid_osc::SIZE) as u32;
            let e = s + (ilp32::sid::env + v * ilp32::sid_env::SIZE) as u32;
            (
                m.soc.read32(o + ilp32::sid_osc::freq as u32),
                m.soc.read8(e + ilp32::sid_env::gate as u32),
                m.soc.read8(e + ilp32::sid_env::attack as u32),
                m.soc.read8(e + ilp32::sid_env::sustain as u32),
            )
        })
        .collect()
}

/// With the song hook set, the keys move to chip B: chip B goes in use, the LIVE view's tap
/// reads it, and A4 sounds from it at 440 Hz; chip B's voices get exactly the registers chip
/// A's got for the same key with no song (the same setup and the instrument's writes), and
/// chip A's are left alone. The hook cleared with no key held, the keys go back to chip A, and
/// chip B goes out of use once its release tail has played out.
#[test]
fn with_a_song_a_key_sounds_from_chip_b_as_it_does_from_chip_a() {
    let Some(fw) = fixture("the keys test") else {
        return;
    };
    // No song: A4 on chip A.
    let mut m = booted(&fw);
    m.key(A4_KEY, true);
    m.run_ms(100);
    let on_a = voices(&mut m, &fw, 0);
    assert_eq!(keys_chip(&mut m, &fw), 0);
    assert_eq!(
        m.soc.read8(fw.symbol("b_on")),
        0,
        "chip B in use with no song"
    );
    // A song: the keys move to chip B.
    let mut m = booted(&fw);
    song(&mut m, &fw, true);
    m.run_ms(50);
    assert_eq!(keys_chip(&mut m, &fw), 1, "the keys stayed on chip A");
    assert_eq!(m.soc.read8(fw.symbol("b_on")), 1, "chip B not in use");
    assert_eq!(m.soc.read8(fw.symbol("tap_chip")), 1, "the tap on chip A");
    let a_before = voices(&mut m, &fw, 0);
    m.key(A4_KEY, true);
    m.run_ms(100);
    let on_b = voices(&mut m, &fw, 1);
    assert_eq!(
        on_b, on_a,
        "chip B's voices against chip A's for the same key"
    );
    assert_eq!(voices(&mut m, &fw, 0), a_before, "the key reached chip A");
    let f0 = pitch(&record(&mut m, 300));
    println!("A4 on chip B: {f0:.2} Hz");
    assert!(
        (f0 / 440.0 - 1.0).abs() < 0.005,
        "A4 on chip B at {f0:.2} Hz"
    );
    m.key(A4_KEY, false);
    song(&mut m, &fw, false);
    m.run_ms(100);
    assert_eq!(keys_chip(&mut m, &fw), 0, "the keys stayed on chip B");
    m.run_ms(1500); // the release tail plays out on chip B first
    assert_eq!(m.soc.read8(fw.symbol("b_on")), 0, "chip B still in use");
}

/// The chip model from the panel (ALGORITHM on LIVE's TONE page) acts on the keys' setup: during
/// a song on chip B (the keys'), not chip A (the song's); after the song chip A takes it, and
/// with no song chip B follows chip A.
#[test]
fn the_panels_chip_model_acts_on_the_keys_setup() {
    let Some(fw) = fixture("the keys model test") else {
        return;
    };
    let mut m = booted(&fw);
    let model = |m: &mut Machine, x: usize| {
        let s = sid_of(m, &fw, x);
        m.soc.read8(s + ilp32::sid::model as u32)
    };
    // No song: the 8580 R5 on chip A, and chip B follows in the background.
    m.turn(ALGORITHM, 1);
    m.run_ms(300);
    assert_eq!(
        (model(&mut m, 0), model(&mut m, 1)),
        (SID_MODEL_8580 as u8, SID_MODEL_8580 as u8)
    );
    // A song: back to the 6581 R4AR on chip B, the keys', leaving chip A, the song's.
    song(&mut m, &fw, true);
    m.run_ms(50);
    m.turn(ALGORITHM, -1);
    m.run_ms(300);
    assert_eq!(
        (model(&mut m, 0), model(&mut m, 1)),
        (SID_MODEL_8580 as u8, SID_MODEL_6581 as u8)
    );
    // The song stops: chip A takes the keys' setup, then the keys go back to it.
    song(&mut m, &fw, false);
    m.run_ms(300);
    assert_eq!(keys_chip(&mut m, &fw), 0);
    assert_eq!(
        (model(&mut m, 0), model(&mut m, 1)),
        (SID_MODEL_6581 as u8, SID_MODEL_6581 as u8)
    );
    let base = fw.chips();
    assert_eq!(
        m.soc
            .read32(base + ilp32::reg_ring_chips::lost_resets as u32),
        0
    );
}

/// Review I-4: the keys move back to chip A one tick after the last key-up, but chip B stays
/// in use until its envelopes are at 0, so the note's release tail plays out on chip B (its
/// envelope counter falls block by block after the move) and nothing of it is left frozen for
/// a later use of chip B.
#[test]
fn a_release_tail_plays_out_on_chip_b_after_the_keys_move_back() {
    let Some(fw) = fixture("the keys tail test") else {
        return;
    };
    let mut m = booted(&fw);
    song(&mut m, &fw, true);
    m.run_ms(50);
    m.key(A4_KEY, true);
    m.run_ms(300); // the attack and the decay to SAW LEAD's sustain
    song(&mut m, &fw, false);
    m.key(A4_KEY, false);
    m.run_ms(30);
    assert_eq!(
        keys_chip(&mut m, &fw),
        0,
        "the keys did not go back to chip A"
    );
    let counter = |m: &mut Machine| {
        let s = sid_of(m, &fw, 1);
        (0..3usize)
            .map(|v| {
                let e = s + (ilp32::sid::env + v * ilp32::sid_env::SIZE) as u32;
                m.soc.read8(e + ilp32::sid_env::counter as u32)
            })
            .max()
            .unwrap()
    };
    let b_on = |m: &mut Machine| m.soc.read8(fw.symbol("b_on"));
    // every millisecond: the last block in use, with the envelopes at 0, is 2.9 ms long, so
    // a coarser grid can step over it (as it did once M2c's song code moved the timing)
    let mut trail = Vec::new();
    for _ in 0..400 {
        trail.push((counter(&mut m), b_on(&mut m)));
        m.run_ms(1);
    }
    println!("chip B after the move (envelope, in use) every 1 ms: {trail:?}");
    assert!(
        trail[0].0 > 0 && trail[0].1 == 1,
        "no tail on chip B after the move"
    );
    let out = trail
        .iter()
        .position(|t| t.1 == 0)
        .expect("chip B never went out of use");
    assert!(
        trail[..out].windows(2).all(|w| w[1].0 <= w[0].0),
        "the tail froze"
    );
    assert!(out > 2, "chip B went out of use with its tail");
    assert_eq!(
        trail[out - 1].0,
        0,
        "out of use before its envelopes were at 0"
    );
    assert!(trail[out..].iter().all(|t| t == &(0, 0)));
}
