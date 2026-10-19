// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: the live instrument in fm1-emu, played from the emulated front panel (keys,
//! encoders, the MASTER pot) and watched on the audio output, the LEDs and the LCD.

mod common;

use common::{build_dir, peak, pitch, power_above, record, Firmware, Machine};
use fm1_core::Bus;
use sidou_sys::{ilp32, SID_MODEL_6581, SID_MODEL_8580};

/// Felucca's button ids.
const OCT_UP: u8 = 1;
const FX: u8 = 2;
const K1: u8 = 2;
/// A MIDI note's key id at octave 0 (note key 0 = F3 = MIDI 53, id 14).
const fn key_id(note: u8) -> u8 {
    14 + note - 53
}
const A4: u8 = 69;
const E4: u8 = 64;
/// A block of 128 frames at 44118 Hz, in instructions at 240 MHz: 2.90 ms.
const BLOCK_BUDGET: u64 = 128 * 240_000_000 / 44_118;
/// docs/art/README.md, the HIGH CONTRAST theme (the default): black background, dark grey
/// border, as the panel received them (RGB565, high byte first).
const BLACK: u16 = 0x0000;
const DIM: u16 = 0x4228;
const LCD_W: usize = 240;

fn fixture(what: &str) -> Option<(std::path::PathBuf, Firmware)> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    let fw = Firmware::load(&build);
    Some((build, fw))
}

/// Past the boot animation (app/boot.h), which a control would skip instead of playing:
/// until the boot view is gone from the screen (common::run_past_boot).
fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, fw);
    m
}

