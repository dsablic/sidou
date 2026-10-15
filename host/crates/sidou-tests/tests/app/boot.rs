// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The boot sequence (app/boot.c) over the views (app/views.c): its timing, the short and full
//! sequences, skipping by any control without its normal action, a note that skips and plays,
//! and what each frame sends.

use sidou_sys::*;

use crate::views::Ui;
use crate::{assert_wipe_holds_still, drain, grid_row, pixels, wipe_frames, Sent, SPI_BUDGET_MS};

const COLS: usize = GRID_COLS as usize;
const W: usize = COLS * 8;

/// The title card is Metal Chrome's scene, pixel for pixel, but for the card's text over it:
/// the version line and PRESS ANY KEY (app/boot.h BOOT_SCENE_*_ROW).
fn assert_title_card_is_the_scene(ui: &Ui) {
    let img = sidou_tests::logo::load("metal-chrome");
    let px = pixels(&ui.g);
    let text = [BOOT_SCENE_VERSION_ROW as usize, BOOT_SCENE_KEY_ROW as usize];
    for y in (0..W).filter(|y| !text.contains(&(y / 8))) {
        assert_eq!(
            px[y * W..(y + 1) * W],
            img.px[y * W..(y + 1) * W],
            "pixel row {y} of the title card"
        );
    }
    // The text rows are black in the scene: the text reads as on any black row.
    for r in text {
        assert!(img.px[r * 8 * W..(r + 1) * 8 * W].iter().all(|&c| c == 0));
    }
}

fn booting(full: bool) -> Ui {
    let mut ui = Ui::new();
    drain(&mut ui.g);
    unsafe { views_boot(&mut *ui.v, i32::from(full)) };
    ui
}

/// Frames until ms from the boot's first frame (the next one).
fn run_to(ui: &mut Ui, start: u32, ms: u32) -> Vec<Sent> {
    let mut sent = Vec::new();
    while ui.now < start + ms {
        ui.frame();
        sent.push(drain(&mut ui.g));
    }
    sent
}

fn phase_at(full: bool, ms: u32) -> boot_phase {
    let mut b = boot::default();
    unsafe {
        boot_init(&mut b, std::ptr::null(), c"V1".as_ptr(), i32::from(full));
        boot_phase_at(&b, ms)
    }
}

#[test]
fn the_full_sequence_keeps_the_prototypes_timing() {
    for (ms, want) in [
        (0, BOOT_PROMPT),
        (799, BOOT_PROMPT),
        (800, BOOT_TYPING),
        (2199, BOOT_TYPING),
        (2200, BOOT_SEARCHING),
        (3000, BOOT_LOADING),
        (5599, BOOT_LOADING),
        (5600, BOOT_READY),
        (6999, BOOT_READY),
        (7000, BOOT_TITLE),
        (9999, BOOT_TITLE),
        (10_000, BOOT_DONE),
    ] {
        assert_eq!(phase_at(true, ms), want, "full at {ms} ms");
    }
    for (ms, want) in [(0, BOOT_TITLE), (1199, BOOT_TITLE), (1200, BOOT_DONE)] {
        assert_eq!(phase_at(false, ms), want, "short at {ms} ms");
    }
}

