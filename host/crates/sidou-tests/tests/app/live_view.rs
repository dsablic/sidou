// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! LIVE, the root view (`app/live_view.c`): the tap only while it shows, the voices' scopes
//! from a chip playing what `live` wrote, the envelope bars, the meters, the filter curve and
//! the slots, and holding still under a transition.

use sidou_sys::*;

use crate::views::{open_status, Ui};
use crate::{drain, Sent};

const COLS: usize = GRID_COLS as usize;
const BLOCK: u32 = 128;

/// A chip that plays what the Ui's instrument writes: the recorder's register file applied
/// before each block, as the device's drain applies the ring.
struct Chip(Box<sid>);

impl Chip {
    fn new() -> Self {
        let mut s = Box::<sid>::default();
        unsafe { sid_init(&mut *s, SID_MODEL_6581, SID_CLOCK_PAL, 44_118) };
        Chip(s)
    }

    /// `blocks` blocks through the tap, then a frame, sent.
    fn play(&mut self, ui: &mut Ui, blocks: u32) -> Sent {
        let mut out = vec![0i16; BLOCK as usize];
        for _ in 0..blocks {
            for reg in 0..0x19u8 {
                unsafe { sid_write(&mut *self.0, reg, ui.rig.reg(reg)) };
            }
            unsafe { scope_render(&mut *ui.sc, &mut *self.0, out.as_mut_ptr(), BLOCK) };
        }
        frame(ui)
    }
}

/// A frame, and its cells sent as the device sends them: LIVE holds itself to a budget of
/// what the panel has yet to get.
fn frame(ui: &mut Ui) -> Sent {
    ui.frame();
    drain(&mut ui.g)
}

/// Past any transition, everything sent.
fn settle(ui: &mut Ui) {
    ui.settle();
    drain(&mut ui.g);
}

/// A voice's lane: its first row.
fn lane(v: usize) -> usize {
    LIVE_VIEW_LANE_ROW as usize + LIVE_VIEW_LANE_ROWS as usize * v
}

/// The scope's first cell column.
const SCOPE: usize = LIVE_VIEW_SCOPE_COL as usize;

/// `n` cells of a row from `col`, as ordinary text (a large character's piece shows the glyph
/// it is a piece of).
fn small(ui: &Ui, row: usize, col: usize, n: usize) -> String {
    (col..col + n)
        .map(|c| {
            let i = row * COLS + c;
            let b = if usize::from(ui.g.ch[i]).wrapping_sub(GRID_BIG_FIRST as usize)
                < GRID_BIG_PIECES as usize
            {
                ui.g.src[i]
            } else {
                ui.g.ch[i]
            };
            if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                '#'
            }
        })
        .collect()
}

/// `n` characters of 2x text from (`row`, `col`): one per two cells.
fn big(ui: &Ui, row: usize, col: usize, n: usize) -> String {
    (0..n)
        .map(|i| {
            let k = row * COLS + col + 2 * i;
            assert_eq!(
                usize::from(ui.g.ch[k]),
                GRID_BIG_FIRST as usize,
                "a large character's top left piece at ({row}, {})",
                col + 2 * i
            );
            ui.g.src[k] as char
        })
        .collect()
}

fn theme_of(ui: &Ui) -> &'static theme {
    unsafe { &*theme_get(u32::from(ui.live().theme)) }
}

/// The overlay masks of voice v's scope cells (4 rows of 20), empty where none.
fn scope_masks(ui: &Ui, v: usize) -> Vec<[u8; 8]> {
    let top = lane(v);
    let mut out = Vec::new();
    for r in top..top + LIVE_VIEW_SCOPE_ROWS as usize {
        for c in SCOPE..SCOPE + LIVE_VIEW_SCOPE_COLS as usize {
            let o = ui.g.ovl[r * COLS + c];
            out.push(if o == 0 {
                [0; 8]
            } else {
                ui.g.ovl_mask[usize::from(o - 1)]
            });
        }
    }
    out
}

/// The scope's pixel rows any trace pixel is on.
fn trace_rows(masks: &[[u8; 8]]) -> std::collections::BTreeSet<usize> {
    let cols = LIVE_VIEW_SCOPE_COLS as usize;
    let mut rows = std::collections::BTreeSet::new();
    for (k, m) in masks.iter().enumerate() {
        for (y, &bits) in m.iter().enumerate() {
            if bits != 0 {
                rows.insert(k / cols * 8 + y);
            }
        }
    }
    rows
}

#[test]
fn live_is_the_root_and_the_tap_runs_only_while_it_shows() {
    let mut ui = Ui::without_cards();
    assert_eq!(ui.top(), "LIVE");
    assert_eq!(ui.sc.on, 1, "the tap is off under LIVE");
    ui.press(PANEL_BTN_HOME);
    ui.frame();
    assert_eq!(ui.sc.on, 0, "the tap runs under the menu");
    ui.press(PANEL_BTN_HOME);
    ui.sc.head = 1234; // points written meanwhile
    ui.frame();
    assert_eq!(ui.sc.on, 1);
    assert_eq!(ui.v.live.since, 1234, "older points are stale");
    open_status(&mut ui);
    assert_eq!(ui.sc.on, 0, "the tap runs under the status screen");
}

