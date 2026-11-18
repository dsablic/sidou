// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use fm1_core::Bus;

use common::panel::*;
use common::{firmware, Firmware, Machine};
use sidou_sys::ilp32;

const A4: u8 = 69;

fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, fw);
    m.run_ms(300);
    m
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

fn keys_chip(m: &mut Machine, fw: &Firmware) -> u8 {
    let l = m.soc.read32(fw.symbol("ui") + ilp32::views::l as u32);
    m.soc.read8(l + ilp32::live::keys_chip as u32)
}

fn sid_of(m: &mut Machine, fw: &Firmware, x: usize) -> u32 {
    let base = fw.chips();
    let active = m
        .soc
        .read8(base + (ilp32::reg_ring_chips::active + x) as u32);
    base + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32
}

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

fn waveforms(m: &mut Machine, fw: &Firmware, x: usize) -> Vec<u8> {
    let s = sid_of(m, fw, x);
    (0..3usize)
        .map(|v| {
            let o =
                s + (ilp32::sid::osc + v * ilp32::sid_osc::SIZE + ilp32::sid_osc::control) as u32;
            m.soc.read8(o) & 0xf0
        })
        .collect()
}

fn routing(m: &mut Machine, fw: &Firmware) -> (u8, u8) {
    let s = sid_of(m, fw, 0);
    (
        m.soc.read8(s + ilp32::sid::routing as u32),
        m.soc.read8(s + ilp32::sid::mode_vol as u32),
    )
}

fn part_name(m: &mut Machine, fw: &Firmware, part: usize) -> String {
    let at = fw.symbol("parts")
        + (ilp32::multi::p
            + part * ilp32::multi_part::SIZE
            + ilp32::multi_part::s
            + ilp32::multi_sound::name) as u32;
    (0..sidou_sys::LAB_SOUND_NAME)
        .map(|i| m.soc.read8(at + i))
        .take_while(|&b| b != 0)
        .map(char::from)
        .collect()
}

const SAW: u8 = 0x20;
const PULSE: u8 = 0x40;
const NOISE: u8 = 0x80;

fn user_sounds_file() -> Vec<u8> {
    let mut file = vec![
        sidou_sys::USER_SOUND_VERSION as u8,
        sidou_sys::LAB_SOUND_BYTES as u8,
        sidou_sys::USER_SOUNDS as u8,
    ];
    for slot in 0..sidou_sys::USER_SOUNDS {
        let mut sound = [0u8; sidou_sys::LAB_SOUND_BYTES as usize];
        unsafe { sidou_sys::live_preset_sound(slot % 4, sound.as_mut_ptr()) };
        let mut name = format!("PAD {}", slot + 1).into_bytes();
        name.resize(sidou_sys::LAB_SOUND_NAME as usize, 0);
        file.push(1);
        file.extend(name);
        file.extend(sound);
    }
    assert_eq!(file.len(), sidou_sys::USER_SOUND_FILE_BYTES as usize);
    file
}

fn booted_with_user_sounds(fw: &Firmware) -> Machine {
    let mut flash = common::settled_flash(&fw.image);
    common::put_file(
        &mut flash,
        "USER SOUNDS",
        sidou_sys::STORE_FILE_SOUNDS,
        &user_sounds_file(),
    );
    let mut m = Machine::boot_with_flash(&fw.image, &flash);
    common::run_past_boot(&mut m, fw);
    m.run_ms(300);
    m
}

fn multi_on(m: &mut Machine, fw: &Firmware) {
    assert_eq!(mode(m, fw), sidou_sys::MIDI_MODE_OMNI as u8);
    open_midi_page(m);
    turn(m, ALGORITHM, -3);
    assert_eq!(mode(m, fw), sidou_sys::MIDI_MODE_MULTI as u8);
    m.run_ms(100);
}

