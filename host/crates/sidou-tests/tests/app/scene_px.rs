// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The tile compositor (app/scene_px.c) and the grid's painter region: tiles against a Rust
//! model of the same layers and against frames that assets/scene/red-moon-rite.js drew
//! (tests/golden/scene/, `SIDOU_SCENE_UPDATE=1` regenerates them when Node is installed);
//! the hash marking every change and nothing else; the scan's slices.

use std::ffi::c_void;
use std::path::PathBuf;
use std::process::Command;

use sidou_sys::*;
use sidou_tests::font::repo_root;
use sidou_tests::scene_art::{self, Picture, CLEAR};

const W: usize = 240;
const TILES: usize = (GRID_COLS * GRID_ROWS) as usize;

/// A grid and a compositor owning its painter region, and the panel the grid's runs drew.
struct Rig {
    g: Box<grid>,
    sp: Box<scene_px>,
    panel: Vec<u8>,
    /// The longest run sent, and how many times each cell was.
    longest: usize,
    sent: Vec<u32>,
}

impl Rig {
    fn new() -> Rig {
        let mut g = Box::<grid>::default();
        let mut sp = Box::<scene_px>::default();
        unsafe {
            grid_init(&mut *g, b' ', (GRID_BLACK << 4 | GRID_WHITE) as u8);
            scene_px_init(&mut *sp, &mut *g);
        }
        Rig {
            g,
            sp,
            panel: vec![0x55; W * W],
            longest: 0,
            sent: vec![0; TILES],
        }
    }

    fn set(&mut self, i: usize, l: Option<&scene_px_layer>) {
        unsafe { scene_px_set(&mut *self.sp, i as u32, l.map_or(std::ptr::null(), |l| l)) };
    }

    /// Scans every candidate, then sends every marked cell to the panel; the cells sent.
    fn frame(&mut self) -> usize {
        while unsafe { scene_px_scan(&mut *self.sp, &mut *self.g, 64) } != 0 {}
        self.send(usize::MAX)
    }

    /// Sends up to `most` marked cells, run by run, as the view does.
    fn send(&mut self, most: usize) -> usize {
        let pal: [u16; 16] = std::array::from_fn(|c| c as u16);
        let mut buf = vec![0u32; GRID_RUN_PIXELS as usize / 2];
        let mut run = grid_run::default();
        let mut sent = 0;
        while sent < most && unsafe { grid_next_run(&mut *self.g, &mut run) } != 0 {
            let px = buf.as_mut_ptr().cast::<u16>();
            unsafe { grid_render_run(&*self.g, &run, pal.as_ptr(), px) };
            let px = unsafe { std::slice::from_raw_parts(px, GRID_RUN_PIXELS as usize) };
            let w = usize::from(run.len) * 8;
            for y in 0..8 {
                for x in 0..w {
                    self.panel[(usize::from(run.row) * 8 + y) * W + usize::from(run.col) * 8 + x] =
                        px[y * w + x] as u8;
                }
            }
            sent += usize::from(run.len);
            self.longest = self.longest.max(usize::from(run.len));
            for c in run.col..run.col + run.len {
                self.sent[usize::from(run.row) * 30 + usize::from(c)] += 1;
            }
        }
        sent
    }

    fn tile(&self, t: usize) -> [u8; 64] {
        let mut out = [0u8; 64];
        let (c, r) = (t % 30, t / 30);
        unsafe { scene_px_tile(&*self.sp, c as u32, r as u32, out.as_mut_ptr()) };
        out
    }

    /// The whole screen composited.
    fn screen(&self) -> Vec<u8> {
        let mut out = vec![0u8; W * W];
        for t in 0..TILES {
            let tile = self.tile(t);
            let (c, r) = (t % 30, t / 30);
            for y in 0..8 {
                out[(r * 8 + y) * W + c * 8..][..8].copy_from_slice(&tile[y * 8..][..8]);
            }
        }
        out
    }
}

fn layer(kind: u32) -> scene_px_layer {
    scene_px_layer {
        kind: kind as u8,
        x0: 0,
        y0: 0,
        x1: W as i16,
        y1: W as i16,
        ..Default::default()
    }
}

fn fill(x0: i16, y0: i16, x1: i16, y1: i16, c: u32) -> scene_px_layer {
    scene_px_layer {
        colour: c as u8,
        x0,
        y0,
        x1,
        y1,
        ..layer(SCENE_PX_FILL)
    }
}

fn image(s: &'static scene_image, x: i16, y: i16, mirror: bool) -> scene_px_layer {
    scene_px_layer {
        mirror: u8::from(mirror),
        x,
        y,
        src: (s as *const scene_image).cast(),
        ..layer(SCENE_PX_IMAGE)
    }
}

/// Every compiled image with its PNG frame (scene_art::sources()'s order).
fn images() -> Vec<(&'static scene_image, Picture)> {
    let mut out: Vec<&'static scene_image> = Vec::new();
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
    }
    out.into_iter()
        .zip(scene_art::sources().into_iter().map(|s| s.frame))
        .collect()
}

