// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! FM-1 Live Link's device side (app/link.c) against the reference decoder (the fm1-live-link
//! crate): the session, the frames, the dirty bitmap, the rate cap, the give-way rules, the
//! auto-stop and the remote control's release, and a mirror rebuilt from the frames that
//! draws what the panel shows, pixel for pixel.

use std::ffi::c_void;

use fm1_live_link as ll;
use sidou_sys::*;

const COLS: usize = GRID_COLS as usize;
const W: usize = COLS * 8;
/// A ring with nothing in it (the platform's 64 slots).
const EMPTY: u32 = 64;
const FRAME_ROOM: u32 = LINK_FRAME_PACKETS + LINK_RING_KEEP;

fn attr(fg: u32, bg: u32) -> u8 {
    ((bg << 4) | fg) as u8
}

struct Link {
    l: Box<link_state>,
    now: u32,
    name: std::ffi::CString,
}

impl Link {
    fn new() -> Self {
        let name = std::ffi::CString::new("SIDOU TEST").unwrap();
        let mut l = Box::<link_state>::default();
        unsafe { link_init(&mut *l, name.as_ptr()) };
        Link { l, now: 1000, name }
    }

    fn feed(&mut self, frame: &[u8]) {
        for &b in frame {
            unsafe { link_byte(&mut *self.l, b, self.now) };
        }
    }

    fn poll_with(&mut self, g: &grid, room: u32, busy: bool) -> Option<Vec<u8>> {
        let mut out = [0u8; LINK_FRAME_MAX as usize];
        let n = unsafe {
            link_poll(
                &mut *self.l,
                g,
                self.now,
                room,
                busy.into(),
                out.as_mut_ptr(),
            )
        };
        assert!(n as usize <= out.len());
        (n > 0).then(|| out[..n as usize].to_vec())
    }

    fn poll(&mut self, g: &grid) -> Option<(u8, Vec<u8>)> {
        let f = self.poll_with(g, EMPTY, false)?;
        Some(ll::parse(&f).unwrap_or_else(|| panic!("an unreadable frame {f:02x?}")))
    }

    /// Every frame the link has now, with the time moving on so the rate never holds it.
    fn drain(&mut self, g: &grid, m: &mut ll::Mirror) -> usize {
        let mut frames = 0;
        for _ in 0..100_000 {
            self.now += 3;
            let Some(f) = self.poll_with(g, EMPTY, false) else {
                if self.l.on != 0 && self.pending(g) {
                    continue;
                }
                return frames;
            };
            assert!(m.take(&f), "the mirror refused {f:02x?}");
            frames += 1;
        }
        panic!("the link never ran dry");
    }

    fn pending(&self, _g: &grid) -> bool {
        self.l.dirty.iter().any(|&b| b != 0) || self.l.font_next != 0xff || self.l.want != 0
    }

    fn mark(&mut self, row: u32, col: u32, len: u32) {
        unsafe { link_mark(&mut *self.l, row, col, len) };
    }
}

/// A grid, and a panel its runs are drawn into, each run marked to the link as ui.c does.
struct Screen {
    g: Box<grid>,
    panel: Vec<u16>,
}

impl Screen {
    fn new() -> Self {
        let mut s = Screen {
            g: Box::<grid>::default(),
            panel: vec![0xdead; W * W],
        };
        unsafe { grid_init(&mut *s.g, b' ', attr(GRID_LIGHT_BLUE, GRID_BLUE)) };
        s
    }

    fn send(&mut self, link: &mut Link) {
        let mut r = grid_run::default();
        while unsafe { grid_next_run(&mut *self.g, &mut r) } != 0 {
            let mut buf = vec![0u32; GRID_RUN_PIXELS as usize / 2];
            let px = buf.as_mut_ptr().cast::<u16>();
            unsafe { grid_render_run(&*self.g, &r, grid_palette.as_ptr(), px) };
            let px = unsafe { std::slice::from_raw_parts(px, GRID_RUN_PIXELS as usize) };
            let w = usize::from(r.len) * 8;
            for y in 0..8 {
                for x in 0..w {
                    let (sx, sy) = (usize::from(r.col) * 8 + x, usize::from(r.row) * 8 + y);
                    self.panel[sy * W + sx] = px[y * w + x];
                }
            }
            link.mark(r.row.into(), r.col.into(), r.len.into());
        }
    }

