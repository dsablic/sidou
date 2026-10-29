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
/// The worst ui_poll that composes LAB's frame while a key sounds and a card turns every frame
/// (the canvas and the drawing by turns, app/lab_view.h). Measured at Task 6: ENVELOPE 74.7k (its
/// drawing about 45k), OSC 61.3k, a page without a drawing 44.9k; the bound keeps room under the
/// poll allowance (common::POLL_INSTRUCTIONS, 120k) for the inputs and MIDI.
const LAB_WORST_POLL: u64 = 90_000;
const _: () = assert!(LAB_WORST_POLL <= common::POLL_INSTRUCTIONS);

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

/// LIVE's one change: ENV, LFO and FX open LAB over LIVE at ENVELOPE, MOD and FX (the press and
/// the busiest pass after it within the poll allowance), and HOME goes back to LIVE; neither
/// toggles the chip model nor steps a slot's algorithm any more.
#[test]
fn env_lfo_and_fx_on_live_open_lab_at_their_pages() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
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
        let (i, p) = start_cost(&mut m, 300, |m| tap(m, b as u8));
        println!("LIVE {what}: the input {i}, the busiest pass {p}");
        assert!(
            i + p <= common::POLL_INSTRUCTIONS,
            "{what}: {i} + {p} instructions"
        );
        wait_for(&mut m, what, |m| {
            common::top_view(m, &fw) == fw.symbol("lab_view_view")
                && !common::screen_animating(m, &fw)
        });
        assert_eq!(lab_page(&mut m, &fw), page as u8, "{what}");
        assert_eq!(depth(&mut m, &fw), 2, "{what}: LAB over LIVE");
        assert_eq!(
            m.soc.read8(l + ilp32::live::model as u32),
            model,
            "{what}: the model"
        );
        assert_eq!(m.soc.read8(slot), 0, "{what}: the slot's algorithm");
        tap(&mut m, HOME);
        wait_for(&mut m, "LIVE", |m| {
            common::top_view(m, &fw) == fw.symbol("live_view_view")
                && !common::screen_animating(m, &fw)
        });
    }
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

/// A page's frames, a key held (the envelope running, the PWM moving) and a card turned every
/// frame: the most cells a frame sent and the worst ui_poll.
fn page_frames(m: &mut Machine, fw: &Firmware, page: u32, frames_n: u64) -> (u32, u64) {
    m.turn(SELECT, -(LAB_PAGES as i32));
    m.run_ms(60);
    m.turn(SELECT, page as i32);
    m.run_ms(200);
    m.key(KEY0 + 12, true);
    let (cells, poll) = frames(m, fw, frames_n, |m, f| {
        m.turn(K1 + (f % 4) as u8, if f % 8 < 4 { 1 } else { -1 });
        if f % 20 == 10 {
            m.key(KEY0 + 12, false); // the release runs, then a new note
        } else if f % 20 == 15 {
            m.key(KEY0 + 12, true);
        }
    });
    m.key(KEY0 + 12, false);
    (cells.iter().copied().max().unwrap_or(0), poll)
}

/// OSC (on SAW LEAD, and on PWM BASS with its LFO moving the edge) and ENVELOPE (its dot moving
/// as the note runs): every frame within LAB_VIEW_FRAME_CELLS, every ui_poll within
/// LAB_WORST_POLL.
#[test]
fn lab_osc_and_envelope_draw_within_their_budgets() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
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
            m.turn(PANEL_ENC_PRESETS as u8, 1);
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

