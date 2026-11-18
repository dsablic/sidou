// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use std::path::PathBuf;

use common::panel::{press, FX, KEY0, PLAY, SEQ};
use common::ring::{chip_b_use, push, settle, worst_on_both_chips};
use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::player::Player;

const CPU2_BUILD: &str = "SIDOU_FM1_CPU2_BUILD";
const CORE1_LOOP: &str = "fm1_cpu1_main";
const OPEN_MS: u64 = 700;
const PLAYING_MS: u64 = 1500;
const DIVED_MS: u64 = 700;
const SWAPPED_MS: u64 = 800;
const STUTTER_MS: u64 = 2500;
const HANDED_BACK_MS: u64 = 600;
const MEASURE_MS: u64 = 1500;
const BLOCK_FRAMES: f64 = 128.0;
const CHANNEL_BYTES: usize = 3 * ilp32::gt2_channel::SIZE;
const HOST_CHANNELS: usize = std::mem::offset_of!(gt2_player, chn);

fn pad(p: u32) -> u32 {
    1 << p
}

fn transport(fw: &Firmware) -> u32 {
    fw.symbol("song_transport")
}

fn song_u32(m: &mut Machine, fw: &Firmware, field: usize) -> u32 {
    m.soc.read32(transport(fw) + field as u32)
}

fn hold(m: &mut Machine, pads: u32) {
    for k in 0..SONG_T_PUNCH_PADS {
        m.key(KEY0 + k as u8, pads >> k & 1 != 0);
    }
    m.run_ms(2 * UI_FRAME_MS);
}

fn punching(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    if fw.has_symbol(CORE1_LOOP) {
        m.core1.mailbox_loop = fw.function_range(CORE1_LOOP);
    }
    run_past_boot(&mut m, fw);
    press(&mut m, PLAY);
    m.run_ms(PLAYING_MS);
    press(&mut m, SEQ);
    m.run_ms(OPEN_MS);
    press(&mut m, FX);
    m.run_ms(OPEN_MS);
    assert_eq!(
        top_view(&mut m, fw),
        fw.symbol("punch_view_view"),
        "FX on STEP opens PUNCH"
    );
    m
}

fn steady<T: PartialEq>(m: &mut Machine, mut read: impl FnMut(&mut Machine) -> T) -> T {
    loop {
        let first = read(m);
        m.run_ms(1);
        if read(m) == first {
            return first;
        }
    }
}

fn chip_a_byte(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    let chip = rendered_sid(m, fw, 0);
    m.soc.read8(chip + field as u32)
}

fn chip_a_filter(m: &mut Machine, fw: &Firmware) -> (u32, u8, u8, u8) {
    let chip = rendered_sid(m, fw, 0);
    (
        m.soc.read32(chip + ilp32::sid::fc as u32),
        m.soc.read8(chip + ilp32::sid::res as u32),
        m.soc.read8(chip + ilp32::sid::routing as u32),
        m.soc.read8(chip + ilp32::sid::mode_vol as u32),
    )
}

fn shadow(m: &mut Machine, fw: &Firmware) -> [u8; GT2_SID_REGS as usize] {
    let at = transport(fw) + ilp32::song_ctl::shadow as u32;
    std::array::from_fn(|r| m.soc.read8(at + r as u32))
}

fn song_filter(r: &[u8; GT2_SID_REGS as usize]) -> (u32, u8, u8, u8) {
    (
        u32::from(r[0x16]) << 3 | u32::from(r[0x15] & 7),
        r[0x17] >> 4,
        r[0x17] & 0x0f,
        r[0x18],
    )
}

fn pitches(m: &mut Machine, fw: &Firmware) -> ([u32; 3], [u32; 3]) {
    let chip = rendered_sid(m, fw, 0);
    let r = shadow(m, fw);
    let chip_freq: [u32; 3] = std::array::from_fn(|v| {
        let o = chip + (ilp32::sid::osc + v * ilp32::sid_osc::SIZE + ilp32::sid_osc::freq) as u32;
        m.soc.read32(o)
    });
    let song_freq = std::array::from_fn(|v| u32::from(r[7 * v]) | u32::from(r[7 * v + 1]) << 8);
    (chip_freq, song_freq)
}

