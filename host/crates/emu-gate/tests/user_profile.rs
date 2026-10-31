// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The USER chip profiles on the device (app/user_profile.h, M2a Task 6), in fm1-emu with its
//! flash kept across a power cycle: a sound saved from the CHIP view into USER 3 (EDIT held +
//! ALGORITHM, then SAVE) is one versioned record under its own key and leaves the settings alone;
//! after a power cycle CC 85 picks it, and the chip plays what was saved: the rendered chip's
//! character and the audio match the host build sample for sample (as character.rs does for the
//! character's CCs), the profile change prepared and swapped as any other. The panel's picker
//! gets there too (ALGORITHM steps onto the slot, skipping the empty ones).

mod common;

use fm1_core::Bus;

use common::{
    build_dir, max_diff, run_past_boot, stored_value, top_view, Firmware, Machine, HALF_FRAMES,
};
use sidou_sys::*;

const SELECT: u8 = 0;
const ALGORITHM: u8 = 1;
const HOME: u8 = 8;
const SAVE: u8 = 9;
const EDIT: u8 = PANEL_BTN_EDIT as u8;
const A4_KEY: u8 = 30;
const A4: u8 = 69;
const MAX_SILENT_HALVES: usize = 4;
/// A CC takes effect within this many blocks of its arrival (character.rs).
const LATE_BLOCKS: usize = 8;
/// USER 3's key (app/user_profile.h) and what the player saves into it: worn 6581, and the
/// character's four controls.
const KEY_USER_3: u32 = 0x103;
const SAVED_CONTROLS: [u8; 4] = [100, 127, 64, 90];
const SAVED_LOW_PASS: u8 = 40;

fn saved_expert() -> [u8; SID_EXPERT_COUNT as usize] {
    let mut expert = [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize];
    expert[SID_EXPERT_EXTERNAL_LP as usize] = SAVED_LOW_PASS;
    expert
}
/// CC 85 over the five profiles and eight USER slots (0..12 in steps of 128/13): 43 is 4 (worn),
/// 70 is 7 (USER 3).
const CC_WORN: u8 = 43;
const CC_USER_3: u8 = 70;
const CC_R4AR: u8 = 20;
/// The settings record's offsets of the profile and the controls (app/settings.h: the header's
/// 6 bytes, the parameters in order, the cutoff and the pulse width two bytes each).
const REC_PROFILE: usize = 6 + LIVE_PARAM_PROFILE as usize + 2;

fn fixture() -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message("the user profile test");
        return None;
    };
    Some(Firmware::load(&build))
}

fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

fn instrument(m: &mut Machine, fw: &Firmware, field: usize) -> u32 {
    let _ = m;
    fw.symbol("instrument") + field as u32
}

fn live_u8(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    let a = instrument(m, fw, field);
    m.soc.read8(a)
}

/// The menu, then CHIP (behind THE CHIP's card, ended), settled.
fn open_chip(m: &mut Machine, fw: &Firmware) {
    press(m, HOME);
    m.run_ms(700);
    m.turn(SELECT, -(VIEWS_MENU_ITEMS as i32));
    m.run_ms(10 * u64::from(VIEWS_MENU_ITEMS));
    m.turn(SELECT, VIEWS_MENU_CHIP as i32);
    m.run_ms(100);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    let from = m.ms();
    while top_view(m, fw) != fw.symbol("chip_view_view") || common::screen_animating(m, fw) {
        assert!(m.ms() < from + 6000, "CHIP did not open");
        m.run_ms(40);
    }
    m.run_ms(200);
}

fn record(base: u32, ctl: [u8; 4]) -> Vec<u8> {
    let mut rec = vec![
        2,
        base as u8,
        4,
        ctl[0],
        ctl[1],
        ctl[2],
        ctl[3],
        SID_EXPERT_COUNT as u8,
    ];
    rec.extend(saved_expert());
    rec
}

