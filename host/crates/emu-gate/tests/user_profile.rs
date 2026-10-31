// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use fm1_core::Bus;

use common::panel::{holding, press, turn, A4, A4_KEY, ALGORITHM, DEBOUNCED_MS, EDIT, HOME, SAVE};
use common::panel::{select_menu_item, wait_for_within};
use common::{
    max_diff, rendered_sid, run_past_boot, stored_value, top_view, Firmware, Machine, HALF_FRAMES,
};
use sidou_sys::*;

const MAX_SILENT_HALVES: usize = 4;
const CC_EFFECT_WITHIN_BLOCKS: usize = 8;
const KEY_USER_1: u32 = SETTINGS_KEY + 1;
const USER_SLOTS: u32 = 8;
const USER_3: u32 = 2;
const KEY_USER_3: u32 = KEY_USER_1 + USER_3;
const CHARACTER_CCS: [u8; 4] = [86, 87, 89, 90];
const SAVED_CONTROLS: [u8; 4] = [100, 127, 64, 90];
const SAVED_LOW_PASS: u8 = 40;
const CC_PROFILE: u8 = 85;
const CC_WORN: u8 = 43;
const CC_USER_3: u8 = 70;
const CC_R4AR: u8 = 20;
const SETTINGS_HEADER_BYTES: usize = 6;
const REC_PROFILE: usize = SETTINGS_HEADER_BYTES + LIVE_PARAM_PROFILE as usize + 2;
const CHARACTER_RAMP_MS: u64 = 2_500;
const MENU_WIPE_MS: u64 = 700;
const MENU_SETTLE_MS: u64 = 100;
const CHIP_OPEN_WITHIN_MS: u64 = 6_000;
const CHIP_SETTLE_MS: u64 = 200;
const FLASH_WITHIN_MS: u64 = 20_000;
const FLASH_POLL_MS: u64 = 100;
const PICKER_STEPS_TO_USER_3: usize = 3;
const PICKER_STEP_MS: u64 = 60;
const SAVE_TARGET_USER_3_CLICKS: i32 = 3;

fn saved_expert() -> [u8; SID_EXPERT_COUNT as usize] {
    let mut expert = [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize];
    expert[SID_EXPERT_EXTERNAL_LP as usize] = SAVED_LOW_PASS;
    expert
}

fn live_u8(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    m.soc.read8(fw.symbol("instrument") + field as u32)
}

fn open_chip(m: &mut Machine, fw: &Firmware) {
    press(m, HOME);
    m.run_ms(MENU_WIPE_MS);
    select_menu_item(m, VIEWS_MENU_CHIP as i32);
    m.run_ms(MENU_SETTLE_MS);
    m.turn(ALGORITHM, 1);
    m.run_ms(MENU_SETTLE_MS);
    wait_for_within(m, CHIP_OPEN_WITHIN_MS, "CHIP open", |m| {
        top_view(m, fw) == fw.symbol("chip_view_view") && !common::screen_animating(m, fw)
    });
    m.run_ms(CHIP_SETTLE_MS);
}

fn saved_record() -> Vec<u8> {
    let mut rec = vec![
        USER_PROFILE_VERSION as u8,
        SID_CHIP_6581_WORN as u8,
        SAVED_CONTROLS.len() as u8,
    ];
    rec.extend(SAVED_CONTROLS);
    rec.push(SID_EXPERT_COUNT as u8);
    rec.extend(saved_expert());
    rec
}

fn character_ccs(profile: u8, controls: [u8; 4]) -> Vec<u8> {
    let mut burst = vec![0xB0, CC_PROFILE, profile];
    for (cc, v) in CHARACTER_CCS.into_iter().zip(controls) {
        burst.extend([cc, v]);
    }
    burst
}

fn poll_flash_until(m: &mut Machine, what: &str, mut done: impl FnMut(&[u8]) -> bool) {
    let start = m.ms();
    while !done(&m.soc.flash.mem) {
        assert!(m.ms() < start + FLASH_WITHIN_MS, "timed out: {what}");
        m.run_ms(FLASH_POLL_MS);
    }
}

fn flash_with_user_3(fw: &Firmware) -> Vec<u8> {
    let mut m = common::booted(fw);
    m.midi(&character_ccs(CC_WORN, SAVED_CONTROLS));
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
    m.run_ms(CHARACTER_RAMP_MS);
    open_chip(&mut m, fw);
    holding(&mut m, EDIT, |m| {
        turn(m, ALGORITHM, SAVE_TARGET_USER_3_CLICKS)
    });
    m.run_ms(DEBOUNCED_MS);
    assert_eq!(
        live_u8(&mut m, fw, ilp32::live::profile),
        SID_CHIP_6581_WORN as u8,
        "the target moved the chip"
    );
    press(&mut m, SAVE);
    let want = saved_record();
    poll_flash_until(&mut m, "the profile in the flash", |flash| {
        stored_value(flash, KEY_USER_3).as_deref() == Some(&want[..])
    });
    for slot in (0..USER_SLOTS).filter(|&slot| slot != USER_3) {
        assert_eq!(
            stored_value(&m.soc.flash.mem, KEY_USER_1 + slot),
            None,
            "USER {} written",
            slot + 1
        );
    }
    m.midi(&character_ccs(CC_R4AR, [0; 4]));
    poll_flash_until(&mut m, "the settings following the defaults", |flash| {
        let rec = stored_value(flash, SETTINGS_KEY).expect("settings");
        rec[REC_PROFILE] == SID_CHIP_6581_R4AR as u8 && rec[REC_PROFILE + 1..][..4] == [0; 4]
    });
    assert_eq!(
        stored_value(&m.soc.flash.mem, KEY_USER_3),
        Some(want),
        "the settings' write left the slot"
    );
    m.soc.flash.mem.clone()
}

