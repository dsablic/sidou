// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! RED MOON RITE's art in flash: app/scene_rmr_art.c is generated from assets/scene/*.png
//! (sidou_tests::scene_art). SIDOU_SCENE_UPDATE=1 rewrites it.

use sidou_sys::*;
use sidou_tests::scene_art::{self, Picture, CLEAR, SKY_Y};

/// Each image's compiled struct, by its place in scene_art::sources().
fn compiled() -> Vec<&'static scene_image> {
    let mut out = Vec::new();
    unsafe {
        out.extend((*std::ptr::addr_of!(scene_rmr_barb)).iter().flatten());
        out.extend(
            (*std::ptr::addr_of!(scene_rmr_barb_unarmed))
                .iter()
                .flatten(),
        );
        out.extend(scene_rmr_sword.iter());
        out.push(&scene_rmr_sword_stuck);
        out.extend(scene_rmr_sky.iter());
        out.push(&scene_rmr_arena);
        out.extend([
            &scene_rmr_hud,
            &scene_rmr_blade_empty,
            &scene_rmr_blade_full,
            &scene_rmr_blade_tip,
        ]);
    }
    out
}

/// A compiled image's frame, a row at a time, through scene_image_row.
fn frame_of(s: &scene_image, mirror: bool) -> Picture {
    let (w, h) = (usize::from(s.fw), usize::from(s.fh));
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        let mut row = vec![0x55u8; w];
        unsafe {
            scene_image_row(
                s,
                0,
                y as u32,
                w as u32,
                i32::from(mirror),
                row.as_mut_ptr(),
            )
        };
        px.extend(row);
    }
    Picture { w, h, px }
}

fn mirrored(p: &Picture) -> Picture {
    let px =
        p.px.chunks(p.w)
            .flat_map(|r| r.iter().rev().copied())
            .collect();
    Picture { w: p.w, h: p.h, px }
}

#[test]
fn the_generated_file_is_up_to_date() {
    let update = std::env::var("SIDOU_SCENE_UPDATE").as_deref() == Ok("1");
    let stale = scene_art::check_or_write(update);
    assert!(
        stale.is_empty(),
        "{stale:?} is stale; rerun with SIDOU_SCENE_UPDATE=1"
    );
    for line in scene_art::c_source().lines() {
        assert!(
            line.len() <= 100,
            "generated line over 100 columns: {line:?}"
        );
    }
}

#[test]
fn the_header_and_the_converter_agree_on_the_set() {
    assert_eq!(SCENE_RMR_POSES as usize, scene_art::POSES.len());
    assert_eq!(SCENE_RMR_UNARMED_POSES as usize, scene_art::UNARMED.len());
    assert_eq!(SCENE_RMR_SKIES as usize, scene_art::SKIES.len());
    assert_eq!(SCENE_RMR_SKY_Y as usize, SKY_Y);
    assert_eq!(compiled().len(), scene_art::sources().len());
    let srcs = scene_art::sources();
    let clear = scene_art::clear_colour(&srcs);
    for (src, s) in srcs.iter().zip(compiled()) {
        assert_eq!(
            (usize::from(s.fw), usize::from(s.fh)),
            (src.frame.w, src.frame.h),
            "{}",
            src.png
        );
        assert_eq!((s.gx, s.clear), (src.gx, clear), "{}", src.png);
        assert_eq!((s.ax, s.ay), src.anchor, "{}", src.png);
    }
    // A fighter's anchor is his feet: the prototype drew him from (x - 58, FEET - 112), and
    // mirrored from (x - 70, ...): 70 = 128 - 58.
    let guard = unsafe { &scene_rmr_barb[1][SCENE_RMR_GUARD as usize] };
    assert_eq!(
        (
            u32::from(guard.fw) * u32::from(guard.gx) - u32::from(guard.ax),
            guard.ay
        ),
        (70, 112)
    );
    assert_eq!(SCENE_RMR_FEET, 228);
}