#[test]
fn the_full_boot_types_loads_and_runs_then_shows_the_title_and_the_status_screen() {
    let mut ui = booting(true);
    let start = ui.now + 40;
    assert_eq!(ui.top(), "BOOT");
    run_to(&mut ui, start, 0);
    assert!(ui.row(3).contains("*** SIDOU V1.2.3 ***"), "{}", ui.row(3));
    assert_eq!(ui.row(5).trim_matches('#').trim(), "CHIP 6581  3 VOICES");
    assert_eq!(&ui.row(7)[2..8], "READY.");
    // The cursor blinks at the prompt, 8 frames on and 8 off.
    let mut cursor = Vec::new();
    while ui.now < start + 760 {
        ui.frame();
        cursor.push(ui.g.ch[8 * COLS + 2] == FONT_FULL as u8);
    }
    assert!(
        cursor.contains(&true) && cursor.contains(&false),
        "{cursor:?}"
    );
    run_to(&mut ui, start, 800 + 5 * 70);
    assert_eq!(&ui.row(8)[2..7], "LOAD\"", "five characters typed");
    assert!(!ui.row(8)[8..].contains('S'), "{}", ui.row(8));
    run_to(&mut ui, start, 2200);
    assert_eq!(&ui.row(8)[2..17], "LOAD\"SIDOU\",8,1");
    assert_eq!(&ui.row(10)[2..21], "SEARCHING FOR SIDOU");
    assert!(
        ui.row(8)[17..].trim_matches('#').trim().is_empty(),
        "the cursor stayed"
    );
    run_to(&mut ui, start, 3000);
    assert_eq!(&ui.row(11)[2..9], "LOADING");
    run_to(&mut ui, start, 6600);
    assert_eq!(&ui.row(12)[2..8], "READY.");
    assert_eq!(&ui.row(13)[2..5], "RUN");
    assert_eq!(ui.top(), "BOOT");
    // The title card wipes in: the logo's scene, the version over it.
    run_to(&mut ui, start, 7000 + 8 * 40);
    let version = BOOT_SCENE_VERSION_ROW as usize;
    assert!(
        ui.row(version).contains("V1.2.3  GPL-3.0"),
        "{}",
        ui.row(version)
    );
    assert_title_card_is_the_scene(&ui);
    run_to(&mut ui, start, 9960);
    assert_eq!(ui.top(), "BOOT");
    run_to(&mut ui, start, 10_000);
    assert_eq!(ui.top(), "LIVE");
    // LIVE wipes in, and then nothing is left of the boot.
    ui.settle();
    assert_eq!(&ui.row(0)[1..5], "LIVE");
    assert_eq!(&ui.row(1)[2..10], " V1 SAW ");
    assert!(ui.v.s.capture.is_none(), "the boot's capture stayed");
    let scope_cell = |i: usize| {
        let (r, c) = (i / COLS, i % COLS);
        (1..=20).contains(&c) && (1..19).contains(&r) && !matches!((r - 1) % 6, 0 | 5)
    };
    assert!(
        (0..COLS * COLS).all(|i| ui.g.ovl[i] == 0 || scope_cell(i)),
        "the logo's overlays stayed"
    );
    assert!(ui.g.region_bitmap.is_null(), "the logo's scene stayed");
}

#[test]
fn press_any_key_blinks_under_the_scene() {
    let mut ui = booting(false);
    let start = ui.now + 40;
    let key = BOOT_SCENE_KEY_ROW as usize;
    let mut shown = Vec::new();
    while ui.now < start + 1100 {
        ui.frame();
        shown.push(ui.row(key).contains("PRESS ANY KEY"));
        assert_title_card_is_the_scene(&ui);
    }
    assert!(shown.contains(&true) && shown.contains(&false), "{shown:?}");
}

/// A logo without a scene (Pulse Train, the alternative) gets the card's slot between two
/// ramp lines, with the name and the version below.
#[test]
fn a_logo_without_a_scene_gets_the_cards_slot() {
    let mut ui = booting(false);
    ui.v.boot.logo = unsafe { &logo_pulse_train };
    let start = ui.now + 40;
    run_to(&mut ui, start, 200);
    assert!(ui.row(16).contains("SID FOR THE FM-1"), "{}", ui.row(16));
    assert!(ui.row(18).contains("V1.2.3  GPL-3.0"), "{}", ui.row(18));
    assert!(
        (8..13).all(|r| (7..22).any(|c| ui.g.ovl[r * COLS + c] != 0)),
        "no logo on rows 8-12"
    );
    assert!(ui.g.region_bitmap.is_null(), "a pixel region on the card");
    for r in [BOOT_TITLE_LINE_TOP, BOOT_TITLE_LINE_BOTTOM] {
        let r = r as usize;
        assert!((0..COLS).all(|c| ui.g.ch[r * COLS + c] == FONT_BOX_H as u8));
    }
}

#[test]
fn the_short_boot_is_the_title_card_alone() {
    let mut ui = booting(false);
    let start = ui.now + 40;
    run_to(&mut ui, start, 0);
    assert_eq!(ui.top(), "BOOT");
    assert!(ui
        .row(BOOT_SCENE_VERSION_ROW as usize)
        .contains("V1.2.3  GPL-3.0"));
    assert_title_card_is_the_scene(&ui);
    assert!(!(0..30).any(|r| ui.row(r).contains("LOAD")));
    run_to(&mut ui, start, 1160);
    assert_eq!(ui.top(), "BOOT");
    run_to(&mut ui, start, 1200);
    assert_eq!(ui.top(), "LIVE");
    assert_eq!(ui.v.s.wipe, SCREEN_WIPE_DOWN as u8);
}

