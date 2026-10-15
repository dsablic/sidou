// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The About cracktro (app/about.c): reached from the menu, the credits pages from
//! CREDITS.md, the logo, raster bars, stars and the sine scroller, and what it sends a frame.

use std::ffi::CStr;

use sidou_sys::*;

use crate::views::Ui;
use crate::{assert_wipe_holds_still, drain, pixels, wipe_frames, Sent, SPI_BUDGET_MS};

const COLS: usize = GRID_COLS as usize;
const W: usize = COLS * 8;
const PAGE: usize = ABOUT_PAGE_FRAMES as usize;
const HEADING: usize = ABOUT_HEADING_ROW as usize;
const LINES: usize = ABOUT_LINES_ROW as usize;

/// The menu, then ABOUT, settled.
pub(crate) fn open_about(ui: &mut Ui) {
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_ABOUT as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
}

fn about_frame(ui: &Ui) -> usize {
    ui.v.about.frame as usize
}

/// Frames until the cracktro's own count reaches `frame`.
fn run_to(ui: &mut Ui, frame: usize) -> Vec<Sent> {
    let mut sent = Vec::new();
    while about_frame(ui) < frame {
        ui.frame();
        sent.push(drain(&mut ui.g));
    }
    sent
}

fn pages() -> &'static [credits_page] {
    unsafe { std::slice::from_raw_parts(credits_pages.as_ptr(), credits_page_count as usize) }
}

fn text(p: *const std::ffi::c_char) -> String {
    unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_owned()
}

#[test]
fn about_opens_from_the_menu_and_home_goes_back() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    assert_eq!(ui.top(), "ABOUT");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
    ui.settle();
    assert_eq!(
        ui.g.ovl_nfree, GRID_OVERLAYS as u8,
        "the cracktro's overlays stayed"
    );
    assert!(ui.g.region_bitmap.is_null(), "the logo's bitmap stayed");
    // The keys still play in it.
    open_about(&mut ui);
    ui.press(PANEL_BTN_OCT_UP);
    assert_eq!(ui.live().octave, 1);
}

#[test]
fn the_first_page_names_sidou_and_its_version_and_then_every_credits_page_shows() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    run_to(&mut ui, PAGE / 2);
    assert!(
        ui.row(HEADING).contains("SIDOU V1.2.3"),
        "{}",
        ui.row(HEADING)
    );
    assert!(
        ui.row(LINES).contains("SID SOUNDS FOR THE FM-1"),
        "{}",
        ui.row(LINES)
    );
    for (k, p) in pages().iter().enumerate() {
        run_to(&mut ui, (k + 1) * PAGE + PAGE / 2);
        let heading = text(p.heading);
        assert!(
            ui.row(HEADING).contains(&heading),
            "page {k}: {} not {heading}",
            ui.row(HEADING)
        );
        for (i, &line) in p.line.iter().take_while(|l| !l.is_null()).enumerate() {
            let line = text(line);
            assert!(
                ui.row(LINES + i).contains(&line),
                "page {k} line {i}: {line}"
            );
        }
    }
    // Then round again.
    run_to(&mut ui, (pages().len() + 1) * PAGE + PAGE / 2);
    assert!(ui.row(HEADING).contains("SIDOU V1.2.3"));
}

#[test]
fn a_page_cascades_in_and_out_two_rows_at_a_time() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    run_to(&mut ui, PAGE);
    let mut last = ui.g.attr;
    for k in 0..PAGE {
        ui.frame();
        let rows: Vec<usize> = (HEADING..ABOUT_RAMP_BOTTOM as usize)
            .filter(|&r| (0..COLS).any(|c| ui.g.attr[r * COLS + c] != last[r * COLS + c]))
            .collect();
        assert!(
            rows.len() <= 2,
            "frame {k} of the page changed rows {rows:?}"
        );
        // Drawn frame PAGE + k: even k is an even frame of the cracktro.
        assert!(
            rows.is_empty() || k % 2 == 0,
            "frame {k} of the page is odd"
        );
        last = ui.g.attr;
    }
}