#[test]
fn multitimbral_plays_three_parts_on_chip_a_and_the_instrument_on_chip_b() {
    let Some(fw) = firmware("the MIDI modes test") else {
        return;
    };
    let mut m = booted_with_user_sounds(&fw);
    multi_on(&mut m, &fw);
    assert_eq!(keys_chip(&mut m, &fw), 1, "the instrument went to chip B");
    m.midi(&[0x90, 60, 100, 0x91, 64, 100, 0x92, 67, 100]);
    m.run_ms(50);
    let a = voices(&mut m, &fw, 0);
    assert_eq!(
        gates(&mut m, &fw, 0),
        3,
        "each channel its own voice: {a:?}"
    );
    assert!(
        a[0].1 < a[1].1 && a[1].1 < a[2].1,
        "at their own pitches: {a:?}"
    );
    assert_eq!(
        waveforms(&mut m, &fw, 0),
        [SAW, PULSE, NOISE],
        "and their own sounds"
    );
    assert_eq!(
        gates(&mut m, &fw, 1),
        0,
        "chip A's channels left the instrument alone"
    );
    assert_eq!(
        routing(&mut m, &fw),
        (0b111, 0x1f),
        "N's filter, each voice routed by its sound"
    );
    m.midi(&[0xE1, 0, 0]);
    m.run_ms(50);
    let bent = voices(&mut m, &fw, 0);
    assert!(bent[1].1 < a[1].1, "N + 1 bent down");
    assert_eq!((bent[0].1, bent[2].1), (a[0].1, a[2].1), "the others not");
    m.midi(&[0xC2, 0, 0x92, 67, 100, 0xC1, 16 + 6]);
    m.run_ms(50);
    assert_eq!(
        waveforms(&mut m, &fw, 0)[2],
        SAW,
        "a program change picks N + 2's sound"
    );
    assert_eq!(part_name(&mut m, &fw, 1), "PAD 7", "and USER 7 N + 1's");
    m.midi(&[0x93, A4, 100]);
    m.run_ms(50);
    let b = voices(&mut m, &fw, 1);
    assert_eq!(gates(&mut m, &fw, 1), 1, "N + 3 on chip B: {b:?}");
    m.midi(&[0x83, A4, 0, 0x80, 60, 0, 0x81, 64, 0, 0x82, 67, 0]);
    m.run_ms(50);
    assert_eq!(gates(&mut m, &fw, 0), 0);
    turn(&mut m, ALGORITHM, 3);
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

#[test]
fn a_dense_four_channel_stream_with_program_changes_keeps_every_pass_within_the_allowance() {
    let Some(fw) = firmware("the MIDI modes stream test") else {
        return;
    };
    let mut m = booted_with_user_sounds(&fw);
    multi_on(&mut m, &fw);
    press(&mut m, HOME);
    m.run_ms(700);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let fed = m.midi_paced(2000, |i| {
        let ch = (i % 4) as u8;
        let n = 48 + (i / 16 % 24) as u8;
        match i / 4 % 5 {
            0 => vec![0x90 | ch, n, 100],
            1 => vec![0xE0 | ch, (i % 128) as u8, (i / 128 % 128) as u8],
            2 => vec![0xB0 | ch, 74, (i % 128) as u8],
            3 => vec![0xC0 | ch, (16 + i / 20 % 16) as u8],
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

#[test]
fn bank_select_picks_profiles_and_songs_over_trs() {
    let Some(fw) = firmware("the MIDI banks test") else {
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
        sidou_sys::MIDI_ROUTE_NO_REQUEST as u8,
        "the song loader consumed the program change"
    );
    let toast = fw.symbol("ui") + ilp32::screen::toast as u32;
    let message: Vec<u8> = (0..sidou_sys::SCREEN_TOAST_CHARS)
        .map(|i| m.soc.read8(toast + i))
        .take_while(|&b| b != 0)
        .collect();
    assert_eq!(message, b"SAVE YOUR SONG FIRST");
    assert_eq!(preset(&mut m), 0);
    m.midi(&[0xB0, 32, 0, 0xC0, 2]);
    m.run_ms(50);
    assert_eq!(preset(&mut m), 2, "bank 0: the presets");
}