#[test]
fn home_held_at_power_on_plays_the_full_boot_and_its_press_edge_is_part_of_the_hold() {
    // platform/fm1/main.c reads HOME held at power-on from the key scan and starts the full
    // boot. The key scan also reports the held key's press edge, once (fm1_input.h): it is
    // the hold, not a skip, whether it comes before the first frame or after it.
    for before in [true, false] {
        let mut ui = booting(true);
        let start = ui.now + 40;
        if !before {
            run_to(&mut ui, start, 200);
        }
        ui.hold(PANEL_BTN_HOME, true);
        ui.press(PANEL_BTN_HOME);
        ui.frame();
        assert_eq!(ui.top(), "BOOT", "the hold's press edge skipped the boot");
        ui.hold(PANEL_BTN_HOME, false);
        run_to(&mut ui, start, 2400);
        assert!(
            ui.row(10).contains("SEARCHING FOR SIDOU"),
            "not the full boot: {}",
            ui.row(10)
        );
        assert_eq!(ui.live().octave, 0);
        // A HOME press after that one skips, and does nothing else.
        ui.press(PANEL_BTN_HOME);
        ui.frame();
        assert_eq!(ui.top(), "LIVE", "a second HOME did not skip");
        ui.settle();
        assert_eq!(ui.top(), "LIVE", "HOME also opened the menu");
    }
}

#[test]
fn home_tapped_as_soon_as_the_card_shows_skips_it() {
    // Without HOME held at power-on, a HOME press is a skip from the very first frame.
    for frames in [0, 1, 5] {
        let mut ui = booting(false);
        for _ in 0..frames {
            ui.frame();
        }
        ui.press(PANEL_BTN_HOME);
        ui.frame();
        assert_eq!(ui.top(), "LIVE", "HOME after {frames} frames did not skip");
        ui.settle();
        assert_eq!(ui.top(), "LIVE", "HOME also opened the menu");
    }
}

#[test]
fn holding_home_later_is_not_a_press() {
    let mut ui = booting(false);
    let start = ui.now + 40;
    run_to(&mut ui, start, 400);
    ui.hold(PANEL_BTN_HOME, true);
    ui.frame();
    assert_eq!(ui.top(), "BOOT");
    assert_title_card_is_the_scene(&ui);
}