    fn text(&mut self, col: u32, row: u32, s: &str, a: u8) {
        let c = std::ffi::CString::new(s).unwrap();
        unsafe { grid_text(&mut *self.g, col, row, c.as_ptr(), a) };
    }

    fn big(&mut self, col: u32, row: u32, scale: u32, s: &str, a: u8) {
        let c = std::ffi::CString::new(s).unwrap();
        unsafe { grid_text_big(&mut *self.g, col, row, scale, c.as_ptr(), a) };
    }
}

/// A started session's mirror, the palette and the font fetched.
fn session(link: &mut Link, s: &Screen) -> ll::Mirror {
    let mut m = ll::Mirror::default();
    link.feed(&ll::start());
    link.feed(&ll::palette());
    link.feed(&ll::font());
    link.drain(&s.g, &mut m);
    assert!(m.on && m.hello.is_some());
    m
}

fn assert_same(m: &ll::Mirror, s: &Screen, what: &str) {
    let g = &*s.g;
    for i in 0..GRID_CELLS as usize {
        assert_eq!(
            (m.cells[i].ch, m.cells[i].attr),
            (g.shown_ch[i], g.shown_attr[i]),
            "{what}: cell ({}, {})",
            i % COLS,
            i / COLS
        );
    }
    let fb = m.render();
    let diff = (0..fb.len()).find(|&i| fb[i] != s.panel[i]);
    assert!(
        diff.is_none(),
        "{what}: the mirror's pixel ({}, {}) is {:#06x}, the panel's {:#06x}",
        diff.unwrap() % W,
        diff.unwrap() / W,
        fb[diff.unwrap()],
        s.panel[diff.unwrap()]
    );
}

#[test]
fn hello_says_what_the_device_offers() {
    let s = Screen::new();
    let mut link = Link::new();
    link.feed(&ll::start());
    assert_eq!(link.l.on, 1);
    let (cmd, p) = link.poll(&s.g).expect("HELLO");
    assert_eq!(cmd, ll::HELLO);
    let h = ll::Hello::parse(&p).unwrap();
    let glyphs = (FONT_LAST as usize + 1) * 8;
    let font = unsafe { store_crc32(0, (&raw const font_glyphs).cast(), glyphs as u32) };
    let pal = unsafe { store_crc32(0, (&raw const grid_palette).cast(), 32) };
    assert_eq!(
        h,
        ll::Hello {
            version: 1,
            features: 7,
            cols: 30,
            rows: 30,
            cell_w: 8,
            cell_h: 8,
            colours: 16,
            font_first: 0,
            font_last: FONT_LAST as u8,
            piece_first: GRID_BIG_FIRST as u8,
            pieces: GRID_BIG_PIECES as u8,
            timeout_ms: 3000,
            buttons: 14,
            encoders: 7,
            font_crc: font,
            palette_crc: pal,
            name: link.name.to_str().unwrap().into(),
        }
    );
}

#[test]
fn a_frame_with_a_bad_checksum_or_another_magic_is_ignored() {
    let s = Screen::new();
    let mut link = Link::new();
    let mut bad = ll::start();
    bad[8] ^= 1;
    link.feed(&bad);
    let mut other = ll::start();
    other[5] = b'X';
    link.feed(&other);
    // SIDOU's own SysEx and a frame cut by a status byte
    link.feed(&[0xf0, 0x7d, 0x53, 0x44, 0x01, 0x00, 0x7f, 0xf7]);
    let cut = ll::start();
    link.feed(&cut[..8]);
    link.feed(&[0x90, 0x40, 0x7f]);
    assert_eq!(link.l.on, 0);
    assert_eq!(link.poll(&s.g), None);
    // nothing but START begins a session; another command is answered STOPPED
    link.feed(&ll::heartbeat());
    assert_eq!(link.poll(&s.g).map(|f| f.0), Some(ll::STOPPED));
    assert_eq!(link.l.on, 0);
}