#[test]
fn the_header_lanes_knobs_and_slots_show_the_instrument() {
    let mut ui = Ui::new();
    ui.settle();
    let t = theme_of(&ui);
    // The preset's name at 2x top left; the octave only when it is not 0.
    assert_eq!(big(&ui, 0, 0, 8), "SAW LEAD");
    assert_eq!(
        small(&ui, 0, 22, 8),
        "        ",
        "no octave at 0, no chip model here"
    );
    assert_eq!(small(&ui, 1, 22, 8), "        ");
    assert_eq!(ui.g.attr[0] & 15, t.value, "the name in the value colour");
    assert_eq!(u32::from(ui.g.attr[0] >> 4), GRID_BLACK, "on black");
    for v in 0..3 {
        let row = lane(v);
        // The voice's number at 2x in its colour, its waveform under it; the note at 2x on the
        // right and the stage under it, at rest.
        assert_eq!(big(&ui, row + 1, 0, 1), format!("{}", v + 1));
        assert_eq!(ui.g.attr[(row + 1) * COLS] & 15, t.voice[v]);
        assert_eq!(small(&ui, row + 3, 0, 3), "SAW");
        assert_eq!(big(&ui, row + 1, 24, 3), "---");
        assert_eq!(small(&ui, row + 3, 24, 3), "---", "the envelope at rest");
        // The scope's centre line: a dotted glyph under the overlay.
        assert_eq!(ui.g.ch[(row + 2) * COLS + 5], FONT_SCOPE_MID as u8);
    }
    // The knob row in K1..K4's order: the page, label, value, bar, name.
    assert_eq!(small(&ui, 14, 0, 4), "TONE");
    assert_eq!(ui.g.attr[14 * COLS] & 15, t.hot);
    for (k, (label, col)) in [("CUTOFF", 0), ("RESO", 8), ("DECAY", 15), ("RELEASE", 22)]
        .into_iter()
        .enumerate()
    {
        assert_eq!(small(&ui, 15, col, label.len()), label);
        assert_eq!(
            ui.g.attr[15 * COLS + col] & 15,
            t.knob_text[k],
            "{label}'s readable colour"
        );
        assert_eq!(
            ui.g.attr[18 * COLS + col] & 15,
            t.knob[k],
            "{label}'s bar's colour"
        );
        assert_eq!(small(&ui, 19, col, 2), format!("K{}", k + 1));
        assert_eq!(
            ui.g.attr[19 * COLS + col] & 15,
            t.hint,
            "the name's legend colour"
        );
        assert_eq!(
            ui.g.attr[16 * COLS + col] & 15,
            t.value,
            "the value in white"
        );
    }
    assert_eq!(big(&ui, 16, 8, 1), "6", "the resonance");
    assert_eq!(
        big(&ui, 16, 15, 1),
        "9",
        "the decay: SAW LEAD's own control"
    );
    assert_eq!(big(&ui, 16, 22, 1), "6", "the release");
    // The bottom band: the filter's mode, the chip and its profile, the slots (the selected one
    // reversed), the selected slot's algorithm at 2x, VU's labels.
    assert_eq!(small(&ui, 21, 0, 6), "FILTER");
    assert_eq!(small(&ui, 21, 11, 9), "CHIP 6581");
    assert_eq!(
        small(&ui, 21, 21, 7),
        "CLASSIC",
        "what R4AR means, in a word"
    );
    assert_eq!(big(&ui, 22, 0, 2), "LP");
    assert_eq!(big(&ui, 22, 11, 4), "R4AR");
    assert_eq!(small(&ui, 24, 0, 4), "DIST");
    assert_eq!(small(&ui, 24, 5, 3), "MOD");
    assert_eq!(small(&ui, 24, 9, 2), "FX");
    assert_eq!(
        ui.g.attr[24 * COLS],
        (t.hot << 4) | t.bg,
        "the selected slot reversed"
    );
    assert_eq!(
        ui.g.attr[24 * COLS + 5],
        (t.bg << 4) | t.dim,
        "a slot that is off, dim"
    );
    assert_eq!(big(&ui, 25, 0, 3), "OFF");
    assert_eq!(small(&ui, 22, 20, 1), "1");
    assert_eq!(small(&ui, 24, 20, 1), "3");
    assert_eq!(small(&ui, 25, 20, 1), "L");
    assert_eq!(small(&ui, 26, 20, 1), "R");
    assert_eq!(
        ui.g.attr[22 * COLS + 20] & 15,
        t.hint,
        "VU labels in the legend colour"
    );
    ui.press(PANEL_BTN_SCL);
    unsafe { live_slot_next_algorithm(&mut *ui.rig.live, 1) };
    ui.rig.set_preset(1);
    ui.press(PANEL_BTN_OCT_UP);
    ui.settle();
    assert_eq!(ui.g.attr[24 * COLS + 5], (t.hot << 4) | t.bg);
    assert_eq!(big(&ui, 25, 0, 6), "CHORUS");
    assert_eq!(small(&ui, lane(0) + 3, 0, 3), "PUL");
    assert_eq!(big(&ui, 0, 0, 8), "PWM BASS");
    assert_eq!(small(&ui, 15, 15, 5), "PULSE", "PWM BASS's own control");
    assert_eq!(big(&ui, 16, 15, 3), "25%");
    assert_eq!(big(&ui, 16, 22, 1), "4", "its release");
    assert_eq!(big(&ui, 0, 26, 2), "+1", "the octave at 2x");
    assert_eq!(ui.g.attr[26] & 15, t.hot);
}

