// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The fixture for the Live Link viewer (`tools/wasm-link`, `web/link.js`, `web/link.html`):
//! SIDOU's device side (app/link.c) over a screen with plain text, large text at 2 and 3 times,
//! overlays, a plain piece and a painter region with text and an overlay on it, and the frames
//! it sends: START's HELLO, PALETTE's COLOURS, FONT's GLYPHS and a KEYFRAME's CELLS and PIXELS.
//! With them the spec's test vectors and the panel's picture (the grid's own runs, as the LCD
//! gets them) as the CRC-32 of its RGB565 pixels, little-endian, which the reference decoder's
//! Mirror::render must give too. `web/link.test.mjs` decodes the frames with the wasm viewer
//! and checks its picture against it; `web/fake-fm1.mjs` and `web/check.mjs` answer with them
//! as a simulated FM-1. The test checks the committed fixture is what this build makes now;
//! `SIDOU_WEB_LINK_FIXTURE_UPDATE=1` rewrites it.
//!
//!   SIDOU_WEB_LINK_FIXTURE_UPDATE=1 cargo nextest run --release -p sidou-tests -E 'test(web_link)'

use std::ffi::{c_void, CString};
use std::fmt::Write as _;
use std::path::PathBuf;

use fm1_live_link as ll;
use sidou_sys::*;

const W: usize = GRID_COLS as usize * 8;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn attr(fg: u32, bg: u32) -> u8 {
    ((bg << 4) | fg) as u8
}

fn crc(b: &[u8]) -> u32 {
    unsafe { store_crc32(0, b.as_ptr().cast(), b.len() as u32) }
}

fn picture_crc(px: &[u16]) -> u32 {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    crc(&bytes)
}

extern "C" fn paint(ctx: *mut c_void, col: u32, row: u32, px: *mut u8) {
    let px = unsafe { std::slice::from_raw_parts_mut(px, 64) };
    let _ = ctx;
    let shade = 1;
    for (k, v) in px.iter_mut().enumerate() {
        // diagonal stripes of four colours, so a pixel out of place shows
        *v = [GRID_PURPLE, GRID_BLUE, GRID_LIGHT_BLUE, GRID_CYAN]
            [(col as usize * 8 + k % 8 + row as usize * 8 + k / 8 + shade) / 3 % 4]
            as u8;
    }
}

/// The screen: every kind of cell the link sends.
fn screen(g: &mut grid) {
    let text = |g: &mut grid, col, row, s: &str, a| {
        let c = CString::new(s).unwrap();
        unsafe { grid_text(g, col, row, c.as_ptr(), a) };
    };
    let big = |g: &mut grid, col, row, scale, s: &str, a| {
        let c = CString::new(s).unwrap();
        unsafe { grid_text_big(g, col, row, scale, c.as_ptr(), a) };
    };
    unsafe { grid_init(g, b' ', attr(GRID_LIGHT_BLUE, GRID_BLUE)) };
    text(g, 1, 1, "LIVE LINK FIXTURE", attr(GRID_WHITE, GRID_BLACK));
    text(g, 1, 2, "0123456789 !?#", attr(GRID_YELLOW, GRID_BLUE));
    big(g, 1, 4, 2, "AB", attr(GRID_YELLOW, GRID_BLUE));
    big(g, 10, 4, 3, "C", attr(GRID_CYAN, GRID_BLACK));
    big(g, 16, 4, 3, "7", attr(GRID_ORANGE, GRID_DARK_GREY));
    unsafe { grid_box(g, 20, 8, 8, 4, attr(GRID_LIGHT_GREEN, GRID_BLUE), 1) };
    for col in 0..30u32 {
        let mask = [1u8 << (col % 8), 0, 0x18, 0, 0, 0x81, 0, 0xff];
        unsafe { grid_overlay(g, col, 12, mask.as_ptr(), (col % 16) as u8) };
    }
    // an overlay over a glyph, and a plain piece over an overlay (which then does not show)
    let solid = [0x0fu8, 0x0f, 0x0f, 0x0f, 0xf0, 0xf0, 0xf0, 0xf0];
    text(g, 2, 14, "XYZ", attr(GRID_WHITE, GRID_RED));
    for col in 2..5 {
        unsafe { grid_overlay(g, col, 14, solid.as_ptr(), GRID_GREEN as u8) };
    }
    unsafe { grid_overlay(g, 8, 14, solid.as_ptr(), GRID_GREEN as u8) };
    unsafe { grid_put_top(g, 8, 14, b'Q', attr(GRID_RED, GRID_BLACK)) };
    // a painter region with text and an overlay on it
    assert_eq!(
        unsafe { grid_region_painter(g, 2, 18, 20, 6, Some(paint), std::ptr::null_mut(), 8) },
        0
    );
    g.region_peek = Some(paint);
    text(g, 3, 19, "ON PIXELS", attr(GRID_WHITE, GRID_BLACK));
    unsafe { grid_overlay(g, 5, 21, [0xffu8; 8].as_ptr(), GRID_YELLOW as u8) };
    unsafe { grid_overlay(g, 6, 21, [0x3cu8; 8].as_ptr(), GRID_RED as u8) };
    text(g, 1, 27, "\u{7f}", attr(GRID_GREY, GRID_BLACK));
}

