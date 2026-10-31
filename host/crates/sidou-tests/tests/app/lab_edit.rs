// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::lab_sound::{random_sound, Xs};
use crate::lab_view::{lab_ui, to_page};
use crate::views::Ui;
use crate::{LiveChip, PANEL_ENC_K2};

const BYTES: usize = LAB_SOUND_BYTES as usize;
const HOST_RATE: usize = 44100;
const DC_BLOCKER_POLE: f64 = 0.995;
const C4: u8 = 60;
const C_MAJOR_LOW: [u8; 3] = [48, 52, 55];

impl LiveChip {
    fn render_ticks(&mut self, out: &mut [i16]) {
        for tick in out.chunks_mut(TICK_SAMPLES) {
            unsafe {
                live_tick(&mut *self.live);
                sid_render(&mut *self.chip, tick.as_mut_ptr(), tick.len() as u32);
            }
        }
    }

    fn take_envelopes_to_zero(&mut self) {
        unsafe {
            for p in [
                LIVE_PARAM_ATTACK,
                LIVE_PARAM_DECAY,
                LIVE_PARAM_SUSTAIN,
                LIVE_PARAM_RELEASE,
            ] {
                live_param_set(&mut *self.live, p, 0);
            }
            for n in C_MAJOR_LOW {
                live_note_on(&mut *self.live, n, PANEL_VELOCITY as u8);
            }
        }
        let mut scratch = vec![0i16; 5 * TICK_SAMPLES];
        self.render_ticks(&mut scratch);
        for n in C_MAJOR_LOW {
            unsafe { live_note_off(&mut *self.live, n) };
        }
        self.render_ticks(&mut scratch);
        assert!(
            self.chip.env.iter().all(|e| e.counter == 0),
            "the envelopes at 0"
        );
    }
}

const TICK_SAMPLES: usize = HOST_RATE / LIVE_TICK_HZ as usize;

fn dc_blocked_rms_dbfs(out: &[i16]) -> f64 {
    let (mut x1, mut y) = (0.0f64, 0.0f64);
    let mut ms = 0.0;
    for &x in out {
        let x = f64::from(x);
        y = x - x1 + DC_BLOCKER_POLE * y;
        x1 = x;
        ms += y * y;
    }
    let ms = ms / out.len() as f64;
    20.0 * (ms.sqrt().max(1e-9) / 32768.0).log10()
}

pub(crate) fn held_c4_db(s: &[u8], model: sid_model, mode: u32) -> f64 {
    let mut c = LiveChip::new(model, HOST_RATE as u32);
    c.take_envelopes_to_zero();
    unsafe {
        live_sound_apply(&mut *c.live, s.as_ptr());
        live_param_set(&mut *c.live, LIVE_PARAM_ENVELOPE_MODE, mode);
        live_note_on(&mut *c.live, C4, PANEL_VELOCITY as u8);
    }
    let ticks = LAB_SILENT_MS as usize * LIVE_TICK_HZ as usize / 1000;
    let mut out = vec![0i16; ticks * TICK_SAMPLES];
    c.render_ticks(&mut out);
    dc_blocked_rms_dbfs(&out)
}

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

const EDIT: u32 = PANEL_BTN_EDIT;

fn silent(db: f64) -> bool {
    db < f64::from(LAB_SILENT_DB)
}

fn audible(s: &[u8], model: sid_model) -> bool {
    unsafe { lab_audible(s.as_ptr(), model) != 0 }
}

