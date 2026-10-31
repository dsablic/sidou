// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use fm1_core::Bus;

use common::panel::*;
use common::{
    booted, firmware, run_past_boot, stored_file, stored_value, toast_text, Firmware, Machine,
    SETTLE_MS,
};
use sidou_sys::*;

const HOOK_KEY: u32 = 4;
const HOOK_RESULT: u32 = 12;
const HOOK_DATA: u32 = 16;
const STORE_FM1_KEEP: u32 = 7;
const BYTES: usize = LAB_SOUND_BYTES as usize;
const STRIDE: usize = USER_SOUND_STRIDE as usize;

fn power_on(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    run_past_boot(&mut m, fw);
    m
}

fn live_at(fw: &Firmware, field: usize) -> u32 {
    fw.symbol("instrument") + field as u32
}

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

fn edit(m: &mut Machine, tune: u8, resonance: u8) {
    m.midi(&[0xB0, 99, 0, 98, 7, 6, 0, 38, tune]);
    m.midi(&[0xB0, LIVE_CC_RESONANCE as u8, resonance]);
    m.run_ms(100);
}

fn until(m: &mut Machine, ms: u64, what: &str, done: impl Fn(&[u8]) -> bool) {
    let end = m.ms() + ms;
    while !done(&m.soc.flash.mem) {
        assert!(m.ms() < end, "{what} never reached the flash");
        m.run_ms(200);
    }
}

#[test]
fn a_kept_sound_survives_a_power_cycle() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    edit(&mut m, 70, 127);
    let tune = m.soc.read8(live_at(&fw, ilp32::live::tune)) as i8;
    assert_eq!(tune, 20, "the NRPN tuned it +20 cents");
    let h = fw.symbol("store_fm1_hook");
    m.soc.write32(h + HOOK_KEY, 2);
    for (i, &b) in b"LAB 03\0".iter().enumerate() {
        m.soc.write8(h + HOOK_DATA + i as u32, b);
    }
    m.soc.write32(h, STORE_FM1_KEEP);
    let end = m.ms() + 5_000;
    while m.soc.read32(h) != 0 {
        assert!(m.ms() < end, "the keep never ran");
        m.run_ms(10);
    }
    assert_eq!(m.soc.read32(h + HOOK_RESULT) as i32, 0, "queued");
    let kept = base(&mut m, &fw);
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
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
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
    let base = base(&mut m, &fw);
    assert_eq!(base[LAB_SOUND_TUNE as usize], 0);
    assert_eq!(base_name(&mut m, &fw), "SAW LEAD");
}

const FRAME_MS: u64 = common::UI_FRAME_MS;
const LAB_OPENED_MS: u64 = 400;
const ATTACK: u8 = K1;
const DECAY: u8 = K1 + 1;
const RELEASE: u8 = K1 + 3;
const NAME_PAGE_SLOT: u8 = K1;
const F4_KEY: u8 = KEY0 + 12;
const LAB_WORST_POLL: u64 = 90_000;
const _: () = assert!(LAB_WORST_POLL <= common::POLL_INSTRUCTIONS);

pub fn open_lab(m: &mut Machine, fw: &Firmware) {
    press(m, HOME);
    wait_for_view(m, fw, "views_menu");
    select_menu_item(m, VIEWS_MENU_LAB as i32);
    m.run_ms(DETENT_MS);
    m.turn(ALGORITHM, 1);
    wait_for_view(m, fw, "lab_view_view");
    m.run_ms(LAB_OPENED_MS);
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
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    assert_eq!(depth(&mut m, &fw), 2, "LAB in the menu's place, over LIVE");
    assert_eq!(lab_page(&mut m, &fw), LAB_PAGE_OSC as u8);
    m.turn(SELECT, 2);
    m.run_ms(100);
    assert_eq!(lab_page(&mut m, &fw), LAB_PAGE_FILTER as u8);
    press(&mut m, ENV);
    assert_eq!(
        lab_page(&mut m, &fw),
        LAB_PAGE_ENVELOPE as u8,
        "ENV: ENVELOPE"
    );
    press(&mut m, HOME);
    wait_for_view(&mut m, &fw, "live_view_view");
    assert_eq!(depth(&mut m, &fw), 1);
}

