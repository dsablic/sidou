// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! LAB on the device (docs/plans/2026-10-25-lab-plan.md), in fm1-emu with its flash kept across
//! a power cycle: a sound kept in a USER slot is the store's file and comes back at power-on as
//! the sound playing; the unsaved edit comes back with its edited mark.

mod common;

use fm1_core::Bus;

use common::{build_dir, run_past_boot, stored_file, stored_value, Firmware, Machine, SETTLE_MS};
use sidou_sys::*;

/// The store's test hook (platform/fm1/store_fm1.h store_fm1_test): its op, key, result, data.
const HOOK_KEY: u32 = 4;
const HOOK_RESULT: u32 = 12;
const HOOK_DATA: u32 = 16;
const KEEP: u32 = 7; /* STORE_FM1_KEEP */
const BYTES: usize = LAB_SOUND_BYTES as usize;
const STRIDE: usize = USER_SOUND_STRIDE as usize;

fn fixture() -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message("the LAB test");
        return None;
    };
    Some(Firmware::load(&build))
}

/// Powered on with `flash`, past the boot.
fn power_on(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    run_past_boot(&mut m, fw);
    m
}

fn live_at(fw: &Firmware, field: usize) -> u32 {
    fw.symbol("instrument") + field as u32
}

/// The current sound's base as the firmware's live has it (lab_sound.h's image).
fn base(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let mut s = vec![0u8; BYTES];
    let a = live_at(fw, ilp32::live::base);
    for (i, b) in s.iter_mut().enumerate() {
        *b = m.soc.read8(a + i as u32);
    }
    s
}

fn base_name(m: &mut Machine, fw: &Firmware) -> String {
    let a = live_at(fw, ilp32::live::base_name);
    (0..LAB_SOUND_NAME)
        .map(|i| m.soc.read8(a + i))
        .take_while(|&c| c != 0)
        .map(char::from)
        .collect()
}

/// The fine tune NRPN (0/7, LIVE_PARAM_TUNE) and the resonance CC, over TRS: an edit that is
/// the sound's alone (the tune) and one the settings record has too.
fn edit(m: &mut Machine, tune: u8, resonance: u8) {
    m.midi(&[0xB0, 99, 0, 98, 7, 6, 0, 38, tune]);
    m.midi(&[0xB0, LIVE_CC_RESONANCE as u8, resonance]);
    m.run_ms(100);
}

/// Waits until `done` holds of the flash, for up to `ms`.
fn until(m: &mut Machine, ms: u64, what: &str, done: impl Fn(&[u8]) -> bool) {
    let end = m.ms() + ms;
    while !done(&m.soc.flash.mem) {
        assert!(m.ms() < end, "{what} never reached the flash");
        m.run_ms(200);
    }
}

#[test]
fn a_kept_sound_survives_a_power_cycle() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    edit(&mut m, 70, 127);
    let tune = m.soc.read8(live_at(&fw, ilp32::live::tune)) as i8;
    assert_eq!(tune, 20, "the NRPN tuned it +20 cents");
    // Kept in USER 3 through the store's test hook (LAB's SAVE is Task 8's).
    let h = fw.symbol("store_fm1_hook");
    m.soc.write32(h + HOOK_KEY, 2);
    for (i, &b) in b"LAB 03\0".iter().enumerate() {
        m.soc.write8(h + HOOK_DATA + i as u32, b);
    }
    m.soc.write32(h, KEEP);
    let end = m.ms() + 5_000;
    while m.soc.read32(h) != 0 {
        assert!(m.ms() < end, "the keep never ran");
        m.run_ms(10);
    }
    assert_eq!(m.soc.read32(h + HOOK_RESULT) as i32, 0, "queued");
    let kept = base(&mut m, &fw); // a keep makes the sound playing the base
    assert_eq!(kept[LAB_SOUND_TUNE as usize] as i8, 20);
    assert_eq!(m.soc.read8(live_at(&fw, ilp32::live::user_sound)), 3);
    until(&mut m, 20_000, "the USER sounds' file", |f| {
        stored_file(f, "USER SOUNDS")
            .is_some_and(|(_, b)| b[USER_SOUND_HEADER as usize + 2 * STRIDE] == 1)
    });
    let (typ, file) = stored_file(&m.soc.flash.mem, "USER SOUNDS").unwrap();
    assert_eq!(u32::from(typ), STORE_FILE_SOUNDS);
    assert_eq!(file.len(), USER_SOUND_FILE_BYTES as usize);
    let at = USER_SOUND_HEADER as usize + 2 * STRIDE;
    assert_eq!(&file[at + 1..at + 7], b"LAB 03");
    assert_eq!(file[at + 13..at + STRIDE], kept);
    // the settings follow: their preset byte names USER 3
    until(&mut m, SETTLE_MS + 5_000, "the settings' USER 3", |f| {
        stored_value(f, SETTINGS_KEY).is_some_and(|r| r[4] == LIVE_PRESET_SLOTS as u8 + 2)
    });
    let flash = m.soc.flash.mem.clone();
    let mut m = power_on(&fw, &flash);
    assert_eq!(
        m.soc.read16(live_at(&fw, ilp32::live::sounds_valid)),
        1 << 2,
        "USER 3 holds a sound"
    );
    assert_eq!(
        m.soc.read8(live_at(&fw, ilp32::live::user_sound)),
        3,
        "and plays"
    );
    assert_eq!(base(&mut m, &fw), kept, "its base, as kept");
    assert_eq!(base_name(&mut m, &fw), "LAB 03");
    assert_eq!(m.soc.read8(live_at(&fw, ilp32::live::tune)) as i8, 20);
}

