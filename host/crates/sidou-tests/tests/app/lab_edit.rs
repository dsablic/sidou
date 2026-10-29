// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! LAB's rolls (app/lab_edit.h, app/lab_view.h): EDIT + ARP varies the page, EDIT + SCL rolls
//! the sound, and neither is ever silent, as LAB_SILENT_MS and LAB_SILENT_DB define it, on the
//! host chip: lab_audible, the predicate the rolls keep to, against renders.

use std::ffi::c_void;

use sidou_sys::*;

use crate::lab_sound::{random_sound, Xs};
use crate::lab_view::{lab_ui, to_page};
use crate::views::Ui;

const BYTES: usize = LAB_SOUND_BYTES as usize;

/// A live instrument on a host chip (as live_instrument.rs's).
struct Chip {
    chip: Box<sid>,
    _target: Box<live_sid_target>,
    _dacs: Box<[sid_dac; 2]>,
    live: Box<live>,
}

impl Chip {
    fn new(model: sid_model) -> Self {
        let mut chip = Box::<sid>::default();
        unsafe { sid_init(&mut *chip, model, SID_CLOCK_PAL, 44100) };
        let mut dacs = Box::<[sid_dac; 2]>::default();
        let mut target = Box::new(live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: 44100,
            dacs: dacs.as_mut_ptr(),
            ..Default::default()
        });
        let mut l = Box::<live>::default();
        let ctx: *mut live_sid_target = &mut *target;
        unsafe { live_init(&mut *l, Some(live_sid_sink), ctx.cast::<c_void>(), model) };
        Chip {
            chip,
            _target: target,
            _dacs: dacs,
            live: l,
        }
    }
}

/// SILENT_MS of a held C4 at the panel's velocity on the host chip, from the note-on, with
/// sound `s` (and the envelope mode `mode`): the RMS of the chip's output through a DC blocker,
/// in dBFS.
pub(crate) fn held_c4_db(s: &[u8], model: sid_model, mode: u32) -> f64 {
    let mut c = Chip::new(model);
    let per_tick = 44100 / LIVE_TICK_HZ as usize;
    // The chip's envelopes start at reSIDfp's power-on 170: a chord with the shortest envelope,
    // let go, takes every voice's envelope to 0, as a played instrument has them.
    unsafe {
        live_param_set(&mut *c.live, LIVE_PARAM_ATTACK, 0);
        live_param_set(&mut *c.live, LIVE_PARAM_DECAY, 0);
        live_param_set(&mut *c.live, LIVE_PARAM_SUSTAIN, 0);
        live_param_set(&mut *c.live, LIVE_PARAM_RELEASE, 0);
        for n in [48, 52, 55] {
            live_note_on(&mut *c.live, n, PANEL_VELOCITY as u8);
        }
    }
    let mut scratch = vec![0i16; per_tick];
    for t in 0..10 {
        if t == 5 {
            for n in [48, 52, 55] {
                unsafe { live_note_off(&mut *c.live, n) };
            }
        }
        unsafe {
            live_tick(&mut *c.live);
            sid_render(&mut *c.chip, scratch.as_mut_ptr(), per_tick as u32);
        }
    }
    assert!(
        c.chip.env.iter().all(|e| e.counter == 0),
        "the envelopes at 0"
    );
    unsafe {
        live_sound_apply(&mut *c.live, s.as_ptr());
        live_param_set(&mut *c.live, LIVE_PARAM_ENVELOPE_MODE, mode);
        live_note_on(&mut *c.live, 60, PANEL_VELOCITY as u8);
    }
    let ticks = LAB_SILENT_MS as usize * LIVE_TICK_HZ as usize / 1000;
    let mut out = vec![0i16; ticks * per_tick];
    for t in 0..ticks {
        unsafe {
            live_tick(&mut *c.live);
            sid_render(
                &mut *c.chip,
                out[t * per_tick..].as_mut_ptr(),
                per_tick as u32,
            );
        }
    }
    // A DC blocker (a first-order high-pass at about 35 Hz): the chip's output moves its DC level
    // with the envelope (a thump at the gate, a ramp under a slow attack), which is not heard.
    let (mut x1, mut y) = (0.0f64, 0.0f64);
    let mut ms = 0.0;
    for &x in &out {
        let x = f64::from(x);
        y = x - x1 + 0.995 * y;
        x1 = x;
        ms += y * y;
    }
    let ms = ms / out.len() as f64;
    20.0 * (ms.sqrt().max(1e-9) / 32768.0).log10()
}

