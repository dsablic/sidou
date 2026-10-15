// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The logos' bitmaps: app/logo_bitmap.c is generated from assets/logo/metal-chrome.png and
//! assets/logo/cracktro-chrome.png (sidou_tests::logo). SIDOU_LOGO_UPDATE=1 rewrites it.

use sidou_sys::*;
use sidou_tests::logo::{self, Image};

fn scene() -> Image {
    logo::load("cracktro-chrome")
}

fn metal() -> Image {
    logo::load("metal-chrome")
}

/// Each logo's compiled bitmap, by its PNG's name.
fn compiled(name: &str) -> &'static grid_bitmap {
    match name {
        "metal-chrome" => unsafe { &logo_metal_chrome_bitmap },
        "cracktro-chrome" => unsafe { &logo_cracktro_chrome_bitmap },
        _ => panic!("no bitmap for {name}"),
    }
}

#[test]
fn the_generated_file_is_up_to_date() {
    let update = std::env::var("SIDOU_LOGO_UPDATE").as_deref() == Ok("1");
    let stale = logo::check_or_write(update);
    assert!(
        stale.is_empty(),
        "{stale:?} is stale; rerun with SIDOU_LOGO_UPDATE=1"
    );
    assert_eq!(logo::LOGOS[0].name, logo::DEFAULT, "the default first");
    for line in logo::c_source().lines() {
        assert!(
            line.len() <= 100,
            "generated line over 100 columns: {line:?}"
        );
    }
}

#[test]
fn metal_chromes_scene_is_a_full_screen_in_the_art_palette() {
    let img = metal();
    assert_eq!((img.w, img.h), (240, 240));
    assert!(img.px.iter().all(|&c| c < 16));
    // The chrome ramp (red, light red, orange, yellow, white), the purple shadow and the glow's
    // blue are all there.
    for c in [
        GRID_RED,
        GRID_LIGHT_RED,
        GRID_ORANGE,
        GRID_YELLOW,
        GRID_WHITE,
        GRID_PURPLE,
        GRID_BLUE,
    ] {
        assert!(img.px.contains(&(c as u8)), "colour {c} missing");
    }
}

#[test]
fn the_scene_is_a_full_screen_in_the_art_palette() {
    let img = scene();
    assert_eq!((img.w, img.h), (240, 240));
    assert!(img.px.iter().all(|&c| c < 16));
    // The lettering's colours (its raster bands, the purple shadow) are all there.
    for c in [
        GRID_WHITE,
        GRID_LIGHT_GREY,
        GRID_CYAN,
        GRID_LIGHT_BLUE,
        GRID_BLUE,
        GRID_BROWN,
        GRID_ORANGE,
        GRID_LIGHT_RED,
        GRID_YELLOW,
        GRID_PURPLE,
    ] {
        assert!(img.px.contains(&(c as u8)), "colour {c} missing");
    }
}

#[test]
fn encoding_round_trips_every_kind_of_row() {
    let mut px = Vec::new();
    // A flat row (runs of 16 and a remainder), alternating pixels, every colour once, runs
    // across the 16-pixel limit, and two rows alike (they share their runs).
    px.extend(std::iter::repeat_n(5, 40));
    px.extend((0..40).map(|x| (x % 2) as u8 * 15));
    px.extend((0..40).map(|x| (x % 16) as u8));
    px.extend((0..40).map(|x| {
        if x < 17 {
            3
        } else if x < 33 {
            9
        } else {
            1
        }
    }));
    px.extend((0..40).map(|x| {
        if x < 17 {
            3
        } else if x < 33 {
            9
        } else {
            1
        }
    }));
    px.extend((0..120).map(|x| (x / 7 % 16) as u8));
    let img = Image { w: 40, h: 8, px };
    let enc = logo::encode(&img);
    assert_eq!(logo::decode(&enc, img.w, img.h), img.px);
    assert_eq!(enc.rows[3], enc.rows[4], "rows alike share their runs");
    assert_eq!(
        enc.data[usize::from(enc.rows[0])..][..3],
        [0xf5, 0xf5, 0x75]
    );
    let imgs: Vec<Image> = logo::LOGOS.iter().map(|l| logo::load(l.name)).collect();
    for img in &imgs {
        assert_eq!(logo::decode(&logo::encode(img), img.w, img.h), img.px);
    }
    // Encoded together, each image decodes to itself from the shared runs.
    let (rows, data) = logo::encode_shared(&imgs.iter().collect::<Vec<_>>());
    for (img, rows) in imgs.iter().zip(rows) {
        let enc = logo::Encoded {
            rows,
            data: data.clone(),
        };
        assert_eq!(logo::decode(&enc, img.w, img.h), img.px);
    }
}