fn fx_slot(m: &mut Machine, fw: &Firmware) -> (u32, f32) {
    let p = fw.symbol("fx") + (ilp32::fx_live::p + SID_SLOT_FX as usize * 20) as u32;
    let send = fw.symbol("slots") + ilp32::sid_slots::send as u32;
    (m.soc.read32(p), f32::from_bits(m.soc.read32(send)))
}

#[test]
fn each_pad_reaches_chip_a_or_the_fx_slot_while_held_and_lets_go_at_release() {
    let Some(fw) = firmware("the PUNCH pads test") else {
        return;
    };
    let mut m = punching(&fw);
    let mute = ilp32::sid::mute;

    hold(&mut m, pad(0));
    assert_eq!(
        chip_a_byte(&mut m, &fw, mute),
        7,
        "MUTE: chip A's voices muted"
    );
    hold(&mut m, 0);
    assert_eq!(chip_a_byte(&mut m, &fw, mute), 0, "MUTE let go");

    hold(&mut m, pad(1));
    let slammed = steady(&mut m, |m| chip_a_filter(m, &fw));
    assert_eq!((slammed.0, slammed.1, slammed.2), (0, 15, 7), "FILTER SLAM");
    assert_eq!(slammed.3 & 0x70, 0x10, "FILTER SLAM: low pass");
    hold(&mut m, 0);
    let (chip, song) = steady(&mut m, |m| {
        (chip_a_filter(m, &fw), song_filter(&shadow(m, &fw)))
    });
    assert_eq!(chip, song, "FILTER SLAM let go: the song's own filter");

    hold(&mut m, pad(2));
    m.run_ms(DIVED_MS);
    let (chip, song) = steady(&mut m, |m| pitches(m, &fw));
    assert_eq!(chip, song.map(|f| f / 2), "PITCH DIVE: an octave down");
    hold(&mut m, 0);
    let (chip, song) = steady(&mut m, |m| pitches(m, &fw));
    assert_eq!(chip, song, "PITCH DIVE let go");

    let own = fx_slot(&mut m, &fw);
    hold(&mut m, pad(4));
    assert_eq!(fx_slot(&mut m, &fw), (SID_FXSLOT_DELAY, 1.0), "ECHO THROW");
    hold(&mut m, 0);
    assert_eq!(
        fx_slot(&mut m, &fw),
        (SID_FXSLOT_DELAY, 0.0),
        "the tail rings"
    );
    m.run_ms(u64::from(PUNCH_ECHO_TAIL_MS) + 100);
    assert_eq!(fx_slot(&mut m, &fw), own, "the FX slot back after the tail");

    let model = chip_a_byte(&mut m, &fw, ilp32::sid::model);
    hold(&mut m, pad(5));
    m.run_ms(SWAPPED_MS);
    assert_ne!(
        chip_a_byte(&mut m, &fw, ilp32::sid::model),
        model,
        "CHIP SWAP: the other model"
    );
    hold(&mut m, 0);
    m.run_ms(SWAPPED_MS);
    assert_eq!(
        chip_a_byte(&mut m, &fw, ilp32::sid::model),
        model,
        "CHIP SWAP let go"
    );
    assert_eq!(
        top_view(&mut m, &fw),
        fw.symbol("punch_view_view"),
        "still on PUNCH"
    );
}

fn channels(m: &mut Machine, fw: &Firmware) -> (u32, Vec<u8>) {
    let at = transport(fw) + (ilp32::song_ctl::player + ilp32::gt2_player::chn) as u32;
    steady(m, |m| {
        (
            song_u32(m, fw, ilp32::song_ctl::calls),
            (0..CHANNEL_BYTES as u32)
                .map(|i| m.soc.read8(at + i))
                .collect::<Vec<u8>>(),
        )
    })
}

