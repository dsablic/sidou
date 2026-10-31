// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::CStr;

use sidou_sys::*;

use crate::{drain, pixels};

const W: usize = 240;
const COL: u32 = 8;
const ROW: u32 = 3;
const RAMP: [u8; 7] = [1, 7, 13, 3, 14, 4, 10];
const BG: u8 = GRID_BLACK as u8;
const U_LEFT_STEM_X: usize = 4 * 24 + 1;

fn pulse() -> &'static logo {
    unsafe { &logo_pulse_train }
}

fn fresh() -> Box<grid> {
    let mut g = Box::<grid>::default();
    unsafe { grid_init(&mut *g, b' ', (GRID_BLACK << 4 | GRID_WHITE) as u8) };
    drain(&mut g);
    g
}

fn draw(g: &mut grid, frame: u32) {
    unsafe { logo_draw(pulse(), g, COL, ROW, frame, BG) };
}

fn logo_pixels(g: &grid) -> Vec<u8> {
    let px = pixels(g);
    let (x0, y0) = (COL as usize * 8, ROW as usize * 8);
    (0..40)
        .flat_map(|y| (0..120).map(move |x| (x, y)))
        .map(|(x, y)| px[(y0 + y) * W + x0 + x])
        .collect()
}

const SIDOU_IN_THE_5X7_FACE: [[&str; 7]; 5] = [
    [
        ".####", "#....", "#....", ".###.", "....#", "....#", "####.",
    ],
    [
        "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "#####",
    ],
    [
        "####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####.",
    ],
    [
        ".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
    ],
    [
        "#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
    ],
];

fn chrome() -> &'static logo {
    unsafe { &logo_cracktro_chrome }
}

fn metal() -> &'static logo {
    unsafe { &logo_metal_chrome }
}

fn name(l: &logo) -> &str {
    unsafe { CStr::from_ptr(l.name) }.to_str().unwrap()
}

const CRACKTRO_BAND: std::ops::Range<usize> = 9..17;
const METAL_BAND: std::ops::Range<usize> = 10..19;

#[test]
fn the_default_logo_is_metal_chrome_and_the_others_the_alternatives() {
    let l = unsafe { &*logo_default };
    assert!(std::ptr::eq(l, metal()));
    assert_eq!(name(l), "METAL CHROME");
    assert_eq!((l.cols, l.rows), (30, 9));
    assert_eq!(unsafe { logo_center_col(l) }, 0);
    assert!(l.scene.is_some(), "a full-screen scene for the title card");
    assert_eq!(LOGO_METAL_BAND_ROW as usize, METAL_BAND.start);
    let c = chrome();
    assert_eq!(name(c), "CRACKTRO CHROME");
    assert_eq!((c.cols, c.rows), (30, 8));
    assert!(c.scene.is_some());
    let p = pulse();
    assert_eq!(name(p), "PULSE TRAIN");
    assert_eq!((p.cols, p.rows), (15, 5));
    assert_eq!(unsafe { logo_center_col(p) }, 7);
    assert!(p.scene.is_none());
}

#[test]
fn metal_chrome_is_the_scenes_lettering_band_pixel_for_pixel() {
    let img = sidou_tests::logo::load("metal-chrome");
    let mut g = fresh();
    unsafe { logo_draw(metal(), &mut *g, 0, ROW, 0, BG) };
    let px = pixels(&g);
    let y0 = ROW as usize * 8;
    let rows = METAL_BAND.len() * 8;
    for y in 0..rows {
        let from = METAL_BAND.start * 8 + y;
        assert_eq!(
            px[(y0 + y) * W..(y0 + y + 1) * W],
            img.px[from * W..(from + 1) * W],
            "pixel row {y}"
        );
    }
    let lettering = [GRID_RED, GRID_ORANGE, GRID_YELLOW];
    for y in 40..160 {
        let inside = (METAL_BAND.start * 8..METAL_BAND.end * 8).contains(&y);
        let has = img.px[y * W..(y + 1) * W]
            .iter()
            .any(|&c| lettering.contains(&u32::from(c)));
        assert!(
            inside || !has,
            "lettering on pixel row {y}, outside the band"
        );
    }
    let chrome = [GRID_WHITE, GRID_RED, GRID_ORANGE, GRID_YELLOW];
    let row_has = |y: usize, cs: &[u32]| {
        img.px[y * W..(y + 1) * W]
            .iter()
            .any(|&c| cs.contains(&u32::from(c)))
    };
    assert!((83..142).all(|y| row_has(y, &chrome)));
    assert!(!row_has(82, &lettering) && !row_has(142, &lettering));
}