#[test]
fn the_knob_row_shows_what_k1_to_k4_turn_and_nothing_else() {
    let mut ui = Ui::new();
    ui.settle();
    let hz =
        |ui: &Ui| unsafe { sid_cutoff_hz(ui.live().model, 0, u32::from(ui.live().cutoff_out)) };
    let want = |hz: f32| {
        let hz = (hz.max(30.0) + 0.5) as u32;
        if hz < 1000 {
            format!("{hz}")
        } else if hz < 9950 {
            format!("{}K{}", (hz + 50) / 100 / 10, (hz + 50) / 100 % 10)
        } else {
            format!("{}K", (hz + 500) / 1000)
        }
    };
    // K1 is the cutoff: the value follows it down and up, as the popup says it in Hz.
    for steps in [-70, -30, 20, 60, -200, 200] {
        ui.turn(PANEL_ENC_K1, steps);
        ui.settle();
        let text = want(hz(&ui));
        assert!(text.len() <= 3, "{text}");
        assert_eq!(big(&ui, 16, 0, text.len()), text, "after {steps}");
        assert_eq!(small(&ui, 16, 2 * text.len(), 0), "");
    }
    // K2 the resonance, 0..15.
    for steps in [-30, 1, 4, 30, -1] {
        ui.turn(PANEL_ENC_K1 + 1, steps);
        ui.settle();
        let r = ui.live().resonance;
        assert_eq!(big(&ui, 16, 8, r.to_string().len()), r.to_string());
    }
    // K3 the sound's own control (the decay on SAW LEAD), K4 the release, 0..15.
    for steps in [-30, 2, 5, 30, -1] {
        ui.turn(PANEL_ENC_K1 + 2, steps);
        ui.settle();
        let d = (ui.live().attack_decay & 15).to_string();
        assert_eq!(small(&ui, 15, 15, 5), "DECAY");
        assert_eq!(big(&ui, 16, 15, d.len()), d);
        ui.turn(PANEL_ENC_K1 + 3, steps);
        ui.settle();
        let r = (ui.live().sustain_release & 15).to_string();
        assert_eq!(big(&ui, 16, 22, r.len()), r);
    }
    // PRESETS steps the sound: the header says it, and K3 changes with it (the pulse width on
    // PWM BASS, in percent of one side, the ratio's semitones on SYNC LEAD).
    ui.turn(PANEL_ENC_PRESETS, 1);
    ui.settle();
    assert_eq!(ui.live().preset, 1);
    assert_eq!(big(&ui, 0, 0, 8), "PWM BASS");
    ui.turn(PANEL_ENC_K1 + 2, -9);
    ui.settle();
    let pc = (u32::from(ui.live().pulse_width) * 99 + 2047) / 4095;
    assert_eq!(small(&ui, 15, 15, 5), "PULSE");
    assert_eq!(big(&ui, 16, 15, 3 - usize::from(pc < 10)), format!("{pc}%"));
    ui.turn(PANEL_ENC_PRESETS, 1);
    ui.turn(PANEL_ENC_K1 + 2, -3);
    ui.settle();
    assert_eq!(small(&ui, 15, 15, 5), "RATIO");
    assert_eq!(big(&ui, 16, 15, 2), "-3");
    // A knob's bar is its share of the range, in eighths of cells: K1 at the top is full.
    ui.rig.live.cutoff_out = LIVE_CUTOFF_MAX as u16;
    ui.frame();
    let bar: Vec<u8> = (0..7).map(|c| ui.g.ch[18 * COLS + c]).collect();
    assert!(bar.iter().all(|&c| c == FONT_BAR8 as u8), "{bar:?}");
    ui.rig.live.cutoff_out = 0;
    ui.frame();
    let bar: Vec<u8> = (0..7).map(|c| ui.g.ch[18 * COLS + c]).collect();
    assert!(bar.iter().all(|&c| c == FONT_BAR0 as u8), "{bar:?}");
}

#[test]
fn every_knob_value_fits_its_column() {
    let mut ui = Ui::new();
    let text = |ui: &Ui, k: u32| {
        let mut buf = [0 as std::ffi::c_char; 5];
        unsafe { live_view_knob_text(ui.live(), k, buf.as_mut_ptr()) };
        unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    };
    // K1: three characters at 2x (6 cells of a column of 8, a blank after them), whatever the
    // cutoff, on both chips.
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        ui.rig.live.model = model;
        for c in 0..=LIVE_CUTOFF_MAX {
            ui.rig.live.cutoff_out = c as u16;
            let t = text(&ui, 0);
            assert!(
                !t.is_empty() && t.len() <= 3,
                "model {model}, cutoff {c}: {t}"
            );
        }
    }
    ui.rig.live.model = SID_MODEL_6581;
    ui.rig.live.cutoff_out = 0;
    let floor = unsafe { sid_cutoff_hz(ui.live().model, 0, 0) };
    assert_eq!(
        text(&ui, 0),
        format!("{}", (floor.max(30.0) + 0.5) as u32),
        "the filter's floor"
    );
    // K2 holds 2 characters, K3 3 in its 7 cells (the pulse width as one side's percent, the
    // sync ratio's semitones, the decay), K4 the release 0..15.
    for r in 0..=LIVE_RESONANCE_MAX {
        unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_RESONANCE, r) };
        assert_eq!(text(&ui, 1), r.to_string());
    }
    ui.rig.set_preset(1); // PWM BASS: K3 is the pulse width, either side of the square
    for pw in 0..=LIVE_PULSE_WIDTH_MAX {
        ui.rig.live.pulse_width = pw as u16;
        let t = text(&ui, 2);
        assert!(t.ends_with('%') && t.len() <= 3, "{pw}: {t}");
    }
    ui.rig.live.pulse_width = 2048;
    assert_eq!(text(&ui, 2), "50%");
    ui.rig.live.pulse_width = 128;
    assert_eq!(text(&ui, 2), "3%");
    ui.rig.set_preset(2); // SYNC LEAD: the ratio's semitones, -12..+12
    for s in -(LIVE_RATIO_SEMIS as i8)..=LIVE_RATIO_SEMIS as i8 {
        ui.rig.live.sync_semis = s;
        let t = text(&ui, 2);
        assert!(t.len() <= 3, "{s}: {t}");
        assert_eq!(t, format!("{s:+}"));
    }
    ui.rig.set_preset(0); // SAW LEAD: the decay
    for d in 0..=15u8 {
        ui.rig.live.attack_decay = d;
        ui.rig.live.sustain_release = d;
        assert_eq!(text(&ui, 2), d.to_string());
        assert_eq!(text(&ui, 3), d.to_string());
    }
    // The slot page's four controls, 0..127, are numbers of at most 3 digits in their columns.
    ui.v.live.page = 1;
    for v in [0u8, 7, 9, 10, 99, 100, 127] {
        for f in 0..4 {
            ui.rig.live.slot[0][LIVE_SLOT_AMOUNT as usize + f] = v;
        }
        ui.frame();
        for (k, col) in [0usize, 8, 15, 22].into_iter().enumerate() {
            assert_eq!(
                big(&ui, 16, col, v.to_string().len()),
                v.to_string(),
                "K{}",
                k + 1
            );
        }
    }
}