#[test]
fn every_image_decodes_to_its_png_pixel_for_pixel() {
    let srcs = scene_art::sources();
    let clear = scene_art::clear_colour(&srcs);
    let coded = scene_art::code(&srcs, clear);
    for (i, (src, s)) in srcs.iter().zip(compiled()).enumerate() {
        // The converter's own model, then the firmware's decoder, both ways round.
        assert_eq!(
            scene_art::decode(&coded, i, &srcs, clear),
            src.frame,
            "{}: the model",
            src.png
        );
        let c = frame_of(s, false);
        assert_eq!(c, src.frame, "{}: scene_image_row", src.png);
        assert_eq!(
            frame_of(s, true),
            mirrored(&src.frame),
            "{}: mirrored",
            src.png
        );
        // Any span of a row, entered part way, writes exactly its n pixels.
        for y in (0..c.h).step_by(7) {
            let spans = [(0, 1), (1, 3), (5, 8), (c.w / 2, 4), (c.w - 4, 4)];
            for (x, n) in spans.into_iter().filter(|&(x, n)| x + n <= c.w) {
                for mirror in [false, true] {
                    let mut part = vec![0x55u8; n + 1];
                    unsafe {
                        scene_image_row(
                            s,
                            x as u32,
                            y as u32,
                            n as u32,
                            i32::from(mirror),
                            part.as_mut_ptr(),
                        )
                    };
                    let want = if mirror {
                        &mirrored(&src.frame).px[y * c.w + x..][..n]
                    } else {
                        &src.frame.px[y * c.w + x..][..n]
                    };
                    assert_eq!(&part[..n], want, "{} row {y} from {x}", src.png);
                    assert_eq!(part[n], 0x55, "{} row {y} from {x}: wrote past n", src.png);
                }
            }
        }
    }
}

#[test]
fn each_sky_is_its_gradient_with_the_moon_wisps_and_hills_over_it() {
    for i in 0..scene_art::SKIES.len() {
        let want = scene_art::load(&format!("sky-{i}"));
        let g = unsafe { &scene_rmr_sky_gradient[i] };
        let over = frame_of(unsafe { &scene_rmr_sky[i] }, false);
        for y in 0..want.h {
            let mut row = vec![0x55u8; want.w];
            unsafe {
                scene_gradient_row(g, 0, (y + SKY_Y) as u32, want.w as u32, row.as_mut_ptr())
            };
            for (x, &g) in row.iter().enumerate() {
                assert_eq!(
                    g,
                    scene_art::SKIES[i].at(x, y + SKY_Y),
                    "sky {i} gradient ({x}, {y})"
                );
                let p = over.at(x, y);
                let got = if p == CLEAR { g } else { p };
                assert_eq!(got, want.at(x, y), "sky {i} ({x}, {y})");
            }
            // Part way along the row.
            let mut part = vec![0x55u8; 9];
            unsafe { scene_gradient_row(g, 37, (y + SKY_Y) as u32, 8, part.as_mut_ptr()) };
            assert_eq!(part[..8], row[37..45], "sky {i} row {y} from 37");
            assert_eq!(part[8], 0x55);
        }
    }
}

#[test]
fn the_arena_leaves_the_sky_band_clear_and_covers_the_floor() {
    let arena = scene_art::load("arena");
    // Under the sky's last rows (136-139) and below, the arena is opaque.
    for y in 140..240 {
        assert!((0..240).all(|x| arena.at(x, y) != CLEAR), "row {y}");
    }
    // The sky shows through above the steps, between the columns.
    assert!((60..180).all(|x| arena.at(x, 40) == CLEAR), "row 40");
    assert!(
        arena.px[..16 * 240].iter().all(|&p| p == CLEAR),
        "the HUD rows"
    );
}

#[test]
fn the_unarmed_poses_are_patches_where_that_is_smaller() {
    let srcs = scene_art::sources();
    let coded = scene_art::code(&srcs, scene_art::clear_colour(&srcs));
    for (src, c) in srcs.iter().zip(&coded) {
        if src.base.is_none() {
            assert!(c.base.is_none(), "{}", src.png);
            continue;
        }
        let (x, y, w, h) = c.rect;
        println!(
            "{}: {} ({w} x {h} from ({x}, {y})), {} bytes; whole, {}",
            src.png,
            if c.base.is_some() { "a patch" } else { "whole" },
            c.cost,
            c.whole_cost
        );
        assert!(c.cost <= c.whole_cost, "{}", src.png);
    }
    let patches = coded.iter().filter(|c| c.base.is_some()).count();
    assert!(patches >= 4, "only {patches} unarmed poses are patches");
}