fn reference_channels(song: &sidou_tests::song::Song, calls: u32) -> Vec<u8> {
    let mut o = gt2_options::default();
    unsafe { song_ext_get_options(song.ptr(), &mut o) };
    let mut p = Player::new(song, 0, &o).expect("the device's song plays on the host");
    for _ in 0..calls {
        p.call_silent();
    }
    let raw = p.raw() as *const gt2_player as *const u8;
    unsafe { std::slice::from_raw_parts(raw.add(HOST_CHANNELS), CHANNEL_BYTES) }.to_vec()
}

#[test]
fn stutter_loops_a_beat_and_the_song_comes_back_where_it_would_have_been() {
    let Some(fw) = firmware("the STUTTER timing test") else {
        return;
    };
    let mut m = punching(&fw);
    let song = song_in_device(&mut m, &fw);
    let (calls, before) = channels(&mut m, &fw);
    assert_eq!(
        before,
        reference_channels(&song, calls),
        "the song plays as on the host"
    );
    hold(&mut m, pad(3));
    let mut looped = 0;
    for _ in 0..STUTTER_MS / 100 {
        m.run_ms(100);
        let (calls, now) = channels(&mut m, &fw);
        looped += usize::from(now != reference_channels(&song, calls));
    }
    assert!(looped > 0, "STUTTER held, the song never left its course");
    hold(&mut m, 0);
    m.run_ms(HANDED_BACK_MS);
    let (calls, after) = channels(&mut m, &fw);
    assert_eq!(
        after,
        reference_channels(&song, calls),
        "call {calls}: the song is not where it would have been"
    );
    println!("STUTTER held {STUTTER_MS} ms; the song back on its course by call {calls}");
}

fn worst_block_with_chip_b_at_its_worst(fw: &Firmware, pads: u32) -> f64 {
    let mut m = punching(fw);
    settle(&mut m, fw);
    push(&mut m, fw, &worst_on_both_chips());
    chip_b_use(&mut m, fw, true);
    hold(&mut m, pads);
    m.run_ms(300);
    let from = m.alnk_blocks.len();
    m.run_ms(MEASURE_MS);
    m.alnk_blocks[from..].iter().copied().max().unwrap_or(0) as f64 / BLOCK_FRAMES
}

fn assert_every_pad_fits(fw: &Firmware, what: &str) {
    let plain = worst_block_with_chip_b_at_its_worst(fw, 0);
    let each: Vec<f64> = (0..SONG_T_PUNCH_PADS)
        .map(|p| worst_block_with_chip_b_at_its_worst(fw, pad(p)) - plain)
        .collect();
    let every = worst_block_with_chip_b_at_its_worst(fw, PUNCH_VIEW_ALL);
    println!(
        "{}",
        serde_json::json!({
            "build": what,
            "worst_block_song_and_chip_b": plain,
            "each_pad_adds": each,
            "worst_block_every_pad_held": every,
            "audio_target": AUDIO_TARGET,
        })
    );
    assert!(
        every <= AUDIO_TARGET,
        "{what}: every pad held, the worst block costs {every:.1} a sample"
    );
    let player_pads = each[..4].iter().copied().fold(0.0, f64::max);
    assert!(
        player_pads <= PLAYER_WORST_BLOCK,
        "{what}: a player pad adds {player_pads:.1} a sample to the worst block"
    );
}

#[test]
fn every_pad_held_fits_the_audio_interrupt_on_one_core() {
    let Some(fw) = firmware("the PUNCH interrupt budget test") else {
        return;
    };
    assert_every_pad_fits(&fw, "one core");
}

#[test]
fn every_pad_held_fits_the_audio_interrupt_with_chip_b_on_core_1() {
    let Some(two) = std::env::var_os(CPU2_BUILD).filter(|d| !d.is_empty()) else {
        println!("skipping the two-core PUNCH budget: set {CPU2_BUILD} to a -DSIDOU_CPU2=ON build");
        return;
    };
    let fw = Firmware::load(&std::path::absolute(PathBuf::from(two)).unwrap());
    assert_every_pad_fits(&fw, "chip B on core 1");
}