#[test]
fn a_note_draws_a_trace_and_silence_a_flat_line() {
    let mut ui = Ui::new();
    let mut chip = Chip::new();
    settle(&mut ui);
    // The chip's power-on envelope runs down first: silent is an envelope of nothing.
    for _ in 0..100 {
        chip.play(&mut ui, 100);
        if ui.v.live.env == [0; 3] {
            break;
        }
    }
    assert_eq!(ui.v.live.env, [0; 3]);
    chip.play(&mut ui, 20);
    // Flat, 2 pixels thick, at rest, in dark grey.
    let mid = (LIVE_VIEW_SCOPE_PX / 2) as usize;
    let t = theme_of(&ui);
    for v in 0..3 {
        let rows = trace_rows(&scope_masks(&ui, v));
        assert_eq!(
            rows.into_iter().collect::<Vec<_>>(),
            [mid, mid + 1],
            "voice {v} at rest"
        );
        let o = ui.g.ovl[(lane(v) + 2) * COLS + SCOPE + 1];
        assert_ne!(o, 0);
        assert_eq!(
            ui.g.ovl_color[usize::from(o - 1)],
            t.dim,
            "voice {v}: silent is dark grey"
        );
    }
    ui.rig.note_on(69);
    chip.play(&mut ui, 30);
    let rows = trace_rows(&scope_masks(&ui, 0));
    let span = rows.last().unwrap() - rows.first().unwrap();
    assert!(
        span >= 20,
        "voice 1 with A4: the trace spans {span} pixel rows ({rows:?})"
    );
    assert_eq!(
        trace_rows(&scope_masks(&ui, 1))
            .into_iter()
            .collect::<Vec<_>>(),
        [mid, mid + 1],
        "voice 2 is silent"
    );
    // The trace is the saw: three periods for A4, so three falling edges across the width.
    let masks = scope_masks(&ui, 0);
    let full_columns = (0..LIVE_VIEW_SCOPE_COLS as usize)
        .filter(|&c| (0..4).all(|r| masks[r * LIVE_VIEW_SCOPE_COLS as usize + c] != [0; 8]))
        .count();
    assert!(
        (3..=6).contains(&full_columns),
        "{full_columns} cell columns cross the whole scope"
    );
    // Its colour is voice 1's (and the silent voice 2's stays dark grey); it is 2 pixels thick:
    // every column has two pixels on at least, a cell column crossing it from edge to edge.
    let o = ui.g.ovl[(lane(0) + 2) * COLS + SCOPE + 1]; // on the centre line, crossed by any trace
    assert_eq!(ui.g.ovl_color[usize::from(o - 1)], theme_of(&ui).voice[0]);
    let o = ui.g.ovl[(lane(1) + 2) * COLS + SCOPE + 1];
    assert_eq!(ui.g.ovl_color[usize::from(o - 1)], theme_of(&ui).dim);
    for c in 0..LIVE_VIEW_SCOPE_COLS as usize * 8 {
        let ink = (0..LIVE_VIEW_SCOPE_ROWS as usize)
            .map(|r| {
                let m = masks[r * LIVE_VIEW_SCOPE_COLS as usize + c / 8];
                m.iter().filter(|&&b| b >> (7 - c % 8) & 1 == 1).count()
            })
            .sum::<usize>();
        assert!(ink >= 2, "pixel column {c}: {ink} pixels on (a thin trace)");
    }
    // Released and silent again: flat.
    ui.rig.note_off(69);
    chip.play(&mut ui, 400);
    assert_eq!(
        trace_rows(&scope_masks(&ui, 0))
            .into_iter()
            .collect::<Vec<_>>(),
        [mid, mid + 1],
        "after the release"
    );
}

