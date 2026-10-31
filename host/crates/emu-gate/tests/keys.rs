// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use fm1_core::Bus;

use common::panel::{A4_KEY, ALGORITHM};
use common::{firmware, pitch, record, rendered_sid, ui_live, Firmware, Machine};
use sidou_sys::{ilp32, SID_MODEL_6581, SID_MODEL_8580};

const PROFILE: u8 = ALGORITHM;
const CHIP_B_SETUP_MS: u64 = 300;

fn booted(fw: &Firmware) -> Machine {
    let mut m = common::booted(fw);
    m.run_ms(CHIP_B_SETUP_MS);
    m
}

fn song(m: &mut Machine, fw: &Firmware, on: bool) {
    m.soc.write8(
        fw.symbol("keys_router") + ilp32::keys_route::song as u32,
        u8::from(on),
    );
}

fn keys_chip(m: &mut Machine, fw: &Firmware) -> u8 {
    let l = ui_live(m, fw);
    m.soc.read8(l + ilp32::live::keys_chip as u32)
}

fn voices(m: &mut Machine, fw: &Firmware, x: usize) -> Vec<(u32, u8, u8, u8)> {
    let s = rendered_sid(m, fw, x);
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

#[test]
fn with_a_song_a_key_sounds_from_chip_b_as_it_does_from_chip_a() {
    let Some(fw) = firmware("the keys test") else {
        return;
    };
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
    m.run_ms(1500);
    assert_eq!(m.soc.read8(fw.symbol("b_on")), 0, "chip B still in use");
}

#[test]
fn the_panels_chip_model_acts_on_the_keys_setup() {
    let Some(fw) = firmware("the keys model test") else {
        return;
    };
    let mut m = booted(&fw);
    let model = |m: &mut Machine, x: usize| {
        let s = rendered_sid(m, &fw, x);
        m.soc.read8(s + ilp32::sid::model as u32)
    };
    m.turn(PROFILE, 1);
    m.run_ms(300);
    assert_eq!(
        (model(&mut m, 0), model(&mut m, 1)),
        (SID_MODEL_8580 as u8, SID_MODEL_8580 as u8)
    );
    song(&mut m, &fw, true);
    m.run_ms(50);
    m.turn(PROFILE, -1);
    m.run_ms(300);
    assert_eq!(
        (model(&mut m, 0), model(&mut m, 1)),
        (SID_MODEL_8580 as u8, SID_MODEL_6581 as u8)
    );
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

#[test]
fn a_release_tail_plays_out_on_chip_b_after_the_keys_move_back() {
    let Some(fw) = firmware("the keys tail test") else {
        return;
    };
    let mut m = booted(&fw);
    song(&mut m, &fw, true);
    m.run_ms(50);
    m.key(A4_KEY, true);
    m.run_ms(300);
    song(&mut m, &fw, false);
    m.key(A4_KEY, false);
    m.run_ms(30);
    assert_eq!(
        keys_chip(&mut m, &fw),
        0,
        "the keys did not go back to chip A"
    );
    let counter = |m: &mut Machine| {
        let s = rendered_sid(m, &fw, 1);
        (0..3usize)
            .map(|v| {
                let e = s + (ilp32::sid::env + v * ilp32::sid_env::SIZE) as u32;
                m.soc.read8(e + ilp32::sid_env::counter as u32)
            })
            .max()
            .unwrap()
    };
    let b_on = |m: &mut Machine| m.soc.read8(fw.symbol("b_on"));
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