#[test]
fn lab_audible_passes_no_silent_sound_and_every_built_in() {
    let mut r = Xs(env_or("LAB_SEED", 0x2468_ace1));
    let (mut passed, mut silent_refused, mut n) = (0, 0, 0);
    let mut wrong = Vec::new();
    let count: u32 = env_or("LAB_SOUNDS", 2000);
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
    assert!(
        passed * 100 >= heard * 12,
        "only {passed} of the {heard} heard passed: the predicate is merely strict"
    );
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut rig = crate::Rig::new(model);
        for p in 0..LIVE_PRESET_COUNT as u8 {
            rig.set_preset(p);
            let s = rig.sound();
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

#[test]
fn what_lab_audible_refuses_is_silent() {
    let mut base = crate::Rig::new(SID_MODEL_6581).sound();
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
    let lp = with(&|s| {
        s[LAB_SOUND_FILTER_VOICES as usize] = 7;
        s[LAB_SOUND_FILTER_MODE as usize] = 0x10;
        s[LAB_SOUND_RESONANCE as usize] = 0;
        s[LAB_SOUND_CUTOFF as usize] = 4;
        s[LAB_SOUND_CUTOFF as usize + 1] = 0;
    });
    assert!(
        !audible(&lp, SID_MODEL_8580),
        "a low-pass shut below the note"
    );
    assert!(silent(held_c4_db(&lp, SID_MODEL_8580, 0)));
    let pulse = with(&|s| {
        s[0] = 0x40;
        s[3] = 0;
        s[4] = 0;
        s[LAB_SOUND_PWM_DEPTH as usize] = 0;
        s[LAB_SOUND_PWM_DEPTH as usize + 1] = 0;
    });
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        assert!(!audible(&pulse, model), "a pulse of no width on {model:?}");
    }
    assert!(silent(held_c4_db(&pulse, SID_MODEL_8580, 0)));
}

fn edit_with(ui: &mut Ui, b: u32) -> Option<String> {
    ui.down(EDIT);
    ui.press(b);
    ui.up(EDIT);
    ui.frame();
    ui.toast()
}

fn sound(ui: &Ui) -> [u8; BYTES] {
    ui.rig.sound()
}

fn hash(s: &[u8]) -> u32 {
    unsafe { lab_edit_hash(s.as_ptr()) }
}

fn lab_on(model: sid_model) -> Ui {
    let mut ui = lab_ui();
    unsafe {
        live_param_set(&mut *ui.rig.live, LIVE_PARAM_MODEL, model);
        live_tick(&mut *ui.rig.live);
    }
    assert_eq!(ui.live().model, model);
    ui
}

#[test]
fn ten_thousand_varies_and_rolls_are_never_silent() {
    let mut r = Xs(env_or("LAB_SEED", 0x1357_9bdf));
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
        edit_with(&mut ui, PANEL_BTN_LFO);
        assert_eq!(sound(&ui), start, "{i}: undone to the start");
    }
}

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
                            "{i}: at most 3 detents"
                        );
                    }
                }
                LAB_PAGE_MOD => {
                    for at in [
                        LAB_SOUND_VIBRATO_RATE,
                        LAB_SOUND_VIBRATO_DEPTH,
                        LAB_SOUND_VIBRATO_DELAY,
                    ] {
                        let p = LIVE_PARAM_VIBRATO_RATE + (at - LAB_SOUND_VIBRATO_RATE);
                        let w = unsafe { live_walk_of(&*ui.rig.live, p) };
                        let (b, a) = (
                            u32::from(before[at as usize]),
                            u32::from(after[at as usize]),
                        );
                        assert!(
                            (-3..=3).any(|d| unsafe { live_walk_step(w, b, d) } == a),
                            "{i}: {b} -> {a} is more than 3 detents along the card's walk"
                        );
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
    assert_eq!(ui.live().fx_select, 0, "SCL with EDIT selected no slot");
    assert_eq!(ui.live().user_sound, 0, "the base stayed");
    let name = unsafe { std::ffi::CStr::from_ptr(ui.live().base_name.as_ptr()) };
    assert_eq!(name.to_str().unwrap(), "SAW LEAD");
}

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

#[test]
fn scl_alone_in_lab_selects_the_next_slot_on_release_or_on_another_control() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FX);
    let before = sound(&ui);
    let slot = ui.live().fx_select;
    ui.press(PANEL_BTN_SCL);
    assert_eq!(ui.live().fx_select, (slot + 1) % LIVE_SLOTS as u8);
    ui.down(PANEL_BTN_SCL);
    ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(
        ui.live().fx_select,
        (slot + 2) % LIVE_SLOTS as u8,
        "a knob while SCL is down selects the slot first"
    );
    ui.down(EDIT);
    ui.up(PANEL_BTN_SCL);
    ui.up(EDIT);
    ui.frame();
    assert_eq!(ui.live().fx_select, (slot + 2) % LIVE_SLOTS as u8);
    assert_ne!(
        ui.toast().as_deref(),
        Some("ROLLED"),
        "EDIT after the knob is no chord"
    );
    assert_ne!(sound(&ui), before, "the knob acted");
    assert_eq!(ui.v.lab.undo.steps, 1, "only the knob's step");
}

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

#[test]
fn lfo_landing_before_edit_is_the_same_undo() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FILTER);
    let before = sound(&ui);
    ui.turn(PANEL_ENC_K2, 3);
    assert_ne!(sound(&ui), before);
    ui.down(PANEL_BTN_LFO);
    assert_eq!(
        u32::from(ui.v.lab.page),
        LAB_PAGE_MOD,
        "LFO's page, at once"
    );
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

#[test]
fn arp_or_lfo_let_go_before_edit_is_no_chord() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FILTER);
    ui.turn(PANEL_ENC_K2, 3);
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

#[test]
fn a_silent_page_varies_nothing_and_says_so() {
    let mut ui = lab_ui();
    let no_waveform = 0;
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_WAVEFORM, no_waveform) };
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    let before = sound(&ui);
    assert_eq!(
        edit_with(&mut ui, PANEL_BTN_ARP).as_deref(),
        Some("NOTHING AUDIBLE")
    );
    assert_eq!(sound(&ui), before);
    assert_eq!(ui.v.lab.undo.steps, 0, "no step");
    assert_eq!(
        edit_with(&mut ui, PANEL_BTN_SCL).as_deref(),
        Some("ROLLED"),
        "the roll sets a waveform"
    );
}