#[test]
fn the_edit_comes_back_at_power_on() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    edit(&mut m, 30, 0);
    let tune = m.soc.read8(live_at(&fw, ilp32::live::tune)) as i8;
    assert_eq!(tune, -20);
    until(&mut m, SETTLE_MS + 5_000, "the unsaved edit", |f| {
        stored_value(f, USER_SOUND_EDIT_KEY)
            .is_some_and(|r| r[LAB_SOUND_TUNE as usize + 16] as i8 == -20)
    });
    let rec = stored_value(&m.soc.flash.mem, USER_SOUND_EDIT_KEY).unwrap();
    assert_eq!(rec.len(), USER_SOUND_EDIT_BYTES as usize);
    assert_eq!(rec[..3], [1, 0, 1], "version, SAW LEAD, edited");
    let flash = m.soc.flash.mem.clone();
    let mut m = power_on(&fw, &flash);
    assert_eq!(
        m.soc.read8(live_at(&fw, ilp32::live::tune)) as i8,
        -20,
        "the tune back"
    );
    assert_eq!(m.soc.read8(live_at(&fw, ilp32::live::resonance)), 0);
    // the base is SAW LEAD's, so the sound shows edited (its tune is not the base's)
    let base = base(&mut m, &fw);
    assert_eq!(base[LAB_SOUND_TUNE as usize], 0);
    assert_eq!(base_name(&mut m, &fw), "SAW LEAD");
}

// ---- LAB's view (Task 5 on): opened from the menu, its inputs and frames ----

const HOME: u8 = PANEL_BTN_HOME as u8;
const SELECT: u8 = PANEL_ENC_SELECT as u8;
const ALGORITHM: u8 = PANEL_ENC_ALGORITHM as u8;
const K1: u8 = PANEL_ENC_K1 as u8;
const KEY0: u8 = PANEL_KEY_ID0 as u8;
/// platform/fm1/ui.c's frame period.
const FRAME_MS: u64 = 40;
/// A ui_poll that composes LAB's frame while a key sounds and a card turns every frame: what the
/// drawings may spend. The input's call and the busiest pass after it within the poll allowance
/// together, as every view's (common::POLL_INSTRUCTIONS).
const LAB_WORST_POLL: u64 = common::POLL_INSTRUCTIONS;