#[test]
fn the_flames_are_the_torches() {
    let f = unsafe { scene_rmr_flames };
    assert_eq!(f, scene_art::flames());
    // The prototype's flame mask: 10 x 12 above each torch at (26, 104) and (212, 104).
    assert_eq!(f, [[21, 90, 10, 12], [207, 90, 10, 12]]);
}

/// What the art costs in flash, against the plan's 48 KB ceiling.
#[test]
fn the_art_fits_its_flash_ceiling() {
    let (runs, offs, structs, total) = scene_art::flash();
    println!(
        "RED MOON RITE's art: {runs} bytes of runs + {offs} of row offsets + {structs} of \
         tables = {total} ({:.1} KB of {} KB)",
        total as f64 / 1024.0,
        scene_art::CEILING / 1024
    );
    assert!(total <= scene_art::CEILING, "{total} bytes");
    let (whole, split) = scene_art::skies_both_ways();
    println!("the skies: {whole} bytes whole, {split} as gradients and overlays");
    assert!(split < whole, "the gradients and overlays are the smaller");
}

#[test]
fn the_converter_takes_art_colours_and_full_transparency_only() {
    fn png_bytes(palette: &[[u8; 3]], trns: Option<&[u8]>, px: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 4, 2);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(palette.concat());
        if let Some(t) = trns {
            enc.set_trns(t.to_vec());
        }
        let mut wr = enc.write_header().unwrap();
        wr.write_image_data(px).unwrap();
        wr.finish().unwrap();
        out
    }
    let pal = [
        sidou_tests::logo::PALETTE[7],
        sidou_tests::logo::PALETTE[0],
        [0, 0, 0],
    ];
    let p = scene_art::parse_png(&png_bytes(
        &pal,
        Some(&[255, 255, 0]),
        &[0, 1, 2, 0, 2, 2, 1, 0],
    ))
    .unwrap();
    assert_eq!(p.px, [7, 0, CLEAR, 7, CLEAR, CLEAR, 0, 7]);
    let e = scene_art::parse_png(&png_bytes(&pal, Some(&[255, 128, 0]), &[1; 8])).unwrap_err();
    assert!(e.contains("alpha"), "{e}");
    let e = scene_art::parse_png(&png_bytes(&[[1, 2, 3]], None, &[0; 8])).unwrap_err();
    assert!(e.contains("art palette"), "{e}");
}

/// The run that holds pixel x of a coded row (x from the coded rectangle's left), and the
/// pixel it starts at, walking from the row's start.
fn runs_of(s: &scene_image, r: usize) -> Vec<(usize, usize)> {
    let row = unsafe { s.data.add(usize::from(*s.rows.add(r))) };
    let (mut at, mut off, mut out) = (0, 0, Vec::new());
    while at < usize::from(s.w) {
        out.push((off, at));
        at += usize::from(unsafe { *row.add(off) } >> 4) + 1;
        off += 1;
    }
    assert_eq!(at, usize::from(s.w), "row {r}'s runs add up to w");
    out
}

#[test]
fn wide_rows_have_a_checkpoint_every_seek_pixels() {
    let step = SCENE_ART_SEEK as usize;
    assert_eq!(step, scene_art::SEEK);
    assert!(step.is_power_of_two());
    let mut wide = 0;
    for (src, s) in scene_art::sources().iter().zip(compiled()) {
        let k = (usize::from(s.w) - 1) / step;
        if k == 0 {
            assert!(s.seek.is_null(), "{}: no checkpoint to keep", src.png);
            continue;
        }
        wide += 1;
        assert!(!s.seek.is_null(), "{}", src.png);
        for r in 0..usize::from(s.h) {
            let runs = runs_of(s, r);
            for j in 1..=k {
                let off = usize::from(unsafe { *s.seek.add(r * k + j - 1) });
                assert!(
                    runs.contains(&(off, j * step)),
                    "{} row {r}: checkpoint {j} at byte {off} is not the run from {}",
                    src.png,
                    j * step
                );
            }
        }
    }
    assert!(wide >= 4, "the arena and the skies have checkpoints");
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 16) % n as u64) as usize
    }
}

