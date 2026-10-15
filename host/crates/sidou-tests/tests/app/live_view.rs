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

/// The overlay masks of voice v's scope cells (4 rows of 20), empty where none.
fn scope_masks(ui: &Ui, v: usize) -> Vec<[u8; 8]> {
    let top = LIVE_VIEW_BOX_ROW as usize + LIVE_VIEW_BOX_ROWS as usize * v + 1;
    let mut out = Vec::new();
    for r in top..top + LIVE_VIEW_SCOPE_ROWS as usize {
        for c in 1..=LIVE_VIEW_SCOPE_COLS as usize {
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
    let mut ui = Ui::new();
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
fn the_header_boxes_and_slots_show_the_instrument() {
    let mut ui = Ui::new();
    ui.settle();
    let head = ui.row(0);
    assert_eq!(&head[1..5], "LIVE");
    assert_eq!(&head[8..12], "6581");
    assert_eq!(&head[13..21], "SAW LEAD");
    assert_eq!(&head[24..29], "OCT+0");
    assert_eq!(
        ui.g.attr[0],
        ((GRID_LIGHT_BLUE << 4) | GRID_BLUE) as u8,
        "reverse video"
    );
    for v in 0..3 {
        let row = 1 + 6 * v;
        assert_eq!(&ui.row(row)[2..10], format!(" V{} SAW ", v + 1));
        assert_eq!(&ui.row(row + 2)[25..28], "SAW");
        assert_eq!(&ui.row(row + 4)[25..28], "---", "the envelope at rest");
        // The scope's centre line: a dotted glyph under the overlay.
        assert_eq!(ui.g.ch[(row + 3) * COLS + 5], FONT_SCOPE_MID as u8);
    }
    assert_eq!(&ui.row(19)[2..13], " FILTER LP ");
    assert_eq!(&ui.row(19)[20..24], " VU ");
    let bottom = ui.row(26);
    assert!(
        bottom[1..].starts_with(" FC ") && bottom.contains(" RES 6 "),
        "{bottom}"
    );
    assert_eq!(&ui.row(27)[1..5], "DIST");
    assert_eq!(&ui.row(27)[6..9], "OFF");
    assert_eq!(&ui.row(28)[1..4], "MOD");
    assert_eq!(&ui.row(29)[1..3], "FX");
    assert_eq!(ui.g.ch[27 * COLS], FONT_PLAY as u8, "the selected slot");
    ui.press(PANEL_BTN_SCL);
    ui.press(PANEL_BTN_ENV);
    ui.rig.set_preset(1);
    ui.settle();
    assert_eq!(ui.g.ch[28 * COLS], FONT_PLAY as u8);
    assert_eq!(&ui.row(28)[6..12], "CHORUS");
    assert_eq!(&ui.row(1)[2..12], " V1 PULSE ");
    assert_eq!(&ui.row(4)[25..27], "PW", "a pulse shows its width");
    assert_eq!(&ui.row(0)[13..21], "PWM BASS");
}

#[test]
fn a_note_draws_a_trace_and_silence_a_flat_line() {
    let mut ui = Ui::new();
    let mut chip = Chip::new();
    settle(&mut ui);
    chip.play(&mut ui, 20);
    let mid = (LIVE_VIEW_SCOPE_PX / 2) as usize;
    for v in 0..3 {
        let rows = trace_rows(&scope_masks(&ui, v));
        assert_eq!(
            rows.into_iter().collect::<Vec<_>>(),
            [mid],
            "voice {v} at rest"
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
        [mid],
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
    // Its colour is the saw's.
    let o = ui.g.ovl[4 * COLS + 2]; // on the centre line, crossed by any trace
    assert_eq!(ui.g.ovl_color[usize::from(o - 1)], GRID_YELLOW as u8);
    // Released and silent again: flat.
    ui.rig.note_off(69);
    chip.play(&mut ui, 400);
    assert_eq!(
        trace_rows(&scope_masks(&ui, 0))
            .into_iter()
            .collect::<Vec<_>>(),
        [mid],
        "after the release"
    );
}

#[test]
fn the_envelope_bars_and_stage_follow_the_tap() {
    let mut ui = Ui::new();
    let mut chip = Chip::new();
    ui.rig.note_on(69);
    chip.play(&mut ui, 600);
    // Sustaining at 0xa: ENV3 0xaa, 21 of 32 eighths.
    assert_eq!(ui.v.live.env[0], 0xaa);
    let col = 22;
    let cells: Vec<u8> = (2..6).map(|r| ui.g.ch[r * COLS + col]).collect();
    assert_eq!(
        cells,
        [FONT_V0 as u8, FONT_V5 as u8, FONT_V8 as u8, FONT_V8 as u8]
    );
    assert_eq!(&ui.row(5)[25..28], "SUS");
    assert_eq!(&ui.row(2)[25..28], "A-4");
    ui.rig.note_off(69);
    chip.play(&mut ui, 2);
    assert_eq!(&ui.row(5)[25..28], "REL");
}

#[test]
fn the_meters_show_the_peaks_and_fall_back() {
    let mut ui = Ui::new();
    settle(&mut ui);
    ui.sc.peak = [32767, 0];
    frame(&mut ui);
    let meter =
        |ui: &Ui, row: usize| -> Vec<u8> { (20..29).map(|c| ui.g.ch[row * COLS + c]).collect() };
    assert!(
        meter(&ui, 24).iter().all(|&c| c >= FONT_BAR7 as u8),
        "L at full: {:?}",
        meter(&ui, 24)
    );
    assert!(
        meter(&ui, 25).iter().all(|&c| c == FONT_BAR0 as u8),
        "R silent"
    );
    assert_eq!(ui.sc.peak, [0, 0], "taken");
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
fn the_filter_curve_follows_mode_cutoff_and_resonance() {
    let mut ui = Ui::new();
    // SAW LEAD's 1200 is about 8 kHz on the 6581: down to about 1 kHz.
    ui.rig.knob(LIVE_KNOB_CUTOFF, -50);
    ui.settle();
    let heights = |ui: &Ui| ui.v.live.height.to_vec();
    let lp = heights(&ui);
    assert!(lp[0] > 30 && lp[15] < lp[0] / 2, "low-pass: {lp:?}");
    let mark = usize::from(ui.v.live.mark);
    let attr = ui.g.attr[25 * COLS + 1 + mark];
    assert_eq!(attr & 15, GRID_YELLOW as u8, "the cutoff's column");
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_FILTER_MODE, 4) };
    ui.frame();
    let hp = heights(&ui);
    assert!(hp[15] > 30 && hp[0] < hp[15] / 2, "high-pass: {hp:?}");
    assert_eq!(&ui.row(19)[2..13], " FILTER HP ");
    ui.rig.knob(LIVE_KNOB_CUTOFF, 40);
    ui.frame();
    assert!(
        usize::from(ui.v.live.mark) > mark,
        "the cutoff went up, its column did not"
    );
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_FILTER_MODE, 0) };
    ui.frame();
    assert!(
        heights(&ui).iter().all(|&h| h == 0),
        "no output with no mode"
    );
    assert_eq!(&ui.row(19)[2..14], " FILTER OFF ");
    // More resonance, a higher peak.
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_FILTER_MODE, 1) };
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_RESONANCE, 0) };
    ui.frame();
    let low = *heights(&ui).iter().max().unwrap();
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_RESONANCE, 15) };
    ui.frame();
    let high = *heights(&ui).iter().max().unwrap();
    assert!(high > low, "resonance 0: {low}, 15: {high}");
}

#[test]
fn live_holds_still_under_a_transition() {
    let mut ui = Ui::new();
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