/// The panel as the LCD gets it: every run the grid hands out, through grid_render_run, each
/// run marked to the link as platform/fm1/ui.c does.
fn send(g: &mut grid, l: &mut link_state) -> Vec<u16> {
    let mut panel = vec![0u16; W * W];
    let mut r = grid_run::default();
    while unsafe { grid_next_run(g, &mut r) } != 0 {
        let mut buf = vec![0u32; GRID_RUN_PIXELS as usize / 2];
        let px = buf.as_mut_ptr().cast::<u16>();
        unsafe { grid_render_run(g, &r, grid_palette.as_ptr(), px) };
        let px = unsafe { std::slice::from_raw_parts(px, GRID_RUN_PIXELS as usize) };
        let w = usize::from(r.len) * 8;
        for y in 0..8 {
            for x in 0..w {
                let (sx, sy) = (usize::from(r.col) * 8 + x, usize::from(r.row) * 8 + y);
                panel[sy * W + sx] = px[y * w + x];
            }
        }
        unsafe { link_mark(l, r.row.into(), r.col.into(), r.len.into()) };
    }
    panel
}

struct Device {
    l: Box<link_state>,
    now: u32,
    _name: CString,
}

impl Device {
    fn new() -> Self {
        let name = CString::new("SIDOU FIXTURE").unwrap();
        let mut l = Box::<link_state>::default();
        unsafe { link_init(&mut *l, name.as_ptr()) };
        Device {
            l,
            now: 1000,
            _name: name,
        }
    }

    /// A host frame in, and every frame the device then sends.
    fn ask(&mut self, g: &grid, frame: &[u8]) -> Vec<Vec<u8>> {
        for &b in frame {
            unsafe { link_byte(&mut *self.l, b, self.now) };
        }
        let mut out = Vec::new();
        let mut idle = 0;
        while idle < 50 {
            self.now += 3;
            let mut f = [0u8; LINK_FRAME_MAX as usize];
            let n = unsafe { link_poll(&mut *self.l, g, self.now, 64, 0, f.as_mut_ptr()) };
            if n == 0 {
                idle += 1;
                continue;
            }
            idle = 0;
            out.push(f[..n as usize].to_vec());
        }
        out
    }
}

/// A frame as the spec writes it, in hex.
fn spec(s: &str) -> String {
    s.split_whitespace().collect::<String>().to_lowercase()
}

fn frames(fs: &[Vec<u8>]) -> String {
    let items: Vec<String> = fs.iter().map(|f| format!("\"{}\"", hex(f))).collect();
    format!("[\n    {}\n  ]", items.join(",\n    "))
}