const EDIT: u32 = PANEL_BTN_EDIT;

fn silent(db: f64) -> bool {
    db < f64::from(LAB_SILENT_DB)
}

fn audible(s: &[u8], model: sid_model) -> bool {
    unsafe { lab_audible(s.as_ptr(), model) != 0 }
}

/// The predicate against the render on 2,000 random sounds on each model, either envelope mode:
/// no sound it passes is silent. (LAB_SOUNDS and LAB_SEED run more or others: 30,000 on each of
/// six seeds passed when the predicate was made, and so did the next test on them.) It passes every built-in, as loaded, on both models, and a
/// fair share of the random sounds that are heard (it is not merely strict).
#[test]
fn lab_audible_agrees_with_the_render() {
    let mut r = Xs(std::env::var("LAB_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0x2468_ace1));
    let (mut passed, mut silent_refused, mut n) = (0, 0, 0);
    let mut wrong = Vec::new();
    let count: u32 = std::env::var("LAB_SOUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);
    for i in 0..count {
        let s = random_sound(&mut r);
        for model in [SID_MODEL_6581, SID_MODEL_8580] {
            let db = held_c4_db(&s, model, i % 2);
            n += 1;
            if audible(&s, model) {
                passed += 1;
                if silent(db) {
                    wrong.push(format!("{db:.1} dB on {model:?}: {s:?}"));
                }
            } else if silent(db) {
                silent_refused += 1;
            }
        }
    }
    let heard = n - silent_refused;
    eprintln!("{passed} of {n} passed, of {heard} heard; {silent_refused} silent, refused");
    assert!(wrong.is_empty(), "passed but silent:\n{}", wrong.join("\n"));
    // random sounds are odd (an arpeggio, ring, sync and a combination at once): the predicate
    // keeps a margin there; on the rolls' sounds every roll finds one (the next test)
    assert!(
        passed * 100 >= heard * 12,
        "only {passed} of the {heard} heard passed"
    );
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut rig = crate::Rig::new(model);
        for p in 0..LIVE_PRESET_COUNT as u8 {
            rig.set_preset(p);
            let mut s = [0u8; BYTES];
            unsafe { live_sound_get(&*rig.live, s.as_mut_ptr()) };
            eprintln!(
                "preset {p} on {model:?}: {:.1} dB {:?}",
                held_c4_db(&s, model, 0),
                &s[..12]
            );
            assert!(audible(&s, model), "preset {p} on {model:?}");
            assert!(!silent(held_c4_db(&s, model, 0)));
        }
    }
}

/// Each of the predicate's refusals is a sound the chip makes silent: no waveform, noise with
/// another waveform, a decay to sustain 0 under 30 ms, the slowest attacks, a voice routed to no
/// mode, a high-pass far above a triangle, a low-pass shut below the note, a pulse of no width.
#[test]
fn what_lab_audible_refuses_is_silent() {
    let rig = crate::Rig::new(SID_MODEL_6581);
    let mut base = [0u8; BYTES];
    unsafe { live_sound_get(&*rig.live, base.as_mut_ptr()) };
    base[LAB_SOUND_FILTER_VOICES as usize] = 0;
    base[LAB_SOUND_VIBRATO_DEPTH as usize] = 0;
    let with = |f: &dyn Fn(&mut [u8; BYTES])| {
        let mut s = base;
        s[LAB_SOUND_AD as usize] = 0x00;
        s[LAB_SOUND_SR as usize] = 0xF0;
        f(&mut s);
        s
    };
    let cases: [(&str, [u8; BYTES]); 6] = [
        ("no waveform", with(&|s| s[0] = 0)),
        ("noise and saw", with(&|s| s[0] = 0xA0)),
        ("sustain 0, decay 0", with(&|s| s[2] = 0)),
        ("attack 15", with(&|s| s[1] = 0xF0)),
        (
            "routed to no mode",
            with(&|s| {
                s[LAB_SOUND_FILTER_VOICES as usize] = 7;
                s[LAB_SOUND_FILTER_MODE as usize] = 0;
            }),
        ),
        (
            "a high-pass at 10 kHz on a triangle",
            with(&|s| {
                s[0] = 0x10;
                s[LAB_SOUND_FILTER_VOICES as usize] = 7;
                s[LAB_SOUND_FILTER_MODE as usize] = 0x40;
                s[LAB_SOUND_RESONANCE as usize] = 0;
                s[LAB_SOUND_CUTOFF as usize] = 0xFF;
                s[LAB_SOUND_CUTOFF as usize + 1] = 0x05;
            }),
        ),
    ];
    for (what, s) in cases {
        for model in [SID_MODEL_6581, SID_MODEL_8580] {
            assert!(!audible(&s, model), "{what} on {model:?} passed");
            let db = held_c4_db(&s, model, 0);
            assert!(silent(db), "{what} on {model:?}: {db:.1} dB is not silent");
        }
    }
    // a low-pass shut below the note: the 8580's (the 6581's stops at about 240 Hz)
    let lp = with(&|s| {
        s[LAB_SOUND_FILTER_VOICES as usize] = 7;
        s[LAB_SOUND_FILTER_MODE as usize] = 0x10;
        s[LAB_SOUND_RESONANCE as usize] = 0;
        s[LAB_SOUND_CUTOFF as usize] = 4;
        s[LAB_SOUND_CUTOFF as usize + 1] = 0;
    });
    assert!(!audible(&lp, SID_MODEL_8580));
    assert!(silent(held_c4_db(&lp, SID_MODEL_8580, 0)));
    // a pulse of no width: the 8580's is silent (the 6581's leaks a little, and is refused too)
    let pulse = with(&|s| {
        s[0] = 0x40;
        s[3] = 0;
        s[4] = 0;
        s[LAB_SOUND_PWM_DEPTH as usize] = 0;
        s[LAB_SOUND_PWM_DEPTH as usize + 1] = 0;
    });
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        assert!(!audible(&pulse, model));
    }
    assert!(silent(held_c4_db(&pulse, SID_MODEL_8580, 0)));
}

/// EDIT held, `b` pressed, EDIT let go; the toast.
fn edit_with(ui: &mut Ui, b: u32) -> Option<String> {
    ui.down(EDIT);
    ui.press(b);
    ui.up(EDIT);
    ui.frame();
    ui.toast()
}

fn sound(ui: &Ui) -> [u8; BYTES] {
    let mut s = [0u8; BYTES];
    unsafe { live_sound_get(&*ui.rig.live, s.as_mut_ptr()) };
    s
}

fn hash(s: &[u8]) -> u32 {
    unsafe { lab_edit_hash(s.as_ptr()) }
}

/// LAB on `model` (its reset applied).
fn lab_on(model: sid_model) -> Ui {
    let mut ui = lab_ui();
    unsafe {
        live_param_set(&mut *ui.rig.live, LIVE_PARAM_MODEL, model);
        live_tick(&mut *ui.rig.live);
    }
    assert_eq!(ui.live().model, model);
    ui
}

/// Ten thousand varies and rolls, on every page and both models, each result held as a C4 on
/// the host chip: none is silent, none is the sound before it or the last result, and every
/// one says so.
#[test]
fn ten_thousand_varies_and_rolls_are_never_silent() {
    let mut r = Xs(std::env::var("LAB_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0x1357_9bdf));
    let mut quiet = Vec::new();
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut ui = lab_on(model);
        let mut last: Option<u32> = None;
        for i in 0..5000 {
            if i % 50 == 0 {
                to_page(&mut ui, r.below(LAB_PAGES));
            }
            let before = sound(&ui);
            let whole = r.below(4) == 0;
            let toast = edit_with(&mut ui, if whole { PANEL_BTN_SCL } else { PANEL_BTN_ARP });
            let after = sound(&ui);
            if toast.as_deref() == Some("NOTHING AUDIBLE") {
                eprintln!("{i} {whole} {before:?}");
            }
            assert_eq!(
                toast.as_deref(),
                Some(if whole { "ROLLED" } else { "VARIED" }),
                "{i} on {model:?}, page {}",
                ui.v.lab.page
            );
            assert_ne!(after, before, "{i}: the sound before it");
            assert_ne!(Some(hash(&after)), last, "{i}: the last result");
            last = Some(hash(&after));
            let db = held_c4_db(&after, model, u32::from(ui.live().envelope_mode));
            if silent(db) {
                quiet.push(format!(
                    "{i} on {model:?}, page {}: {db:.1} dB {after:?}",
                    ui.v.lab.page
                ));
            }
        }
    }
    assert!(quiet.is_empty(), "silent results:\n{}", quiet.join("\n"));
}

/// A roll, again and again from one sound (undone between): never the sound before it, never the
/// last roll's result.
#[test]
fn a_roll_never_repeats_the_sound_before_it_or_the_last_roll() {
    let mut ui = lab_ui();
    let start = sound(&ui);
    let mut last = None;
    for i in 0..300 {
        let b = if i % 2 == 0 {
            PANEL_BTN_SCL
        } else {
            PANEL_BTN_ARP
        };
        edit_with(&mut ui, b);
        let after = sound(&ui);
        assert_ne!(after, start, "{i}");
        assert_ne!(Some(after), last, "{i}");
        last = Some(after);
        edit_with(&mut ui, PANEL_BTN_LFO); // back to the start: the next roll starts there again
        assert_eq!(sound(&ui), start);
    }
}

/// A variation moves only its page's cards (and choice), each a little: 1 to 3 detents.
#[test]
fn a_vary_moves_only_its_pages_cards_a_little() {
    let b = |f: u32| f as usize;
    let pages: [(u32, Vec<usize>); 5] = [
        (
            LAB_PAGE_OSC,
            vec![
                b(LAB_SOUND_CONTROL),
                b(LAB_SOUND_PULSE),
                b(LAB_SOUND_PULSE) + 1,
                b(LAB_SOUND_PWM_DEPTH),
                b(LAB_SOUND_PWM_DEPTH) + 1,
                b(LAB_SOUND_PWM_RATE),
                b(LAB_SOUND_PWM_RATE) + 1,
                b(LAB_SOUND_TUNE),
                b(LAB_SOUND_SYNC_SEMIS),
            ],
        ),
        (LAB_PAGE_ENVELOPE, vec![b(LAB_SOUND_AD), b(LAB_SOUND_SR)]),
        (
            LAB_PAGE_FILTER,
            vec![
                b(LAB_SOUND_CUTOFF),
                b(LAB_SOUND_CUTOFF) + 1,
                b(LAB_SOUND_RESONANCE),
                b(LAB_SOUND_FILTER_VOICES),
                b(LAB_SOUND_FILTER_MODE),
                b(LAB_SOUND_FILTER_LFO),
                // the LFO's rate, PWM HZ's, on K4's name line: 1 Hz when K4 turns it on
                b(LAB_SOUND_PWM_RATE),
                b(LAB_SOUND_PWM_RATE) + 1,
            ],
        ),
        (
            LAB_PAGE_MOD,
            (b(LAB_SOUND_VIBRATO_RATE)..=b(LAB_SOUND_VIBRATO_DELAY))
                .chain([b(LAB_SOUND_VELOCITY)])
                .chain(b(LAB_SOUND_ARP)..b(LAB_SOUND_ARP) + 4)
                .collect(),
        ),
        (
            LAB_PAGE_FX,
            (b(LAB_SOUND_SLOTS)..b(LAB_SOUND_FILTER_LFO)).collect(),
        ),
    ];
    let mut ui = lab_ui();
    for (page, own) in pages {
        to_page(&mut ui, page);
        for i in 0..200 {
            let before = sound(&ui);
            edit_with(&mut ui, PANEL_BTN_ARP);
            let after = sound(&ui);
            for k in 0..BYTES {
                if !own.contains(&k) {
                    assert_eq!(after[k], before[k], "page {page} moved byte {k} ({i})");
                }
            }
            let nib = |s: &[u8; BYTES], at: u32, hi: bool| {
                let v = s[at as usize];
                i32::from(if hi { v >> 4 } else { v & 15 })
            };
            match page {
                LAB_PAGE_ENVELOPE => {
                    for (at, hi) in [
                        (LAB_SOUND_AD, true),
                        (LAB_SOUND_AD, false),
                        (LAB_SOUND_SR, true),
                        (LAB_SOUND_SR, false),
                    ] {
                        assert!(
                            (nib(&after, at, hi) - nib(&before, at, hi)).abs() <= 3,
                            "{i}"
                        );
                    }
                }
                LAB_PAGE_MOD => {
                    for at in [
                        LAB_SOUND_VIBRATO_RATE,
                        LAB_SOUND_VIBRATO_DEPTH,
                        LAB_SOUND_VIBRATO_DELAY,
                    ] {
                        let d = i32::from(after[at as usize]) - i32::from(before[at as usize]);
                        assert!(d.abs() <= 3, "{i}: {d}");
                    }
                }
                LAB_PAGE_OSC => {
                    let d = i32::from(after[LAB_SOUND_TUNE as usize] as i8)
                        - i32::from(before[LAB_SOUND_TUNE as usize] as i8);
                    assert!(d.abs() <= 3, "{i}: the tune {d}");
                }
                _ => {}
            }
        }
    }
}

/// A roll and a variation are one undo step each, back to the sound before them.
#[test]
fn a_roll_or_a_vary_undoes_in_one_step() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FILTER);
    let start = sound(&ui);
    edit_with(&mut ui, PANEL_BTN_ARP);
    let varied = sound(&ui);
    edit_with(&mut ui, PANEL_BTN_SCL);
    assert_ne!(sound(&ui), varied);
    assert_eq!(ui.v.lab.undo.steps, 2);
    assert_eq!(
        edit_with(&mut ui, PANEL_BTN_LFO).as_deref(),
        Some("UNDO (1)")
    );
    assert_eq!(sound(&ui), varied, "the roll undone");
    assert_eq!(
        edit_with(&mut ui, PANEL_BTN_LFO).as_deref(),
        Some("UNDO (0)")
    );
    assert_eq!(sound(&ui), start, "the variation undone");
    // SCL with EDIT selected no slot; the base and the name stayed
    assert_eq!(ui.live().fx_select, 0);
    assert_eq!(ui.live().user_sound, 0);
    let name = unsafe { std::ffi::CStr::from_ptr(ui.live().base_name.as_ptr()) };
    assert_eq!(name.to_str().unwrap(), "SAW LEAD");
}

