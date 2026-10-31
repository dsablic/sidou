// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::credits::compiled_pages;
use crate::views::Ui;
use crate::{assert_wipe_holds_still, c_utf8, drain, pixels, wipe_frames, Sent, SPI_BUDGET_MS};

const COLS: usize = GRID_COLS as usize;
const W: usize = COLS * 8;
const PAGE: usize = ABOUT_PAGE_FRAMES as usize;
const HEADING: usize = ABOUT_HEADING_ROW as usize;
const LINES: usize = ABOUT_LINES_ROW as usize;

pub(crate) fn open_about(ui: &mut Ui) {
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_ABOUT as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
}

fn about_frame(ui: &Ui) -> usize {
    ui.v.about.frame as usize
}

fn run_to(ui: &mut Ui, frame: usize) -> Vec<Sent> {
    let mut sent = Vec::new();
    while about_frame(ui) < frame {
        ui.frame();
        sent.push(drain(&mut ui.g));
    }
    sent
}

struct SongWithBuiltinTune {
    _model: Box<gt2_song>,
    _ctl: Box<song_ctl>,
    io: Box<song_io>,
}

impl SongWithBuiltinTune {
    fn new() -> Self {
        unsafe extern "C" fn builtin(_: *mut std::ffi::c_void, _: u32, _: *mut u8, _: u32) -> u32 {
            0
        }
        let mut model = Box::<gt2_song>::default();
        let mut ctl = Box::new(song_ctl {
            song: &mut *model,
            ..Default::default()
        });
        let io = Box::new(song_io {
            ctl: &mut *ctl,
            hooks: song_io_hooks {
                builtin: Some(builtin),
                ..Default::default()
            },
            ..Default::default()
        });
        Self {
            _model: model,
            _ctl: ctl,
            io,
        }
    }
}

fn changed_rows(ui: &Ui, last: &[u8]) -> Vec<usize> {
    (HEADING..ABOUT_RAMP_BOTTOM as usize)
        .filter(|&r| (0..COLS).any(|c| ui.g.attr[r * COLS + c] != last[r * COLS + c]))
        .collect()
}

#[test]
fn about_opens_from_the_menu_plays_the_keys_and_home_goes_back() {
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
    open_about(&mut ui);
    ui.press(PANEL_BTN_OCT_UP);
    assert_eq!(ui.live().octave, 1);
}

#[test]
fn unsaved_song_keeps_about_silent_and_the_silent_scroller_stays_in_budget() {
    let mut ui = Ui::without_cards();
    let mut io = Box::new(song_io {
        dirty: 1,
        ..Default::default()
    });
    unsafe { views_song_io(&mut *ui.v, &mut *io) };
    drain(&mut ui.g);
    open_about(&mut ui);
    drain(&mut ui.g);
    assert_eq!(ui.v.about.silent, 1);
    assert_eq!(io.about, 0, "About must not replace an unsaved song");
    for _ in 0..300 {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(sent.spi_ms() <= SPI_BUDGET_MS, "{sent:?}");
    }
    ui.press(PANEL_BTN_HOME);
    assert_eq!(io.dirty, 1);
    assert_eq!(io.state, SONG_IO_IDLE as u8);
}

#[test]
fn clean_about_requests_its_tune_once_and_close_requests_restoration() {
    let mut ui = Ui::without_cards();
    let mut tune = SongWithBuiltinTune::new();
    let io = &mut *tune.io;
    unsafe { views_song_io(&mut *ui.v, io) };
    open_about(&mut ui);
    assert_eq!(io.about, 1);
    assert_eq!(io.state, SONG_IO_STOP as u8);
    assert_eq!(ui.v.about.silent, 0);
    io.about_play = 1;
    ui.frames(10);
    assert_eq!(io.about_play, 1, "frames must not reopen About's music");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(io.about_close, 1);
    assert_eq!(io.about_play, 0);
}

#[test]
fn the_song_views_do_not_open_while_abouts_tune_is_the_song() {
    let mut ui = Ui::without_cards();
    let mut tune = SongWithBuiltinTune::new();
    let io = &mut *tune.io;
    unsafe { views_song_io(&mut *ui.v, io) };
    open_about(&mut ui);
    assert_eq!(io.about, 1);
    ui.press(PANEL_BTN_SEQ);
    ui.settle();
    assert_eq!(ui.top(), "ABOUT", "SEQ refused");
    assert_eq!(ui.toast().as_deref(), Some("LEAVE ABOUT FIRST"));
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    assert_eq!(ui.top(), "MENU");
    assert_eq!(io.about, 1, "the restore runs");
    ui.choose_menu_item(VIEWS_MENU_TRACKER);
    ui.settle();
    assert_eq!(ui.top(), "MENU", "TRACKER refused");
    assert_eq!(ui.toast().as_deref(), Some("LOADING"));
    ui.press(PANEL_BTN_SEQ);
    ui.settle();
    assert_eq!(ui.top(), "MENU", "SEQ refused");
    io.about = 0;
    ui.press(PANEL_BTN_SEQ);
    ui.settle();
    assert_eq!(ui.top(), "STEP");
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
    for (k, p) in compiled_pages().iter().enumerate() {
        run_to(&mut ui, (k + 1) * PAGE + PAGE / 2);
        let heading = c_utf8(p.heading);
        assert!(
            ui.row(HEADING).contains(&heading),
            "page {k}: {} not {heading}",
            ui.row(HEADING)
        );
        for (i, &line) in p.line.iter().take_while(|l| !l.is_null()).enumerate() {
            let line = c_utf8(line);
            assert!(
                ui.row(LINES + i).contains(&line),
                "page {k} line {i}: {line}"
            );
        }
    }
    run_to(&mut ui, (compiled_pages().len() + 1) * PAGE + PAGE / 2);
    assert!(ui.row(HEADING).contains("SIDOU V1.2.3"));
}