#[test]
fn env_lfo_and_fx_on_live_open_lab_at_their_pages() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    let l = m.soc.read32(fw.symbol("ui") + ilp32::views::l as u32);
    let model = m.soc.read8(l + ilp32::live::model as u32);
    let slot = l + ilp32::live::slot as u32;
    m.watch_calls(fw.symbol("ui_buttons"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    for (what, b, page) in [
        ("ENV", PANEL_BTN_ENV, LAB_PAGE_ENVELOPE),
        ("LFO", PANEL_BTN_LFO, LAB_PAGE_MOD),
        ("FX", PANEL_BTN_FX, LAB_PAGE_FX),
    ] {
        let (i, p) = start_cost(&mut m, 300, |m| press(m, b as u8));
        println!("LIVE {what}: the input {i}, the busiest pass {p}");
        assert!(
            i + p <= common::POLL_INSTRUCTIONS,
            "{what}: {i} + {p} instructions"
        );
        wait_for_view(&mut m, &fw, "lab_view_view");
        assert_eq!(lab_page(&mut m, &fw), page as u8, "{what}");
        assert_eq!(depth(&mut m, &fw), 2, "{what}: LAB over LIVE");
        assert_eq!(
            m.soc.read8(l + ilp32::live::model as u32),
            model,
            "{what}: the model"
        );
        assert_eq!(m.soc.read8(slot), 0, "{what}: the slot's algorithm");
        press(&mut m, HOME);
        wait_for_view(&mut m, &fw, "live_view_view");
    }
}

fn start_cost(m: &mut Machine, ms: u64, act: impl FnOnce(&mut Machine)) -> (u64, u64) {
    let (calls, passes) = (m.watched_calls.len(), m.passes.len());
    act(m);
    m.run_ms(ms);
    let input = m.watched_calls[calls..].iter().copied().max().unwrap_or(0);
    let pass = m.passes[passes..].iter().copied().max().unwrap_or(0);
    (input, pass)
}

fn sent(m: &mut Machine, fw: &Firmware) -> (u32, u32) {
    let base = fw.symbol("lcd_count");
    (m.soc.read32(base), m.soc.read32(base + 4))
}

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

#[test]
fn every_lab_input_and_pass_stays_within_poll_instructions() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
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
        let (i, p) = start_cost(&mut m, 300, |m| press(m, b as u8));
        println!("LAB {what}: the input {i}, the busiest pass {p}");
        if i + p > common::POLL_INSTRUCTIONS {
            over.push(format!("{what}: {i} + {p}"));
        }
    }
    println!("LAB's worst encoder input: {worst_input}");
    m.watch_calls(fw.symbol("ui_poll"));
    m.key(F4_KEY, true);
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
    m.key(F4_KEY, false);
    assert!(over.is_empty(), "{over:#?}");
    assert!(most <= LAB_VIEW_FRAME_CELLS, "a frame sent {most} cells");
    assert!(
        worst <= LAB_WORST_POLL,
        "a ui_poll took {worst} instructions"
    );
}

#[test]
fn the_keys_play_and_a_song_plays_on_in_lab() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
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
    m.key(F4_KEY, true);
    m.run_ms(60);
    assert!(gated(&mut m), "a key in LAB gates a voice");
    m.key(F4_KEY, false);
    m.run_ms(60);
    let playing = |m: &mut Machine| {
        m.soc
            .read8(fw.symbol("song_transport") + ilp32::song_ctl::isr_playing as u32)
            != 0
    };
    press(&mut m, PLAY);
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

fn page_frames(m: &mut Machine, fw: &Firmware, page: u32, frames_n: u64) -> (u32, u64) {
    m.turn(SELECT, -(LAB_PAGES as i32));
    m.run_ms(60);
    m.turn(SELECT, page as i32);
    m.run_ms(200);
    m.key(F4_KEY, true);
    let (cells, poll) = frames(m, fw, frames_n, |m, f| {
        m.turn(K1 + (f % 4) as u8, if f % 8 < 4 { 1 } else { -1 });
        if f % 20 == 10 {
            m.key(F4_KEY, false);
        } else if f % 20 == 15 {
            m.key(F4_KEY, true);
        }
    });
    m.key(F4_KEY, false);
    (cells.iter().copied().max().unwrap_or(0), poll)
}