/// EDIT + SCL is the roll whichever finger lands first: SCL a few milliseconds before EDIT
/// (two fingers pressed together) rolls the sound and selects no slot, as EDIT first does, and
/// letting SCL go after does nothing more.
#[test]
fn scl_landing_before_edit_is_the_same_roll() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FX);
    let before = sound(&ui);
    let slot = ui.live().fx_select;
    ui.down(PANEL_BTN_SCL);
    assert_eq!(ui.live().fx_select, slot, "it waits for EDIT");
    ui.down(EDIT);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("ROLLED"));
    assert_ne!(sound(&ui), before);
    let rolled = sound(&ui);
    ui.up(PANEL_BTN_SCL);
    ui.up(EDIT);
    ui.frame();
    assert_eq!(ui.live().fx_select, slot, "SCL selected no slot");
    assert_eq!(sound(&ui), rolled, "one roll");
    assert_eq!(ui.v.lab.undo.steps, 1);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF, "no A/B tap");
}

/// SCL alone in LAB still selects the next effect slot, once it is let go (it waits for EDIT
/// that long) or as soon as another control is used while it is down, which then acts as ever.
#[test]
fn scl_alone_in_lab_selects_the_next_slot() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FX);
    let before = sound(&ui);
    let slot = ui.live().fx_select;
    ui.press(PANEL_BTN_SCL);
    assert_eq!(ui.live().fx_select, (slot + 1) % LIVE_SLOTS as u8);
    ui.down(PANEL_BTN_SCL);
    ui.turn(PANEL_ENC_K1, 1); // a knob while it is down: the slot first, then the knob
    assert_eq!(ui.live().fx_select, (slot + 2) % LIVE_SLOTS as u8);
    ui.down(EDIT); // EDIT after the knob: no chord any more
    ui.up(PANEL_BTN_SCL);
    ui.up(EDIT);
    ui.frame();
    assert_eq!(ui.live().fx_select, (slot + 2) % LIVE_SLOTS as u8);
    assert_ne!(ui.toast().as_deref(), Some("ROLLED"));
    assert_ne!(sound(&ui), before, "the knob acted");
    assert_eq!(ui.v.lab.undo.steps, 1, "only the knob's step");
}