fn tap(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

fn wait_for(m: &mut Machine, what: &str, mut done: impl FnMut(&mut Machine) -> bool) {
    let start = m.ms();
    while !done(m) {
        assert!(m.ms() - start < 4_000, "timed out: {what}");
        m.run_ms(1);
    }
}

/// LAB from LIVE through the menu: HOME, SELECT to LAB, ALGORITHM clockwise; its wipe over.
pub fn open_lab(m: &mut Machine, fw: &Firmware) {
    tap(m, HOME);
    wait_for(m, "the menu", |m| {
        common::top_view(m, fw) == fw.symbol("views_menu") && !common::screen_animating(m, fw)
    });
    let items = VIEWS_MENU_ITEMS as i32;
    m.turn(SELECT, -items);
    m.run_ms(10 * items as u64);
    m.turn(SELECT, VIEWS_MENU_LAB as i32);
    m.run_ms(10 * u64::from(VIEWS_MENU_LAB) + 10);
    m.turn(ALGORITHM, 1);
    wait_for(m, "LAB", |m| {
        common::top_view(m, fw) == fw.symbol("lab_view_view") && !common::screen_animating(m, fw)
    });
    m.run_ms(400);
}

fn lab_page(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc
        .read8(fw.symbol("ui") + (ilp32::views::lab + ilp32::lab_view::page) as u32)
}

fn depth(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc.read8(fw.symbol("ui") + ilp32::screen::depth as u32)
}

#[test]
fn lab_opens_from_the_menu_and_home_goes_back_to_live() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_lab(&mut m, &fw);
    assert_eq!(depth(&mut m, &fw), 2, "LAB in the menu's place, over LIVE");
    assert_eq!(lab_page(&mut m, &fw), LAB_PAGE_OSC as u8);
    m.turn(SELECT, 2);
    m.run_ms(100);
    assert_eq!(lab_page(&mut m, &fw), LAB_PAGE_FILTER as u8);
    tap(&mut m, PANEL_BTN_ENV as u8);
    assert_eq!(
        lab_page(&mut m, &fw),
        LAB_PAGE_ENVELOPE as u8,
        "ENV: ENVELOPE"
    );
    tap(&mut m, HOME);
    wait_for(&mut m, "LIVE", |m| {
        common::top_view(m, &fw) == fw.symbol("live_view_view") && !common::screen_animating(m, &fw)
    });
    assert_eq!(depth(&mut m, &fw), 1);
}

/// The inputs' calls and the busiest pass after each (from midi_drain to midi_service).
fn start_cost(m: &mut Machine, ms: u64, act: impl FnOnce(&mut Machine)) -> (u64, u64) {
    let (calls, passes) = (m.watched_calls.len(), m.passes.len());
    act(m);
    m.run_ms(ms);
    let input = m.watched_calls[calls..].iter().copied().max().unwrap_or(0);
    let pass = m.passes[passes..].iter().copied().max().unwrap_or(0);
    (input, pass)
}

/// platform/fm1/lcd.h lcd_count: windows and bytes sent so far.
fn sent(m: &mut Machine, fw: &Firmware) -> (u32, u32) {
    let base = fw.symbol("lcd_count");
    (m.soc.read32(base), m.soc.read32(base + 4))
}

/// `frames` frames from half way between two, `act(m, f)` at the start of frame f: the cells each
/// sent, and the worst ui_poll (ui_poll watched).
pub fn frames(
    m: &mut Machine,
    fw: &Firmware,
    n: u64,
    mut act: impl FnMut(&mut Machine, u64),
) -> (Vec<u32>, u64) {
    common::to_mid_frame(m, fw);
    let from = m.watched_calls.len();
    let mut cells = Vec::new();
    for f in 0..n {
        act(m, f);
        let (w0, b0) = sent(m, fw);
        m.run_ms(FRAME_MS);
        let (w1, b1) = sent(m, fw);
        cells.push(((b1 - b0) - (w1 - w0) * 11) / 128);
    }
    let worst = m.watched_calls[from..].iter().copied().max().unwrap_or(0);
    (cells, worst)
}

