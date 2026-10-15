// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: the live instrument in fm1-emu, played from the emulated front panel (keys,
//! encoders, the MASTER pot) and watched on the audio output, the LEDs and the LCD.

mod common;

use common::{build_dir, peak, pitch, power_above, record, Firmware, Machine};
use fm1_core::Bus;
use sidou_sys::{ilp32, SID_MODEL_6581, SID_MODEL_8580};

/// Past the LCD's reset delays, the first audio blocks and the short boot animation
/// (app/boot.h), which a control would skip instead of playing; a whole number of frames
/// after the 500 ms of before it, so that the screen's frames fall as they did.
const BOOT_MS: u64 = 2020;
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
/// docs/art/README.md: the palette's blue (background) and light blue (text and border), as
/// the panel received them (RGB565, high byte first).
const BLUE: u16 = 0x314F;
const LIGHT_BLUE: u16 = 0x6AF6;
const LCD_W: usize = 240;

fn fixture(what: &str) -> Option<(std::path::PathBuf, Firmware)> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    let fw = Firmware::load(&build);
    Some((build, fw))
}

fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    m.run_ms(BOOT_MS);
    m
}

/// A button pressed and let go (longer than the debounce both ways).
fn tap(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

/// The device's two chips (platform/fm1/audio.c `chips`), read from emulated RAM. The
/// struct has no pointers, so the host build's layout (sidou-sys) is the device's.
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
        assert!(peak(&x) > 2000.0, "key {note} is silent");
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
    assert_eq!(at(120, 20), BLUE, "the background is not the C64 blue");
    assert_eq!(at(4, 4), LIGHT_BLUE, "the border is not the C64 light blue");
    let fg = fb.iter().filter(|&&p| p == LIGHT_BLUE).count();
    let bg = fb.iter().filter(|&&p| p == BLUE).count();
    assert_eq!(
        fg + bg,
        fb.len(),
        "colours other than the two of the palette"
    );
    assert!(
        fg > 240 * 4 * 8 && bg > fb.len() / 2,
        "not a status screen: {fg} light blue, {bg} blue pixels"
    );
    // Text inside: some light blue pixels away from the border.
    let inner = (8..232)
        .flat_map(|y| (8..232).map(move |x| (x, y)))
        .filter(|&(x, y)| at(x, y) == LIGHT_BLUE)
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
    m.watch_calls(fw.symbol("sid_init"));
    m.run_ms(BOOT_MS);
    m.key(key_id(A4), true);
    m.run_ms(200);
    let blocks_before = m.alnk_blocks.len();
    let quiet_before = m.alnk_blocks.iter().copied().max().unwrap_or(0);

    // live_init's reset at boot took chip 1, initialised for the 6581.
    let boot_state = chips(&mut m, &fw);
    let expect = |active, model| Chips {
        active,
        spare_ready: 0,
        spare_model: model as u8,
        lost_resets: 0,
    };
    assert_eq!(boot_state, expect(1, SID_MODEL_6581), "after boot");

    let from = m.soc.periph.audio.len() / 2;
    tap(&mut m, FX);
    m.run_ms(200);
    let x: Vec<f32> = m.left()[from..].iter().map(|&s| f32::from(s)).collect();
    m.soc.periph.led_take();
    m.run_ms(50);
    let fx_led = m.soc.periph.led_take()[usize::from(FX)];
    let first = chips(&mut m, &fw);
    assert_eq!(first, expect(0, SID_MODEL_8580), "after the first FX");
    tap(&mut m, FX);
    m.run_ms(100);
    let second = chips(&mut m, &fw);
    assert_eq!(second, expect(1, SID_MODEL_6581), "after the second FX");

    let toggle_blocks = &m.alnk_blocks[blocks_before..];
    let worst = toggle_blocks.iter().copied().max().unwrap_or(0);
    let calls = m.watched_calls.clone();
    let silent_blocks = x
        .chunks(128)
        .filter(|b| b.iter().all(|&s| s.abs() < 1.0))
        .count();
    let f0 = pitch(&x[x.len() / 2..]);
    let report = serde_json::json!({
        "sid_init_calls_main_loop": calls,
        "block_budget_instructions": BLOCK_BUDGET,
        "max_block_instructions_before": quiet_before,
        "max_block_instructions_across_toggle": worst,
        "silent_blocks_across_toggle": silent_blocks,
        "f0_after_hz": f0,
        "fx_led": fx_led,
        "chips_after_boot_fx_fx": format!("{boot_state:?} {first:?} {second:?}"),
    });
    println!("{report}");

    // At boot reg_ring_chips_init and live_init's reset, then the two toggles: all four in
    // the main loop (the watch counts main-loop calls only), and the blocks stay small, so
    // none ran in the interrupt.
    assert_eq!(
        calls.len(),
        4,
        "sid_init calls from the main loop: {calls:?}"
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
    // Past the first USB retry (once a second while no host answers, platform/fm1/usb/usb.c),
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