/// The model: the layers drawn over a black screen, pixel by pixel, from the PNGs.
struct Model<'a> {
    images: &'a [(&'static scene_image, Picture)],
}

enum ML {
    Fill([i32; 4], u8),
    Gradient(usize, [i32; 4]),
    Image(usize, i32, i32, bool, [i32; 4], Option<[u8; 16]>),
    Marks(Vec<(i32, i32, u8)>),
    Text(i32, i32, Vec<u8>, u8),
    Checker([i32; 4], u8),
}

impl Model<'_> {
    fn draw(&self, layers: &[ML]) -> Vec<u8> {
        let mut px = vec![GRID_BLACK as u8; W * W];
        let mut put = |x: i32, y: i32, c: u8| {
            if (0..W as i32).contains(&x) && (0..W as i32).contains(&y) {
                px[y as usize * W + x as usize] = c;
            }
        };
        let inside = |r: &[i32; 4], x: i32, y: i32| x >= r[0] && x < r[2] && y >= r[1] && y < r[3];
        for l in layers {
            match l {
                ML::Fill(r, c) => {
                    for y in r[1]..r[3] {
                        for x in r[0]..r[2] {
                            put(x, y, *c);
                        }
                    }
                }
                ML::Gradient(i, r) => {
                    let g = &scene_art::SKIES[*i];
                    for y in r[1].max(i32::from(g.y0))..r[3].min(i32::from(g.y1)) {
                        for x in r[0].max(0)..r[2].min(W as i32) {
                            put(x, y, g.at(x as usize, y as usize));
                        }
                    }
                }
                ML::Image(k, x0, y0, mirror, clip, remap) => {
                    let (s, p) = &self.images[*k];
                    let gx = i32::from(s.gx);
                    for fy in 0..p.h as i32 {
                        for m in 0..p.w as i32 {
                            let fx = if *mirror { p.w as i32 - 1 - m } else { m };
                            let c = p.at(fx as usize, fy as usize);
                            if c == CLEAR {
                                continue;
                            }
                            let c = remap.map_or(c, |r| r[usize::from(c)]);
                            for k in 0..gx {
                                let (x, y) = (x0 + m * gx + k, y0 + fy);
                                if inside(clip, x, y) {
                                    put(x, y, c);
                                }
                            }
                        }
                    }
                }
                ML::Marks(m) => {
                    for &(x, y, c) in m {
                        put(x, y, c);
                    }
                }
                ML::Text(x0, y0, text, c) => {
                    let glyphs = unsafe { &*std::ptr::addr_of!(font_glyphs) };
                    for (i, &ch) in text.iter().enumerate() {
                        for (y, bits) in (0i32..).zip(glyphs[usize::from(ch)]) {
                            for x in 0i32..8 {
                                if bits & (0x80 >> x) != 0 {
                                    put(x0 + i as i32 * 8 + x, y0 + y, *c);
                                }
                            }
                        }
                    }
                }
                ML::Checker(r, c) => {
                    for y in r[1]..r[3] {
                        for x in r[0]..r[2] {
                            if (x + y) & 1 == 0 {
                                put(x, y, *c);
                            }
                        }
                    }
                }
            }
        }
        px
    }
}

/// The CALL layer the tests use: a checkerboard of one colour (ctx points at it).
unsafe extern "C" fn checker(ctx: *const c_void, x: u32, y: u32, n: u32, out: *mut u8) {
    let c = unsafe { *ctx.cast::<u8>() };
    for k in 0..n {
        if (x + k + y) & 1 == 0 {
            unsafe { *out.add(k as usize) = c };
        }
    }
}

const CHECKER_COLOURS: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
static TEXTS: [&[u8]; 3] = [b"VS", b"RED MOON", b"8"];
const REMAP: [u8; 16] = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }

    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }

    fn range(&mut self, a: i32, b: i32) -> i32 {
        a + self.below((b - a) as u32) as i32
    }
}