#[test]
fn the_envelope_stage_and_the_note_follow_the_tap() {
    let mut ui = Ui::new();
    let mut chip = Chip::new();
    ui.rig.note_on(69);
    chip.play(&mut ui, 600);
    // Sustaining at 0xa: ENV3 0xaa.
    assert_eq!(ui.v.live.env[0], 0xaa);
    assert_eq!(small(&ui, lane(0) + 3, 24, 3), "SUS");
    assert_eq!(big(&ui, lane(0) + 1, 24, 3), "A-4");
    assert_eq!(
        ui.g.attr[(lane(0) + 1) * COLS + 24] & 15,
        theme_of(&ui).value
    );
    assert_eq!(big(&ui, lane(1) + 1, 24, 3), "---", "the other voices rest");
    ui.rig.note_off(69);
    chip.play(&mut ui, 2);
    assert_eq!(small(&ui, lane(0) + 3, 24, 3), "REL");
    // A released note stays in the lane, dimmed; the voice's colour goes with its sound.
    assert_eq!(ui.g.attr[(lane(0) + 1) * COLS + 24] & 15, theme_of(&ui).dim);
}

#[test]
fn the_meters_show_the_peaks_and_fall_back() {
    let mut ui = Ui::new();
    settle(&mut ui);
    let k = usize::from(ui.sc.peak_slot & 1); // the producer's slot (app/scope.h)
    ui.sc.peak[k] = [32767, 0];
    frame(&mut ui);
    let meter =
        |ui: &Ui, row: usize| -> Vec<u8> { (21..30).map(|c| ui.g.ch[row * COLS + c]).collect() };
    assert!(
        meter(&ui, 25).iter().all(|&c| c >= FONT_BAR7 as u8),
        "L at full: {:?}",
        meter(&ui, 25)
    );
    assert!(
        meter(&ui, 26).iter().all(|&c| c == b' '),
        "R silent: blank cells, as the screen's own"
    );
    assert_eq!(ui.sc.peak, [[0, 0], [0, 0]], "taken");
    for _ in 0..8 {
        frame(&mut ui);
    }
    let fallen = ui.v.live.meter[3];
    assert!(
        fallen < SCOPE_METER_MAX as u8 - 20 && fallen > 0,
        "{fallen} after 8 frames"
    );
}

#[test]
fn the_filter_mode_and_the_knob_values_follow_the_filter() {
    let mut ui = Ui::new();
    ui.settle();
    assert_eq!(big(&ui, 22, 0, 2), "LP");
    let before = big(&ui, 16, 0, 3);
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_FILTER_MODE, 4) };
    ui.frame();
    assert_eq!(big(&ui, 22, 0, 2), "HP");
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_FILTER_MODE, 3) };
    ui.frame();
    assert_eq!(big(&ui, 22, 0, 5), "LP+BP");
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_FILTER_MODE, 0) };
    ui.frame();
    assert_eq!(big(&ui, 22, 0, 3), "OFF");
    assert_eq!(ui.g.attr[22 * COLS] & 15, theme_of(&ui).dim, "no mode: dim");
    // The cutoff and the resonance are the first two knobs' columns.
    ui.rig.knob(LIVE_KNOB_CUTOFF, -50);
    ui.frame();
    assert_ne!(big(&ui, 16, 0, 3), before);
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_RESONANCE, 15) };
    ui.frame();
    assert_eq!(big(&ui, 16, 8, 2), "15");
}

#[test]
fn live_holds_still_under_a_transition() {
    let mut ui = Ui::without_cards();
    ui.settle();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.press(PANEL_BTN_HOME); // back: a wipe into LIVE
    let before = ui.v.live.updates;
    let mut wiping = 0;
    while unsafe { screen_animating(&ui.v.s) } != 0 {
        ui.frame();
        wiping += 1;
    }
    assert!(wiping >= 6);
    assert!(
        ui.v.live.updates <= before + 1,
        "{} updates during {wiping} frames of wipe",
        ui.v.live.updates - before
    );
    ui.frame();
    assert!(ui.v.live.updates > before);
}

#[test]
fn three_voices_of_noise_take_turns_within_the_frame_budget() {
    let mut ui = Ui::new();
    let mut chip = Chip::new();
    settle(&mut ui);
    ui.rig.cc(20, 64); // noise
    ui.rig.cc(79, 127); // sustain 15
    for n in [57, 64, 76] {
        ui.rig.note_on(n);
    }
    let mut last = ui.v.live.y;
    let mut since_change = [0u32; 3];
    let mut most = 0;
    for f in 0..60 {
        let sent = chip.play(&mut ui, 14);
        most = most.max(sent.cells);
        assert!(
            sent.cells <= LIVE_VIEW_FRAME_CELLS as usize,
            "frame {f}: {} cells",
            sent.cells
        );
        for v in 0..3 {
            if ui.v.live.y[v] != last[v] {
                since_change[v] = 0;
            } else {
                since_change[v] += 1;
                assert!(
                    f < 3 || since_change[v] < 3,
                    "frame {f}: voice {v} stood still"
                );
            }
        }
        last = ui.v.live.y;
    }
    assert!(
        ui.v.live.held > 0,
        "the budget never held a trace back: no test"
    );
    assert!(most > 150, "{most} cells at most: the scopes are not busy");
}