#[test]
fn the_mirror_draws_what_the_panel_shows() {
    let mut s = Screen::new();
    let mut link = Link::new();
    s.send(&mut link);
    let mut m = session(&mut link, &s);
    assert_eq!(m.font.len(), 256);
    assert_same(&m, &s, "the first keyframe");

    // text, large text at 2 and 3 times, overlays, a plain piece and a bitmap region
    s.text(1, 1, "LIVE LINK", attr(GRID_WHITE, GRID_BLACK));
    s.big(1, 3, 2, "AB", attr(GRID_YELLOW, GRID_BLUE));
    s.big(10, 3, 3, "C", attr(GRID_CYAN, GRID_BLACK));
    for col in 0..30u32 {
        let mask = [1u8 << (col % 8), 0, 0x18, 0, 0, 0x81, 0, 0xff];
        unsafe { grid_overlay(&mut *s.g, col, 12, mask.as_ptr(), (col % 16) as u8) };
    }
    unsafe { grid_put_top(&mut *s.g, 4, 14, b'Q', attr(GRID_RED, GRID_BLACK)) };
    let b = unsafe { &logo_metal_chrome_bitmap };
    assert_eq!(
        unsafe { grid_region(&mut *s.g, 2, 18, 20, 6, b, 0, 0) },
        0,
        "the logo's region"
    );
    s.text(3, 19, "ON", attr(GRID_WHITE, GRID_BLACK));
    let mask = [0xffu8; 8];
    unsafe { grid_overlay(&mut *s.g, 5, 20, mask.as_ptr(), GRID_PURPLE as u8) };
    s.send(&mut link);
    link.drain(&s.g, &mut m);
    assert_same(&m, &s, "text, pieces, overlays, a region");
    assert_eq!(m.rejected, 0);

    // the newest state wins: cells changed again before the link sent them go once, as they
    // are then
    for k in 0..3u8 {
        s.text(1, 1, &format!("FRAME {k}"), attr(GRID_GREEN, GRID_BLACK));
        unsafe { grid_overlay_clear(&mut *s.g, u32::from(k), 12) };
        s.send(&mut link);
    }
    unsafe { grid_region_clear(&mut *s.g) };
    s.send(&mut link);
    link.drain(&s.g, &mut m);
    assert_same(&m, &s, "changed three times, the region gone");
}

/// A painter region with a peek: a pattern by position and a counter the painter keeps.
struct Painted {
    shade: u8,
    painted: u32,
}

extern "C" fn paint(ctx: *mut c_void, col: u32, row: u32, px: *mut u8) {
    let p = unsafe { &mut *ctx.cast::<Painted>() };
    p.painted += 1;
    peek(ctx, col, row, px);
}

extern "C" fn peek(ctx: *mut c_void, col: u32, row: u32, px: *mut u8) {
    let p = unsafe { &*ctx.cast::<Painted>() };
    let px = unsafe { std::slice::from_raw_parts_mut(px, 64) };
    for (k, v) in px.iter_mut().enumerate() {
        *v = ((col as usize + row as usize + k / 8 + k % 8 + usize::from(p.shade)) % 16) as u8;
    }
}