/// Saves the sound the CCs set into USER 3 from the CHIP view, restores the settings' defaults
/// (so a power cycle boots the default instrument), and returns the flash.
fn flash_with_user_3(fw: &Firmware) -> Vec<u8> {
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, fw);
    // The worn 6581 with the four controls, over MIDI, settled (the DACs' ramp takes 1.72 s).
    let mut burst = vec![0xB0, 85, CC_WORN];
    for (cc, v) in [(86, 100), (87, 127), (89, 64), (90, 90)] {
        burst.extend([cc, v]);
    }
    m.midi(&burst);
    m.midi(&[
        0xB0,
        99,
        2,
        98,
        (5 + SID_EXPERT_EXTERNAL_LP) as u8,
        6,
        0,
        38,
        SAVED_LOW_PASS,
    ]);
    m.run_ms(2500);
    open_chip(&mut m, fw);
    // EDIT held + ALGORITHM: the save target USER 3; the chip does not change.
    m.key(EDIT, true);
    m.run_ms(30);
    m.turn(ALGORITHM, 3);
    m.run_ms(100);
    m.key(EDIT, false);
    m.run_ms(30);
    assert_eq!(
        live_u8(&mut m, fw, ilp32::live::profile),
        SID_CHIP_6581_WORN as u8,
        "the target moved the chip"
    );
    press(&mut m, SAVE);
    let want = record(SID_CHIP_6581_WORN, SAVED_CONTROLS);
    let start = m.ms();
    loop {
        if stored_value(&m.soc.flash.mem, KEY_USER_3).as_deref() == Some(&want[..]) {
            break;
        }
        assert!(
            m.ms() < start + 20_000,
            "the profile never reached the flash: {:?}",
            stored_value(&m.soc.flash.mem, KEY_USER_3)
        );
        m.run_ms(100);
    }
    for n in [0u32, 1, 3, 4, 5, 6, 7] {
        assert_eq!(
            stored_value(&m.soc.flash.mem, 0x101 + n),
            None,
            "USER {} written",
            n + 1
        );
    }
    // The instrument back to its defaults, and the debounce lets the settings follow.
    let mut back = vec![0xB0, 85, CC_R4AR];
    for cc in [86, 87, 89, 90] {
        back.extend([cc, 0]);
    }
    m.midi(&back);
    let start = m.ms();
    loop {
        m.run_ms(200);
        let rec = stored_value(&m.soc.flash.mem, SETTINGS_KEY).expect("settings");
        if rec[REC_PROFILE] == SID_CHIP_6581_R4AR as u8 && rec[REC_PROFILE + 1..][..4] == [0; 4] {
            break;
        }
        assert!(m.ms() < start + 20_000, "the settings never followed");
    }
    assert_eq!(
        stored_value(&m.soc.flash.mem, KEY_USER_3),
        Some(want),
        "the settings' write left the slot"
    );
    m.soc.flash.mem.clone()
}

/// The firmware's instrument at boot rendered by the host build with USER 3 holding the saved
/// profile, CC 85 (USER 3) sent to live before block `cc_block`, ticks[i] live_ticks before
/// block i (the device's), and the note on before `note_block`.
fn render_host(frames: usize, cc_block: usize, note_block: usize, ticks: &[u32]) -> Vec<i16> {
    common::render_instrument(frames, |i, l| unsafe {
        if i == 0 {
            l.user_valid = 1 << 2;
            l.user_base[2] = SID_CHIP_6581_WORN as u8;
            l.user_ctl[2] = SAVED_CONTROLS;
            l.user_expert[2] = saved_expert();
        }
        if i == cc_block {
            live_cc(l, 85, CC_USER_3);
        }
        for _ in 0..ticks.get(i).copied().unwrap_or(0) {
            live_tick(l);
        }
        if i == note_block {
            live_note_on(l, A4, 100);
        }
    })
}

fn active_sid(m: &mut Machine, fw: &Firmware) -> u32 {
    let base = fw.chips();
    let active = m.soc.read8(base + ilp32::reg_ring_chips::active as u32);
    base + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32
}

#[test]
fn bank_2_program_change_applies_a_saved_user_profile_like_cc_85() {
    let Some(fw) = fixture() else { return };
    let flash = flash_with_user_3(&fw);
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(&flash);
    run_past_boot(&mut m, &fw);
    m.midi(&[0xB0, 0, 0, 32, 2, 0xC0, 7]);
    m.run_ms(2500);
    assert_eq!(
        live_u8(&mut m, &fw, ilp32::live::profile),
        SID_CHIP_6581_WORN as u8
    );
    assert_eq!(live_u8(&mut m, &fw, ilp32::live::user_applied), 3);
    for (i, control) in SAVED_CONTROLS.iter().enumerate() {
        assert_eq!(live_u8(&mut m, &fw, ilp32::live::character + i), *control);
    }
    m.midi(&[0xC0, 5, 12, 13]);
    m.run_ms(100);
    assert_eq!(
        live_u8(&mut m, &fw, ilp32::live::user_applied),
        3,
        "empty or invalid pick"
    );
    m.midi(&[0xB0, 85, CC_R4AR]);
    m.run_ms(400);
    assert_eq!(live_u8(&mut m, &fw, ilp32::live::user_applied), 0);
    m.midi(&[0xB0, 85, CC_USER_3]);
    m.run_ms(2500);
    assert_eq!(
        live_u8(&mut m, &fw, ilp32::live::profile),
        SID_CHIP_6581_WORN as u8
    );
    assert_eq!(live_u8(&mut m, &fw, ilp32::live::user_applied), 3);
}