#[test]
fn a_frame_prepared_half_way_draws_as_one_composed_whole() {
    // Two instruments playing the same: one frame prepared between frames as the device does
    // (views_prepare), the other composed in one go; the same grid either way.
    let mut a = Ui::new();
    let mut b = Ui::new();
    let (mut ca, mut cb) = (Chip::new(), Chip::new());
    for ui in [&mut a, &mut b] {
        settle(ui);
        ui.rig.note_on(69);
    }
    for _ in 0..20 {
        let before = a.v.live.updates;
        let mut out = vec![0i16; BLOCK as usize];
        for _ in 0..14 {
            for reg in 0..0x19u8 {
                unsafe { sid_write(&mut *ca.0, reg, a.rig.reg(reg)) };
            }
            unsafe { scope_render(&mut *a.sc, &mut *ca.0, out.as_mut_ptr(), BLOCK) };
        }
        unsafe { views_prepare(&mut *a.v) };
        frame(&mut a);
        assert_eq!(
            a.v.live.updates,
            before + 1,
            "the compose read the tap again"
        );
        cb.play(&mut b, 14);
        assert_eq!(a.g.ch, b.g.ch);
        assert_eq!(a.v.live.y, b.v.live.y);
    }
    unsafe { views_prepare(&mut *a.v) };
    let prepared = a.v.live.updates;
    a.press(PANEL_BTN_HOME);
    unsafe { views_prepare(&mut *a.v) };
    assert_eq!(a.v.live.updates, prepared, "prepared under a wipe");
}

/// Noise on every voice at full sustain, a note each: every scope cell its trace crosses
/// moves every frame.
fn noise_chord(ui: &mut Ui) {
    ui.rig.cc(20, 64); // noise
    ui.rig.cc(79, 127); // sustain 15
    for n in [57, 64, 76] {
        ui.rig.note_on(n);
    }
}

#[test]
fn meters_envelopes_notes_and_a_toast_redraw_their_cells_not_the_canvas() {
    let mut ui = Ui::new();
    let mut chip = Chip::new();
    settle(&mut ui);
    let built = ui.v.live.rebuilds;
    assert!(built > 0, "never built");
    // The meters jump and fall back.
    let k = usize::from(ui.sc.peak_slot & 1);
    ui.sc.peak[k] = [32767, 20000];
    for _ in 0..10 {
        frame(&mut ui);
    }
    // A note: the envelope bar, its stage and the note's name.
    ui.rig.note_on(69);
    chip.play(&mut ui, 30);
    ui.rig.note_off(69);
    ui.rig.note_on(72);
    chip.play(&mut ui, 30);
    // A toast for a while: the screen asks for a redraw every frame it shows.
    unsafe { screen_toast(&mut ui.v.s, c"OCTAVE +1".as_ptr(), 400) };
    for _ in 0..12 {
        chip.play(&mut ui, 14);
    }
    assert_eq!(
        ui.v.live.rebuilds, built,
        "the whole canvas was built again for a meter, an envelope, a note or a toast"
    );
}

#[test]
fn what_changed_draws_as_the_whole_canvas_built_again() {
    // Two instruments playing the same: one drawing only what changed, the other building
    // its whole canvas every frame; the same grid either way.
    let mut a = Ui::new();
    let mut b = Ui::new();
    let (mut ca, mut cb) = (Chip::new(), Chip::new());
    settle(&mut a);
    settle(&mut b);
    let steps: [&dyn Fn(&mut Ui); 12] = [
        &|ui| ui.rig.note_on(69),
        &|ui| ui.rig.note_on(49),
        &|ui| ui.rig.note_off(69),
        &|ui| ui.rig.knob(LIVE_KNOB_CUTOFF, -30),
        &|ui| ui.rig.set_preset(1), // PWM BASS: a pulse, its width shown
        &|ui| ui.rig.note_on(30),
        &|ui| ui.press(PANEL_BTN_SCL), // the slot selected
        &|ui| ui.rig.cc(20, 64),       // noise
        &|ui| unsafe { screen_toast(&mut ui.v.s, c"SOMETHING".as_ptr(), 200) },
        &|ui| ui.rig.button(LIVE_BUTTON_OCT_UP),
        &|ui| ui.rig.note_off(49),
        &|ui| ui.rig.note_off(30),
    ];
    for step in steps {
        for (ui, chip, whole) in [(&mut a, &mut ca, false), (&mut b, &mut cb, true)] {
            step(ui);
            for _ in 0..6 {
                if whole {
                    ui.v.live.built = 0;
                }
                chip.play(ui, 14);
            }
        }
        assert!(a.g.ch == b.g.ch && a.g.attr == b.g.attr, "the grids differ");
        assert_eq!(a.g.ovl, b.g.ovl);
    }
}

#[test]
fn back_from_the_menu_the_scopes_and_envelopes_start_flat() {
    let mut ui = Ui::without_cards();
    let mut chip = Chip::new();
    settle(&mut ui);
    ui.rig.note_on(69);
    chip.play(&mut ui, 30);
    assert!(ui.v.live.env[0] > 0);
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.rig.note_off(69);
    ui.press(PANEL_BTN_HOME); // back: the wipe's frames draw what LIVE last saw
    ui.frame();
    let mid = (LIVE_VIEW_SCOPE_PX / 2) as u8;
    assert!(
        ui.v.live.y.iter().flatten().all(|&y| y == mid),
        "the trace from before the menu came back"
    );
    assert_eq!(ui.v.live.env, [0; 3], "the envelope from before the menu");
    assert_eq!(small(&ui, lane(0) + 3, 24, 3), "---", "voice 1's stage");
}