#[test]
fn a_painter_regions_cells_go_as_pixels_through_its_peek_alone() {
    let mut s = Screen::new();
    let mut link = Link::new();
    let p = Box::into_raw(Box::new(Painted {
        shade: 0,
        painted: 0,
    }));
    let ctx = p.cast::<c_void>();
    let painted = || unsafe { (*p).painted };
    unsafe { grid_region_painter(&mut *s.g, 0, 0, 30, 30, Some(paint), ctx, 8) };
    s.g.region_peek = Some(peek);
    s.send(&mut link);
    assert_eq!(painted(), GRID_CELLS);
    let mut m = session(&mut link, &s);
    assert_eq!(
        painted(),
        GRID_CELLS,
        "the link painted through the panel's painter"
    );
    assert_same(&m, &s, "a painter region");
    assert!(m.cells.iter().all(|c| c.pixels.is_some()));

    unsafe { (*p).shade = 5 };
    for col in 3..9 {
        unsafe { grid_mark(&mut *s.g, col, 7) };
    }
    s.send(&mut link);
    link.drain(&s.g, &mut m);
    assert_same(&m, &s, "six cells painted again");

    // a new painter without a peek: its cells go as cells (the mirror cannot read them)
    unsafe {
        grid_region_painter(
            &mut *s.g,
            0,
            0,
            30,
            30,
            Some(paint),
            std::ptr::null_mut(),
            8,
        )
    };
    assert!(s.g.region_peek.is_none(), "a new painter keeps no old peek");
    drop(unsafe { Box::from_raw(p) });
}

#[test]
fn only_marked_cells_go_and_a_row_goes_in_frames_of_at_most_48_bytes() {
    let mut s = Screen::new();
    let mut link = Link::new();
    s.send(&mut link);
    let mut m = session(&mut link, &s);
    assert_eq!(link.poll(&s.g), None, "nothing marked");
    link.mark(2, 5, 5);
    let (cmd, p) = link.poll(&s.g).expect("a CELLS frame");
    assert_eq!((cmd, p.len()), (ll::CELLS, 2 + 5 * 3));
    assert_eq!(&p[..2], &[2, 5]);
    assert_eq!(link.poll(&s.g), None);

    // a row of overlays: 4 cells a frame (11 bytes each)
    let mask = [0x55u8; 8];
    for col in 0..30 {
        unsafe { grid_overlay(&mut *s.g, col, 9, mask.as_ptr(), 3) };
    }
    s.send(&mut link);
    let mut sizes = Vec::new();
    while let Some((cmd, p)) = link.poll(&s.g) {
        assert_eq!(cmd, ll::CELLS);
        assert!(p.len() <= ll::PAYLOAD_MAX);
        sizes.push(p.len());
        m.apply(cmd, &p);
        link.now += 5;
    }
    assert_eq!(sizes.len(), 8, "{sizes:?}");
    assert_eq!(sizes.iter().sum::<usize>(), 8 * 2 + 30 * 11);
}

#[test]
fn the_rate_cap_holds_a_busy_screen_to_22_bytes_a_millisecond() {
    let s = Screen::new();
    let mut link = Link::new();
    let mut m = ll::Mirror::default();
    link.feed(&ll::start());
    link.drain(&s.g, &mut m);
    let mut bytes = 0;
    let t0 = link.now;
    for _ in 0..2000 {
        for row in 0..GRID_ROWS {
            link.mark(row, 0, GRID_COLS); // everything, all the time
        }
        link.now += 1;
        while let Some((_, p)) = link.poll(&s.g) {
            bytes += p.len();
        }
    }
    let ms = (link.now - t0) as usize;
    let cap = ms * LINK_RATE as usize + LINK_BURST as usize + ll::PAYLOAD_MAX;
    println!("{bytes} payload bytes in {ms} ms (cap {cap})");
    assert!(bytes <= cap, "{bytes} > {cap}");
    assert!(
        bytes * 10 >= cap * 9,
        "the cap is not reached: {bytes} of {cap}"
    );
}

#[test]
fn it_gives_way_to_other_output_and_keeps_the_rings_last_slots() {
    let s = Screen::new();
    let mut link = Link::new();
    link.feed(&ll::start());
    link.now += 1;
    assert_eq!(
        link.poll_with(&s.g, EMPTY, true),
        None,
        "busy: a SysEx reply going out"
    );
    assert_eq!(
        link.poll_with(&s.g, FRAME_ROOM - 1, false),
        None,
        "too little room"
    );
    let hello = link
        .poll_with(&s.g, FRAME_ROOM, false)
        .expect("room for a frame");
    assert_eq!(ll::parse(&hello).unwrap().0, ll::HELLO);
    assert!(hello.len().div_ceil(3) <= LINK_FRAME_PACKETS as usize);
    // the longest frame fits the packets the rule reserves
    assert_eq!(LINK_FRAME_MAX, 64);
    assert_eq!(
        ll::frame(ll::CELLS, &[0x80; 48]).len(),
        LINK_FRAME_MAX as usize
    );
}