#[test]
fn metal_chromes_scene_is_the_png_whole() {
    let img = sidou_tests::logo::load("metal-chrome");
    let mut g = fresh();
    unsafe { metal().scene.unwrap()(&mut *g, 0) };
    assert!(
        pixels(&g) == img.px,
        "the scene is not assets/logo/metal-chrome.png"
    );
    assert_eq!(drain(&mut g).cells, 900);
    for f in 1..50 {
        unsafe { metal().scene.unwrap()(&mut *g, f) };
        assert_eq!(drain(&mut g).cells, 0, "frame {f}: the scene is still");
    }
}

#[test]
fn cracktro_chrome_is_the_scenes_lettering_band_pixel_for_pixel() {
    let img = sidou_tests::logo::load("cracktro-chrome");
    let mut g = fresh();
    unsafe { logo_draw(chrome(), &mut *g, 0, ROW, 0, BG) };
    let px = pixels(&g);
    let y0 = ROW as usize * 8;
    for y in 0..64 {
        assert_eq!(
            px[(y0 + y) * W..(y0 + y + 1) * W],
            img.px[(CRACKTRO_BAND.start * 8 + y) * W..(CRACKTRO_BAND.start * 8 + y + 1) * W],
            "pixel row {y}"
        );
    }
    let lettering = [
        GRID_PURPLE,
        GRID_BROWN,
        GRID_ORANGE,
        GRID_LIGHT_RED,
        GRID_YELLOW,
    ];
    for y in 40..168 {
        let inside = (CRACKTRO_BAND.start * 8..CRACKTRO_BAND.end * 8).contains(&y);
        let has = img.px[y * W..(y + 1) * W]
            .iter()
            .any(|&c| lettering.contains(&u32::from(c)));
        assert!(
            inside || !has,
            "lettering on pixel row {y}, outside the band"
        );
    }
    assert!((76..136).all(|y| img.px[y * W..(y + 1) * W].iter().any(|&c| c != BG)));
}

#[test]
fn cracktro_chromes_scene_is_the_png_whole() {
    let img = sidou_tests::logo::load("cracktro-chrome");
    let mut g = fresh();
    unsafe { chrome().scene.unwrap()(&mut *g, 0) };
    assert!(
        pixels(&g) == img.px,
        "the scene is not assets/logo/cracktro-chrome.png"
    );
    assert_eq!(drain(&mut g).cells, 900);
    for f in 1..50 {
        unsafe { chrome().scene.unwrap()(&mut *g, f) };
        assert_eq!(drain(&mut g).cells, 0, "frame {f}: the scene is still");
    }
}

#[test]
fn the_still_is_slotted_raster_letters_over_a_pulse_wave() {
    let mut g = fresh();
    draw(&mut g, 0);
    let img = logo_pixels(&g);
    for (k, letter) in SIDOU_IN_THE_5X7_FACE.iter().enumerate() {
        for (row, bits) in letter.iter().enumerate() {
            for (col, b) in bits.chars().enumerate() {
                for dy in 0..4 {
                    for dx in 0..4 {
                        let (x, y) = (k * 24 + col * 4 + dx, row * 4 + dy);
                        let want = if b == '#' && dy < 3 { RAMP[row] } else { BG };
                        assert_eq!(img[y * 120 + x], want, "letter {k} ({x}, {y})");
                    }
                }
            }
        }
    }
    for x in (0..5).map(|k| k * 24 + 20).flat_map(|x| x..x + 4) {
        assert!((0..28).all(|y| img[y * 120 + x] == BG), "column {x}");
    }
    let cyan = GRID_CYAN as u8;
    for x in 1..119 {
        let high = x % 24 < 12;
        let edge = (x % 12 == 0) || (x % 12 == 11);
        for y in 28..40 {
            let level = if high { [31, 32] } else { [36, 37] }.contains(&y);
            let want = level || (edge && (31..=37).contains(&y));
            assert_eq!(img[y * 120 + x] == cyan, want, "pulse at ({x}, {y})");
        }
    }
}

#[test]
fn each_draws_only_its_footprint() {
    for l in [pulse(), chrome(), metal()] {
        let mut g = fresh();
        let before = (g.ch, g.attr);
        let col = unsafe { logo_center_col(l) };
        for f in 0..40 {
            unsafe { logo_draw(l, &mut *g, col, ROW, f, BG) };
        }
        let cols = GRID_COLS as usize;
        let (w, h) = (u32::from(l.cols), u32::from(l.rows));
        let px = pixels(&g);
        let mut clean = fresh();
        let blank = pixels(&clean);
        for i in 0..cols * cols {
            let (c, r) = ((i % cols) as u32, (i / cols) as u32);
            let inside = (col..col + w).contains(&c) && (ROW..ROW + h).contains(&r);
            if !inside {
                assert_eq!(
                    (g.ch[i], g.attr[i], g.ovl[i]),
                    (before.0[i], before.1[i], 0),
                    "{}",
                    name(l)
                );
                let (x, y) = (c as usize * 8, r as usize * 8);
                assert_eq!(px[y * W + x], blank[y * W + x], "{}: ({c}, {r})", name(l));
            }
        }
        let _ = drain(&mut clean);
    }
}