#[test]
fn a_toast_over_three_voices_of_noise_stays_within_the_frame_budget() {
    let mut ui = Ui::new();
    let mut chip = Chip::new();
    settle(&mut ui);
    noise_chord(&mut ui);
    let mut most = 0;
    // A toast every 7 frames: its first frames meet the scopes at every phase of their turns.
    // The header's and the first slot's rows to go out again every frame (as after
    // grid_invalidate): 60 cells the scopes' share comes after, so the budget binds.
    for f in 0..420 {
        for i in (0..COLS).chain(24 * COLS..25 * COLS) {
            ui.g.stale[i >> 3] |= 1 << (i & 7);
        }
        ui.g.rows |= 1 | 1 << 24;
        if f % 7 == 3 {
            // As long as a toast gets, over the slots' rows: every cell of it changes.
            unsafe { screen_toast(&mut ui.v.s, c"ABCDEFGHIJKLMNOPQRSTUVWXYZ".as_ptr(), 80) };
        }
        let sent = chip.play(&mut ui, 14);
        most = most.max(sent.cells);
        assert!(
            sent.cells <= LIVE_VIEW_FRAME_CELLS as usize,
            "frame {f}: {} cells",
            sent.cells
        );
    }
    assert!(most > 150, "{most} cells at most: the scopes are not busy");
}

#[test]
fn the_tap_runs_while_live_is_under_midi_learns_frame() {
    let mut ui = Ui::new();
    let mut chip = Chip::new();
    let mut ml = Box::<midi_learn>::default();
    unsafe {
        midi_learn_init(&mut *ml);
        views_learn(&mut *ui.v, &mut *ml);
    }
    settle(&mut ui);
    ui.learn_on(PANEL_ENC_K1);
    assert_eq!(ui.top(), "LEARN");
    ui.rig.note_on(69);
    for _ in 0..4 {
        chip.play(&mut ui, 14);
    }
    assert_eq!(ui.sc.on, 1, "the tap is off under learn's frame over LIVE");
    // LIVE composes under the frame: its scope follows the note (at the sustain level by now;
    // flat, it spans none).
    let rows = trace_rows(&scope_masks(&ui, 0));
    let span = rows.last().unwrap() - rows.first().unwrap();
    assert!(
        span >= 12,
        "voice 1 under learn: the trace spans {span} pixel rows"
    );
    ui.rig.note_off(69);
    // Learn over the menu: LIVE does not show, the tap is off.
    ui.press(PANEL_BTN_HOME); // learn cancelled: LIVE
    ui.settle();
    ui.press(PANEL_BTN_HOME); // the menu
    ui.settle();
    ui.learn_on(PANEL_ENC_K1);
    assert_eq!(ui.top(), "LEARN");
    ui.frame();
    assert_eq!(
        ui.sc.on, 0,
        "the tap runs under learn's frame over the menu"
    );
}

#[test]
fn the_slot_page_names_the_slot_controls() {
    let mut ui = Ui::new();
    ui.settle();
    let t = theme_of(&ui);
    // DIST OVERDRIVE: DRIVE, TONE, BIAS, MIX, in the knobs' readable colours, the page DIST.
    unsafe { live_slot_set(&mut *ui.rig.live, 0, LIVE_SLOT_ALGORITHM, 2) };
    ui.v.live.page = 1;
    ui.settle();
    assert_eq!(small(&ui, 14, 0, 4), "DIST");
    for (k, (label, col)) in [("DRIVE", 0), ("TONE", 8), ("BIAS", 15), ("MIX", 22)]
        .into_iter()
        .enumerate()
    {
        assert_eq!(small(&ui, 15, col, label.len()), label);
        assert_eq!(ui.g.attr[15 * COLS + col] & 15, t.knob_text[k], "{label}");
        assert_eq!(ui.g.attr[18 * COLS + col] & 15, t.knob[k], "{label}'s bar");
    }
    assert_eq!(big(&ui, 25, 0, 9), "OVERDRIVE");
    assert_eq!(
        ui.g.attr[25 * COLS],
        (t.hot << 4) | t.bg,
        "the slot's algorithm reversed while its page shows"
    );
    // TAPE has no use for A and B: their label, value and bar are dim.
    unsafe { live_slot_set(&mut *ui.rig.live, 0, LIVE_SLOT_ALGORITHM, 1) };
    ui.settle();
    for (col, used) in [(0, true), (8, false), (15, false), (22, true)] {
        let want = if used { t.value } else { t.dim };
        assert_eq!(ui.g.attr[16 * COLS + col] & 15, want, "value at {col}");
        let label = if used { t.knob_text[col / 8] } else { t.dim };
        let _ = label;
        assert_eq!(
            ui.g.attr[15 * COLS + col] & 15 == t.dim,
            !used,
            "label at {col}"
        );
        assert_eq!(
            ui.g.attr[18 * COLS + col] & 15 == t.dim,
            !used,
            "bar at {col}"
        );
    }
    // Back on TONE: the page's own labels.
    ui.v.live.page = 0;
    ui.settle();
    assert_eq!(small(&ui, 14, 0, 4), "TONE");
    assert_eq!(small(&ui, 15, 0, 6), "CUTOFF");
}