#[test]
fn the_first_pages_heading_is_at_2x_and_fits() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    run_to(&mut ui, PAGE / 2);
    assert_eq!(ui.big(HEADING, 3, 2, 12), "SIDOU V1.2.3");
    let long = std::ffi::CString::new("V10.20.30-RC1").unwrap();
    let mut ui = Ui::new();
    ui.v.version = long.as_ptr();
    open_about(&mut ui);
    let mut last = ui.g.attr;
    let start = about_frame(&ui);
    for _ in start..PAGE {
        ui.frame();
        let rows = changed_rows(&ui, &last);
        assert!(
            rows.len() <= 2,
            "frame {} changed rows {rows:?}",
            about_frame(&ui)
        );
        last = ui.g.attr;
        if about_frame(&ui) == PAGE / 2 {
            assert_eq!(
                ui.big(HEADING, 2, 2, 13),
                "V10.20.30-RC1",
                "the version alone"
            );
            assert!(ui.row(LINES).contains("SID SOUNDS FOR THE FM-1"));
        }
    }
}

#[test]
fn a_page_cascades_in_and_out_two_rows_at_a_time() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    run_to(&mut ui, PAGE);
    let mut last = ui.g.attr;
    for k in 0..PAGE {
        ui.frame();
        let rows = changed_rows(&ui, &last);
        assert!(
            rows.len() <= 2,
            "frame {k} of the page changed rows {rows:?}"
        );
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
    assert_eq!(
        (ABOUT_LOGO_ROW, ABOUT_LOGO_ROWS),
        (0, 9),
        "the band is 240 x 72 at the top"
    );
    let img = sidou_tests::logo::load("metal-chrome");
    let px = pixels(&ui.g);
    let band = LOGO_METAL_BAND_ROW as usize * 8 * W;
    assert!(
        px[..72 * W] == img.px[band..band + 72 * W],
        "the logo is not the scene's lettering band"
    );
    let scroll: usize = SCROLL.map(ovl).sum();
    assert!((24..=60).contains(&scroll), "{scroll} scroller cells");
    let stars: usize = STARS.map(ovl).sum();
    assert!((8..=24).contains(&stars), "{stars} star cells");
    let coloured: Vec<usize> = (72..W)
        .filter(|&y| {
            let c = px[y * W + 4];
            c != GRID_BLACK as u8 && c != GRID_WHITE as u8
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
    for r in [ABOUT_RAMP_TOP as usize, ABOUT_RAMP_BOTTOM as usize] {
        assert!((0..COLS).all(|c| ui.g.ch[r * COLS + c] == FONT_BOX_H as u8));
    }
}

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
        for i in BARS.flat_map(|r| r * COLS..(r + 1) * COLS) {
            let a = u32::from(ui.g.attr[i]);
            assert!(
                a & 15 != GRID_WHITE && a >> 4 != GRID_WHITE,
                "cell {i}: {a:#04x}"
            );
        }
    }
}

#[test]
fn a_logo_drawn_in_cells_fits_the_band() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    ui.v.about.logo = unsafe { &logo_pulse_train };
    run_to(&mut ui, 20);
    assert!(ui.g.region_bitmap.is_null(), "the bitmap stayed");
    let ovl = |r: usize| (7..22).filter(|&c| ui.g.ovl[r * COLS + c] != 0).count();
    assert!((2..7).all(|r| ovl(r) > 0), "no logo on rows 2-6");
    assert!([0, 1, 7, 8].iter().all(|&r| ovl(r) == 0));
}

#[test]
fn cracktro_chrome_fits_the_band() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    ui.v.about.logo = unsafe { &logo_cracktro_chrome };
    run_to(&mut ui, 20);
    let img = sidou_tests::logo::load("cracktro-chrome");
    let px = pixels(&ui.g);
    let band = LOGO_CHROME_BAND_ROW as usize * 8 * W;
    assert!(px[..64 * W] == img.px[band..band + 64 * W]);
    assert!(px[64 * W..72 * W].iter().all(|&c| c == GRID_BLACK as u8));
}

#[test]
fn every_cell_is_composed_every_frame() {
    let mut a = Ui::new();
    let mut b = Ui::new();
    open_about(&mut a);
    open_about(&mut b);
    run_to(&mut a, 30);
    run_to(&mut b, 30);
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

#[test]
fn the_wipe_into_about_sends_only_its_rows() {
    let mut worst = Sent::default();
    for warm in 0..24 {
        let mut ui = Ui::new();
        ui.press(PANEL_BTN_HOME);
        ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_ABOUT as i32);
        ui.settle();
        ui.frames(warm);
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

#[test]
fn the_cracktro_averages_120_cells_a_frame_within_the_spi_and_overlay_budgets() {
    let mut ui = Ui::new();
    open_about(&mut ui);
    drain(&mut ui.g);
    let frames = (compiled_pages().len() + 1) * PAGE;
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