fn cmds(fs: &[Vec<u8>]) -> Vec<u8> {
    fs.iter()
        .map(|f| ll::parse(f).expect("a Live Link frame").0)
        .collect()
}

#[test]
fn web_link_fixture() {
    let mut g = Box::<grid>::default();
    let mut dev = Device::new();
    screen(&mut g);
    let panel = send(&mut g, &mut dev.l);

    let start = dev.ask(&g, &ll::start());
    assert_eq!(cmds(&start[..1]), [ll::HELLO]);
    let hello = start[..1].to_vec();
    let palette = dev.ask(&g, &ll::palette());
    assert_eq!(cmds(&palette), [ll::COLOURS]);
    let font = dev.ask(&g, &ll::font());
    assert!(cmds(&font).iter().all(|&c| c == ll::GLYPHS));
    let keyframe = dev.ask(&g, &ll::keyframe());
    assert!(cmds(&keyframe).contains(&ll::PIXELS) && cmds(&keyframe).contains(&ll::CELLS));
    assert!(cmds(&keyframe)
        .iter()
        .all(|&c| c == ll::CELLS || c == ll::PIXELS));
    assert_eq!(start[1..], keyframe[..], "START's keyframe is a KEYFRAME's");
    let beat = dev.ask(&g, &ll::heartbeat());
    assert_eq!(cmds(&beat), [ll::BEAT]);
    let stopped = dev.ask(&g, &ll::stop());
    assert_eq!(cmds(&stopped), [ll::STOPPED]);

    // the reference decoder draws the panel from them, and from them alone
    let mut m = ll::Mirror::default();
    for f in hello.iter().chain(&palette).chain(&font).chain(&keyframe) {
        assert!(m.take(f), "the mirror refused {f:02x?}");
    }
    let picture = picture_crc(&panel);
    assert_eq!(m.render(), panel, "the mirror draws what the panel shows");
    assert!(m.cells.iter().any(|c| c.pixels.is_some()));

    let h = ll::Hello::parse(&ll::parse(&hello[0]).unwrap().1).unwrap();
    let vectors = [
        ("START", ll::start()),
        ("HEARTBEAT", ll::heartbeat()),
        ("STOP", ll::stop()),
        ("KEYFRAME", ll::keyframe()),
        ("FONT", ll::font()),
        ("PALETTE", ll::palette()),
        ("PANEL", ll::panel(1 << 8, 1 << 8, &[0, 0, 1])),
    ];
    let vectors: Vec<String> = vectors
        .iter()
        .map(|(n, f)| format!("\"{n}\": \"{}\"", hex(f)))
        .collect();
    let json = format!(
        "{{\n  \"vectors\": {{\n    {}\n  }},\n  \"device_vectors\": [\n    \"{}\",\n    \"{}\"\n  ],\n  \
         \"name\": \"{}\",\n  \"font_crc\": {},\n  \"palette_crc\": {},\n  \"picture_crc\": {},\n  \
         \"hello\": {},\n  \"palette\": {},\n  \"font\": {},\n  \"keyframe\": {},\n  \"beat\": {},\n  \
         \"stopped\": {}\n}}\n",
        vectors.join(",\n    "),
        spec("F0 7D 46 4D 31 4C 46 00 00 00 41 0E 00 6B F7"),
        spec("F0 7D 46 4D 31 4C 46 20 05 03 20 0E 13 00 40 00 00 00 00 00 00 01 01 00 7E 41 50 F7"),
        h.name,
        h.font_crc,
        h.palette_crc,
        picture,
        frames(&hello),
        frames(&palette),
        frames(&font),
        frames(&keyframe),
        frames(&beat),
        frames(&stopped),
    );
    let path = root().join("web/link-fixtures.json");
    if std::env::var_os("SIDOU_WEB_LINK_FIXTURE_UPDATE").is_some() {
        std::fs::write(&path, &json).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == json,
        "web/link-fixtures.json is not what this build makes: rerun with SIDOU_WEB_LINK_FIXTURE_UPDATE=1"
    );
}