#[test]
fn every_cell_of_the_footprint_is_redrawn_whatever_was_there() {
    for l in [pulse(), chrome(), metal()] {
        let mut clean = fresh();
        unsafe { logo_draw(l, &mut *clean, 0, ROW, 6, BG) };
        let mut g = fresh();
        for r in 0..30 {
            for c in 0..30 {
                unsafe {
                    grid_put(&mut *g, c, r, b'X', 0x52);
                    grid_overlay(&mut *g, c, r, [0xff; 8].as_ptr(), 9);
                }
            }
        }
        unsafe { logo_draw(l, &mut *g, 0, ROW, 6, BG) };
        let rows = ROW as usize * 8 * W..(ROW as usize + usize::from(l.rows)) * 8 * W;
        let w = usize::from(l.cols) * 8;
        let (a, b) = (pixels(&g), pixels(&clean));
        for y in rows.clone().step_by(W) {
            assert_eq!(a[y..y + w], b[y..y + w], "{}", name(l));
        }
    }
}

#[test]
fn the_stripes_roll_and_the_wave_moves_every_four_frames() {
    let mut g = fresh();
    draw(&mut g, 0);
    drain(&mut g);
    let mut last = logo_pixels(&g);
    let band_colour = |img: &[u8], band: usize| img[band * 4 * 120 + U_LEFT_STEM_X];
    let mut stripes = Vec::new();
    for f in 1..=28u32 {
        draw(&mut g, f);
        let sent = drain(&mut g);
        let img = logo_pixels(&g);
        match f % 4 {
            1 | 3 => assert_eq!(sent.cells, 0, "frame {f} changed {sent:?}"),
            2 => {
                assert_ne!(img[..28 * 120], last[..28 * 120], "frame {f}: no roll");
                assert_eq!(
                    img[28 * 120..],
                    last[28 * 120..],
                    "frame {f}: the wave moved"
                );
            }
            _ => {
                assert_eq!(
                    img[..28 * 120],
                    last[..28 * 120],
                    "frame {f}: the stripes moved"
                );
                assert_ne!(
                    img[28 * 120..],
                    last[28 * 120..],
                    "frame {f}: the wave stood"
                );
            }
        }
        stripes.push(band_colour(&img, 0));
        last = img;
    }
    let rolls: Vec<u8> = stripes.iter().copied().step_by(4).collect();
    for w in rolls.windows(2) {
        let k = RAMP.iter().position(|&c| c == w[0]).unwrap();
        assert_eq!(w[1], RAMP[(k + 6) % 7], "{rolls:?}");
    }
}

#[test]
fn each_ones_declared_cost_and_overlays_are_what_it_takes() {
    for l in [pulse(), chrome(), metal()] {
        let mut g = fresh();
        unsafe { logo_draw(l, &mut *g, 0, ROW, 0, BG) };
        drain(&mut g);
        let (mut worst, mut overlays) = ([0usize; 4], 0usize);
        for f in 1..=200u32 {
            unsafe { logo_draw(l, &mut *g, 0, ROW, f, BG) };
            let k = f as usize % 4;
            worst[k] = worst[k].max(drain(&mut g).cells);
            overlays = overlays.max(GRID_OVERLAYS as usize - usize::from(g.ovl_nfree));
        }
        println!(
            "{}: worst cells by frame mod 4 {worst:?}, {overlays} overlays",
            name(l)
        );
        let worst = *worst.iter().max().unwrap();
        assert_eq!(usize::from(l.cells_per_frame), worst, "{}", name(l));
        assert_eq!(usize::from(l.overlays), overlays, "{}", name(l));
    }
}

#[test]
fn each_logo_fits_each_consumers_slot() {
    for l in [pulse(), chrome(), metal()] {
        let (cols, rows) = (u32::from(l.cols), u32::from(l.rows));
        let mut slots = vec![("About's band", ABOUT_LOGO_ROWS)];
        if l.scene.is_none() {
            slots.push(("the title card", BOOT_LOGO_ROWS));
        }
        for (what, max_rows) in slots {
            assert!(cols <= GRID_COLS, "{}: {what}: {cols} columns", name(l));
            assert!(
                rows <= max_rows,
                "{}: {what}: {rows} rows, the slot has {max_rows}",
                name(l)
            );
        }
        let about = u32::from(l.overlays) + ABOUT_STARS + 2 * GRID_COLS;
        assert!(about < GRID_OVERLAYS, "About: {about} overlays");
        assert!(u32::from(l.overlays) < GRID_OVERLAYS, "the title card");
    }
}