#[test]
fn lab_osc_and_envelope_draw_within_their_budgets() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    m.watch_calls(fw.symbol("ui_poll"));
    let mut over = Vec::new();
    for (what, page, preset) in [
        ("OSC, SAW LEAD", LAB_PAGE_OSC, 0),
        ("ENVELOPE, SAW LEAD", LAB_PAGE_ENVELOPE, 0),
        ("OSC, PWM BASS", LAB_PAGE_OSC, 1),
        ("ENVELOPE, PWM BASS", LAB_PAGE_ENVELOPE, 1),
    ] {
        if preset == 1 {
            m.turn(PRESETS, 1);
            m.run_ms(300);
        }
        let (most, poll) = page_frames(&mut m, &fw, page, 60);
        println!("LAB {what}: most cells {most}, worst ui_poll {poll}");
        if most > LAB_VIEW_FRAME_CELLS || poll > LAB_WORST_POLL {
            over.push(format!("{what}: {most} cells, {poll} instructions"));
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}

#[test]
fn lab_filter_mod_and_fx_draw_within_their_budgets() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    m.watch_calls(fw.symbol("ui_poll"));
    let mut over = Vec::new();
    let mut run = |m: &mut Machine, what: &str, page: u32| {
        let (most, poll) = page_frames(m, &fw, page, 60);
        println!("LAB {what}: most cells {most}, worst ui_poll {poll}");
        if most > LAB_VIEW_FRAME_CELLS || poll > LAB_WORST_POLL {
            over.push(format!("{what}: {most} cells, {poll} instructions"));
        }
    };
    run(&mut m, "FILTER, LP", LAB_PAGE_FILTER);
    m.turn(ALGORITHM, 3);
    m.run_ms(100);
    run(&mut m, "FILTER, NOTCH", LAB_PAGE_FILTER);
    m.turn(K1 + 3, 12);
    m.run_ms(200);
    run(&mut m, "FILTER, NOTCH, LFO", LAB_PAGE_FILTER);
    for _ in 0..3 {
        m.turn(K1 + 1, 40);
    }
    run(&mut m, "MOD, vibrato", LAB_PAGE_MOD);
    m.turn(ALGORITHM, 3);
    m.run_ms(100);
    run(&mut m, "MOD, ARP", LAB_PAGE_MOD);
    m.turn(SELECT, -(LAB_PAGES as i32));
    m.run_ms(60);
    m.turn(SELECT, LAB_PAGE_FX as i32);
    m.run_ms(100);
    m.turn(ALGORITHM, 2);
    m.run_ms(100);
    run(&mut m, "FX, OVERDRIVE", LAB_PAGE_FX);
    press(&mut m, SCL);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    run(&mut m, "FX, CHORUS", LAB_PAGE_FX);
    press(&mut m, SCL);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    run(&mut m, "FX, DELAY", LAB_PAGE_FX);
    run(&mut m, "MACRO", LAB_PAGE_MACRO);
    assert!(over.is_empty(), "{over:#?}");
}

#[test]
fn a_happy_accident_is_kept_with_one_press() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    press(&mut m, ENV);
    assert_eq!(lab_page(&mut m, &fw), LAB_PAGE_ENVELOPE as u8);
    m.turn(ATTACK, 2);
    m.turn(RELEASE, -3);
    m.run_ms(100);
    let release = m.soc.read8(live_at(&fw, ilp32::live::sustain_release)) & 15;
    press(&mut m, SAVE);
    let mut toast = String::new();
    let end = m.ms() + 20_000;
    while !toast.starts_with("KEPT") {
        assert!(m.ms() < end, "the keep never said so: {toast:?}");
        m.run_ms(20);
        toast = toast_text(&mut m, &fw);
    }
    assert_eq!(toast, "KEPT AS U01 LAB 01");
    let (typ, file) = stored_file(&m.soc.flash.mem, "USER SOUNDS").expect("the file is stored");
    assert_eq!(u32::from(typ), STORE_FILE_SOUNDS);
    assert_eq!(file[USER_SOUND_HEADER as usize], 1, "USER 1 is used");
    let at = USER_SOUND_HEADER as usize;
    assert_eq!(&file[at + 1..at + 7], b"LAB 01");
    let sound = &file[at + 1 + LAB_SOUND_NAME as usize..at + STRIDE];
    assert_eq!(sound[LAB_SOUND_AD as usize] >> 4, 2, "the attack kept");
    assert_eq!(
        sound[LAB_SOUND_SR as usize] & 15,
        release,
        "the release kept"
    );
    assert_eq!(
        sound,
        base(&mut m, &fw).as_slice(),
        "the base is what was kept"
    );
    assert_eq!(m.soc.read8(live_at(&fw, ilp32::live::user_sound)), 1);
}

fn lab_byte(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    m.soc
        .read8(fw.symbol("ui") + (ilp32::views::lab + field) as u32)
}

