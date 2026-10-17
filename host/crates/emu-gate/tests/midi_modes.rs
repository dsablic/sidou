// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: MIDI's modes (M2e Task 3, app/midi_route.h) on the firmware, set on the MIDI
//! page and played over TRS. Chip A's channels go to the player's jam voices, which song
//! control binds once it has them (until then they are routed and tracked, and sound nothing:
//! `routing`'s voices show what they would play); the instrument's channel plays chip B.

mod common;

use fm1_core::Bus;

use common::{build_dir, Firmware, Machine};
use sidou_sys::ilp32;

/// app/panel.h: HOME, and the SELECT and ALGORITHM encoders.
const HOME: u8 = 8;
const SELECT: u8 = 0;
const ALGORITHM: u8 = 1;
const MENU_ITEMS: i32 = sidou_sys::VIEWS_MENU_ITEMS as i32;
const MENU_MIDI: i32 = sidou_sys::VIEWS_MENU_MIDI as i32;
const A4: u8 = 69;

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
    m.run_ms(300); // chip B made with the keys' setup
    m
}

fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

/// fm1-emu turns an encoder a detent every 8 ms.
fn turn(m: &mut Machine, enc: u8, clicks: i32) {
    m.turn(enc, clicks);
    m.run_ms(10 * u64::from(clicks.unsigned_abs()) + 20);
}

/// The MIDI page from LIVE, its cursor on MODE.
fn open_midi_page(m: &mut Machine) {
    press(m, HOME);
    m.run_ms(700);
    turn(m, SELECT, -MENU_ITEMS);
    turn(m, SELECT, MENU_MIDI);
    turn(m, ALGORITHM, 1);
    m.run_ms(700);
}

fn route_byte(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    m.soc.read8(fw.symbol("routing") + offset as u32)
}

fn mode(m: &mut Machine, fw: &Firmware) -> u8 {
    route_byte(
        m,
        fw,
        ilp32::midi_route::setup + ilp32::midi_route_setup::mode,
    )
}

/// The MIDI note the router's jam voice v sounds (0xff: none).
fn jam_note(m: &mut Machine, fw: &Firmware, v: usize) -> u8 {
    route_byte(
        m,
        fw,
        ilp32::midi_route::v + v * ilp32::midi_route_voice::SIZE + ilp32::midi_route_voice::note,
    )
}