#[test]
fn a_blended_span_draws_its_opaque_pixels_over_what_is_there() {
    let remap: [u8; 16] = std::array::from_fn(|c| (15 - c) as u8);
    let mut rng = Rng(0x0b1e_9d5a_77c3);
    for (src, s) in scene_art::sources().iter().zip(compiled()) {
        let f = &src.frame;
        let gx = usize::from(s.gx);
        let sw = f.w * gx;
        for i in 0..300 {
            let y = rng.below(f.h);
            let h = 1 + rng.below((f.h - y).min(8));
            let (sx, n) = if i < 4 {
                (0, sw) // whole rows
            } else {
                let sx = rng.below(sw);
                (sx, 1 + rng.below((sw - sx).min(12)))
            };
            let stride = n + rng.below(4);
            let mirror = rng.below(2) == 1;
            let map = rng.below(3) == 0;
            let under: Vec<u8> = (0..=h * stride).map(|k| 0x40 + (k % 7) as u8).collect();
            let mut out = under.clone();
            unsafe {
                scene_image_blend(
                    s,
                    sx as u32,
                    y as u32,
                    n as u32,
                    h as u32,
                    stride as u32,
                    i32::from(mirror),
                    if map {
                        remap.as_ptr()
                    } else {
                        std::ptr::null()
                    },
                    out.as_mut_ptr(),
                )
            };
            let mut want = under.clone();
            for r in 0..h {
                for k in 0..n {
                    let p = (sx + k) / gx;
                    let c = f.at(if mirror { f.w - 1 - p } else { p }, y + r);
                    if c != CLEAR {
                        want[r * stride + k] = if map { remap[usize::from(c)] } else { c };
                    }
                }
            }
            assert_eq!(
                out,
                want,
                "{}: rows {y}..{}, {n} from {sx}{}",
                src.png,
                y + h,
                if mirror { " mirrored" } else { "" }
            );
        }
    }
}

/// The motion tables in flash are red-moon-rite-motion.txt's: each fighter's flight and where
/// his sword sticks, and the spark's stages, row by row as the script draws them (a pixel
/// drawn twice keeping its last colour).
#[test]
fn the_motion_tables_are_the_scripts() {
    let m = scene_art::motion();
    let flights = unsafe { &*std::ptr::addr_of!(scene_rmr_flights) };
    for (v, f) in m.flights.iter().enumerate() {
        let c = &flights[v];
        assert_eq!(usize::from(c.frames), f.frames, "flight {v}");
        let path: Vec<(i32, i32, u8)> = (0..f.frames - 1)
            .map(|n| {
                let g = unsafe { *c.path.add(n) };
                (i32::from(g.dx), i32::from(g.y), g.k)
            })
            .collect();
        assert_eq!(path, f.path, "flight {v}");
        assert!(
            path.iter().all(|g| g.2 < 8),
            "flight {v}: a rotation past 7"
        );
        assert_eq!(
            (i32::from(c.stuck.dx), i32::from(c.stuck.y)),
            f.stuck,
            "flight {v}"
        );
    }
    let sparks = unsafe { &*std::ptr::addr_of!(scene_rmr_sparks) };
    for (i, marks) in m.sparks.iter().enumerate() {
        let sp = &sparks[i];
        let mut want = std::collections::BTreeMap::new();
        for &(x, y, c) in marks {
            want.insert((y, x), c);
        }
        let mut got = std::collections::BTreeMap::new();
        for r in 0..usize::from(sp.rows) {
            let (a, b) = unsafe { (*sp.at.add(r), *sp.at.add(r + 1)) };
            let mut last = i32::MIN;
            for k in usize::from(a)..usize::from(b) {
                let x = i32::from(unsafe { *sp.dx.add(k) });
                assert!(x > last, "stage {}: row {r} not left to right", i + 1);
                last = x;
                got.insert((i32::from(sp.dy0) + r as i32, x), unsafe {
                    *sp.colour.add(k)
                });
            }
        }
        assert_eq!(got, want, "spark stage {}", i + 1);
    }
    // The script's stages: 141, 109 and 77 pixels drawn, a disc of 29 among them.
    assert_eq!(
        m.sparks.iter().map(Vec::len).collect::<Vec<_>>(),
        [77, 109, 141]
    );
}