#[test]
fn a_page_fades_in_and_out() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    let heading_fg = |ui: &Ui| ui.g.attr[HEADING * COLS + 15] & 15;
    let mut seen = Vec::new();
    run_to(&mut ui, PAGE);
    for _ in 0..PAGE {
        seen.push(heading_fg(&ui));
        ui.frame();
    }
    // Hidden (a blank cell is white on black), then the four levels up and down.
    let levels = [
        GRID_WHITE,
        GRID_BLUE,
        GRID_PURPLE,
        GRID_LIGHT_BLUE,
        GRID_CYAN,
    ]
    .map(|c| c as u8);
    let mut steps: Vec<u8> = seen.clone();
    steps.dedup();
    assert_eq!(
        steps,
        [0, 1, 2, 3, 4, 3, 2, 1, 0].map(|i| levels[i]),
        "{seen:?}"
    );
}

/// The rows of the bars and the scroller: the bottom of the screen, below the second ramp
/// line.
const BARS: std::ops::Range<usize> =
    ABOUT_BAR_ROW as usize..(ABOUT_BAR_ROW + ABOUT_BAR_ROWS) as usize;
const SCROLL: std::ops::Range<usize> = ABOUT_SCROLL_ROW as usize - 2..ABOUT_SCROLL_ROW as usize + 3;
const STARS: std::ops::Range<usize> = ABOUT_RAMP_TOP as usize + 1..ABOUT_RAMP_BOTTOM as usize;

#[test]
fn the_logo_bars_stars_and_scroller_are_where_they_belong() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    run_to(&mut ui, 60);
    let ovl = |r: usize| (0..COLS).filter(|&c| ui.g.ovl[r * COLS + c] != 0).count();
    // The logo: Cracktro Chrome's lettering band, pixel for pixel, at the top.
    assert_eq!(
        (ABOUT_LOGO_ROW, ABOUT_LOGO_ROWS),
        (0, 8),
        "the band is 240 x 64 at the top"
    );
    let img = sidou_tests::logo::load();
    let px = pixels(&ui.g);
    let band = LOGO_CHROME_BAND_ROW as usize * 8 * W;
    assert!(
        px[..64 * W] == img.px[band..band + 64 * W],
        "the logo is not the scene's lettering band"
    );
    // The scroller: overlays on its five rows, one or two a column (over the bars: app/about.c
    // asserts it).
    let scroll: usize = SCROLL.map(ovl).sum();
    assert!((24..=60).contains(&scroll), "{scroll} scroller cells");
    // Stars on black between the ramp lines.
    let stars: usize = STARS.map(ovl).sum();
    assert!((8..=24).contains(&stars), "{stars} star cells");
    // Bars behind the scroller: at least one bar's 20 pixel rows of colour at the left edge,
    // all of them in the bars' rows.
    let coloured: Vec<usize> = (64..W)
        .filter(|&y| {
            let c = px[y * W + 4];
            c != GRID_BLACK as u8 && c != GRID_WHITE as u8 // not a letter of the scroller
        })
        .filter(|&y| {
            !(ABOUT_RAMP_BOTTOM as usize * 8..ABOUT_RAMP_BOTTOM as usize * 8 + 8).contains(&y)
        })
        .collect();
    assert!(
        (20..=40).contains(&coloured.len()),
        "{} pixel rows of bars",
        coloured.len()
    );
    assert!(
        coloured.iter().all(|&y| BARS.contains(&(y / 8))),
        "{coloured:?}"
    );
    // The ramp lines.
    for r in [ABOUT_RAMP_TOP as usize, ABOUT_RAMP_BOTTOM as usize] {
        assert!((0..COLS).all(|c| ui.g.ch[r * COLS + c] == FONT_BOX_H as u8));
    }
}

/// The scroller's letters read over the bars: it is white, and no bar has white in it.
#[test]
fn the_scroller_reads_over_the_bars() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    for _ in 0..200 {
        ui.frame();
        for i in SCROLL.flat_map(|r| r * COLS..(r + 1) * COLS) {
            let o = usize::from(ui.g.ovl[i]);
            if o != 0 {
                assert_eq!(u32::from(ui.g.ovl_color[o - 1]), GRID_WHITE);
            }
        }
        // A bar cell's two colours, its top and bottom halves.
        for i in BARS.flat_map(|r| r * COLS..(r + 1) * COLS) {
            let a = u32::from(ui.g.attr[i]);
            assert!(
                a & 15 != GRID_WHITE && a >> 4 != GRID_WHITE,
                "cell {i}: {a:#04x}"
            );
        }
    }
}