/// A random layer of any kind, as C's and as the model's. A marks layer's marks go into
/// `kept`, which must outlive the rig the layer is set in (C keeps the pointer).
fn random_layer(
    rng: &mut Rng,
    imgs: &[(&'static scene_image, Picture)],
    kept: &mut Vec<Box<[scene_px_mark]>>,
) -> (scene_px_layer, ML) {
    let rect = |rng: &mut Rng| {
        let (x0, y0) = (rng.range(-20, 240), rng.range(-20, 240));
        [x0, y0, x0 + rng.range(1, 90), y0 + rng.range(1, 90)]
    };
    let to16 = |r: [i32; 4]| r.map(|v| v as i16);
    match rng.below(6) {
        0 => {
            let r = rect(rng);
            let c = rng.below(16);
            let [x0, y0, x1, y1] = to16(r);
            (fill(x0, y0, x1, y1, c), ML::Fill(r, c as u8))
        }
        1 => {
            let i = rng.below(3) as usize;
            let r = rect(rng);
            let [x0, y0, x1, y1] = to16(r);
            let l = scene_px_layer {
                src: unsafe { (&scene_rmr_sky_gradient[i] as *const scene_gradient).cast() },
                x0,
                y0,
                x1,
                y1,
                ..layer(SCENE_PX_GRADIENT)
            };
            (l, ML::Gradient(i, r))
        }
        2 => {
            let k = rng.below(imgs.len() as u32) as usize;
            let (x, y) = (rng.range(-120, 220), rng.range(-120, 220));
            let mirror = rng.below(2) == 1;
            let full = rng.below(2) == 0;
            let clip = if full { [0, 0, 240, 240] } else { rect(rng) };
            let remap = (rng.below(4) == 0).then_some(REMAP);
            let [x0, y0, x1, y1] = to16(clip);
            let mut l = image(imgs[k].0, x as i16, y as i16, mirror);
            (l.x0, l.y0, l.x1, l.y1) = (x0, y0, x1, y1);
            if remap.is_some() {
                l.remap = REMAP.as_ptr();
            }
            (l, ML::Image(k, x, y, mirror, clip, remap))
        }
        3 => {
            let n = rng.range(1, 60) as usize;
            let marks: Vec<scene_px_mark> = (0..n)
                .map(|_| scene_px_mark {
                    x: rng.range(-3, 243) as i16,
                    y: rng.range(-3, 243) as i16,
                    colour: rng.below(16) as u8,
                })
                .collect();
            let model = marks
                .iter()
                .map(|m| (i32::from(m.x), i32::from(m.y), m.colour))
                .collect();
            let marks = marks.into_boxed_slice();
            let l = scene_px_layer {
                n: n as u8,
                src: marks.as_ptr().cast(),
                ..layer(SCENE_PX_MARKS)
            };
            kept.push(marks);
            (l, ML::Marks(model))
        }
        4 => {
            let t = TEXTS[rng.below(3) as usize];
            let (x, y) = (rng.range(-30, 230), rng.range(-6, 236));
            let c = rng.below(16) as u8;
            let l = scene_px_layer {
                n: t.len() as u8,
                colour: c,
                x: x as i16,
                y: y as i16,
                src: t.as_ptr().cast(),
                ..layer(SCENE_PX_TEXT)
            };
            (l, ML::Text(x, y, t.to_vec(), c))
        }
        _ => {
            let r = rect(rng);
            let c = rng.below(16) as usize;
            let [x0, y0, x1, y1] = to16(r);
            let l = scene_px_layer {
                fn_: Some(checker),
                src: (&CHECKER_COLOURS[c] as *const u8).cast(),
                x0,
                y0,
                x1,
                y1,
                ..layer(SCENE_PX_CALL)
            };
            (l, ML::Checker(r, c as u8))
        }
    }
}

#[test]
fn random_layers_composite_as_the_model_draws_them() {
    let imgs = images();
    let model = Model { images: &imgs };
    let mut rng = Rng(0x5eed_1234_abcd);
    for round in 0..40 {
        let mut kept = Vec::new();
        let mut rig = Rig::new();
        let n = rng.range(1, SCENE_PX_LAYERS as i32 + 1) as usize;
        let mut ml = Vec::new();
        for i in 0..n {
            let (l, m) = random_layer(&mut rng, &imgs, &mut kept);
            rig.set(i, Some(&l));
            ml.push(m);
        }
        let want = model.draw(&ml);
        assert_eq!(rig.screen(), want, "round {round}");
        // And as the grid sends it.
        rig.frame();
        assert_eq!(rig.panel, want, "round {round}: the panel");
    }
}

#[test]
fn mirroring_transparency_order_and_remaps() {
    let imgs = images();
    let model = Model { images: &imgs };
    let guard = unsafe { &scene_rmr_barb[0][SCENE_RMR_GUARD as usize] };
    let k = imgs
        .iter()
        .position(|(s, _)| std::ptr::eq(*s, guard))
        .unwrap();
    let mut rig = Rig::new();
    // A fill under the fighter shows through his transparent pixels; mirrored, remapped,
    // and a fill over him hides him where it covers.
    rig.set(0, Some(&fill(0, 0, 240, 240, GRID_BLUE)));
    let mut l = image(guard, 60, 100, true);
    l.remap = REMAP.as_ptr();
    rig.set(1, Some(&l));
    rig.set(2, Some(&fill(100, 150, 120, 170, GRID_RED)));
    let want = model.draw(&[
        ML::Fill([0, 0, 240, 240], GRID_BLUE as u8),
        ML::Image(k, 60, 100, true, [0, 0, 240, 240], Some(REMAP)),
        ML::Fill([100, 150, 120, 170], GRID_RED as u8),
    ]);
    assert_eq!(rig.screen(), want);
    let blue = want.iter().filter(|&&c| c == GRID_BLUE as u8).count();
    assert!(blue > 240 * 240 - 128 * 124, "the fill shows round him");
    assert!(want.contains(&(GRID_WHITE as u8)), "the remap");
    // Swapping the order puts the fill under him.
    rig.set(2, None);
    rig.set(0, Some(&fill(100, 150, 120, 170, GRID_RED)));
    let want = model.draw(&[
        ML::Fill([100, 150, 120, 170], GRID_RED as u8),
        ML::Image(k, 60, 100, true, [0, 0, 240, 240], Some(REMAP)),
    ]);
    assert_eq!(rig.screen(), want);
    // Unmirrored, his pixels are the PNG's.
    let mut rig = Rig::new();
    rig.set(0, Some(&image(guard, 0, 0, false)));
    let png = &imgs[k].1;
    let screen = rig.screen();
    for y in 0..png.h {
        for x in 0..png.w * 2 {
            let c = png.at(x / 2, y);
            let want = if c == CLEAR { GRID_BLACK as u8 } else { c };
            assert_eq!(screen[y * W + x], want, "({x}, {y})");
        }
    }
}

#[test]
fn the_hash_marks_every_change_and_nothing_else() {
    let imgs = images();
    let mut rng = Rng(0xfeed_beef_0001);
    // Every layer's marks, for the test's length: a replaced layer is still the scene's
    // until the next set.
    let mut kept = Vec::new();
    let mut rig = Rig::new();
    let mut layers: Vec<scene_px_layer> = (0..12)
        .map(|_| random_layer(&mut rng, &imgs, &mut kept).0)
        .collect();
    for (i, l) in layers.iter().enumerate() {
        rig.set(i, Some(l));
    }
    assert_eq!(rig.frame(), TILES, "the first frame sends the whole screen");
    assert_eq!(rig.panel, rig.screen());
    for step in 0..200 {
        // Change one layer at random, or nothing, or set every layer again unchanged.
        match rng.below(4) {
            0 => {}
            1 => {
                for (i, l) in layers.iter().enumerate() {
                    rig.set(i, Some(l));
                }
            }
            _ => {
                let i = rng.below(12) as usize;
                layers[i] = random_layer(&mut rng, &imgs, &mut kept).0;
                rig.set(i, Some(&layers[i]));
            }
        }
        let before = rig.panel.clone();
        let want = rig.screen();
        let sent = rig.frame();
        assert_eq!(rig.panel, want, "step {step}: the panel");
        // Only tiles that changed were sent.
        let changed = (0..TILES)
            .filter(|&t| {
                let (c, r) = (t % 30, t / 30);
                (0..8).any(|y| {
                    before[(r * 8 + y) * W + c * 8..][..8] != want[(r * 8 + y) * W + c * 8..][..8]
                })
            })
            .count();
        assert_eq!(sent, changed, "step {step}: sent {sent}, changed {changed}");
    }
}

#[test]
fn an_unchanged_frame_scans_and_sends_nothing() {
    let mut rig = Rig::new();
    let arena = unsafe { &scene_rmr_arena };
    rig.set(0, Some(&fill(0, 0, 240, 240, GRID_PURPLE)));
    rig.set(1, Some(&image(arena, 0, 0, false)));
    rig.frame();
    let scanned = rig.sp.scanned;
    rig.set(0, Some(&fill(0, 0, 240, 240, GRID_PURPLE)));
    rig.set(1, Some(&image(arena, 0, 0, false)));
    assert_eq!(rig.frame(), 0);
    assert_eq!(rig.sp.scanned, scanned, "nothing was a candidate");
    // A change that leaves the pixels as they were (the same fill under a new `param`)
    // composites its tiles and sends none.
    let mut again = fill(0, 0, 240, 240, GRID_PURPLE);
    again.param = 1;
    rig.set(0, Some(&again));
    let painted = rig.sp.painted;
    assert_eq!(rig.frame(), 0);
    assert!(rig.sp.scanned > scanned);
    assert_eq!(rig.sp.painted, painted);
}

#[test]
fn the_scan_works_in_slices_and_drops_cells_already_marked() {
    let mut rig = Rig::new();
    rig.frame();
    rig.set(0, Some(&fill(0, 0, 240, 80, GRID_RED))); // ten rows of tiles
    assert_eq!(rig.sp.ncand, 300);
    let before = rig.sp.scanned;
    let left = unsafe { scene_px_scan(&mut *rig.sp, &mut *rig.g, 50) };
    assert_eq!((left, rig.sp.scanned - before), (250, 50));
    // The 50 are marked; touching them again drops them uncomposited.
    rig.set(0, Some(&fill(0, 0, 240, 80, GRID_GREEN)));
    while unsafe { scene_px_scan(&mut *rig.sp, &mut *rig.g, 50) } != 0 {}
    assert_eq!(
        rig.sp.scanned - before,
        300,
        "50 + the other 250, the 50 dropped"
    );
    // The send keeps to a budget a frame and goes on where it stopped, so every tile goes out.
    let mut frames = 0;
    while rig.send(200) > 0 {
        frames += 1;
    }
    assert_eq!(frames, 2);
    assert_eq!(rig.panel, rig.screen());
}

#[test]
fn the_painter_region_replaces_a_bitmap_region_and_the_reverse() {
    let mut rig = Rig::new();
    rig.frame();
    // A bitmap region over the painter's: its cells go out as the bitmap.
    unsafe {
        assert_eq!(
            grid_region(&mut *rig.g, 0, 0, 30, 30, &logo_metal_chrome_bitmap, 0, 0),
            0
        );
    }
    assert!(rig.g.region_painter.is_none());
    assert_eq!(rig.send(usize::MAX), TILES);
    // Back to the painter: every cell again.
    unsafe { scene_px_init(&mut *rig.sp, &mut *rig.g) };
    assert_eq!(rig.send(usize::MAX), TILES);
    assert_eq!(rig.panel, rig.screen());
    // The same painter again sends nothing; a clear sends the cells in their colour.
    let ctx: *mut c_void = (&mut *rig.sp as *mut scene_px).cast();
    let paint = rig.g.region_painter;
    unsafe {
        assert_eq!(
            grid_region_painter(&mut *rig.g, 0, 0, 30, 30, paint, ctx, SCENE_PX_RUN_CELLS),
            0
        );
    }
    assert_eq!(rig.send(usize::MAX), 0);
    unsafe {
        assert_eq!(
            grid_region_painter(&mut *rig.g, 0, 0, 0, 30, paint, ctx, SCENE_PX_RUN_CELLS),
            -1
        );
        assert_eq!(
            grid_region_painter(&mut *rig.g, 0, 0, 30, 30, None, ctx, SCENE_PX_RUN_CELLS),
            -1
        );
        grid_region_clear(&mut *rig.g);
    }
    assert!(rig.g.region_painter.is_none());
    assert_eq!(rig.send(usize::MAX), TILES);
    // Text drawn on the grid over the painter's cells shows its glyphs over the pixels.
    unsafe { scene_px_init(&mut *rig.sp, &mut *rig.g) };
    rig.set(0, Some(&fill(0, 0, 240, 240, GRID_BLUE)));
    unsafe { grid_text(&mut *rig.g, 0, 0, c"A".as_ptr(), GRID_WHITE as u8) };
    rig.frame();
    let glyph = unsafe { (*std::ptr::addr_of!(font_glyphs))[usize::from(b'A')] };
    for (y, bits) in glyph.iter().enumerate() {
        for x in 0..8 {
            let want = if bits & (0x80 >> x) != 0 {
                GRID_WHITE
            } else {
                GRID_BLUE
            };
            assert_eq!(u32::from(rig.panel[y * W + x]), want, "({x}, {y})");
        }
    }
}

/// A painter that fills its cells in colour 5 and counts its calls (ctx: a usize).
unsafe extern "C" fn counting(ctx: *mut c_void, _col: u32, _row: u32, px: *mut u8) {
    unsafe {
        *ctx.cast::<usize>() += 1;
        std::ptr::write_bytes(px, 5, 64);
    }
}

#[test]
fn a_run_takes_at_most_the_painters_cap_of_its_cells() {
    // The scene's region: the first frame sends the whole screen in runs of the cap.
    let mut rig = Rig::new();
    rig.set(0, Some(&fill(0, 0, 240, 240, GRID_BLUE)));
    assert_eq!(rig.frame(), TILES);
    assert_eq!(rig.longest, SCENE_PX_RUN_CELLS as usize);
    assert!(rig.sent.iter().all(|&n| n == 1));
    assert_eq!(rig.panel, rig.screen());
    // A region over part of the screen: a run counts only the region's cells against the cap,
    // and every changed cell goes out once.
    let mut g = Box::<grid>::default();
    let mut calls = 0usize;
    let mut run = grid_run::default();
    unsafe {
        grid_init(&mut *g, b' ', 0x10);
        let ctx = (&mut calls as *mut usize).cast();
        assert_eq!(
            grid_region_painter(&mut *g, 5, 2, 20, 3, Some(counting), ctx, 3),
            0
        );
        assert_eq!(
            grid_region_painter(&mut *g, 5, 2, 20, 3, Some(counting), ctx, 0),
            -1
        );
        while grid_next_run(&mut *g, &mut run) != 0 {}
        grid_fill(&mut *g, 0, 0, 30, 30, b'x', 0x21);
    }
    let mut sent = vec![0u32; TILES];
    let pal = [0u16; 16];
    let mut px = vec![0u32; GRID_RUN_PIXELS as usize / 2];
    while unsafe { grid_next_run(&mut *g, &mut run) } != 0 {
        unsafe { grid_render_run(&*g, &run, pal.as_ptr(), px.as_mut_ptr().cast()) };
        let (r, c0, c1) = (run.row, run.col, run.col + run.len);
        let painted = if (2..5).contains(&r) {
            (c0..c1).filter(|c| (5..25).contains(c)).count()
        } else {
            0
        };
        assert!(
            painted <= 3,
            "row {r}: {} cells from {c0}, {painted} painted",
            run.len
        );
        if !(2..5).contains(&r) {
            assert_eq!((c0, run.len), (0, 30), "row {r}: no painter, one run");
        }
        for c in c0..c1 {
            sent[usize::from(r) * 30 + usize::from(c)] += 1;
        }
    }
    assert!(sent.iter().all(|&n| n == 1), "every cell once");
    assert_eq!(calls, 60, "the region's cells painted as they were sent");
}

#[test]
fn the_refresh_sweep_resends_every_tile_and_heals_a_missed_change() {
    let mut marks: Box<[scene_px_mark]> = (0..4)
        .map(|i| scene_px_mark {
            x: 100 + 9 * i,
            y: 100,
            colour: GRID_RED as u8,
        })
        .collect();
    let p = marks.as_mut_ptr();
    let mut rig = Rig::new();
    rig.set(0, Some(&fill(0, 0, 240, 240, GRID_BLUE)));
    let l = scene_px_layer {
        n: 4,
        src: p.cast_const().cast(),
        ..layer(SCENE_PX_MARKS)
    };
    rig.set(1, Some(&l));
    rig.frame();
    assert_eq!(rig.panel, rig.screen());
    // A mark changes colour in place and no one bumps the layer's param: a change the scene
    // misses.
    unsafe { (*p).colour = GRID_YELLOW as u8 };
    assert_eq!(rig.frame(), 0);
    assert_ne!(rig.panel, rig.screen());
    // Two tiles a frame: every tile once in 450 frames, and the panel heals.
    rig.sent.fill(0);
    let painted = rig.sp.painted;
    for f in 0..450 {
        unsafe { scene_px_refresh(&mut *rig.sp, &mut *rig.g, 2) };
        assert_eq!(rig.frame(), 2, "frame {f}");
    }
    assert!(rig.sent.iter().all(|&n| n == 1), "each tile once");
    assert_eq!(rig.sp.painted - painted, TILES as u32);
    assert_eq!(rig.panel, rig.screen());
    assert_eq!(rig.sp.sweep, 0, "round to the first tile again");
}

/// A golden frame: its layers, as red-moon-rite.js recorded them, and its pixels.
struct Golden {
    name: String,
    sky: Vec<usize>,
    ops: Vec<Vec<String>>,
    png: Picture,
}

fn golden_dir() -> PathBuf {
    repo_root().join("tests/golden/scene")
}

fn goldens() -> Vec<Golden> {
    let mut names: Vec<String> = std::fs::read_dir(golden_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter_map(|n| n.strip_suffix(".txt").map(str::to_string))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let text = std::fs::read_to_string(golden_dir().join(format!("{name}.txt"))).unwrap();
            let mut sky = Vec::new();
            let mut ops = Vec::new();
            for line in text.lines().filter(|l| !l.starts_with('#')) {
                let words: Vec<String> = line.split(' ').map(str::to_string).collect();
                if words[0] == "sky" {
                    sky = words[1..].iter().map(|w| w.parse().unwrap()).collect();
                } else {
                    ops.push(words);
                }
            }
            let bytes = std::fs::read(golden_dir().join(format!("{name}.png"))).unwrap();
            let png = scene_art::parse_png(&bytes).unwrap();
            Golden {
                name,
                sky,
                ops,
                png,
            }
        })
        .collect()
}

/// A golden frame's layers on the compositor: the status bar's rows black, the sky a band per
/// run of cell rows in one palette (gradient and overlay), the arena, then what the sprites
/// stage drew. Returns the slots used.
fn stack(rig: &mut Rig, f: &Golden, marks: &mut Vec<Vec<scene_px_mark>>) -> usize {
    let imgs = images();
    let by_name = |n: &str| {
        let i = scene_art::sources()
            .iter()
            .position(|s| s.png == n)
            .unwrap_or_else(|| panic!("{}: no image {n}", f.name));
        imgs[i].0
    };
    let mut slot = 0;
    let mut push = |rig: &mut Rig, l: scene_px_layer| {
        rig.set(slot, Some(&l));
        slot += 1;
    };
    push(rig, fill(0, 0, 240, 16, GRID_BLACK));
    // The JS: cell row r shows sky rowPal[min(r, 16)] from row 2 on, over rows 16-139.
    let pal = |r: usize| f.sky[r.min(16)];
    let mut r0 = 2;
    while r0 < 18 {
        let mut r1 = r0 + 1;
        while r1 < 18 && pal(r1) == pal(r0) {
            r1 += 1;
        }
        let (y0, y1) = ((r0 * 8).max(16) as i16, (r1 * 8).min(140) as i16);
        let i = pal(r0);
        push(
            rig,
            scene_px_layer {
                src: unsafe { (&scene_rmr_sky_gradient[i] as *const scene_gradient).cast() },
                y0,
                y1,
                ..layer(SCENE_PX_GRADIENT)
            },
        );
        let mut sky = image(
            unsafe { &scene_rmr_sky[i] },
            0,
            SCENE_RMR_SKY_Y as i16,
            false,
        );
        (sky.y0, sky.y1) = (y0, y1);
        push(rig, sky);
        r0 = r1;
    }
    push(rig, image(unsafe { &scene_rmr_arena }, 0, 0, false));
    let mut ops = f.ops.iter().peekable();
    while let Some(op) = ops.next() {
        let n = |k: usize| op[k].parse::<i16>().unwrap();
        match op[0].as_str() {
            "fill" => push(rig, fill(n(1), n(2), n(1) + n(3), n(2) + n(4), n(5) as u32)),
            "image" => push(rig, image(by_name(&op[1]), n(2), n(3), op[4] == "1")),
            "mark" => {
                let mut m = vec![scene_px_mark {
                    x: n(1),
                    y: n(2),
                    colour: n(3) as u8,
                }];
                while m.len() < 255 && ops.peek().is_some_and(|o| o[0] == "mark") {
                    let o = ops.next().unwrap();
                    let v = |k: usize| o[k].parse::<i16>().unwrap();
                    m.push(scene_px_mark {
                        x: v(1),
                        y: v(2),
                        colour: v(3) as u8,
                    });
                }
                marks.push(m);
                let m = marks.last().unwrap();
                push(
                    rig,
                    scene_px_layer {
                        n: m.len() as u8,
                        src: m.as_ptr().cast(),
                        ..layer(SCENE_PX_MARKS)
                    },
                );
            }
            other => panic!("{}: unknown op {other}", f.name),
        }
    }
    slot
}

#[test]
fn red_moon_rites_frames_composite_as_the_script_drew_them() {
    let frames = goldens();
    assert!(frames.len() >= 7, "{} golden frames", frames.len());
    for f in &frames {
        let mut rig = Rig::new();
        let mut marks = Vec::new();
        let slots = stack(&mut rig, f, &mut marks);
        assert!(
            slots <= SCENE_PX_LAYERS as usize,
            "{}: {slots} slots",
            f.name
        );
        let screen = rig.screen();
        for t in 0..TILES {
            let (c, r) = (t % 30, t / 30);
            for y in 0..8 {
                assert_eq!(
                    screen[(r * 8 + y) * W + c * 8..][..8],
                    f.png.px[(r * 8 + y) * W + c * 8..][..8],
                    "{}: tile ({c}, {r}) row {y}",
                    f.name
                );
            }
        }
        rig.frame();
        assert!(rig.panel == f.png.px, "{}: the panel", f.name);
        println!(
            "{}: {slots} layers, {} tiles composited to send the screen",
            f.name, rig.sp.painted
        );
    }
}

#[test]
fn the_golden_frames_are_what_the_script_draws() {
    let node = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !node {
        eprintln!("skipped: Node is not installed, so assets/scene/red-moon-rite.js cannot run");
        return;
    }
    let update = std::env::var("SIDOU_SCENE_UPDATE").as_deref() == Ok("1");
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("scene-goldens");
    let _ = std::fs::remove_dir_all(&out);
    let art = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("scene-goldens-art");
    let dir = if update { golden_dir() } else { out.clone() };
    let status = Command::new("node")
        .arg(repo_root().join("assets/scene/red-moon-rite.js"))
        .arg(&art)
        .arg("--frames")
        .arg(&dir)
        .status()
        .expect("node");
    assert!(status.success());
    if update {
        return;
    }
    // The vignette's state frame by frame (tests/app/scene_rmr.rs checks the device's against
    // it).
    let trace = std::fs::read_to_string(out.join("rmr-trace.tsv")).unwrap();
    let want = std::fs::read_to_string(golden_dir().join("rmr-trace.tsv")).unwrap();
    assert!(
        trace == want,
        "rmr-trace.tsv is stale: rerun with SIDOU_SCENE_UPDATE=1"
    );
    for f in goldens() {
        let text = std::fs::read_to_string(out.join(format!("{}.txt", f.name))).unwrap();
        let want = std::fs::read_to_string(golden_dir().join(format!("{}.txt", f.name))).unwrap();
        assert!(
            text == want,
            "{}.txt is stale: rerun with SIDOU_SCENE_UPDATE=1",
            f.name
        );
        let png =
            scene_art::parse_png(&std::fs::read(out.join(format!("{}.png", f.name))).unwrap())
                .unwrap();
        assert!(
            png == f.png,
            "{}.png is stale: rerun with SIDOU_SCENE_UPDATE=1",
            f.name
        );
    }
}

/// What compositing costs on the host: a tile of each golden frame composited, timed (the
/// device's instruction counts come with the view, M2f Task 4), and the layers a tile visits.
#[test]
fn compositing_a_tile_visits_only_the_layers_over_it() {
    for f in goldens() {
        let mut rig = Rig::new();
        let mut marks = Vec::new();
        let slots = stack(&mut rig, &f, &mut marks);
        let boxes = &rig.sp.box_;
        let mut visits = 0usize;
        for t in 0..TILES {
            let (tx, ty) = ((t % 30 * 8) as i16, (t / 30 * 8) as i16);
            visits += boxes[..slots]
                .iter()
                .filter(|b| b[0] < tx + 8 && b[2] > tx && b[1] < ty + 8 && b[3] > ty)
                .count();
        }
        let start = std::time::Instant::now();
        let mut sink = 0u32;
        for _ in 0..20 {
            for t in 0..TILES {
                sink ^= rig.tile(t)[t % 64] as u32;
            }
        }
        let ns = start.elapsed().as_nanos() as f64 / (20 * TILES) as f64;
        println!(
            "{}: {slots} layers, {:.2} a tile on average, {ns:.0} ns a tile on the host ({sink})",
            f.name,
            visits as f64 / TILES as f64
        );
        assert!(
            visits < slots * TILES / 2,
            "{}: tiles visit most layers",
            f.name
        );
    }
}

/// The hashes hold under any interleaving the view can make: slots changed at random, the
/// scan in slices of any size, a run rendered and left waiting for the panel while the scene
/// changes on (as platform/fm1/ui.c keeps one), sends delayed, rows held and released, and
/// the refresh sweep. However they interleave, once everything is scanned and sent the
/// panel is the screen. (The invariant in app/scene_px.h: every rendered run reaches it.)
#[test]
fn any_interleaving_of_changes_scans_and_sends_converges_on_the_screen() {
    let imgs = images();
    let kept: Vec<Box<[scene_px_mark]>> = (0..8)
        .map(|i| {
            (0..12)
                .map(|k| scene_px_mark {
                    x: (17 * i + 23 * k) % 250 - 5,
                    y: (31 * i + 11 * k) % 250 - 5,
                    colour: ((i + k) % 16) as u8,
                })
                .collect()
        })
        .collect();
    let marks: Vec<scene_px_layer> = kept
        .iter()
        .map(|m| scene_px_layer {
            n: 12,
            src: m.as_ptr().cast(),
            ..layer(SCENE_PX_MARKS)
        })
        .collect();
    let pal: [u16; 16] = std::array::from_fn(|c| c as u16);
    for seed in 1..=400u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut rig = Rig::new();
        let mut buf = vec![0u32; GRID_RUN_PIXELS as usize / 2];
        // A rendered run waiting for the panel.
        let mut waiting: Option<(grid_run, Vec<u16>)> = None;
        let apply = |panel: &mut Vec<u8>, run: &grid_run, px: &[u16]| {
            let w = usize::from(run.len) * 8;
            for y in 0..8 {
                for x in 0..w {
                    panel[(usize::from(run.row) * 8 + y) * W + usize::from(run.col) * 8 + x] =
                        px[y * w + x] as u8;
                }
            }
        };
        for _ in 0..3000 {
            match rng.below(10) {
                0 | 1 => {
                    let i = rng.below(12);
                    let l = if rng.below(8) == 0 {
                        marks[rng.below(8) as usize]
                    } else {
                        // A layer of any other kind: a marks layer drawn here is never
                        // set, so its marks go with it.
                        loop {
                            let (l, _) = random_layer(&mut rng, &imgs, &mut Vec::new());
                            if u32::from(l.kind) != SCENE_PX_MARKS {
                                break l;
                            }
                        }
                    };
                    rig.set(i as usize, Some(&l));
                }
                2 => rig.set(rng.below(12) as usize, None),
                3 | 4 => unsafe {
                    scene_px_scan(&mut *rig.sp, &mut *rig.g, rng.range(1, 40) as u32);
                },
                5 | 6 => {
                    if waiting.is_none() {
                        let mut run = grid_run::default();
                        if unsafe { grid_next_run(&mut *rig.g, &mut run) } != 0 {
                            let px = buf.as_mut_ptr().cast::<u16>();
                            unsafe { grid_render_run(&*rig.g, &run, pal.as_ptr(), px) };
                            let px =
                                unsafe { std::slice::from_raw_parts(px, GRID_RUN_PIXELS as usize) };
                            waiting = Some((run, px.to_vec()));
                        }
                    }
                }
                7 => {
                    if let Some((run, px)) = waiting.take() {
                        apply(&mut rig.panel, &run, &px);
                    }
                }
                8 => unsafe {
                    scene_px_refresh(&mut *rig.sp, &mut *rig.g, rng.below(3));
                },
                _ => {
                    let mask = if rng.below(2) == 0 { 0 } else { rng.next() };
                    unsafe { grid_hold(&mut *rig.g, mask) };
                }
            }
        }
        unsafe { grid_hold(&mut *rig.g, 0) };
        if let Some((run, px)) = waiting.take() {
            apply(&mut rig.panel, &run, &px);
        }
        rig.frame();
        let want = rig.screen();
        let stale = (0..TILES)
            .filter(|t| {
                let (c, r) = (t % 30, t / 30);
                (0..8).any(|y| {
                    let i = (r * 8 + y) * W + c * 8;
                    rig.panel[i..i + 8] != want[i..i + 8]
                })
            })
            .count();
        assert_eq!(
            stale, 0,
            "seed {seed}: {stale} tiles differ from the screen"
        );
    }
}