/// Every LAB input (SELECT, each card and the choice both ways on every page, ENV, LFO and FX)
/// with the busiest pass after it, within the poll allowance; and every frame, a key held and a
/// card turned every frame on each page, within LAB_VIEW_FRAME_CELLS and its ui_poll within
/// LAB_WORST_POLL.
#[test]
fn every_lab_input_and_pass_stays_within_poll_instructions() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_lab(&mut m, &fw);
    m.watch_calls(fw.symbol("ui_encoder"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut over = Vec::new();
    let mut worst_input = 0;
    for page in 0..LAB_PAGES {
        if page > 0 {
            let (i, p) = start_cost(&mut m, 300, |m| m.turn(SELECT, 1));
            println!("LAB SELECT to page {page}: the input {i}, the busiest pass {p}");
            if i + p > common::POLL_INSTRUCTIONS {
                over.push(format!("SELECT to {page}: {i} + {p}"));
            }
        }
        for (what, enc) in [
            ("K1", K1),
            ("K2", K1 + 1),
            ("K3", K1 + 2),
            ("K4", K1 + 3),
            ("ALGORITHM", ALGORITHM),
        ] {
            for steps in [1, -1] {
                let (i, p) = start_cost(&mut m, 300, |m| m.turn(enc, steps));
                println!("LAB page {page} {what} {steps:+}: the input {i}, the busiest pass {p}");
                worst_input = worst_input.max(i);
                if i + p > common::POLL_INSTRUCTIONS {
                    over.push(format!("page {page} {what} {steps:+}: {i} + {p}"));
                }
            }
        }
    }
    m.watch_calls(fw.symbol("ui_buttons"));
    for (what, b) in [
        ("ENV", PANEL_BTN_ENV),
        ("LFO", PANEL_BTN_LFO),
        ("FX", PANEL_BTN_FX),
    ] {
        let (i, p) = start_cost(&mut m, 300, |m| tap(m, b as u8));
        println!("LAB {what}: the input {i}, the busiest pass {p}");
        if i + p > common::POLL_INSTRUCTIONS {
            over.push(format!("{what}: {i} + {p}"));
        }
    }
    println!("LAB's worst encoder input: {worst_input}");
    // The frames: a key held, a card turned every frame, on each page.
    m.watch_calls(fw.symbol("ui_poll"));
    m.key(KEY0 + 12, true);
    let mut most = 0;
    let mut worst = 0;
    for page in 0..LAB_PAGES {
        m.turn(SELECT, -(LAB_PAGES as i32));
        m.run_ms(60);
        m.turn(SELECT, page as i32);
        let (cells, poll) = frames(&mut m, &fw, 40, |m, f| {
            m.turn(K1 + (f % 4) as u8, if f % 8 < 4 { 1 } else { -1 })
        });
        let page_most = cells.iter().copied().max().unwrap_or(0);
        println!("LAB page {page}, a key held, a card a frame: most cells {page_most}, worst ui_poll {poll}");
        most = most.max(page_most);
        worst = worst.max(poll);
    }
    m.key(KEY0 + 12, false);
    assert!(over.is_empty(), "{over:#?}");
    assert!(most <= LAB_VIEW_FRAME_CELLS, "a frame sent {most} cells");
    assert!(
        worst <= LAB_WORST_POLL,
        "a ui_poll took {worst} instructions"
    );
}

/// In LAB the keys play (a voice gates) and PLAY plays the song, which plays on while the cards
/// turn.
#[test]
fn the_keys_play_and_a_song_plays_on_in_lab() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_lab(&mut m, &fw);
    let instrument = fw.symbol("instrument");
    let gated = |m: &mut Machine| {
        (0..LIVE_VOICES as usize).any(|v| {
            m.soc.read8(
                instrument
                    + (ilp32::live::voice + v * ilp32::live_voice::SIZE + ilp32::live_voice::gate)
                        as u32,
            ) != 0
        })
    };
    m.key(KEY0 + 12, true);
    m.run_ms(60);
    assert!(gated(&mut m), "a key in LAB gates a voice");
    m.key(KEY0 + 12, false);
    m.run_ms(60);
    let playing = |m: &mut Machine| {
        m.soc
            .read8(fw.symbol("song_transport") + ilp32::song_ctl::isr_playing as u32)
            != 0
    };
    tap(&mut m, PANEL_BTN_PLAY as u8);
    m.run_ms(500);
    assert!(playing(&mut m), "PLAY in LAB plays the song");
    for k in 0..4 {
        m.turn(K1 + k, 2);
        m.run_ms(100);
    }
    m.run_ms(500);
    assert!(playing(&mut m), "and it plays on while the cards turn");
    assert_eq!(
        common::top_view(&mut m, &fw),
        fw.symbol("lab_view_view"),
        "LAB still shows"
    );
}