#[test]
fn edit_save_names_and_writes_a_slot() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    press(&mut m, ENV);
    m.turn(ATTACK, 3);
    m.run_ms(100);
    chord(&mut m, EDIT, SAVE);
    m.run_ms(100);
    assert_eq!(
        u32::from(lab_byte(&mut m, &fw, ilp32::lab_view::mode)),
        LAB_VIEW_MODE_NAME,
        "the name page"
    );
    assert_eq!(lab_byte(&mut m, &fw, ilp32::lab_view::slot), 0);
    m.turn(NAME_PAGE_SLOT, 4);
    m.run_ms(60);
    for key in [7u8, 8u8] {
        m.key(KEY0 + key, true);
        m.run_ms(40);
        let instrument = fw.symbol("instrument");
        let gated = (0..LIVE_VOICES as usize).any(|v| {
            m.soc.read8(
                instrument
                    + (ilp32::live::voice + v * ilp32::live_voice::SIZE + ilp32::live_voice::gate)
                        as u32,
            ) != 0
        });
        assert!(!gated, "a typing key makes no sound");
        m.key(KEY0 + key, false);
        m.run_ms(40);
    }
    assert_eq!(lab_byte(&mut m, &fw, ilp32::lab_view::cursor_at), 2);
    press(&mut m, SAVE);
    let mut toast = String::new();
    let end = m.ms() + 20_000;
    while !toast.starts_with("KEPT") {
        assert!(m.ms() < end, "the keep never said so: {toast:?}");
        m.run_ms(20);
        toast = toast_text(&mut m, &fw);
    }
    assert_eq!(toast, "KEPT AS U05 HI");
    assert_eq!(
        u32::from(lab_byte(&mut m, &fw, ilp32::lab_view::mode)),
        LAB_VIEW_MODE_PAGES
    );
    let (_, file) = stored_file(&m.soc.flash.mem, "USER SOUNDS").unwrap();
    let at = USER_SOUND_HEADER as usize + 4 * STRIDE;
    assert_eq!(file[at], 1, "USER 5 is used");
    assert_eq!(&file[at + 1..at + 4], b"HI\0");
    let sound = &file[at + 1 + LAB_SOUND_NAME as usize..at + STRIDE];
    assert_eq!(sound[LAB_SOUND_AD as usize] >> 4, 3, "the attack kept");
    assert_eq!(sound, base(&mut m, &fw).as_slice());
    assert_eq!(file[USER_SOUND_HEADER as usize], 0, "USER 1 stays empty");
}