/// EDIT + ARP is the variation whichever finger lands first: ARP a few milliseconds before EDIT
/// varies the page as EDIT first does (one undo step, no A/B tap), and letting them go does
/// nothing more.
#[test]
fn arp_landing_before_edit_is_the_same_variation() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FILTER);
    let before = sound(&ui);
    ui.down(PANEL_BTN_ARP);
    assert_eq!(sound(&ui), before, "ARP alone changes nothing in LAB");
    ui.down(EDIT);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("VARIED"));
    assert_ne!(sound(&ui), before);
    let varied = sound(&ui);
    ui.up(PANEL_BTN_ARP);
    ui.up(EDIT);
    ui.frame();
    assert_eq!(sound(&ui), varied, "one variation");
    assert_eq!(ui.v.lab.undo.steps, 1);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF, "no A/B tap");
}

/// EDIT + LFO is undo whichever finger lands first: LFO a few milliseconds before EDIT has
/// already stepped to MOD (harmless), and EDIT then undoes the last step.
#[test]
fn lfo_landing_before_edit_is_the_same_undo() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FILTER);
    let before = sound(&ui);
    ui.turn(PANEL_ENC_K1 + 1, 3);
    assert_ne!(sound(&ui), before);
    ui.down(PANEL_BTN_LFO);
    assert_eq!(u32::from(ui.v.lab.page), LAB_PAGE_MOD, "LFO's page, at once");
    ui.down(EDIT);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("UNDO (0)"));
    assert_eq!(sound(&ui), before, "undone");
    ui.up(PANEL_BTN_LFO);
    ui.up(EDIT);
    ui.frame();
    assert_eq!(sound(&ui), before);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF, "no A/B tap");
}