#[test]
fn a_user_profile_saved_from_the_chip_view_survives_a_power_cycle_and_sounds_as_saved() {
    let Some(fw) = fixture() else {
        return;
    };
    let flash = flash_with_user_3(&fw);

    // Power on with that flash: the slot is loaded, the instrument is the default one.
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(&flash);
    let base = fw.chips();
    m.snapshot_bytes = vec![base + ilp32::reg_ring_chips::ticks as u32];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 1)));
    run_past_boot(&mut m, &fw);
    assert_eq!(live_u8(&mut m, &fw, ilp32::live::user_valid), 0b100);
    assert_eq!(
        live_u8(&mut m, &fw, ilp32::live::user_base + 2),
        SID_CHIP_6581_WORN as u8
    );
    for (i, c) in SAVED_CONTROLS.iter().enumerate() {
        assert_eq!(live_u8(&mut m, &fw, ilp32::live::user_ctl + 4 * 2 + i), *c);
    }
    for (i, value) in saved_expert().iter().enumerate() {
        assert_eq!(
            live_u8(
                &mut m,
                &fw,
                ilp32::live::user_expert + SID_EXPERT_COUNT as usize * 2 + i
            ),
            *value
        );
    }
    assert_eq!(
        live_u8(&mut m, &fw, ilp32::live::profile),
        SID_CHIP_6581_R4AR as u8
    );
    let halves = |m: &Machine| m.soc.periph.audio.len() / (2 * HALF_FRAMES);
    let cc_sent = halves(&m);
    m.midi(&[0xB0, 85, CC_USER_3]);
    m.run_ms(100);
    let key_sent = halves(&m);
    m.key(A4_KEY, true);
    m.run_ms(300);

    let mut ticks = Vec::new();
    let mut count = 0u8;
    for snap in &m.buffer_snapshots {
        ticks.push(u32::from(snap.bytes[0].wrapping_sub(count)));
        count = snap.bytes[0];
    }
    assert!(ticks.iter().all(|&n| n <= 2), "ticks: {ticks:?}");
    // The slot's profile is the chip's, with its controls.
    assert_eq!(
        live_u8(&mut m, &fw, ilp32::live::user_applied),
        3,
        "USER 3 applied"
    );
    assert_eq!(
        live_u8(&mut m, &fw, ilp32::live::profile),
        SID_CHIP_6581_WORN as u8
    );
    let s = active_sid(&mut m, &fw);
    assert_eq!(
        m.soc.read8(s + ilp32::sid::filter_curve as u32),
        3,
        "worn's filter row"
    );
    let now = base + (ilp32::reg_ring_chips::ramp + ilp32::sid_character_ramp::now) as u32;
    let settings: Vec<u8> = (0..5).map(|i| m.soc.read8(now + i)).collect();
    assert_eq!(
        settings,
        [
            SID_CHIP_6581_WORN as u8 + 1,
            SAVED_CONTROLS[0],
            SAVED_CONTROLS[1],
            SAVED_CONTROLS[2],
            SAVED_CONTROLS[3]
        ],
        "the ramp got to the saved setup"
    );
    assert_eq!(
        m.soc
            .read8(now + ilp32::sid_character::expert as u32 + SID_EXPERT_EXTERNAL_LP),
        SAVED_LOW_PASS,
        "the saved expert control reached the rendered chip"
    );

    // The audio: the host with the same slot and CC, its ticks where the device's were.
    let audio = m.soc.periph.audio.clone();
    let mut found = None;
    let mut min_diff = i32::MAX;
    'search: for silent in 0..=MAX_SILENT_HALVES {
        if audio[..2 * silent * HALF_FRAMES].iter().any(|&v| v != 0) {
            break;
        }
        let played = &audio[2 * silent * HALF_FRAMES..];
        let before_note = (key_sent - silent) * HALF_FRAMES;
        for first in (cc_sent - silent..).take(LATE_BLOCKS) {
            let host = render_host(before_note, first, usize::MAX, &ticks);
            if max_diff(&played[..2 * before_note], &host) != 0 {
                continue;
            }
            for note_block in (key_sent - silent..).take(LATE_BLOCKS) {
                let host = render_host(played.len() / 2, first, note_block, &ticks);
                let d = max_diff(played, &host);
                min_diff = min_diff.min(d);
                if d == 0 {
                    found = Some((silent, first, note_block));
                    break 'search;
                }
            }
        }
    }
    println!(
        "USER 3 on the device vs host: (silent halves, CC block, note block) {found:?} (sent \
         before blocks {cc_sent}, {key_sent}); closest max difference {min_diff} LSB over {} frames",
        audio.len() / 2
    );
    assert!(
        found.is_some(),
        "the firmware's audio with the saved profile matches no host render: closest max \
         difference {min_diff} LSB"
    );
}

#[test]
fn the_picker_steps_onto_the_saved_slot_after_a_power_cycle() {
    let Some(fw) = fixture() else {
        return;
    };
    let flash = flash_with_user_3(&fw);
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(&flash);
    run_past_boot(&mut m, &fw);
    open_chip(&mut m, &fw);
    // R4AR, 8580 R5, worn, then USER 1 and 2 are empty: skipped, USER 3.
    for _ in 0..3 {
        m.turn(ALGORITHM, 1);
        m.run_ms(60);
    }
    m.run_ms(2500);
    assert_eq!(live_u8(&mut m, &fw, ilp32::live::user_applied), 3);
    assert_eq!(
        live_u8(&mut m, &fw, ilp32::live::profile),
        SID_CHIP_6581_WORN as u8
    );
    for (i, c) in SAVED_CONTROLS.iter().enumerate() {
        assert_eq!(live_u8(&mut m, &fw, ilp32::live::character + i), *c);
    }
}