#[test]
fn a_button_or_a_knob_skips_and_does_nothing_else() {
    type Skip = (&'static str, fn(&mut Ui));
    let skips: [Skip; 5] = [
        ("OCT+", |ui| ui.press(PANEL_BTN_OCT_UP)),
        ("FX", |ui| ui.press(PANEL_BTN_FX)),
        ("ENV", |ui| ui.press(PANEL_BTN_ENV)),
        ("K1", |ui| ui.turn(PANEL_ENC_K1, -3)),
        ("SELECT", |ui| ui.turn(PANEL_ENC_SELECT, 1)),
    ];
    for (what, skip) in skips {
        let mut ui = booting(true);
        let start = ui.now + 40;
        run_to(&mut ui, start, 2500);
        let before = (
            ui.live().octave,
            ui.live().model,
            ui.live().cutoff,
            ui.live().slot,
        );
        skip(&mut ui);
        // Pressed again before the frame: still taken.
        skip(&mut ui);
        ui.frame();
        assert_eq!(ui.top(), "LIVE", "{what} did not skip");
        let after = (
            ui.live().octave,
            ui.live().model,
            ui.live().cutoff,
            ui.live().slot,
        );
        assert_eq!(after, before, "{what} also played");
        assert_eq!(ui.toast(), None, "{what} showed a toast");
    }
}

#[test]
fn a_note_skips_and_plays() {
    for full in [true, false] {
        let mut ui = booting(full);
        let start = ui.now + 40;
        run_to(&mut ui, start, 400);
        ui.rig.note_on(69);
        ui.frame();
        assert_eq!(ui.top(), "LIVE", "full {full}: the note did not skip");
        assert!(ui.live().voice.iter().any(|v| v.gate != 0 && v.note == 69));
    }
}

#[test]
fn a_note_shorter_than_a_frame_skips_the_boot_too() {
    // On and off between two frames (a fast tap, or MIDI): no gate is left for the frame to
    // see, but the note played.
    for full in [true, false] {
        let mut ui = booting(full);
        let start = ui.now + 40;
        run_to(&mut ui, start, 400);
        ui.rig.note_on(69);
        ui.rig.note_off(69);
        ui.frame();
        assert_eq!(ui.top(), "LIVE", "full {full}: the short note did not skip");
    }
}

#[test]
fn changes_during_the_boot_draw_nothing_over_it() {
    // MIDI can change the chip and the octave while the boot shows: no toast or raster over
    // it, then or after it.
    let mut ui = booting(true);
    let start = ui.now + 40;
    run_to(&mut ui, start, 400);
    ui.rig.button(LIVE_BUTTON_FX);
    ui.rig.button(LIVE_BUTTON_OCT_UP);
    ui.frame();
    assert_eq!(ui.top(), "BOOT");
    assert_eq!(ui.toast(), None, "a toast over the boot");
    assert_eq!(ui.v.s.raster_frames, 0, "a raster over the boot");
    ui.press(PANEL_BTN_FX);
    ui.frame();
    assert_eq!(ui.top(), "LIVE");
    ui.settle();
    assert_eq!(ui.toast(), None, "the boot's changes showed after it");
    assert_eq!(ui.live().model, SID_MODEL_8580);
}

/// The border cells of a grid.
fn border(i: usize) -> bool {
    let (c, r) = (i % COLS, i / COLS);
    r == 0 || r == COLS - 1 || c == 0 || c == COLS - 1
}

#[test]
fn the_loading_stripes_stay_in_the_border_every_other_frame() {
    let mut ui = booting(true);
    let start = ui.now + 40;
    run_to(&mut ui, start, 3000);
    let mut changes = 0;
    let mut frames = 0;
    let (mut ch, mut attr) = (ui.g.ch, ui.g.attr);
    let mut last_changed = false;
    while ui.now < start + 5560 {
        ui.frame();
        frames += 1;
        let moved: Vec<usize> = (0..COLS * COLS)
            .filter(|&i| ui.g.ch[i] != ch[i] || ui.g.attr[i] != attr[i])
            .collect();
        assert!(moved.iter().all(|&i| border(i)), "{moved:?} off the border");
        let changed = !moved.is_empty();
        assert!(!(changed && last_changed), "stripes on two frames in a row");
        changes += usize::from(changed);
        last_changed = changed;
        (ch, attr) = (ui.g.ch, ui.g.attr);
        // Two colours a cell: the glyph splits the cell into a top and a bottom stripe.
        for i in (0..COLS * COLS).filter(|&i| border(i)) {
            let k = ui.g.ch[i];
            assert!(
                (FONT_V0 as u8..=FONT_V8 as u8).contains(&k),
                "border cell {i} is {k:#04x}"
            );
        }
    }
    assert!(
        changes >= frames / 2 - 1,
        "{changes} changes in {frames} frames"
    );
    run_to(&mut ui, start, 5600);
    let lb = (GRID_BLUE << 4 | GRID_LIGHT_BLUE) as u8;
    assert!(
        (0..COLS * COLS)
            .filter(|&i| border(i))
            .all(|i| ui.g.ch[i] == FONT_FULL as u8 && ui.g.attr[i] == lb),
        "the border did not come back at READY."
    );
}

#[test]
fn every_frame_of_the_boot_fits_the_spi_budget() {
    for full in [true, false] {
        let mut ui = booting(full);
        let start = ui.now + 40;
        let sent = run_to(&mut ui, start, if full { 10_000 } else { 1200 } + 12 * 40);
        assert_eq!(ui.top(), "LIVE");
        // The first frame draws the whole screen (platform/fm1/ui.c ui_init, before the
        // display is on); every other one stays inside the budget.
        for (k, s) in sent.iter().enumerate().skip(1) {
            assert!(
                s.spi_ms() <= SPI_BUDGET_MS,
                "full {full}: frame {k} took {:.1} ms ({s:?})",
                s.spi_ms()
            );
        }
        let total: usize = sent.iter().skip(1).map(|s| s.cells).sum();
        println!(
            "boot (full {full}): {} frames, {total} cells after the first, worst {:.1} ms",
            sent.len(),
            sent.iter().skip(1).map(|s| s.spi_ms()).fold(0.0, f64::max)
        );
    }
}

/// The wipe from the prompt into the title card: the logo holds still under it, so each frame
/// sends the wipe's rows and nothing else, whatever the boot's start time against the frames.
#[test]
fn the_wipe_into_the_title_card_sends_only_its_rows() {
    let mut worst = Sent::default();
    for offset in 0..40 {
        let mut ui = Ui::new();
        ui.now += offset;
        drain(&mut ui.g);
        unsafe { views_boot(&mut *ui.v, 1) };
        let start = ui.now + 40;
        run_to(&mut ui, start, BOOT_TITLE_MS - 200);
        let frames = wipe_frames(&mut || {
            ui.frame();
            (drain(&mut ui.g), ui.v.s.wipe != 0)
        });
        assert_eq!(ui.top(), "BOOT");
        worst = frames
            .iter()
            .copied()
            .fold(worst, |w, s| if s.cells > w.cells { s } else { w });
        assert_wipe_holds_still(&format!("the title card, {offset} ms late"), &frames);
    }
    println!(
        "wipe into the title card: worst frame {worst:?} {:.2} ms",
        worst.spi_ms()
    );
}

#[test]
fn the_banner_fits_the_longest_version() {
    let mut ui = Ui::new();
    let v = c"V1234567890123";
    ui.v.version = v.as_ptr();
    unsafe { views_boot(&mut *ui.v, 1) };
    ui.frame();
    let row = ui.row(3);
    assert!(row.contains("*** SIDOU V1234567890123 ***"), "{row}");
    assert_eq!(&row[0..1], "#");
    assert_eq!(&row[29..30], "#", "the banner covered the border");
    assert_eq!(grid_row(&ui.g, 3), row);
}