fn render_host(frames: usize, cc_block: usize, note_block: usize, ticks: &[u32]) -> Vec<i16> {
    common::render_instrument(frames, |i, l| unsafe {
        if i == 0 {
            l.user_valid = 1 << USER_3;
            l.user_base[USER_3 as usize] = SID_CHIP_6581_WORN as u8;
            l.user_ctl[USER_3 as usize] = SAVED_CONTROLS;
            l.user_expert[USER_3 as usize] = saved_expert();
        }
        if i == cc_block {
            live_cc(l, CC_PROFILE, CC_USER_3);
        }
        for _ in 0..ticks.get(i).copied().unwrap_or(0) {
            live_tick(l);
        }
        if i == note_block {
            live_note_on(l, A4, 100);
        }
    })
}

fn powered_on_with(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    m
}

fn assert_user_3_applied(m: &mut Machine, fw: &Firmware) {
    assert_eq!(
        live_u8(m, fw, ilp32::live::profile),
        SID_CHIP_6581_WORN as u8
    );
    assert_eq!(
        live_u8(m, fw, ilp32::live::user_applied),
        USER_3 as u8 + 1,
        "USER 3 applied"
    );
}

fn assert_saved_controls_live(m: &mut Machine, fw: &Firmware) {
    for (i, control) in SAVED_CONTROLS.iter().enumerate() {
        assert_eq!(live_u8(m, fw, ilp32::live::character + i), *control);
    }
}

#[test]
fn bank_2_program_change_applies_a_saved_user_profile_like_cc_85() {
    let Some(fw) = common::firmware("the user profile test") else {
        return;
    };
    let flash = flash_with_user_3(&fw);
    let mut m = powered_on_with(&fw, &flash);
    run_past_boot(&mut m, &fw);
    m.midi(&[0xB0, 0, 0, 32, 2, 0xC0, 7]);
    m.run_ms(CHARACTER_RAMP_MS);
    assert_user_3_applied(&mut m, &fw);
    assert_saved_controls_live(&mut m, &fw);
    m.midi(&[0xC0, 5, 12, 13]);
    m.run_ms(100);
    assert_eq!(
        live_u8(&mut m, &fw, ilp32::live::user_applied),
        USER_3 as u8 + 1,
        "empty or invalid pick"
    );
    m.midi(&[0xB0, CC_PROFILE, CC_R4AR]);
    m.run_ms(400);
    assert_eq!(live_u8(&mut m, &fw, ilp32::live::user_applied), 0);
    m.midi(&[0xB0, CC_PROFILE, CC_USER_3]);
    m.run_ms(CHARACTER_RAMP_MS);
    assert_user_3_applied(&mut m, &fw);
}

#[test]
fn a_user_profile_saved_from_the_chip_view_survives_a_power_cycle_and_sounds_as_saved() {
    let Some(fw) = common::firmware("the user profile test") else {
        return;
    };
    let flash = flash_with_user_3(&fw);
    let mut m = powered_on_with(&fw, &flash);
    let base = fw.chips();
    m.snapshot_bytes = vec![base + ilp32::reg_ring_chips::ticks as u32];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 1)));
    run_past_boot(&mut m, &fw);
    let slot = USER_3 as usize;
    assert_eq!(live_u8(&mut m, &fw, ilp32::live::user_valid), 1 << USER_3);
    assert_eq!(
        live_u8(&mut m, &fw, ilp32::live::user_base + slot),
        SID_CHIP_6581_WORN as u8
    );
    for (i, c) in SAVED_CONTROLS.iter().enumerate() {
        assert_eq!(
            live_u8(
                &mut m,
                &fw,
                ilp32::live::user_ctl + SAVED_CONTROLS.len() * slot + i
            ),
            *c
        );
    }
    for (i, value) in saved_expert().iter().enumerate() {
        assert_eq!(
            live_u8(
                &mut m,
                &fw,
                ilp32::live::user_expert + SID_EXPERT_COUNT as usize * slot + i
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
    m.midi(&[0xB0, CC_PROFILE, CC_USER_3]);
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
    assert_user_3_applied(&mut m, &fw);
    let s = rendered_sid(&mut m, &fw, 0);
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

    let audio = m.soc.periph.audio.clone();
    let mut found = None;
    let mut min_diff = i32::MAX;
    'search: for silent in 0..=MAX_SILENT_HALVES {
        if audio[..2 * silent * HALF_FRAMES].iter().any(|&v| v != 0) {
            break;
        }
        let played = &audio[2 * silent * HALF_FRAMES..];
        let before_note = (key_sent - silent) * HALF_FRAMES;
        for first in (cc_sent - silent..).take(CC_EFFECT_WITHIN_BLOCKS) {
            let host = render_host(before_note, first, usize::MAX, &ticks);
            if max_diff(&played[..2 * before_note], &host) != 0 {
                continue;
            }
            for note_block in (key_sent - silent..).take(CC_EFFECT_WITHIN_BLOCKS) {
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
fn the_picker_steps_onto_the_saved_slot_skipping_the_empty_ones_after_a_power_cycle() {
    let Some(fw) = common::firmware("the user profile test") else {
        return;
    };
    let flash = flash_with_user_3(&fw);
    let mut m = powered_on_with(&fw, &flash);
    run_past_boot(&mut m, &fw);
    open_chip(&mut m, &fw);
    for _ in 0..PICKER_STEPS_TO_USER_3 {
        m.turn(ALGORITHM, 1);
        m.run_ms(PICKER_STEP_MS);
    }
    m.run_ms(CHARACTER_RAMP_MS);
    assert_user_3_applied(&mut m, &fw);
    assert_saved_controls_live(&mut m, &fw);
}