/// A button pressed and let go (longer than the debounce both ways).
fn tap(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

/// The device's chips (platform/fm1/audio.c `chips`: A, B and their spare), read from
/// emulated RAM: chip A's sid, the spare's state. The struct has no pointers, so the host
/// build's layout (sidou-sys) is the device's.
#[derive(Debug, PartialEq)]
struct Chips {
    active: u8,
    spare_ready: u8,
    spare_model: u8,
    lost_resets: u32,
}

fn chips(m: &mut Machine, fw: &Firmware) -> Chips {
    let base = fw.chips();
    let at = |o: usize| base + o as u32;
    Chips {
        active: m.soc.read8(at(ilp32::reg_ring_chips::active)),
        spare_ready: m.soc.read8(at(ilp32::reg_ring_chips::spare_ready)),
        spare_model: m.soc.read8(at(ilp32::reg_ring_chips::spare_model)),
        lost_resets: m.soc.read32(at(ilp32::reg_ring_chips::lost_resets)),
    }
}

#[test]
fn a_key_sounds_at_its_pitch() {
    let Some((_, fw)) = fixture("the live test") else {
        return;
    };
    let mut m = booted(&fw);
    let quiet = record(&mut m, 100);
    for (note, hz) in [(A4, 440.0), (E4, 329.628)] {
        m.key(key_id(note), true);
        m.run_ms(100);
        let x = record(&mut m, 300);
        let f0 = pitch(&x);
        println!("key {note}: {f0:.2} Hz, peak {}", peak(&x));
        assert!(
            peak(&x) > 2000.0 * common::boot_gain(),
            "key {note} is silent"
        );
        assert!(
            (f0 / hz - 1.0).abs() < 0.005,
            "key {note} plays {f0:.2} Hz, expected {hz} Hz"
        );
        m.key(key_id(note), false);
        m.run_ms(600);
    }
    assert!(
        peak(&quiet[quiet.len() / 2..]) < 500.0,
        "the instrument plays before a key is down"
    );
}

#[test]
fn oct_up_doubles_the_pitch_and_lights_its_led() {
    let Some((_, fw)) = fixture("the live test") else {
        return;
    };
    let mut m = booted(&fw);
    m.key(key_id(A4), true);
    m.run_ms(100);
    let low = pitch(&record(&mut m, 300));
    m.key(key_id(A4), false);
    m.run_ms(300);

    tap(&mut m, OCT_UP);
    m.key(key_id(A4), true);
    m.run_ms(100);
    m.soc.periph.led_take();
    let high = pitch(&record(&mut m, 300));
    let leds = m.soc.periph.led_take();
    println!(
        "A4 {low:.2} Hz, after OCT+ {high:.2} Hz; LEDs: OCT+ {:.3}, A4 key {:.3}, FX {:.3}",
        leds[usize::from(OCT_UP)],
        leds[usize::from(key_id(A4))],
        leds[usize::from(FX)]
    );
    assert!((low / 440.0 - 1.0).abs() < 0.005, "A4 at {low:.2} Hz");
    assert!(
        (high / (2.0 * low) - 1.0).abs() < 0.005,
        "OCT+ plays {high:.2} Hz, expected twice {low:.2} Hz"
    );
    assert!(leds[usize::from(OCT_UP)] > 0.0, "the OCT+ LED is dark");
    assert!(
        leds[usize::from(key_id(A4))] > 0.0,
        "the held key's LED is dark"
    );
    assert_eq!(leds[usize::from(FX)], 0.0, "the FX LED is lit for the 6581");
}

#[test]
fn turning_k1_down_takes_the_highs_out() {
    let Some((_, fw)) = fixture("the live test") else {
        return;
    };
    let mut m = booted(&fw);
    m.key(key_id(A4), true);
    m.run_ms(200);
    let before = power_above(&record(&mut m, 200), 2000.0);
    // 40 detents of 16 steps: cutoff 1200 -> 560. The emulator turns a detent in 8 ms.
    m.turn(K1, -40);
    m.run_ms(500);
    let after = power_above(&record(&mut m, 200), 2000.0);
    println!("power above 2 kHz: {before:.4} before, {after:.4} after K1 -40");
    assert!(
        after < before / 4.0,
        "K1 down left the highs: {before:.4} -> {after:.4} of the power above 2 kHz"
    );
}

#[test]
fn the_lcd_shows_the_status_screen_and_redraws_only_on_change() {
    let Some((_, fw)) = fixture("the live test") else {
        return;
    };
    let mut m = booted(&fw);
    common::open_status(&mut m, &fw);
    let lcd = &m.soc.lcd;
    assert!(lcd.display_on, "DISPON was not sent");
    let fb = lcd.fb.clone();
    // (120, 20): text row 2 right of the title, blank; (4, 4): the border.
    let at = |x: usize, y: usize| fb[y * LCD_W + x];
    println!(
        "LCD: background {:#06x} at (120, 20), border {:#06x} at (4, 4), {} windows",
        at(120, 20),
        at(4, 4),
        lcd.frames
    );
    assert_eq!(at(120, 20), BLACK, "the background is not black");
    assert_eq!(at(4, 4), DIM, "the border is not the dark grey");
    let fg = fb.iter().filter(|&&p| p != BLACK).count();
    let bg = fb.iter().filter(|&&p| p == BLACK).count();
    assert!(
        fg > 240 * 4 * 8 && bg > fb.len() / 2,
        "not a status screen: {fg} lit, {bg} black pixels"
    );
    // Text inside: some lit pixels away from the border.
    let inner = (8..232)
        .flat_map(|y| (8..232).map(move |x| (x, y)))
        .filter(|&(x, y)| at(x, y) != BLACK)
        .count();
    assert!(inner > 500, "no text on the screen ({inner} pixels)");

    // Nothing changes: nothing is sent.
    let windows = m.soc.lcd.frames;
    m.run_ms(300);
    assert_eq!(
        m.soc.lcd.frames, windows,
        "the screen was redrawn without a change"
    );

    // A note changes the notes line: only its cells go out, as one or two runs (windows).
    m.key(key_id(A4), true);
    m.run_ms(200);
    let sent = m.soc.lcd.frames - windows;
    let changed = m.soc.lcd.fb.iter().zip(&fb).filter(|(a, b)| a != b).count();
    println!("after a key: {sent} windows sent, {changed} pixels changed");
    assert!((1..=2).contains(&sent), "{sent} windows sent for one note");
    assert!(changed > 0, "the held note is not shown");
}

#[test]
fn the_model_toggle_swaps_chips_without_a_gap() {
    let Some((_, fw)) = fixture("the live test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    // The spare chip's preparation, a slice a tick (app/reg_ring.h): every slice of both
    // toggles. (Boot's chips are made at once, by sid_init, before the main loop.)
    m.watch_calls(fw.symbol("sid_character_prep_step"));
    common::run_past_boot(&mut m, &fw);
    m.key(key_id(A4), true);
    m.run_ms(200);
    let blocks_before = m.alnk_blocks.len();
    let quiet_before = m.alnk_blocks.iter().copied().max().unwrap_or(0);

    // Chip A's sid, and its model, after boot and each toggle: a swap each time (chip B, which
    // follows the keys' setup in the background from M2b Task 4, takes the spare between them,
    // so which of the three sids chip A is on depends on when it did), nothing lost.
    let a_model = |m: &mut Machine| {
        let base = fw.chips();
        let active = m.soc.read8(base + ilp32::reg_ring_chips::active as u32);
        let s =
            base + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32;
        m.soc.read8(s + ilp32::sid::model as u32)
    };
    let boot_state = chips(&mut m, &fw);
    assert_eq!(a_model(&mut m), SID_MODEL_6581 as u8, "after boot");

    let from = m.soc.periph.audio.len() / 2;
    tap(&mut m, FX);
    m.run_ms(200);
    let x: Vec<f32> = m.left()[from..].iter().map(|&s| f32::from(s)).collect();
    m.soc.periph.led_take();
    m.run_ms(50);
    let fx_led = m.soc.periph.led_take()[usize::from(FX)];
    let first = chips(&mut m, &fw);
    assert_eq!(a_model(&mut m), SID_MODEL_8580 as u8, "after the first FX");
    assert_ne!(
        first.active, boot_state.active,
        "the first FX swapped no chip"
    );
    tap(&mut m, FX);
    m.run_ms(300); // chip A's preparation, then chip B's following it
    let second = chips(&mut m, &fw);
    assert_eq!(a_model(&mut m), SID_MODEL_6581 as u8, "after the second FX");
    assert_ne!(second.active, first.active, "the second FX swapped no chip");
    for c in [&boot_state, &first, &second] {
        assert_eq!((c.spare_ready, c.lost_resets), (0, 0), "{c:?}");
    }

    let toggle_blocks = &m.alnk_blocks[blocks_before..];
    let worst = toggle_blocks.iter().copied().max().unwrap_or(0);
    let calls = m.watched_calls.clone();
    let silent_blocks = x
        .chunks(128)
        .filter(|b| b.iter().all(|&s| s.abs() < 1.0))
        .count();
    let f0 = pitch(&x[x.len() / 2..]);
    let report = serde_json::json!({
        "preparation_slices_main_loop": calls,
        "block_budget_instructions": BLOCK_BUDGET,
        "max_block_instructions_before": quiet_before,
        "max_block_instructions_across_toggle": worst,
        "silent_blocks_across_toggle": silent_blocks,
        "f0_after_hz": f0,
        "fx_led": fx_led,
        "chips_after_boot_fx_fx": format!("{boot_state:?} {first:?} {second:?}"),
    });
    println!("{report}");

    // Chip B's boot preparation (the 6581 R4AR, 7 slices), then each toggle's on chip A and
    // chip B following it, 7 slices each (the 8580 R5 and the 6581 R4AR with the model's own
    // DACs), the first 4 more for the 8580's default DACs, built on first use: all in the main
    // loop (the watch counts main-loop calls only), and the blocks stay small, so none ran in
    // the interrupt.
    assert_eq!(
        calls.len(),
        7 + 11 + 7 + 7 + 7,
        "preparation slices from the main loop: {calls:?}"
    );
    assert!(
        worst < BLOCK_BUDGET / 4,
        "a block took {worst} instructions across the toggle (budget {BLOCK_BUDGET})"
    );
    assert_eq!(silent_blocks, 0, "the toggle left a silent block");
    assert!(
        (f0 / 440.0 - 1.0).abs() < 0.005,
        "after the toggle: {f0:.2} Hz"
    );
    assert!(fx_led > 0.0, "the FX LED is dark with the 8580");
}

#[test]
fn master_turns_the_volume_down() {
    let Some((_, fw)) = fixture("the live test") else {
        return;
    };
    let mut m = booted(&fw);
    m.key(key_id(A4), true);
    m.run_ms(200);
    let loud = peak(&record(&mut m, 200));
    m.pot(300);
    m.run_ms(200);
    let soft = peak(&record(&mut m, 200));
    println!("peak {loud} at pot 900, {soft} at pot 300");
    assert!(
        soft < loud * 0.6,
        "MASTER 900 -> 300 left the peak at {soft} (was {loud})"
    );
}

/// Main-loop instructions per second of emulated time over `ms`.
fn main_rate(m: &mut Machine, ms: u64) -> f64 {
    let before = m.main_steps;
    m.run_ms(ms);
    (m.main_steps - before) as f64 * 1000.0 / ms as f64
}

#[test]
fn the_main_loop_is_cheap_when_idle() {
    let Some((_, fw)) = fixture("the live test") else {
        return;
    };
    let mut m = booted(&fw);
    // Past the first USB retry (once a second while no host answers, platform/fm1/midi.c usb_retry_quiet),
    // and whole seconds from then on, so each window holds exactly one retry.
    m.run_ms(1000);
    let idle = main_rate(&mut m, 1000);
    m.key(key_id(A4), true);
    let held = main_rate(&mut m, 1000);
    println!(
        "main loop: {:.2}M instructions/s idle, {:.2}M with a held key ({:.1} % and {:.1} % of 240 MHz)",
        idle / 1e6,
        held / 1e6,
        idle / 2.4e6,
        held / 2.4e6
    );
    // 52.5M/s (22 % of the core) before the main loop was paced by the key scan's frames
    // and the screen by a dirty-strip mask; 1.4M/s after; 2.0M/s with MIDI in, 0.4M/s of it
    // the USB retry's SIE timeouts while no host answers. 2.2M/s before the cell-grid renderer,
    // 2.59M/s with it (it composes the whole grid every 40 ms; tests/screen.rs).
    for (what, rate) in [("idle", idle), ("with a held key", held)] {
        assert!(
            rate < 5e6,
            "the main loop runs {:.2}M instructions/s {what}",
            rate / 1e6
        );
    }
}