#[test]
fn the_name_page_stays_within_the_frame_budget() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    chord(&mut m, EDIT, SAVE);
    m.run_ms(100);
    assert_eq!(
        u32::from(lab_byte(&mut m, &fw, ilp32::lab_view::mode)),
        LAB_VIEW_MODE_NAME
    );
    m.watch_calls(fw.symbol("ui_encoder"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    for (what, enc, steps) in [
        ("K1", K1, 1),
        ("SELECT", SELECT, 1),
        ("ALGORITHM", ALGORITHM, 1),
        ("ALGORITHM", ALGORITHM, -1),
    ] {
        let (i, p) = start_cost(&mut m, 300, |m| m.turn(enc, steps));
        println!("LAB name page {what} {steps:+}: the input {i}, the busiest pass {p}");
        assert!(i + p <= common::POLL_INSTRUCTIONS, "{what}: {i} + {p}");
    }
    m.watch_calls(fw.symbol("ui_poll"));
    let (cells, worst) = frames(&mut m, &fw, 40, |m, f| match f % 4 {
        0 => m.turn(K1, if f % 8 < 4 { 1 } else { -1 }),
        1 => m.turn(ALGORITHM, 1),
        2 => m.key(KEY0 + (f % 26) as u8, true),
        _ => m.key(KEY0 + ((f - 1) % 26) as u8, false),
    });
    let most = cells.iter().copied().max().unwrap_or(0);
    println!("LAB name page: most cells {most}, worst ui_poll {worst}");
    assert!(most <= LAB_VIEW_FRAME_CELLS, "a frame sent {most} cells");
    assert!(
        worst <= LAB_WORST_POLL,
        "a ui_poll took {worst} instructions"
    );
}

#[test]
fn presets_reaches_the_user_sounds_on_live_and_in_lab() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    press(&mut m, ENV);
    m.turn(ATTACK, 5);
    m.run_ms(100);
    press(&mut m, SAVE);
    let mut toast = String::new();
    let end = m.ms() + 20_000;
    while !toast.starts_with("KEPT") {
        assert!(m.ms() < end, "the keep never said so: {toast:?}");
        m.run_ms(20);
        toast = toast_text(&mut m, &fw);
    }
    let user = |m: &mut Machine| m.soc.read8(live_at(&fw, ilp32::live::user_sound));
    let attack = |m: &mut Machine| m.soc.read8(live_at(&fw, ilp32::live::attack_decay)) >> 4;
    assert_eq!((user(&mut m), attack(&mut m)), (1, 5), "USER 1 plays");
    let presets = PRESETS;
    m.turn(presets, -1);
    m.run_ms(200);
    assert_eq!(user(&mut m), 0, "DICE");
    assert_eq!(
        m.soc.read8(live_at(&fw, ilp32::live::preset)),
        LIVE_PRESET_DICE as u8
    );
    m.watch_calls(fw.symbol("ui_encoder"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let (input, pass) = start_cost(&mut m, 200, |m| m.turn(presets, 1));
    println!("PRESETS into a USER sound: the input {input}, the busiest pass {pass}");
    assert!(
        input + pass <= common::POLL_INSTRUCTIONS,
        "{input} + {pass} over the poll allowance"
    );
    assert_eq!(
        (user(&mut m), attack(&mut m)),
        (1, 5),
        "USER 1 again, in LAB"
    );
    assert_eq!(base_name(&mut m, &fw), "LAB 01");
    press(&mut m, HOME);
    wait_for_view(&mut m, &fw, "live_view_view");
    m.turn(presets, -1);
    m.run_ms(200);
    assert_eq!(user(&mut m), 0);
    m.turn(presets, 1);
    m.run_ms(200);
    assert_eq!((user(&mut m), attack(&mut m)), (1, 5), "USER 1 on LIVE");
    m.turn(presets, 1);
    m.run_ms(200);
    assert_eq!(user(&mut m), 1, "the end clamps");
}

fn cutoff(m: &mut Machine, fw: &Firmware) -> u16 {
    let a = live_at(fw, ilp32::live::cutoff);
    u16::from(m.soc.read8(a)) | u16::from(m.soc.read8(a + 1)) << 8
}

#[test]
fn edit_held_and_a_key_peeks_at_a() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    let a = cutoff(&mut m, &fw);
    m.turn(SELECT, LAB_PAGE_FILTER as i32);
    m.run_ms(100);
    m.turn(K1, -30);
    m.run_ms(400);
    let b = cutoff(&mut m, &fw);
    assert!(b < a / 2, "the cutoff well down: {a} -> {b}");
    let bright = |x: &[f32]| common::power_above(x, 2000.0);
    m.key(F4_KEY, true);
    m.run_ms(600);
    let heard_b = bright(&common::record(&mut m, 200));
    m.key(F4_KEY, false);
    m.run_ms(300);
    m.key(EDIT, true);
    m.run_ms(60);
    m.watch_calls(fw.symbol("ui_note_keys"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let (input, pass) = start_cost(&mut m, 100, |m| m.key(F4_KEY, true));
    println!("a peek's key: the input {input}, the busiest pass {pass}");
    assert!(
        input + pass <= common::POLL_INSTRUCTIONS,
        "{input} + {pass}"
    );
    assert_eq!(cutoff(&mut m, &fw), a, "A plays");
    m.run_ms(500);
    let heard_a = bright(&common::record(&mut m, 200));
    m.key(F4_KEY, false);
    m.key(EDIT, false);
    m.run_ms(100);
    assert_eq!(cutoff(&mut m, &fw), b, "B back");
    assert_eq!(
        u32::from(lab_byte(&mut m, &fw, ilp32::lab_view::ab)),
        LAB_VIEW_AB_OFF,
        "no tap"
    );
    println!("the power above 2 kHz: B {heard_b:.4}, A peeked {heard_a:.4}");
    assert!(
        heard_a > 4.0 * heard_b,
        "A is heard: {heard_a} against {heard_b}"
    );
}

#[test]
fn edit_tapped_swaps_the_sound_heard() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    let a = cutoff(&mut m, &fw);
    m.turn(SELECT, LAB_PAGE_FILTER as i32);
    m.run_ms(100);
    m.turn(K1, -30);
    m.key(F4_KEY, true);
    m.run_ms(600);
    let bright = |x: &[f32]| common::power_above(x, 2000.0);
    let heard_b = bright(&common::record(&mut m, 200));
    let b = cutoff(&mut m, &fw);
    assert!(b < a / 2, "the cutoff well down: {a} -> {b}");
    m.watch_calls(fw.symbol("ui_buttons"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let edit = EDIT;
    let (input, pass) = start_cost(&mut m, 100, |m| press(m, edit));
    println!("EDIT's tap: the input {input}, the busiest pass {pass}");
    assert!(
        input + pass <= common::POLL_INSTRUCTIONS,
        "{input} + {pass}"
    );
    assert_eq!(cutoff(&mut m, &fw), a, "A: the sound as loaded");
    assert_eq!(
        u32::from(lab_byte(&mut m, &fw, ilp32::lab_view::ab)),
        LAB_VIEW_AB_A
    );
    let heard_a = bright(&common::record(&mut m, 200));
    let (input, pass) = start_cost(&mut m, 100, |m| press(m, edit));
    println!("EDIT's tap back: the input {input}, the busiest pass {pass}");
    assert!(
        input + pass <= common::POLL_INSTRUCTIONS,
        "{input} + {pass}"
    );
    assert_eq!(cutoff(&mut m, &fw), b, "B: the edit again");
    let heard_b2 = bright(&common::record(&mut m, 200));
    m.key(F4_KEY, false);
    println!("the power above 2 kHz: B {heard_b:.4}, A {heard_a:.4}, B again {heard_b2:.4}");
    assert!(
        heard_a > 4.0 * heard_b && heard_a > 4.0 * heard_b2,
        "A is heard brighter than B: {heard_a} against {heard_b}, {heard_b2}"
    );
    assert!(
        (heard_b2 - heard_b).abs() <= 0.5 * heard_b.max(heard_b2),
        "B sounds as it did: {heard_b} and {heard_b2}"
    );
}

fn edit_with(m: &mut Machine, b: u32) {
    chord(m, EDIT, b as u8);
    m.run_ms(CHORD_HELD_MS);
}

#[test]
fn edit_lfo_undoes_in_lab() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    press(&mut m, ENV);
    let ad = |m: &mut Machine| m.soc.read8(live_at(&fw, ilp32::live::attack_decay));
    let start = ad(&mut m);
    for _ in 0..3 {
        m.turn(ATTACK, 1);
        m.run_ms(60);
    }
    m.turn(DECAY, 2);
    m.run_ms(100);
    let turned = ad(&mut m);
    assert_eq!(turned >> 4, (start >> 4) + 3);
    m.watch_calls(fw.symbol("ui_buttons"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let (input, pass) = start_cost(&mut m, 100, |m| edit_with(m, PANEL_BTN_LFO));
    println!("EDIT + LFO: the input {input}, the busiest pass {pass}");
    assert!(
        input + pass <= common::POLL_INSTRUCTIONS,
        "{input} + {pass}"
    );
    assert_eq!(
        ad(&mut m),
        (turned & 0xF0) | (start & 15),
        "the decay undone"
    );
    assert_eq!(toast_text(&mut m, &fw), "UNDO (1)");
    edit_with(&mut m, PANEL_BTN_LFO);
    assert_eq!(ad(&mut m), start, "the attack's run undone at once");
    assert_eq!(toast_text(&mut m, &fw), "UNDO (0)");
    edit_with(&mut m, PANEL_BTN_LFO);
    assert_eq!(toast_text(&mut m, &fw), "NOTHING TO UNDO");
    assert_eq!(
        lab_page(&mut m, &fw),
        LAB_PAGE_ENVELOPE as u8,
        "LFO stayed on the page"
    );
}

fn voice_fields(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let mut out = Vec::new();
    for (f, n) in [
        (ilp32::live::control, 1),
        (ilp32::live::attack_decay, 1),
        (ilp32::live::sustain_release, 1),
        (ilp32::live::pulse_width, 2),
        (ilp32::live::cutoff, 2),
        (ilp32::live::resonance, 1),
        (ilp32::live::filter_voices, 1),
        (ilp32::live::filter_mode, 1),
        (ilp32::live::vibrato_rate, 1),
        (ilp32::live::vibrato_depth, 1),
        (ilp32::live::vibrato_delay, 1),
        (ilp32::live::velocity_target, 1),
        (ilp32::live::arp_on, 1),
        (ilp32::live::arp_order, 1),
        (ilp32::live::pwm_depth, 2),
        (ilp32::live::pwm_rate, 2),
        (ilp32::live::tune, 1),
    ] {
        for i in 0..n {
            out.push(m.soc.read8(live_at(fw, f) + i));
        }
    }
    out
}

fn heard_db(x: &[f32]) -> f64 {
    let (mut x1, mut y, mut ms) = (0.0f64, 0.0f64, 0.0f64);
    for &v in x {
        let v = f64::from(v);
        y = v - x1 + 0.995 * y;
        x1 = v;
        ms += y * y;
    }
    20.0 * ((ms / x.len() as f64).sqrt().max(1e-9) / 32768.0).log10()
        - 20.0 * f64::from(common::boot_gain()).log10()
}

#[test]
fn edit_arp_and_edit_scl_change_the_sound_heard_and_undo() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    let c4 = KEY0 + (60 - PANEL_BASE_NOTE) as u8;
    let start = voice_fields(&mut m, &fw);
    assert_eq!(
        m.soc.read8(live_at(&fw, ilp32::live::slot)),
        0,
        "the first slot is off: the chips' level is heard"
    );
    m.watch_calls(fw.symbol("ui_buttons"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut worst = (0, 0);
    let mut results = vec![start.clone()];
    for i in 0..12u32 {
        if i % 3 == 0 {
            m.turn(SELECT, if i == 0 { 0 } else { 1 });
            m.run_ms(60);
        }
        let (b, what) = if i % 2 == 0 {
            (PANEL_BTN_ARP, "VARIED")
        } else {
            (PANEL_BTN_SCL, "ROLLED")
        };
        let (input, pass) = start_cost(&mut m, 60, |m| edit_with(m, b));
        if input + pass > worst.0 + worst.1 {
            worst = (input, pass);
        }
        assert_eq!(toast_text(&mut m, &fw), what, "{i}");
        let now = voice_fields(&mut m, &fw);
        assert_ne!(Some(&now), results.last(), "{i}: the sound changed");
        results.push(now);
        m.key(c4, true);
        let db = heard_db(&common::record(&mut m, u64::from(LAB_SILENT_MS)));
        m.key(c4, false);
        m.run_ms(300);
        println!("{i} {what}: {db:.1} dB (the chips' dBFS)");
        assert!(
            db >= f64::from(LAB_SILENT_DB),
            "{i} {what}: {db:.1} dB is silent"
        );
    }
    println!(
        "EDIT + ARP / SCL: the worst input {} and its busiest pass {}",
        worst.0, worst.1
    );
    assert!(worst.0 + worst.1 <= common::POLL_INSTRUCTIONS, "{worst:?}");
    let held = usize::from(lab_byte(
        &mut m,
        &fw,
        ilp32::lab_view::undo + ilp32::lab_edit::steps,
    ));
    println!("undo holds {held} of the 12");
    assert!(held >= 4, "{held} steps");
    for k in 1..=held {
        edit_with(&mut m, PANEL_BTN_LFO);
        assert_eq!(voice_fields(&mut m, &fw), results[12 - k], "undone {k}");
    }
    assert_eq!(toast_text(&mut m, &fw), "UNDO (0)");
    edit_with(&mut m, PANEL_BTN_LFO);
    assert_eq!(toast_text(&mut m, &fw), "NOTHING TO UNDO");
}

#[test]
fn scl_before_edit_rolls_in_lab_too() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    press(&mut m, FX);
    let fx = live_at(&fw, ilp32::live::fx_select);
    let slot = m.soc.read8(fx);
    let before = voice_fields(&mut m, &fw);
    chord_key_first(&mut m, SCL, EDIT);
    m.run_ms(200);
    assert_eq!(toast_text(&mut m, &fw), "ROLLED");
    assert_ne!(voice_fields(&mut m, &fw), before, "rolled");
    assert_eq!(m.soc.read8(fx), slot, "SCL selected no slot");
    press(&mut m, SCL);
    m.run_ms(60);
    assert_ne!(m.soc.read8(fx), slot, "SCL alone: the next slot");
}

#[test]
fn arp_or_lfo_before_edit_vary_and_undo_in_lab_too() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    m.turn(SELECT, LAB_PAGE_FILTER as i32);
    m.run_ms(200);
    let before = voice_fields(&mut m, &fw);
    let lead = |m: &mut Machine, first: u32| {
        chord_key_first(m, first as u8, EDIT);
        m.run_ms(200);
    };
    lead(&mut m, PANEL_BTN_ARP);
    assert_eq!(toast_text(&mut m, &fw), "VARIED");
    assert_ne!(voice_fields(&mut m, &fw), before, "varied");
    lead(&mut m, PANEL_BTN_LFO);
    assert_eq!(toast_text(&mut m, &fw), "UNDO (0)");
    assert_eq!(voice_fields(&mut m, &fw), before, "undone");
    let ab = fw.symbol("ui") + (ilp32::views::lab + ilp32::lab_view::ab) as u32;
    assert_eq!(m.soc.read8(ab), LAB_VIEW_AB_OFF as u8, "no A/B tap");
}

#[test]
fn midi_learn_on_a_lab_card_over_trs() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    let lab = fw.symbol("lab_view_view");
    let learn = fw.symbol("learn_view_view");
    let depth_of = |m: &mut Machine| {
        let a = live_at(&fw, ilp32::live::pwm_depth);
        u32::from(m.soc.read8(a)) | u32::from(m.soc.read8(a + 1)) << 8
    };
    let before = depth_of(&mut m);
    let row = unsafe { midi_map_for_param(LIVE_PARAM_PWM_DEPTH) };
    let from_cc = |v: u8| unsafe { midi_map_from_cc(std::ptr::null(), row, v) };

    m.key(SAVE, true);
    m.run_ms(30);
    m.turn(K1 + 1, 1);
    m.run_ms(80);
    assert_eq!(
        common::top_view(&mut m, &fw),
        learn,
        "SAVE + K2 opened learn"
    );
    m.key(SAVE, false);
    m.run_ms(80);
    assert_eq!(depth_of(&mut m), before, "K2 picks, it does not turn");
    let state = |m: &mut Machine| {
        m.soc
            .read8(fw.symbol("learned") + ilp32::midi_learn::state as u32)
    };
    assert_eq!(u32::from(state(&mut m)), MIDI_LEARN_CC);
    m.midi(&[0xB0, 14, 64]);
    m.run_ms(150);
    assert_eq!(
        common::top_view(&mut m, &fw),
        lab,
        "the frame closed over LAB"
    );
    assert_eq!(u32::from(state(&mut m)), MIDI_LEARN_IDLE);
    assert_eq!(depth_of(&mut m), from_cc(64), "CC 14 set PWM depth");
    m.midi(&[0xB3, 14, 127]);
    m.run_ms(60);
    assert_eq!(depth_of(&mut m), from_cc(127), "and the next CC follows");
    m.midi(&[0xB0, 14, 0]);
    m.run_ms(60);
    assert_eq!(depth_of(&mut m), from_cc(0));
}

fn device_song(m: &mut Machine, fw: &Firmware) -> sidou_tests::song::Song {
    let mut song = sidou_tests::song::Song::device();
    let base = fw.symbol("__song_region");
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(song.ptr_mut().cast::<u8>(), SONG_REGION_BYTES as usize)
    };
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = m.soc.read8(base + i as u32);
    }
    song
}

fn attack_decay(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc
        .read8(fw.symbol("instrument") + ilp32::live::attack_decay as u32)
}

#[test]
fn rec_in_lab_sends_to_the_playing_song_and_env_on_instr_pulls_it_back() {
    let Some(fw) = firmware("the LAB test") else {
        return;
    };
    let mut m = booted(&fw);
    open_lab(&mut m, &fw);
    press(&mut m, PLAY);
    m.run_ms(300);
    let playing = |m: &mut Machine| {
        m.soc
            .read8(fw.symbol("song_transport") + ilp32::song_ctl::isr_playing as u32)
            != 0
    };
    assert!(playing(&mut m));
    let free = unsafe { lab_song_free(device_song(&mut m, &fw).ptr()) };
    assert!(free != 0, "the built-in song has a free instrument");
    let ad = attack_decay(&mut m, &fw);
    press(&mut m, PANEL_BTN_REC as u8);
    m.run_ms(1500);
    assert!(playing(&mut m), "the song played on");
    let song = device_song(&mut m, &fw);
    let ins = song.raw().instr[free as usize - 1];
    let want_ad = if ad & 0xf0 == 0 { ad | 0x10 } else { ad };
    assert_eq!(ins[GT2_INS_AD as usize], want_ad);
    assert_eq!(ins[GT2_INS_GATETIMER as usize], 2);
    assert_ne!(ins[GT2_INS_WAVE as usize], 0);
    let tag = unsafe { lab_song_tag(free) };
    assert!(song.ext_find(tag).is_some(), "the sound kept with it");
    m.turn(SELECT, LAB_PAGE_ENVELOPE as i32);
    m.run_ms(100);
    m.turn(K1, 3);
    m.run_ms(200);
    assert_ne!(attack_decay(&mut m, &fw), ad);
    for _ in 0..4 {
        press(&mut m, SEQ);
        m.run_ms(400);
    }
    assert_eq!(
        common::top_view(&mut m, &fw),
        fw.symbol("song_pages_instr_view")
    );
    let presets = SONG_PAGES_ENC_PRESETS as u8;
    m.turn(presets, -(GT2_MAX_INSTR as i32));
    m.run_ms(turn_time(GT2_MAX_INSTR as i32) + 2 * TURN_SETTLE_MS);
    m.turn(presets, free as i32 - 1);
    m.run_ms(turn_time(free as i32) + 2 * TURN_SETTLE_MS);
    press(&mut m, ENV);
    m.run_ms(400);
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("lab_view_view"));
    assert_eq!(attack_decay(&mut m, &fw), ad, "the sound sent, exactly");
}