#[test]
fn a_session_ends_three_seconds_after_the_last_heartbeat() {
    let s = Screen::new();
    let mut link = Link::new();
    let mut m = ll::Mirror::default();
    link.feed(&ll::start());
    link.drain(&s.g, &mut m);
    let mut beats = 0;
    for _ in 0..5 {
        link.now += 2000;
        link.feed(&ll::heartbeat());
        while let Some((cmd, _)) = link.poll(&s.g) {
            assert_eq!(cmd, ll::BEAT);
            beats += 1;
        }
    }
    assert_eq!(beats, 5);
    let heard = link.now;
    link.now = heard + LINK_TIMEOUT_MS - 1;
    assert_eq!(link.poll(&s.g), None);
    assert_eq!(link.l.on, 1);
    link.now = heard + LINK_TIMEOUT_MS;
    assert_eq!(link.poll(&s.g).map(|f| f.0), Some(ll::STOPPED));
    assert_eq!(link.l.on, 0);
    assert_eq!(link.poll(&s.g), None);
    // STOP ends it at once, answered
    link.feed(&ll::start());
    link.drain(&s.g, &mut m);
    link.feed(&ll::stop());
    assert_eq!(link.l.on, 0);
    assert_eq!(link.poll(&s.g).map(|f| f.0), Some(ll::STOPPED));
    // the host gone: dropped, nothing sent
    link.feed(&ll::start());
    unsafe { link_drop(&mut *link.l) };
    assert_eq!(link.poll(&s.g), None);
}

#[test]
fn the_hosts_buttons_are_let_go_half_a_second_after_its_last_panel_frame() {
    let s = Screen::new();
    let mut link = Link::new();
    link.feed(&ll::start());
    let home = 1u16 << PANEL_BTN_HOME;
    let edit = 1u16 << PANEL_BTN_EDIT;
    link.feed(&ll::panel(edit, 0, &[0, 0, 3, -2]));
    let mut pressed = 0;
    let held = unsafe { link_take(&mut *link.l, &mut pressed) };
    assert_eq!(
        (held, pressed),
        (edit.into(), edit.into()),
        "held, and pressed as it went down"
    );
    assert_eq!(unsafe { link_steps(&mut *link.l, 2) }, 3);
    assert_eq!(unsafe { link_steps(&mut *link.l, 3) }, -2);
    assert_eq!(unsafe { link_steps(&mut *link.l, 2) }, 0, "taken");
    // a tap between two frames: pressed, not held
    link.now += 100;
    link.feed(&ll::panel(edit, home, &[]));
    let held = unsafe { link_take(&mut *link.l, &mut pressed) };
    assert_eq!((held, pressed), (edit.into(), home.into()));
    link.now += LINK_RELEASE_MS - 1;
    link.poll(&s.g);
    assert_eq!(
        unsafe { link_take(&mut *link.l, &mut pressed) },
        edit.into()
    );
    link.now += 1;
    link.poll(&s.g);
    assert_eq!(
        unsafe { link_take(&mut *link.l, &mut pressed) },
        0,
        "let go"
    );
    // nothing without a session
    link.feed(&ll::stop());
    link.feed(&ll::panel(home, home, &[5]));
    assert_eq!(unsafe { link_take(&mut *link.l, &mut pressed) }, 0);
    assert_eq!(pressed, 0);
}

#[test]
fn the_font_and_the_palette_come_as_the_device_has_them() {
    let s = Screen::new();
    let mut link = Link::new();
    let m = session(&mut link, &s);
    for (c, glyph) in m.font.iter().enumerate().take(FONT_LAST as usize + 1) {
        assert_eq!(*glyph, unsafe { font_glyphs[c] }, "glyph {c:#x}");
    }
    assert_eq!(m.palette, unsafe { grid_palette });
}