/// ARP or LFO let go before EDIT lands is no chord: EDIT alone is A/B's tap, as ever.
#[test]
fn arp_or_lfo_let_go_before_edit_is_no_chord() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FILTER);
    ui.turn(PANEL_ENC_K1 + 1, 3);
    let edited = sound(&ui);
    for b in [PANEL_BTN_ARP, PANEL_BTN_LFO] {
        ui.press(b);
        ui.down(EDIT);
        ui.up(EDIT);
        ui.frame();
        assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_A, "EDIT's tap: A");
        ui.press(EDIT);
        ui.frame();
        assert_eq!(sound(&ui), edited, "B again");
        assert_eq!(ui.v.lab.undo.steps, 1, "nothing undone or varied");
    }
}

/// A sound nothing on its page can make heard: the variation leaves it and says so.
#[test]
fn a_silent_page_varies_nothing_and_says_so() {
    let mut ui = lab_ui();
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_WAVEFORM, 0) }; // no waveform
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    let before = sound(&ui);
    assert_eq!(
        edit_with(&mut ui, PANEL_BTN_ARP).as_deref(),
        Some("NOTHING AUDIBLE")
    );
    assert_eq!(sound(&ui), before);
    assert_eq!(ui.v.lab.undo.steps, 0, "no step");
    // the roll sets a waveform
    assert_eq!(edit_with(&mut ui, PANEL_BTN_SCL).as_deref(), Some("ROLLED"));
}