/// FILTER (each mode, the cutoff turned every frame), MOD (the vibrato, PWM BASS's PWM and ARP's
/// staircase) and FX (the distortion's curve, the modulation's wave, the delay's taps): every
/// frame within LAB_VIEW_FRAME_CELLS, every ui_poll within LAB_WORST_POLL.
#[test]
fn lab_filter_mod_and_fx_draw_within_their_budgets() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
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
    // FILTER: LP, then NOTCH (the response's two ends and its peak)
    run(&mut m, "FILTER, LP", LAB_PAGE_FILTER);
    m.turn(ALGORITHM, 3);
    m.run_ms(100);
    run(&mut m, "FILTER, NOTCH", LAB_PAGE_FILTER);
    // MOD: the vibrato and the PWM on, then ARP up-down
    for _ in 0..3 {
        m.turn(K1 + 1, 40);
    }
    run(&mut m, "MOD, vibrato", LAB_PAGE_MOD);
    m.turn(ALGORITHM, 3);
    m.run_ms(100);
    run(&mut m, "MOD, ARP", LAB_PAGE_MOD);
    // FX: OVERDRIVE on the first slot, then CHORUS, then DELAY
    m.turn(SELECT, -(LAB_PAGES as i32));
    m.run_ms(60);
    m.turn(SELECT, LAB_PAGE_FX as i32);
    m.run_ms(100);
    m.turn(ALGORITHM, 2);
    m.run_ms(100);
    run(&mut m, "FX, OVERDRIVE", LAB_PAGE_FX);
    tap(&mut m, PANEL_BTN_SCL as u8);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    run(&mut m, "FX, CHORUS", LAB_PAGE_FX);
    tap(&mut m, PANEL_BTN_SCL as u8);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    run(&mut m, "FX, DELAY", LAB_PAGE_FX);
    assert!(over.is_empty(), "{over:#?}");
}

/// The screen's toast text (app/screen.h).
fn toast_text(m: &mut Machine, fw: &Firmware) -> String {
    let at = fw.symbol("ui") + (ilp32::views::s + ilp32::screen::toast) as u32;
    (0..SCREEN_TOAST_CHARS)
        .map(|i| m.soc.read8(at + i))
        .take_while(|&b| b != 0)
        .map(char::from)
        .collect()
}

/// LIVE, the menu's LAB, ENVELOPE, two turns, SAVE: one press keeps the sound in USER 1 as `LAB
/// 01` with those values, and the toast says so once the store has it.
#[test]
fn a_happy_accident_is_kept_with_one_press() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_lab(&mut m, &fw);
    tap(&mut m, PANEL_BTN_ENV as u8);
    assert_eq!(lab_page(&mut m, &fw), LAB_PAGE_ENVELOPE as u8);
    m.turn(K1, 2); // the attack 0 -> 2
    m.turn(K1 + 3, -3); // the release
    m.run_ms(100);
    let release = m.soc.read8(live_at(&fw, ilp32::live::sustain_release)) & 15;
    tap(&mut m, PANEL_BTN_SAVE as u8);
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

