// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The logo (app/logo.c): Pulse Train drawn on the cell grid, what it covers and what each
//! frame of its animation costs.

use std::ffi::CStr;

use sidou_sys::*;

use crate::{drain, pixels};

const W: usize = 240;
const COL: u32 = 8;
const ROW: u32 = 3;
const RAMP: [u8; 7] = [1, 7, 13, 3, 14, 4, 10];
const BG: u8 = GRID_BLACK as u8;

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

/// The logo's 120 x 40 pixels from a grid it was drawn on.
fn image(g: &grid) -> Vec<u8> {
    let px = pixels(g);
    let (x0, y0) = (COL as usize * 8, ROW as usize * 8);
    (0..40)
        .flat_map(|y| (0..120).map(move |x| (x, y)))
        .map(|(x, y)| px[(y0 + y) * W + x0 + x])
        .collect()
}

/// S I D O U in the 5 x 7 face, rows top first, '#' set.
const FACE: [[&str; 7]; 5] = [
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

#[test]
fn the_default_logo_is_pulse_train_and_fits_the_screen() {
    let l = unsafe { &*logo_default };
    assert!(std::ptr::eq(l, pulse()));
    assert_eq!(
        unsafe { CStr::from_ptr(l.name) }.to_str(),
        Ok("PULSE TRAIN")
    );
    assert_eq!((l.cols, l.rows), (15, 5));
    assert_eq!(unsafe { logo_center_col(l) }, 7);
}

#[test]
fn the_still_is_slotted_raster_letters_over_a_pulse_wave() {
    let mut g = fresh();
    draw(&mut g, 0);
    let img = image(&g);
    for (k, letter) in FACE.iter().enumerate() {
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
    // The gaps between the letters stay clear.
    for x in (0..5).map(|k| k * 24 + 20).flat_map(|x| x..x + 4) {
        assert!((0..28).all(|y| img[y * 120 + x] == BG), "column {x}");
    }
    // Frame 0: duty 12 of 24, no shift: high on rows 31-32 where x % 24 < 12, low on 36-37.
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
fn it_draws_only_its_footprint() {
    let mut g = fresh();
    let before = (g.ch, g.attr);
    for f in 0..40 {
        draw(&mut g, f);
    }
    let cols = GRID_COLS as usize;
    for i in 0..cols * cols {
        let (c, r) = ((i % cols) as u32, (i / cols) as u32);
        let inside = (COL..COL + 15).contains(&c) && (ROW..ROW + 5).contains(&r);
        if !inside {
            assert_eq!(
                (g.ch[i], g.attr[i], g.ovl[i]),
                (before.0[i], before.1[i], 0)
            );
        }
    }
}

#[test]
fn every_cell_of_the_footprint_is_redrawn_whatever_was_there() {
    // Compose draws every cell: over garbage and stray overlays the logo looks as on a clear
    // grid.
    let mut clean = fresh();
    draw(&mut clean, 6);
    let mut g = fresh();
    for r in 0..30 {
        for c in 0..30 {
            unsafe {
                grid_put(&mut *g, c, r, b'X', 0x52);
                grid_overlay(&mut *g, c, r, [0xff; 8].as_ptr(), 9);
            }
        }
    }
    draw(&mut g, 6);
    assert_eq!(image(&g), image(&clean));
}

#[test]
fn the_stripes_roll_and_the_wave_moves_every_four_frames() {
    let mut g = fresh();
    draw(&mut g, 0);
    drain(&mut g);
    let mut last = image(&g);
    let band_colour = |img: &[u8], band: usize| img[band * 4 * 120 + 4 * 24 + 1]; // U's left stem
    let mut stripes = Vec::new();
    for f in 1..=28u32 {
        draw(&mut g, f);
        let sent = drain(&mut g);
        let img = image(&g);
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
    // Each roll moves the ramp down one band: band 0 takes the colour before it in the ramp.
    let rolls: Vec<u8> = stripes.iter().copied().step_by(4).collect();
    for w in rolls.windows(2) {
        let k = RAMP.iter().position(|&c| c == w[0]).unwrap();
        assert_eq!(w[1], RAMP[(k + 6) % 7], "{rolls:?}");
    }
}

#[test]
fn its_declared_cost_and_overlays_are_what_it_takes() {
    let mut g = fresh();
    draw(&mut g, 0);
    drain(&mut g);
    let (mut worst, mut overlays) = ([0usize; 4], 0usize);
    // Two full cycles of the stripes (7 rolls) and more than one of the duty sweep.
    for f in 1..=200u32 {
        draw(&mut g, f);
        let k = f as usize % 4;
        worst[k] = worst[k].max(drain(&mut g).cells);
        overlays = overlays.max(GRID_OVERLAYS as usize - usize::from(g.ovl_nfree));
    }
    println!("pulse train: worst cells by frame mod 4 {worst:?}, {overlays} overlays");
    let worst = *worst.iter().max().unwrap();
    assert_eq!(usize::from(pulse().cells_per_frame), worst);
    assert_eq!(usize::from(pulse().overlays), overlays);
}