fn keys_chip(m: &mut Machine, fw: &Firmware) -> u8 {
    let l = m.soc.read32(fw.symbol("ui") + ilp32::views::l as u32);
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

/// Chip x's voices' gates and frequencies.
fn voices(m: &mut Machine, fw: &Firmware, x: usize) -> Vec<(u8, u32)> {
    let s = sid_of(m, fw, x);
    (0..3usize)
        .map(|v| {
            let o = s + (ilp32::sid::osc + v * ilp32::sid_osc::SIZE) as u32;
            let e = s + (ilp32::sid::env + v * ilp32::sid_env::SIZE) as u32;
            (
                m.soc.read8(e + ilp32::sid_env::gate as u32),
                m.soc.read32(o + ilp32::sid_osc::freq as u32),
            )
        })
        .collect()
}

fn gates(m: &mut Machine, fw: &Firmware, x: usize) -> usize {
    voices(m, fw, x).iter().filter(|v| v.0 == 1).count()
}

/// MULTITIMBRAL from the MIDI page: chip A becomes the player's, so the instrument moves to
/// chip B and plays there from channel N + 3 (4: N is channel 1); channels 1..3 reach the
/// router's jam voices and not the instrument; OMNI gives chip A back to the keys.
#[test]
fn multitimbral_plays_the_instrument_on_chip_b_from_n_plus_3() {
    let Some(fw) = fixture("the MIDI modes test") else {
        return;
    };
    let mut m = booted(&fw);
    assert_eq!(mode(&mut m, &fw), sidou_sys::MIDI_MODE_OMNI as u8);
    open_midi_page(&mut m);
    turn(&mut m, ALGORITHM, -3); // OMNI to MULTI
    assert_eq!(mode(&mut m, &fw), sidou_sys::MIDI_MODE_MULTI as u8);
    m.run_ms(100);
    assert_eq!(keys_chip(&mut m, &fw), 1, "the instrument stayed on chip A");
    m.midi(&[0x90, 60, 100, 0x91, 64, 100, 0x92, 67, 100]);
    m.run_ms(50);
    assert_eq!(
        (0..3).map(|v| jam_note(&mut m, &fw, v)).collect::<Vec<_>>(),
        [60, 64, 67],
        "chip A's channels to the jam voices"
    );
    assert_eq!(
        gates(&mut m, &fw, 0),
        0,
        "nothing on chip A until song control binds them"
    );
    assert_eq!(
        gates(&mut m, &fw, 1),
        0,
        "chip A's channels played the instrument"
    );
    m.midi(&[0x93, A4, 100]);
    m.run_ms(50);
    let b = voices(&mut m, &fw, 1);
    assert_eq!(gates(&mut m, &fw, 1), 1, "N + 3 on chip B: {b:?}");
    assert_eq!(gates(&mut m, &fw, 0), 0);
    m.midi(&[0x83, A4, 0, 0x90, 60, 0, 0x91, 64, 0, 0x92, 67, 0]);
    m.run_ms(50);
    assert_eq!(
        (0..3).map(|v| jam_note(&mut m, &fw, v)).collect::<Vec<_>>(),
        [0xff; 3]
    );
    turn(&mut m, ALGORITHM, 3); // back to OMNI
    m.run_ms(1500);
    assert_eq!(
        keys_chip(&mut m, &fw),
        0,
        "the instrument came back to chip A"
    );
    m.midi(&[0x95, A4, 100]);
    m.run_ms(50);
    assert_eq!(
        gates(&mut m, &fw, 0),
        1,
        "OMNI: any channel plays it on chip A"
    );
}

/// A dense stream on the four channels of MULTITIMBRAL (notes, bends, CCs on each, at TRS's
/// full rate) keeps every main-loop pass within the poll allowance.
#[test]
fn a_dense_four_channel_stream_keeps_every_pass_within_the_allowance() {
    let Some(fw) = fixture("the MIDI modes stream test") else {
        return;
    };
    let mut m = booted(&fw);
    open_midi_page(&mut m);
    turn(&mut m, ALGORITHM, -3);
    assert_eq!(mode(&mut m, &fw), sidou_sys::MIDI_MODE_MULTI as u8);
    press(&mut m, HOME); // back to the menu: the page's composes are screen.rs's
    m.run_ms(700);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let fed = m.midi_paced(2000, |i| {
        let ch = (i % 4) as u8;
        let n = 48 + (i / 16 % 24) as u8;
        match i / 4 % 4 {
            0 => vec![0x90 | ch, n, 100],
            1 => vec![0xE0 | ch, (i % 128) as u8, (i / 128 % 128) as u8],
            2 => vec![0xB0 | ch, 74, (i % 128) as u8],
            _ => vec![0x80 | ch, n, 0],
        }
    });
    assert_eq!(m.soc.periph.midi_dropped(), 0);
    assert!(m.soc.periph.midi_queued() > 0, "the jack ran dry");
    let worst = m.passes.iter().copied().max().unwrap_or(0);
    println!("MULTITIMBRAL: {fed} messages in 2 s, the busiest pass {worst}");
    assert!(
        worst <= common::POLL_INSTRUCTIONS,
        "a pass took {worst} instructions, over the poll allowance"
    );
}

fn live_byte(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    let l = m.soc.read32(fw.symbol("ui") + ilp32::views::l as u32);
    m.soc.read8(l + offset as u32)
}

/// M2e Task 4 over TRS: bank 2's program 3 picks the 8580 R5 profile for the instrument (its
/// chip prepared and swapped in), bank 1's program 0 asks for the first stored song (the
/// request waits for the song loader, M2c Task 8) and changes no preset, and bank 0 is the
/// presets again.
#[test]
fn bank_select_picks_profiles_and_songs_over_trs() {
    let Some(fw) = fixture("the MIDI banks test") else {
        return;
    };
    let mut m = booted(&fw);
    let profile = |m: &mut Machine| live_byte(m, &fw, ilp32::live::profile);
    let preset = |m: &mut Machine| live_byte(m, &fw, ilp32::live::preset);
    assert_eq!(u32::from(profile(&mut m)), sidou_sys::SID_CHIP_6581_R4AR);
    m.midi(&[0xB0, 0, 0, 0xB0, 32, 2, 0xC0, 3]);
    m.run_ms(400);
    assert_eq!(u32::from(profile(&mut m)), sidou_sys::SID_CHIP_8580_R5);
    assert_eq!(preset(&mut m), 0);
    m.midi(&[0xB0, 32, 1, 0xC0, 0]);
    m.run_ms(50);
    assert_eq!(
        route_byte(&mut m, &fw, ilp32::midi_route::song_req),
        0,
        "the first song asked for"
    );
    assert_eq!(preset(&mut m), 0);
    m.midi(&[0xB0, 32, 0, 0xC0, 2]);
    m.run_ms(50);
    assert_eq!(preset(&mut m), 2, "bank 0: the presets");
}