/// EDIT + SAVE opens the name page; the keys type, K1 chooses the slot, SAVE writes: USER 5 holds
/// `HI` and the sound as it was; the keys made no sound.
#[test]
fn edit_save_names_and_writes_a_slot() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_lab(&mut m, &fw);
    tap(&mut m, PANEL_BTN_ENV as u8);
    m.turn(K1, 3); // the attack
    m.run_ms(100);
    m.key(PANEL_BTN_EDIT as u8, true);
    m.run_ms(30);
    tap(&mut m, PANEL_BTN_SAVE as u8);
    m.key(PANEL_BTN_EDIT as u8, false);
    m.run_ms(100);
    assert_eq!(
        u32::from(lab_byte(&mut m, &fw, ilp32::lab_view::mode)),
        LAB_VIEW_MODE_NAME,
        "the name page"
    );
    assert_eq!(lab_byte(&mut m, &fw, ilp32::lab_view::slot), 0);
    m.turn(K1, 4); // USER 5
    m.run_ms(60);
    for key in [7u8, 8u8] {
        // H, I
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
    tap(&mut m, PANEL_BTN_SAVE as u8);
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

/// The name page's frames: a character or the slot changed every frame, and a typing key, within
/// LAB_VIEW_FRAME_CELLS and LAB_WORST_POLL; and its inputs within the poll allowance.
#[test]
fn the_name_page_stays_within_the_frame_budget() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_lab(&mut m, &fw);
    m.key(PANEL_BTN_EDIT as u8, true);
    m.run_ms(30);
    tap(&mut m, PANEL_BTN_SAVE as u8);
    m.key(PANEL_BTN_EDIT as u8, false);
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

/// PRESETS past DICE reaches a kept sound on LIVE and in LAB, and back.
#[test]
fn presets_reaches_the_user_sounds_on_live_and_in_lab() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_lab(&mut m, &fw);
    tap(&mut m, PANEL_BTN_ENV as u8);
    m.turn(K1, 5); // the attack
    m.run_ms(100);
    tap(&mut m, PANEL_BTN_SAVE as u8);
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
    let presets = PANEL_ENC_PRESETS as u8;
    // in LAB: back to DICE, on again to USER 1
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
    // on LIVE
    tap(&mut m, HOME);
    wait_for(&mut m, "LIVE", |m| {
        common::top_view(m, &fw) == fw.symbol("live_view_view") && !common::screen_animating(m, &fw)
    });
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

/// The instrument's cutoff (live.cutoff, the sound's, before the wheel or an LFO).
fn cutoff(m: &mut Machine, fw: &Firmware) -> u16 {
    let a = live_at(fw, ilp32::live::cutoff);
    u16::from(m.soc.read8(a)) | u16::from(m.soc.read8(a + 1)) << 8
}

/// EDIT tapped in LAB swaps the sound heard: a held note with the cutoff turned well down (B)
/// against the sound as loaded (A), each tap within the poll allowance; the swap is heard (the
/// share of the power above 2 kHz) and is the instrument's (its cutoff), both ways and back.
#[test]
fn edit_tapped_swaps_the_sound_heard() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_lab(&mut m, &fw);
    let a = cutoff(&mut m, &fw);
    m.turn(SELECT, LAB_PAGE_FILTER as i32);
    m.run_ms(100);
    m.turn(K1, -30);
    m.key(KEY0 + 12, true);
    m.run_ms(600);
    let bright = |x: &[f32]| common::power_above(x, 2000.0);
    let heard_b = bright(&common::record(&mut m, 200));
    let b = cutoff(&mut m, &fw);
    assert!(b < a / 2, "the cutoff well down: {a} -> {b}");
    m.watch_calls(fw.symbol("ui_buttons"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let edit = PANEL_BTN_EDIT as u8;
    let (input, pass) = start_cost(&mut m, 100, |m| tap(m, edit));
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
    let (input, pass) = start_cost(&mut m, 100, |m| tap(m, edit));
    println!("EDIT's tap back: the input {input}, the busiest pass {pass}");
    assert!(
        input + pass <= common::POLL_INSTRUCTIONS,
        "{input} + {pass}"
    );
    assert_eq!(cutoff(&mut m, &fw), b, "B: the edit again");
    let heard_b2 = bright(&common::record(&mut m, 200));
    m.key(KEY0 + 12, false);
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

/// EDIT held, `b` tapped, EDIT let go.
fn edit_with(m: &mut Machine, b: u32) {
    m.key(PANEL_BTN_EDIT as u8, true);
    m.run_ms(30);
    tap(m, b as u8);
    m.key(PANEL_BTN_EDIT as u8, false);
    m.run_ms(60);
}

/// EDIT + LFO in LAB undoes a step at a time, each within the poll allowance, and says so; a run
/// of detents is one step.
#[test]
fn edit_lfo_undoes_in_lab() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_lab(&mut m, &fw);
    tap(&mut m, PANEL_BTN_ENV as u8);
    let ad = |m: &mut Machine| m.soc.read8(live_at(&fw, ilp32::live::attack_decay));
    let start = ad(&mut m);
    for _ in 0..3 {
        m.turn(K1, 1); // the attack: a run
        m.run_ms(60);
    }
    m.turn(K1 + 1, 2); // the decay
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

/// The instrument's voice as LAB's rolls change it: the control, the envelope, the pulse width,
/// the filter, the vibrato, the velocity target, the arpeggio, the PWM and the tune.
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

/// The device's output (the left channel) through the host test's DC blocker, in dBFS of the
/// chips' own output: the MASTER gain at the pot's power-on reading divided out
/// (common::boot_gain), so it reads as LAB_SILENT_DB's level does (the chip alone; the effect
/// slots are off on the pages this test varies).
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

/// EDIT + ARP varies the page and EDIT + SCL rolls the sound in LAB, each within the poll
/// allowance: every result, a C4 held LAB_SILENT_MS from its note-on, is heard on the device
/// above LAB_SILENT_DB (the output's RMS through a DC blocker); each toasts, and EDIT + LFO
/// undoes them one at a time, each back to the sound before it.
#[test]
fn edit_arp_and_edit_scl_change_the_sound_heard_and_undo() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
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
    // the undo, a step at a time, back through the results it holds (its 16 steps share
    // LAB_EDIT_ENTRIES bytes: a roll takes about a dozen, so the oldest went)
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

/// EDIT + SCL is the roll whichever finger lands first: SCL 5 ms before EDIT on LAB's FX page
/// (two fingers pressing together) rolls the sound and selects no effect slot; SCL alone selects
/// the next one at its release.
#[test]
fn scl_before_edit_rolls_in_lab_too() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_lab(&mut m, &fw);
    tap(&mut m, PANEL_BTN_FX as u8);
    let fx = live_at(&fw, ilp32::live::fx_select);
    let slot = m.soc.read8(fx);
    let before = voice_fields(&mut m, &fw);
    m.key(PANEL_BTN_SCL as u8, true);
    m.run_ms(5);
    m.key(PANEL_BTN_EDIT as u8, true);
    m.run_ms(60);
    m.key(PANEL_BTN_SCL as u8, false);
    m.key(PANEL_BTN_EDIT as u8, false);
    m.run_ms(200);
    assert_eq!(toast_text(&mut m, &fw), "ROLLED");
    assert_ne!(voice_fields(&mut m, &fw), before, "rolled");
    assert_eq!(m.soc.read8(fx), slot, "SCL selected no slot");
    tap(&mut m, PANEL_BTN_SCL as u8);
    m.run_ms(60);
    assert_ne!(m.soc.read8(fx), slot, "SCL alone: the next slot");
}

/// MIDI learn on a LAB card, over TRS: SAVE held + K2 on OSC picks PWM depth (which has an NRPN
/// and no CC), CC 14 binds to it and sets it through the map's curve, and the next CC follows;
/// learn's frame closes over LAB and leaves it on top.
#[test]
fn midi_learn_on_a_lab_card_over_trs() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    open_lab(&mut m, &fw);
    let lab = fw.symbol("lab_view_view");
    let learn = fw.symbol("learn_view_view");
    let depth_of = |m: &mut Machine| {
        let a = live_at(&fw, ilp32::live::pwm_depth);
        u32::from(m.soc.read8(a)) | u32::from(m.soc.read8(a + 1)) << 8
    };
    let before = depth_of(&mut m);
    let row = unsafe { midi_map_for_param(LIVE_PARAM_PWM_DEPTH) };
    let from_cc = |v: u8| unsafe { midi_map_from_cc(row, v) };

    m.key(PANEL_BTN_SAVE as u8, true);
    m.run_ms(30);
    m.turn(K1 + 1, 1);
    m.run_ms(80);
    assert_eq!(
        common::top_view(&mut m, &fw),
        learn,
        "SAVE + K2 opened learn"
    );
    m.key(PANEL_BTN_SAVE as u8, false);
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