#[test]
fn a_flash_reverses_a_knobs_label_for_a_second() {
    let mut ui = Ui::new();
    ui.settle();
    let t = theme_of(&ui);
    let label_attr = |ui: &Ui, col: usize| ui.g.attr[15 * COLS + col];
    assert_eq!(label_attr(&ui, 0), (t.bg << 4) | t.knob_text[0]);
    unsafe { live_view_flash(&mut ui.v.live, 1, ui.now) };
    ui.frame();
    assert_eq!(label_attr(&ui, 0), (t.knob_text[0] << 4) | t.bg, "reversed");
    assert_eq!(
        label_attr(&ui, 8),
        (t.bg << 4) | t.knob_text[1],
        "only that knob's"
    );
    for _ in 0..20 {
        ui.frame();
    }
    assert_eq!(
        label_attr(&ui, 0),
        (t.knob_text[0] << 4) | t.bg,
        "still, 800 ms on"
    );
    for _ in 0..8 {
        ui.frame();
    }
    assert_eq!(
        label_attr(&ui, 0),
        (t.bg << 4) | t.knob_text[0],
        "normal after 1,000 ms"
    );
    // The profile and the slot's algorithm reverse the same way.
    unsafe { live_view_flash(&mut ui.v.live, LIVE_VIEW_HL_PROFILE, ui.now) };
    ui.frame();
    assert_eq!(ui.g.attr[22 * COLS + 11], (t.value << 4) | t.bg);
    unsafe { live_view_flash(&mut ui.v.live, LIVE_VIEW_HL_ALGORITHM, ui.now) };
    ui.frame();
    assert_eq!(ui.g.attr[25 * COLS], (t.hot << 4) | t.bg);
}

#[test]
fn the_octave_shows_at_2x_when_not_0() {
    let mut ui = Ui::new();
    ui.settle();
    let t = theme_of(&ui);
    assert_eq!(small(&ui, 0, 26, 4), "    ");
    ui.press(PANEL_BTN_OCT_DOWN);
    ui.press(PANEL_BTN_OCT_DOWN);
    ui.press(PANEL_BTN_OCT_DOWN);
    ui.settle();
    assert_eq!(big(&ui, 0, 26, 2), "-3");
    assert_eq!(ui.g.attr[26] & 15, t.hot);
    for _ in 0..3 {
        ui.press(PANEL_BTN_OCT_UP);
    }
    ui.settle();
    assert_eq!(small(&ui, 0, 26, 4), "    ", "nothing at 0");
}

/// ARP on LIVE (app/live_view.h): a press and release switches the chord arpeggio, which a 2x
/// badge under the band then names with its settings (`ARP 3F UP 1OCT`, read at arm's length:
/// ARP in the hot role, its settings in the value role); held, the knob row is ARP's (RATE,
/// ORDER, SPAN) and K1..K3 turn them, and its release then switches nothing. K4 does nothing
/// while it is held. Off, the badge's rows are blank.
#[test]
fn arp_switches_the_arpeggio_and_held_turns_its_settings() {
    let mut ui = Ui::without_cards();
    ui.settle();
    assert_eq!(ui.top(), "LIVE");
    let badge = LIVE_VIEW_ARP_ROW as usize;
    let blank = |ui: &Ui| ui.row(badge).trim().is_empty() && ui.row(badge + 1).trim().is_empty();
    assert!(blank(&ui), "{:?}", ui.row(badge));
    ui.press(PANEL_BTN_ARP);
    assert_eq!(ui.rig.live.arp_on, 1, "the release switched it on");
    ui.settle();
    assert_eq!(
        ui.big(badge, 0, 2, 14),
        "ARP 3F UP 1OCT",
        "the default rate: 3F"
    );
    let t = unsafe { &*theme_get(u32::from(ui.rig.live.theme)) };
    assert_eq!(ui.g.attr[badge * 30], (t.bg << 4) | t.hot, "ARP");
    assert_eq!(ui.g.attr[badge * 30 + 8], (t.bg << 4) | t.value, "3F");
    assert!(
        !ui.row(LIVE_VIEW_STRIP_ROW as usize).contains("ARP"),
        "the badge alone says it"
    );
    // Held: the knob row is ARP's.
    ui.down(PANEL_BTN_ARP);
    ui.frame();
    assert!(ui.row(LIVE_VIEW_PAGE_ROW as usize).starts_with("ARP"));
    let labels = ui.row(LIVE_VIEW_KNOB_ROW as usize);
    for w in ["RATE", "ORDER", "SPAN"] {
        assert!(labels.contains(w), "{labels:?}");
    }
    let cutoff = ui.rig.live.cutoff;
    ui.turn(PANEL_ENC_K1, 4);
    ui.turn(PANEL_ENC_K1 + 1, 1);
    ui.turn(PANEL_ENC_K1 + 2, 1);
    ui.turn(PANEL_ENC_K4, 1);
    assert_eq!(ui.rig.live.cutoff, cutoff, "K1 was the arpeggio's");
    assert_eq!(ui.rig.live.arp_rate, 6);
    assert_eq!(u32::from(ui.rig.live.arp_order), LIVE_ARP_DOWN);
    assert_eq!(ui.rig.live.arp_span, 2);
    ui.frame();
    assert_eq!(ui.big(LIVE_VIEW_KNOB_VALUE_ROW as usize, 0, 2, 3), "/32");
    ui.up(PANEL_BTN_ARP);
    assert_eq!(
        ui.rig.live.arp_on, 1,
        "a knob turned: the release kept it on"
    );
    ui.settle();
    assert_eq!(ui.big(badge, 0, 2, 15), "ARP /32 DN 2OCT");
    assert!(ui.row(LIVE_VIEW_PAGE_ROW as usize).starts_with("TONE"));
    ui.press(PANEL_BTN_ARP);
    assert_eq!(ui.rig.live.arp_on, 0);
    ui.settle();
    assert!(blank(&ui), "{:?}", ui.row(badge));
}