/// Pulse Train, the alternative logo, fits About's band too: centred in it, with no bitmap.
#[test]
fn a_logo_drawn_in_cells_fits_the_band() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    ui.v.about.logo = unsafe { &logo_pulse_train };
    run_to(&mut ui, 20);
    assert!(ui.g.region_bitmap.is_null(), "the bitmap stayed");
    let ovl = |r: usize| (7..22).filter(|&c| ui.g.ovl[r * COLS + c] != 0).count();
    // 5 rows centred in 8: rows 1-5.
    assert!((1..6).all(|r| ovl(r) > 0), "no logo on rows 1-5");
    assert!([0, 6, 7].iter().all(|&r| ovl(r) == 0));
}

#[test]
fn every_cell_is_composed_every_frame() {
    let mut a = Ui::new();
    let mut b = Ui::new();
    open_about(&mut a);
    open_about(&mut b);
    run_to(&mut a, 30);
    run_to(&mut b, 30);
    // Any glyph and colour, and overlays where the cracktro draws them (the stars and the
    // scroller; the others only a view switch clears).
    for i in 0..COLS * COLS {
        let (c, r) = (i % COLS, i / COLS);
        b.g.ch[i] = b'X';
        b.g.attr[i] = 0x52;
        if STARS.contains(&r) || SCROLL.contains(&r) {
            unsafe { grid_overlay(&mut *b.g, c as u32, r as u32, [0x81; 8].as_ptr(), 9) };
        }
    }
    a.frame();
    b.frame();
    assert!(pixels(&a.g) == pixels(&b.g), "a cell kept what was there");
}

/// The wipe into About: the cracktro holds still under it, so each frame sends the wipe's rows
/// and nothing else (app/screen.h sizes a wipe for a view that does not move), after any time
/// in the menu.
#[test]
fn the_wipe_into_about_sends_only_its_rows() {
    let mut worst = Sent::default();
    for warm in 0..24 {
        let mut ui = Ui::new();
        ui.press(PANEL_BTN_HOME);
        ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_ABOUT as i32);
        ui.settle();
        for _ in 0..warm {
            ui.frame();
        }
        drain(&mut ui.g);
        ui.turn(PANEL_ENC_ALGORITHM, 1);
        assert_eq!(ui.top(), "ABOUT");
        let frames = wipe_frames(&mut || {
            ui.frame();
            (drain(&mut ui.g), ui.v.s.wipe != 0)
        });
        worst = frames
            .iter()
            .copied()
            .fold(worst, |w, s| if s.cells > w.cells { s } else { w });
        assert_wipe_holds_still(&format!("About after {warm} frames"), &frames);
    }
    println!(
        "wipe into About: worst frame {worst:?} {:.2} ms",
        worst.spi_ms()
    );
}

/// How the cracktro spends the SPI: the averages and worst frames over a full round of the
/// credits.
#[test]
fn the_cracktro_stays_within_the_budget() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    drain(&mut ui.g);
    let frames = (pages().len() + 1) * PAGE;
    let mut most_overlays = 0;
    let mut sent = Vec::new();
    while about_frame(&ui) < frames {
        ui.frame();
        sent.push(drain(&mut ui.g));
        most_overlays = most_overlays.max(GRID_OVERLAYS as usize - usize::from(ui.g.ovl_nfree));
    }
    let n = sent.len() as f64;
    let avg = sent.iter().map(|s| s.cells).sum::<usize>() as f64 / n;
    let avg_windows = sent.iter().map(|s| s.windows).sum::<usize>() as f64 / n;
    let avg_ms = sent.iter().map(|s| s.spi_ms()).sum::<f64>() / n;
    let worst = sent.iter().copied().max_by_key(|s| s.bytes()).unwrap();
    let max_cells = sent.iter().map(|s| s.cells).max().unwrap();
    println!(
        "about: {} frames, {avg:.1} cells and {avg_windows:.1} windows a frame ({avg_ms:.2} ms), \
         at most {max_cells} cells; worst frame {worst:?} {:.2} ms; {most_overlays} overlays",
        sent.len(),
        worst.spi_ms()
    );
    // docs/plans/2026-10-15-m1e-look-and-feel.md: trimmed from the prototype's 240 cells a
    // frame to about 120.
    assert!(avg <= 120.0, "{avg:.1} cells a frame on average");
    assert!(
        worst.spi_ms() <= SPI_BUDGET_MS,
        "a frame took {:.1} ms",
        worst.spi_ms()
    );
    assert!(
        most_overlays < GRID_OVERLAYS as usize,
        "{most_overlays} overlays: the pool ran out"
    );
}