#[test]
fn the_compiled_bitmaps_are_the_pngs_pixel_for_pixel() {
    for l in logo::LOGOS {
        compiled_is_png(l.name);
    }
}

fn compiled_is_png(name: &str) {
    let img = logo::load(name);
    let b = compiled(name);
    assert_eq!((usize::from(b.w), usize::from(b.h)), (img.w, img.h));
    for y in 0..img.h {
        let want = &img.px[y * img.w..(y + 1) * img.w];
        let mut row = vec![0xffu8; img.w];
        unsafe { grid_bitmap_row(b, 0, y as u32, img.w as u32, row.as_mut_ptr()) };
        assert_eq!(row, want, "row {y}");
        // From any pixel on: a run is entered part way.
        for x in [1usize, 7, 8, 63, 100, 239] {
            let n = (img.w - x).min(37);
            let mut part = vec![0xffu8; n + 1];
            unsafe { grid_bitmap_row(b, x as u32, y as u32, n as u32, part.as_mut_ptr()) };
            assert_eq!(part[..n], want[x..x + n], "row {y} from {x}");
            assert_eq!(part[n], 0xff, "row {y} from {x}: wrote past n");
        }
    }
}

/// What each scene costs in flash: its runs and row offsets, against 4 bits a pixel raw.
/// Cracktro Chrome is a quarter of raw; Metal Chrome's dithered glow (single pixels of blue,
/// purple and light red over black) makes short runs: a third.
#[test]
fn the_bitmaps_are_a_quarter_and_a_third_of_their_raw_size() {
    let mut total = 0;
    for (name, most) in [("cracktro-chrome", 4526), ("metal-chrome", 9753)] {
        let img = logo::load(name);
        let (runs, offs) = logo::size(&img);
        let bytes = runs + offs;
        let raw = img.w * img.h / 2;
        println!(
            "{name}: {runs} bytes of runs + {offs} of row offsets = {bytes}, raw {raw} ({:.1} %)",
            100.0 * bytes as f64 / raw as f64
        );
        assert!(bytes <= most, "{name}: {bytes} bytes");
        total += bytes;
    }
    let imgs: Vec<Image> = logo::LOGOS.iter().map(|l| logo::load(l.name)).collect();
    let (rows, data) = logo::encode_shared(&imgs.iter().collect::<Vec<_>>());
    let together = data.len() + rows.iter().map(|r| 2 * r.len()).sum::<usize>();
    println!("together: {together} bytes ({total} apart)");
    assert!(
        together <= total,
        "sharing rows across the scenes costs nothing"
    );
}

fn png_bytes(w: u32, h: u32, palette: &[[u8; 3]], px: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Indexed);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_palette(palette.concat());
    let mut wr = enc.write_header().unwrap();
    wr.write_image_data(px).unwrap();
    wr.finish().unwrap();
    out
}

#[test]
fn the_converter_takes_art_colours_only_in_whole_cells() {
    // Palette entries in any order map to the art palette by colour.
    let pal = [logo::PALETTE[7], logo::PALETTE[0], logo::PALETTE[14]];
    let px: Vec<u8> = (0..64).map(|i| (i % 3) as u8).collect();
    let img = logo::parse_png(&png_bytes(8, 8, &pal, &px)).unwrap();
    assert_eq!(img.px[..3], [7, 0, 14]);
    for (bytes, why) in [
        (
            png_bytes(8, 8, &[[1, 2, 3]], &[0; 64]),
            "not an art palette colour",
        ),
        (png_bytes(12, 8, &pal, &[0; 96]), "whole cells"),
        (png_bytes(8, 4, &pal, &[0; 32]), "whole cells"),
        (b"not a png".to_vec(), "PNG"),
    ] {
        let e = logo::parse_png(&bytes).expect_err(why);
        assert!(e.contains(why), "{e:?} does not say {why:?}");
    }
}
