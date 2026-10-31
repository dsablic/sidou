// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::piece_part;
use sidou_tests::font;

const COLS: usize = GRID_COLS as usize;
const ROWS: usize = GRID_ROWS as usize;
const W: usize = COLS * 8;

fn pal(colour: u32) -> u16 {
    let palette = unsafe { grid_palette };
    palette[colour as usize]
}

fn attr(fg: u32, bg: u32) -> u8 {
    ((bg << 4) | fg) as u8
}

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
        s.drain();
        s
    }

    fn put(&mut self, col: u32, row: u32, ch: u8, a: u8) {
        unsafe { grid_put(&mut *self.g, col, row, ch, a) }
    }

    fn text(&mut self, col: u32, row: u32, s: &str, a: u8) -> u32 {
        let c = std::ffi::CString::new(s).unwrap();
        unsafe { grid_text(&mut *self.g, col, row, c.as_ptr(), a) }
    }

    fn overlay(&mut self, col: u32, row: u32, mask: [u8; 8], color: u32) -> i32 {
        unsafe { grid_overlay(&mut *self.g, col, row, mask.as_ptr(), color as u8) }
    }

    fn clear_overlay(&mut self, col: u32, row: u32) {
        unsafe { grid_overlay_clear(&mut *self.g, col, row) }
    }

    fn next(&mut self) -> Option<(u8, u8, u8)> {
        let mut r = grid_run::default();
        if unsafe { grid_next_run(&mut *self.g, &mut r) } == 0 {
            return None;
        }
        assert!(r.len >= 1 && usize::from(r.col) + usize::from(r.len) <= COLS);
        assert!(usize::from(r.row) < ROWS);
        let mut word_aligned = vec![0u32; GRID_RUN_PIXELS as usize / 2];
        let px = unsafe {
            std::slice::from_raw_parts_mut(
                word_aligned.as_mut_ptr().cast::<u16>(),
                GRID_RUN_PIXELS as usize,
            )
        };
        unsafe { grid_render_run(&*self.g, &r, grid_palette.as_ptr(), px.as_mut_ptr()) };
        let w = usize::from(r.len) * 8;
        for y in 0..8 {
            for x in 0..w {
                let (sx, sy) = (usize::from(r.col) * 8 + x, usize::from(r.row) * 8 + y);
                self.panel[sy * W + sx] = px[y * w + x];
            }
        }
        Some((r.row, r.col, r.len))
    }

    fn runs(&mut self) -> Vec<(u8, u8, u8)> {
        std::iter::from_fn(|| self.next()).collect()
    }

    fn sorted_runs(&mut self) -> Vec<(u8, u8, u8)> {
        let mut runs = self.runs();
        runs.sort_unstable();
        runs
    }

    fn send_up_to(&mut self, runs: u32) {
        for _ in 0..runs {
            if self.next().is_none() {
                break;
            }
        }
    }

    fn drain(&mut self) {
        let n = self.runs().len();
        assert!(n <= COLS * ROWS);
    }

    fn assert_cells_show(
        &self,
        origin: (usize, usize),
        size: usize,
        lit: impl Fn(usize, usize) -> bool,
        fg: u32,
        bg: u32,
    ) {
        for y in 0..size {
            for x in 0..size {
                let want = pal(if lit(x, y) { fg } else { bg });
                assert_eq!(
                    self.panel[(origin.1 + y) * W + origin.0 + x],
                    want,
                    "({x}, {y})"
                );
            }
        }
    }

    fn composed_from_scratch(&self) -> Vec<u16> {
        let f = font::load();
        let g = &*self.g;
        let pal = unsafe { grid_palette };
        let mut out = vec![0u16; W * W];
        for i in 0..COLS * ROWS {
            let (col, row) = (i % COLS, i / COLS);
            let ovl = usize::from(g.ovl[i]);
            for y in 0..8 {
                for x in 0..8 {
                    let a = g.attr[i];
                    let piece = usize::from(g.ch[i]).wrapping_sub(GRID_BIG_FIRST as usize);
                    let is_large_text_piece = piece < GRID_BIG_PIECES as usize;
                    let mut c = if is_large_text_piece {
                        let (scale, q) = piece_part(piece);
                        let (gx, gy) = ((q % scale) * 8 + x, (q / scale) * 8 + y);
                        pal[usize::from(if f.pixel(g.src[i], gx / scale, gy / scale) {
                            a & 15
                        } else {
                            a >> 4
                        })]
                    } else if f.pixel(g.ch[i], x, y) {
                        pal[usize::from(a & 15)]
                    } else if let Some((bx, by)) = bitmap_cell_under(g, col, row) {
                        pal[usize::from(bitmap_pixel_decoded_from_runs(
                            g.region_bitmap,
                            bx * 8 + x,
                            by * 8 + y,
                        ))]
                    } else {
                        pal[usize::from(a >> 4)]
                    };
                    if !is_large_text_piece && ovl != 0 && g.ovl_mask[ovl - 1][y] & (0x80 >> x) != 0
                    {
                        c = pal[usize::from(g.ovl_color[ovl - 1] & 15)];
                    }
                    out[(row * 8 + y) * W + col * 8 + x] = c;
                }
            }
        }
        out
    }
}

fn bitmap_cell_under(g: &grid, col: usize, row: usize) -> Option<(usize, usize)> {
    let (c0, r0) = (usize::from(g.region_col), usize::from(g.region_row));
    let inside = !g.region_bitmap.is_null()
        && (c0..c0 + usize::from(g.region_w)).contains(&col)
        && (r0..r0 + usize::from(g.region_h)).contains(&row);
    inside.then(|| {
        (
            usize::from(g.region_bx) + col - c0,
            usize::from(g.region_by) + row - r0,
        )
    })
}

fn bitmap_pixel_decoded_from_runs(b: *const grid_bitmap, x: usize, y: usize) -> u8 {
    let b = unsafe { &*b };
    assert!(x < usize::from(b.w) && y < usize::from(b.h));
    let row = usize::from(unsafe { *b.rows.add(y) });
    let flags = unsafe { *b.data.add(15) };
    let step = 1usize << (flags & 15);
    let header = (usize::from(b.w) - 1) / step;
    let mut cursor = 2 * (row + header);
    let nibble = |i: usize| (unsafe { *b.data.add(i / 2) } >> ((i & 1) * 4)) & 15;
    let mut at = 0;
    loop {
        let run = if flags & 128 != 0 {
            let run = unsafe { *b.data.add(cursor / 2) };
            cursor += 2;
            run
        } else {
            let code = nibble(cursor);
            cursor += 1;
            if code == 15 {
                let run = nibble(cursor) | (nibble(cursor + 1) << 4);
                cursor += 2;
                run
            } else {
                unsafe { *b.data.add(usize::from(code)) }
            }
        };
        at += usize::from(run >> 4) + 1;
        if x < at {
            return run & 15;
        }
    }
}

#[test]
fn a_new_grid_sends_every_row_as_one_run() {
    let mut s = Screen::new();
    unsafe { grid_init(&mut *s.g, b'A', attr(GRID_WHITE, GRID_BLACK)) };
    let runs = s.runs();
    let want: Vec<_> = (0..ROWS as u8).map(|r| (r, 0, COLS as u8)).collect();
    assert_eq!(runs, want);
    assert_eq!(s.panel, s.composed_from_scratch());
    assert_eq!(s.runs(), vec![], "sent twice");
}

#[test]
fn an_unchanged_frame_sends_nothing() {
    let mut s = Screen::new();
    for row in 0..ROWS as u32 {
        for col in 0..COLS as u32 {
            s.put(col, row, b'X', attr(1, 0));
            s.put(col, row, b' ', attr(GRID_LIGHT_BLUE, GRID_BLUE));
        }
    }
    assert_eq!(
        s.runs(),
        vec![],
        "rewritten with the same content, as a full compose does every frame"
    );
    assert_eq!(s.g.rows, 0, "rows left marked");
}

#[test]
fn adjacent_changes_on_a_row_go_out_as_one_run() {
    let mut s = Screen::new();
    s.text(3, 7, "ABC", attr(1, 6));
    s.put(9, 7, b'Z', attr(1, 6));
    s.put(0, 2, b'Q', attr(1, 6));
    assert_eq!(s.runs(), vec![(2, 0, 1), (7, 3, 3), (7, 9, 1)]);
    assert_eq!(s.panel, s.composed_from_scratch());
}

#[test]
fn a_colour_change_alone_resends_the_cell() {
    let mut s = Screen::new();
    s.put(5, 5, b' ', attr(GRID_LIGHT_BLUE, GRID_RED));
    assert_eq!(s.runs(), vec![(5, 5, 1)]);
    assert_eq!(s.panel, s.composed_from_scratch());
}

#[test]
fn runs_stop_at_the_end_of_a_row() {
    let mut s = Screen::new();
    s.put(29, 1, b'A', attr(1, 0));
    s.put(0, 2, b'B', attr(1, 0));
    assert_eq!(s.runs(), vec![(1, 29, 1), (2, 0, 1)]);
}

#[test]
fn a_cell_changed_and_changed_back_sends_nothing() {
    let mut s = Screen::new();
    s.put(4, 4, b'A', attr(1, 0));
    s.put(4, 4, b' ', attr(GRID_LIGHT_BLUE, GRID_BLUE));
    assert_eq!(s.runs(), vec![]);
}

#[test]
fn changes_behind_the_scan_are_not_lost() {
    let mut s = Screen::new();
    s.put(10, 5, b'A', attr(1, 0));
    s.put(20, 5, b'B', attr(1, 0));
    s.put(5, 8, b'C', attr(1, 0));
    assert_eq!(s.next(), Some((5, 10, 1)));
    let (behind_on_its_row, on_a_row_above) = ((2, 5), (0, 0));
    s.put(behind_on_its_row.0, behind_on_its_row.1, b'D', attr(1, 0));
    s.put(on_a_row_above.0, on_a_row_above.1, b'E', attr(1, 0));
    assert_eq!(
        s.sorted_runs(),
        vec![(0, 0, 1), (5, 2, 1), (5, 20, 1), (8, 5, 1)]
    );
    assert_eq!(s.panel, s.composed_from_scratch());
}

#[test]
fn text_and_fills_are_clipped_and_outside_cells_are_ignored() {
    let mut s = Screen::new();
    assert_eq!(s.text(27, 3, "ABCDEF", attr(1, 0)), 30);
    s.put(30, 0, b'A', attr(1, 0));
    s.put(0, 30, b'A', attr(1, 0));
    s.text(0, 30, "ABC", attr(1, 0));
    unsafe {
        grid_fill(&mut *s.g, 28, 10, 5, 2, b'F', attr(1, 0));
        grid_fill(&mut *s.g, 30, 12, 2, 2, b'F', attr(1, 0));
        grid_fill(&mut *s.g, 1, 30, 2, 2, b'F', attr(1, 0));
    }
    assert_eq!(s.runs(), vec![(3, 27, 3), (10, 28, 2), (11, 28, 2)]);
    assert_eq!(s.panel, s.composed_from_scratch());
}

#[test]
fn render_lays_glyph_bits_out_in_the_cell_colours() {
    let mut s = Screen::new();
    s.text(0, 0, "A", attr(GRID_YELLOW, GRID_BROWN));
    s.drain();
    let f = font::load();
    s.assert_cells_show(
        (0, 0),
        8,
        |x, y| f.pixel(b'A', x, y),
        GRID_YELLOW,
        GRID_BROWN,
    );
}

#[test]
fn an_overlay_draws_over_the_glyph_and_is_sent_only_when_it_changes() {
    let mut s = Screen::new();
    let sine = [0x18, 0x24, 0x42, 0x81, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(s.overlay(6, 9, sine, GRID_CYAN), 0);
    assert_eq!(s.runs(), vec![(9, 6, 1)]);
    assert_eq!(s.panel, s.composed_from_scratch());
    assert_eq!(s.panel[(9 * 8 + 3) * W + 6 * 8], pal(GRID_CYAN));

    assert_eq!(s.overlay(6, 9, sine, GRID_CYAN), 0);
    assert_eq!(s.runs(), vec![], "the same overlay was sent again");

    assert_eq!(s.overlay(6, 9, sine, GRID_WHITE), 0);
    assert_eq!(s.runs(), vec![(9, 6, 1)], "a colour change");
    let mut moved = sine;
    moved[7] = 0xff;
    assert_eq!(s.overlay(6, 9, moved, GRID_WHITE), 0);
    assert_eq!(s.runs(), vec![(9, 6, 1)], "a mask change");
    assert_eq!(s.panel, s.composed_from_scratch());

    s.clear_overlay(6, 9);
    assert_eq!(s.runs(), vec![(9, 6, 1)]);
    assert_eq!(s.panel, s.composed_from_scratch());
    s.clear_overlay(6, 9);
    assert_eq!(s.runs(), vec![], "clearing nothing sends nothing");
}

#[test]
fn overlay_slots_run_out_and_come_back() {
    let mut s = Screen::new();
    let mask = [0xff; 8];
    for i in 0..GRID_OVERLAYS {
        assert_eq!(s.overlay(i % 30, i / 30, mask, 1), 0, "slot {i}");
    }
    assert_eq!(s.overlay(0, 20, mask, 1), -1);
    s.clear_overlay(0, 0);
    assert_eq!(s.overlay(0, 20, mask, 1), 0);
    s.drain();
    assert_eq!(s.panel, s.composed_from_scratch());
}

#[test]
fn a_slot_reused_by_another_cell_resends_both() {
    let mut s = Screen::new();
    assert_eq!(s.overlay(1, 1, [0xf0; 8], 1), 0);
    s.drain();
    s.clear_overlay(1, 1);
    assert_eq!(s.overlay(2, 1, [0x0f; 8], 2), 0);
    assert_eq!(s.runs(), vec![(1, 1, 2)]);
    assert_eq!(s.panel, s.composed_from_scratch());
}

#[test]
fn the_overlay_pool_holds_three_scopes() {
    const { assert!(GRID_OVERLAYS >= 240 && GRID_OVERLAYS <= 255) };
    let mut s = Screen::new();
    for i in 0..240 {
        let mask = [(i as u8).wrapping_mul(37) | 1; 8];
        assert_eq!(
            s.overlay(i % 30, 2 + i / 30, mask, 1 + i % 15),
            0,
            "cell {i}"
        );
    }
    s.drain();
    assert_eq!(s.panel, s.composed_from_scratch());
}

#[test]
fn clearing_every_overlay_frees_the_slots_and_resends_those_cells() {
    let mut s = Screen::new();
    for i in 0..GRID_OVERLAYS {
        assert_eq!(s.overlay(i % 30, i / 30, [0x81; 8], 2), 0);
    }
    s.drain();
    unsafe { grid_overlay_clear_all(&mut *s.g) };
    let rows = GRID_OVERLAYS.div_ceil(30) as usize;
    let sent: usize = s.runs().iter().map(|r| usize::from(r.2)).sum();
    assert_eq!(sent, GRID_OVERLAYS as usize, "{rows} rows of overlays");
    assert_eq!(s.panel, s.composed_from_scratch());
    for i in 0..GRID_OVERLAYS {
        assert_eq!(
            s.overlay(29 - i % 30, 29 - i / 30, [0x18; 8], 3),
            0,
            "slot {i}"
        );
    }
    s.drain();
    assert_eq!(s.panel, s.composed_from_scratch());
}

#[test]
fn held_rows_wait_until_they_are_released() {
    let mut s = Screen::new();
    s.text(0, 3, "AB", attr(1, 0));
    s.text(5, 10, "CD", attr(1, 0));
    s.text(5, 29, "E", attr(1, 0));
    unsafe { grid_hold(&mut *s.g, (1 << 10) | (1 << 29)) };
    assert_eq!(s.runs(), vec![(3, 0, 2)]);
    s.text(0, 10, "X", attr(1, 0));
    assert_eq!(s.runs(), vec![], "changes go on composing into held rows");
    unsafe { grid_hold(&mut *s.g, 1 << 29) };
    assert_eq!(s.runs(), vec![(10, 0, 1), (10, 5, 2)]);
    unsafe { grid_hold(&mut *s.g, 0) };
    assert_eq!(s.runs(), vec![(29, 5, 1)]);
    assert_eq!(s.panel, s.composed_from_scratch());
}

#[test]
fn box_frames_use_the_frame_glyphs() {
    let mut s = Screen::new();
    let f = font::load();
    unsafe { grid_box(&mut *s.g, 2, 3, 5, 4, attr(1, 6), 1) };
    let at = |s: &Screen, c: usize, r: usize| s.g.ch[r * COLS + c];
    assert_eq!(at(&s, 2, 3), f.code("ROUND_TL"));
    assert_eq!(at(&s, 6, 3), f.code("ROUND_TR"));
    assert_eq!(at(&s, 2, 6), f.code("ROUND_BL"));
    assert_eq!(at(&s, 6, 6), f.code("ROUND_BR"));
    assert_eq!(at(&s, 4, 3), f.code("BOX_H"));
    assert_eq!(at(&s, 2, 4), f.code("BOX_V"));
    assert_eq!(at(&s, 4, 4), b' ', "the inside is left alone");
    unsafe { grid_box(&mut *s.g, 10, 10, 2, 2, attr(1, 6), 0) };
    assert_eq!(at(&s, 10, 10), f.code("BOX_TL"));
    assert_eq!(at(&s, 11, 11), f.code("BOX_BR"));
}

struct XorShift32(u32);

impl XorShift32 {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }

    fn mirrored_mask(&mut self) -> [u8; 8] {
        let m = self.next().to_le_bytes();
        [m[0], m[1], m[2], m[3], m[3], m[2], m[1], m[0]]
    }
}

#[test]
fn random_edits_interleaved_with_partial_sends_end_on_the_composed_frame() {
    let mut s = Screen::new();
    let mut rng = XorShift32(0x5eed_1234);
    for frame in 0..400 {
        let edits = match frame % 4 {
            0 => 1,
            1 => 20,
            2 => 300,
            _ => rng.below(60),
        };
        for _ in 0..edits {
            let (c, r) = (rng.below(32), rng.below(32));
            match rng.below(10) {
                0 => {
                    let mask = rng.mirrored_mask();
                    let _ = s.overlay(c, r, mask, rng.below(16));
                }
                1 => s.clear_overlay(c, r),
                _ => s.put(c, r, (rng.below(8) as u8) + b'A', rng.below(4) as u8),
            }
        }
        let runs_the_main_loop_sends = rng.below(12);
        s.send_up_to(runs_the_main_loop_sends);
    }
    s.drain();
    assert_eq!(s.panel, s.composed_from_scratch());
    assert_eq!(s.runs(), vec![]);
}

#[test]
fn every_run_is_maximal_and_holds_only_changed_cells() {
    let mut s = Screen::new();
    let mut rng = XorShift32(42);
    let mut changed = [[false; COLS]; ROWS];
    for _ in 0..200 {
        let (c, r) = (rng.below(30) as usize, rng.below(30) as usize);
        s.put(c as u32, r as u32, b'#', attr(1, 0));
        changed[r][c] = true;
    }
    let mut sent = [[false; COLS]; ROWS];
    for (row, col, len) in s.runs() {
        let (row, col, len) = (usize::from(row), usize::from(col), usize::from(len));
        for c in col..col + len {
            assert!(changed[row][c], "unchanged cell ({c}, {row}) sent");
            assert!(!sent[row][c], "cell ({c}, {row}) sent twice");
            sent[row][c] = true;
        }
        assert!(col == 0 || !changed[row][col - 1] || sent[row][col - 1]);
        assert!(
            col + len == COLS || !changed[row][col + len],
            "run could go on"
        );
    }
    assert_eq!(sent, changed);
}

#[test]
fn pending_counts_the_cells_the_next_runs_would_send() {
    let mut s = Screen::new();
    let pending = |s: &Screen| unsafe { grid_pending(&*s.g) };
    assert_eq!(pending(&s), 0);
    s.text(3, 4, "ABC", attr(GRID_WHITE, GRID_BLUE));
    s.put(29, 29, b'x', attr(GRID_WHITE, GRID_BLUE));
    assert_eq!(s.overlay(10, 12, [1; 8], GRID_CYAN), 0);
    assert_eq!(pending(&s), 5);
    unsafe { grid_hold(&mut *s.g, 1 << 4) };
    assert_eq!(pending(&s), 5, "held rows count too");
    unsafe { grid_hold(&mut *s.g, 0) };
    assert_eq!(s.runs().len(), 3);
    assert_eq!(pending(&s), 0);
    s.put(3, 4, b'A', attr(GRID_WHITE, GRID_BLUE));
    assert_eq!(pending(&s), 0, "the same cell again");
    s.text(3, 4, "XYZ", attr(GRID_WHITE, GRID_BLUE));
    s.put(29, 29, b'y', attr(GRID_WHITE, GRID_BLUE));
    let within =
        |s: &Screen, c: u32, r: u32, w: u32, h: u32| unsafe { grid_pending_in(&*s.g, c, r, w, h) };
    assert_eq!(within(&s, 4, 4, 2, 1), 2);
    assert_eq!(within(&s, 0, 0, 4, 5), 1);
    assert_eq!(within(&s, 3, 5, 3, 1), 0);
    assert_eq!(within(&s, 28, 28, 5, 5), 1, "clipped");
    assert_eq!(within(&s, 0, 0, GRID_COLS, GRID_ROWS), pending(&s));
}

#[test]
fn overlay_same_says_when_setting_one_would_send_nothing() {
    let mut s = Screen::new();
    let same = |s: &Screen, mask: [u8; 8], color: u32| unsafe {
        grid_overlay_same(&*s.g, 6, 9, mask.as_ptr(), color as u8) != 0
    };
    let sine = [0x18, 0x24, 0x42, 0x81, 0, 0, 0, 0];
    assert!(same(&s, [0; 8], GRID_CYAN), "no overlay, an empty mask");
    assert!(!same(&s, sine, GRID_CYAN));
    s.overlay(6, 9, sine, GRID_CYAN);
    assert!(same(&s, sine, GRID_CYAN));
    assert!(!same(&s, sine, GRID_WHITE), "another colour");
    assert!(!same(&s, [0; 8], GRID_CYAN), "clearing it would send it");
    let mut moved = sine;
    moved[7] = 1;
    assert!(!same(&s, moved, GRID_CYAN));
    assert!(
        unsafe { grid_overlay_same(&*s.g, 30, 0, sine.as_ptr(), 3) } != 0,
        "outside"
    );
}

#[test]
fn a_row_is_written_whole_and_marked_only_when_it_changed() {
    let mut s = Screen::new();
    let a = attr(GRID_WHITE, GRID_BLUE);
    let mut ch = [b' '; COLS];
    let at = [attr(GRID_LIGHT_BLUE, GRID_BLUE); COLS];
    unsafe { grid_row(&mut *s.g, 7, ch.as_ptr(), at.as_ptr()) };
    assert_eq!(s.runs(), vec![], "the row as it was");
    ch[3] = b'Q';
    ch[4] = b'R';
    let mut at2 = at;
    at2[20] = a;
    unsafe { grid_row(&mut *s.g, 7, ch.as_ptr(), at2.as_ptr()) };
    assert_eq!(s.runs(), vec![(7, 3, 2), (7, 20, 1)]);
    assert_eq!(s.panel, s.composed_from_scratch());
    let outside_row = 30;
    unsafe { grid_row(&mut *s.g, outside_row, ch.as_ptr(), at2.as_ptr()) };
    assert_eq!(s.runs(), vec![], "a row outside is ignored");
}

#[test]
fn every_glyph_in_every_colour_renders_with_and_without_an_overlay() {
    let mut s = Screen::new();
    let mut rng = XorShift32(0x0dd_ba11);
    for pass in 0..4u32 {
        for i in 0..COLS * ROWS {
            let (c, r) = ((i % COLS) as u32, (i / COLS) as u32);
            let codes_in_flash = FONT_LAST + 1;
            let ch = ((i as u32 + pass * 128) % codes_in_flash) as u8;
            s.put(c, r, ch, rng.below(256) as u8);
            if (i as u32 + pass).is_multiple_of(4) {
                let m = rng.next().to_le_bytes();
                let n = rng.next().to_le_bytes();
                let mask = [m[0], m[1], m[2], m[3], n[0], n[1], n[2], n[3]];
                assert_eq!(s.overlay(c, r, mask, rng.below(16)), 0);
            } else {
                s.clear_overlay(c, r);
            }
        }
        s.drain();
        assert!(
            s.panel == s.composed_from_scratch(),
            "pass {pass}: the panel differs"
        );
    }
}

struct TestBitmap {
    _runs_kept_alive: Box<sidou_tests::logo::Encoded>,
    b: Box<grid_bitmap>,
}

impl TestBitmap {
    fn new(img: &sidou_tests::logo::Image) -> Self {
        let enc = Box::new(sidou_tests::logo::encode(img));
        let b = Box::new(grid_bitmap {
            w: img.w as u16,
            h: img.h as u16,
            rows: enc.rows.as_ptr(),
            data: enc.data.as_ptr(),
        });
        TestBitmap {
            _runs_kept_alive: enc,
            b,
        }
    }

    fn random_runs_of_cells(rng: &mut XorShift32, w: usize, h: usize) -> Self {
        let mut px = Vec::with_capacity(w * h * 64);
        while px.len() < w * h * 64 {
            let c = rng.below(16) as u8;
            px.extend(std::iter::repeat_n(c, 1 + rng.below(40) as usize));
        }
        px.truncate(w * h * 64);
        Self::new(&sidou_tests::logo::Image {
            w: w * 8,
            h: h * 8,
            px,
        })
    }
}

fn region(
    s: &mut Screen,
    at: (u32, u32),
    size: (u32, u32),
    b: &grid_bitmap,
    from: (u32, u32),
) -> i32 {
    unsafe { grid_region(&mut *s.g, at.0, at.1, size.0, size.1, b, from.0, from.1) }
}

fn scene() -> &'static grid_bitmap {
    unsafe { &logo_cracktro_chrome_bitmap }
}

fn scene_image() -> sidou_tests::logo::Image {
    sidou_tests::logo::load("cracktro-chrome")
}

#[test]
fn a_full_screen_region_shows_its_bitmap_pixel_for_pixel() {
    let mut s = Screen::new();
    assert_eq!(region(&mut s, (0, 0), (30, 30), scene(), (0, 0)), 0);
    s.drain();
    let want: Vec<u16> = scene_image()
        .px
        .iter()
        .map(|&c| pal(u32::from(c)))
        .collect();
    assert!(
        s.panel == want,
        "the panel is not assets/logo/cracktro-chrome.png"
    );
}

#[test]
fn a_region_shows_any_cells_of_its_bitmap_and_the_grid_around_it() {
    let mut s = Screen::new();
    s.text(0, 12, "BELOW THE REGION", attr(GRID_WHITE, GRID_RED));
    assert_eq!(region(&mut s, (3, 5), (20, 6), scene(), (7, 10)), 0);
    s.drain();
    assert!(s.panel == s.composed_from_scratch());
    let img = scene_image();
    for y in 0..48 {
        for x in 0..160 {
            assert_eq!(
                s.panel[(40 + y) * W + 24 + x],
                pal(u32::from(img.px[(80 + y) * 240 + 56 + x])),
                "({x}, {y}) of the region"
            );
        }
    }
    let text_row_below = 12;
    let blank_bottom_pixel_row = 7;
    assert_eq!(
        s.panel[(text_row_below * 8 + blank_bottom_pixel_row) * W],
        pal(GRID_RED),
        "below the region, the row's own text on red"
    );
}

#[test]
fn text_and_overlays_draw_over_a_region() {
    let mut s = Screen::new();
    assert_eq!(region(&mut s, (0, 0), (30, 30), scene(), (0, 0)), 0);
    s.drain();
    s.text(2, 28, "V1.2.3", attr(GRID_WHITE, GRID_RED));
    assert_eq!(
        s.overlay(15, 12, [0xff, 0, 0x81, 0, 0x18, 0, 0x3c, 0], GRID_GREEN),
        0
    );
    assert_eq!(s.runs(), vec![(12, 15, 1), (28, 2, 6)]);
    assert!(s.panel == s.composed_from_scratch());
    let f = font::load();
    let img = scene_image();
    let red = pal(GRID_RED);
    assert!(
        (0..W * W).all(|i| s.panel[i] != red || img.px[i] == GRID_RED as u8),
        "red, the text's background colour, shows only where the scene has it"
    );
    for y in 0..8 {
        for x in 0..8 {
            let p = s.panel[(28 * 8 + y) * W + 2 * 8 + x];
            if f.pixel(b'V', x, y) {
                assert_eq!(p, pal(GRID_WHITE), "the glyph's foreground over the bitmap");
            }
        }
    }
}

#[test]
fn a_region_is_sent_when_it_changes_and_only_then() {
    let mut s = Screen::new();
    assert_eq!(region(&mut s, (4, 10), (20, 8), scene(), (0, 9)), 0);
    let rows: Vec<(u8, u8, u8)> = (10..18).map(|r| (r, 4, 20)).collect();
    assert_eq!(s.runs(), rows, "each row of it one run");
    assert_eq!(region(&mut s, (4, 10), (20, 8), scene(), (0, 9)), 0);
    assert_eq!(s.runs(), vec![], "the same region again");
    s.put(4, 10, b' ', attr(GRID_LIGHT_BLUE, GRID_BLUE));
    assert_eq!(
        s.runs(),
        vec![],
        "its cells as they were, as a view composing over it every frame"
    );
    assert_eq!(region(&mut s, (4, 10), (20, 8), scene(), (1, 9)), 0);
    assert_eq!(
        s.runs(),
        rows,
        "other cells of the bitmap: every cell again"
    );
    assert!(s.panel == s.composed_from_scratch());
    assert_eq!(region(&mut s, (6, 11), (20, 8), scene(), (1, 9)), 0);
    let runs = s.sorted_runs();
    let moved: Vec<(u8, u8, u8)> = (10..19)
        .map(|r| match r {
            10 => (r, 4, 20),
            18 => (r, 6, 20),
            _ => (r, 4, 22),
        })
        .collect();
    assert_eq!(
        runs, moved,
        "moved: the cells it left and the cells it covers"
    );
    assert!(s.panel == s.composed_from_scratch());
    unsafe { grid_region_clear(&mut *s.g) };
    assert_eq!(
        s.sorted_runs(),
        (11..19).map(|r| (r, 6, 20)).collect::<Vec<_>>(),
        "gone: its cells show the grid again"
    );
    assert!(s.panel == s.composed_from_scratch());
    unsafe { grid_region_clear(&mut *s.g) };
    assert_eq!(s.runs(), vec![], "no region to clear");
    assert_eq!(unsafe { grid_pending(&*s.g) }, 0);
}

#[test]
fn a_region_grown_over_the_same_bitmap_cells_sends_only_what_it_adds() {
    let mut s = Screen::new();
    assert_eq!(region(&mut s, (0, 0), (3, 30), scene(), (0, 0)), 0);
    let cols = |c: u8, n: u8| (0..30).map(|r| (r, c, n)).collect::<Vec<(u8, u8, u8)>>();
    assert_eq!(s.runs(), cols(0, 3));
    for w in [6u8, 9, 30] {
        assert_eq!(
            region(&mut s, (0, 0), (u32::from(w), 30), scene(), (0, 0)),
            0
        );
        let runs = s.sorted_runs();
        let prev = match w {
            6 => 3,
            9 => 6,
            _ => 9,
        };
        assert_eq!(runs, cols(prev, w - prev), "grown to {w} columns");
        assert!(s.panel == s.composed_from_scratch());
    }
    assert_eq!(region(&mut s, (0, 2), (30, 26), scene(), (0, 2)), 0);
    assert_eq!(
        s.sorted_runs(),
        [0u8, 1, 28, 29].map(|r| (r, 0, 30)).to_vec(),
        "shrunk: the cells it leaves"
    );
    assert!(s.panel == s.composed_from_scratch());
    assert_eq!(region(&mut s, (0, 2), (30, 26), scene(), (0, 3)), 0);
    assert_eq!(
        s.runs().len(),
        26,
        "other cells of the bitmap: every cell again"
    );
    assert!(s.panel == s.composed_from_scratch());
    let mut rng = XorShift32(3);
    let other = TestBitmap::random_runs_of_cells(&mut rng, 30, 30);
    assert_eq!(region(&mut s, (0, 2), (30, 26), &other.b, (0, 2)), 0);
    assert_eq!(s.runs().len(), 26, "another bitmap: every cell again");
    assert!(s.panel == s.composed_from_scratch());
}

#[test]
fn a_region_outside_the_grid_or_its_bitmap_is_refused() {
    let mut rng = XorShift32(7);
    let small = TestBitmap::random_runs_of_cells(&mut rng, 4, 3);
    let mut s = Screen::new();
    for (why, at, size, from) in [
        ("past the grid's right edge", (27, 0), (4, 3), (0, 0)),
        ("past its bottom", (0, 28), (4, 3), (0, 0)),
        ("wider than the bitmap", (0, 0), (5, 3), (0, 0)),
        ("past the bitmap's bottom", (0, 0), (4, 3), (0, 1)),
        ("empty", (0, 0), (0, 3), (0, 0)),
        ("outside", (30, 30), (1, 1), (0, 0)),
    ] {
        assert_eq!(region(&mut s, at, size, &small.b, from), -1, "{why}");
        assert_eq!(s.runs(), vec![]);
    }
    assert_eq!(region(&mut s, (26, 27), (4, 3), &small.b, (0, 0)), 0);
    assert_eq!(s.runs().len(), 3);
    assert!(s.panel == s.composed_from_scratch());
    assert_eq!(region(&mut s, (27, 27), (4, 3), &small.b, (0, 0)), -1);
    assert_eq!(s.runs(), vec![], "a refused region leaves the one in place");
    assert!(s.panel == s.composed_from_scratch());
}

#[test]
fn random_regions_text_and_overlays_end_on_the_composed_frame() {
    let mut rng = XorShift32(0xb17_3a95);
    let bitmaps = [
        TestBitmap::random_runs_of_cells(&mut rng, 30, 30),
        TestBitmap::random_runs_of_cells(&mut rng, 7, 5),
        TestBitmap::random_runs_of_cells(&mut rng, 1, 1),
    ];
    let mut s = Screen::new();
    for frame in 0..300 {
        for _ in 0..rng.below(40) {
            let (c, r) = (rng.below(30), rng.below(30));
            match rng.below(12) {
                0 => {
                    let b = &bitmaps[rng.below(3) as usize];
                    let (bw, bh) = (u32::from(b.b.w) / 8, u32::from(b.b.h) / 8);
                    let (w, h) = (1 + rng.below(bw), 1 + rng.below(bh));
                    let from = (rng.below(bw - w + 1), rng.below(bh - h + 1));
                    let ok = c + w <= 30 && r + h <= 30;
                    assert_eq!(
                        region(&mut s, (c, r), (w, h), &b.b, from),
                        if ok { 0 } else { -1 }
                    );
                }
                1 => unsafe { grid_region_clear(&mut *s.g) },
                11 if !s.g.region_bitmap.is_null() => {
                    let g = &*s.g;
                    let b = unsafe { &*g.region_bitmap };
                    let (bw, bh) = (i64::from(b.w) / 8, i64::from(b.h) / 8);
                    let bitmap_col_at_screen_col_0 =
                        i64::from(g.region_bx) - i64::from(g.region_col);
                    let bitmap_row_at_screen_row_0 =
                        i64::from(g.region_by) - i64::from(g.region_row);
                    let (c, r) = (i64::from(c), i64::from(r));
                    let w = 1 + i64::from(rng.below(30 - c as u32));
                    let h = 1 + i64::from(rng.below(30 - r as u32));
                    let (bx, by) = (
                        c + bitmap_col_at_screen_col_0,
                        r + bitmap_row_at_screen_row_0,
                    );
                    let ok = bx >= 0 && by >= 0 && bx + w <= bw && by + h <= bh;
                    let got = unsafe {
                        grid_region(
                            &mut *s.g,
                            c as u32,
                            r as u32,
                            w as u32,
                            h as u32,
                            b,
                            bx.max(0) as u32,
                            by.max(0) as u32,
                        )
                    };
                    if ok {
                        assert_eq!(got, 0);
                    }
                }
                2 => {
                    let mask = rng.mirrored_mask();
                    let _ = s.overlay(c, r, mask, rng.below(16));
                }
                3 => s.clear_overlay(c, r),
                4 => s.put(c, r, b' ', rng.below(256) as u8),
                _ => s.put(c, r, (rng.below(64) as u8) + b' ', rng.below(256) as u8),
            }
        }
        let runs_the_main_loop_sends = rng.below(20);
        s.send_up_to(runs_the_main_loop_sends);
        if frame % 50 == 49 {
            s.drain();
            assert!(s.panel == s.composed_from_scratch(), "frame {frame}");
        }
    }
    s.drain();
    assert!(s.panel == s.composed_from_scratch());
}

fn big(s: &mut Screen, col: u32, row: u32, scale: u32, text: &str, a: u8) -> u32 {
    let c = std::ffi::CString::new(text).unwrap();
    unsafe { grid_text_big(&mut *s.g, col, row, scale, c.as_ptr(), a) }
}

#[test]
fn large_text_is_the_font_pixel_doubled_and_tripled() {
    for scale in [2u32, 3] {
        let mut s = Screen::new();
        let a = attr(GRID_YELLOW, GRID_BROWN);
        let end = big(&mut s, 1, 2, scale, "AK8.", a);
        assert_eq!(end, 1 + 4 * scale);
        s.drain();
        assert_eq!(s.panel, s.composed_from_scratch(), "scale {scale}");
        let f = font::load();
        let scale = scale as usize;
        for (i, &ch) in b"AK8.".iter().enumerate() {
            s.assert_cells_show(
                (8 + i * 8 * scale, 16),
                8 * scale,
                |x, y| f.pixel(ch, x / scale, y / scale),
                GRID_YELLOW,
                GRID_BROWN,
            );
        }
    }
}

#[test]
fn a_tripled_full_stop_is_a_six_pixel_square_across_two_cells() {
    let mut s = Screen::new();
    big(&mut s, 0, 0, 3, ".", attr(GRID_WHITE, GRID_BLACK));
    s.drain();
    let (dot_cols_tripled, dot_rows_tripled) = (6..=11, 15..=20);
    s.assert_cells_show(
        (0, 0),
        24,
        |x, y| dot_cols_tripled.contains(&x) && dot_rows_tripled.contains(&y),
        GRID_WHITE,
        GRID_BLACK,
    );
}

#[test]
fn a_doubled_glyph_has_every_pixel_twice_over() {
    let mut s = Screen::new();
    big(&mut s, 0, 0, 2, "T", attr(GRID_WHITE, GRID_BLACK));
    s.drain();
    let (bar_rows_doubled, bar_width_doubled) = (2, 12);
    let (stem_cols_doubled, stem_end_row_doubled) = (4..8, 14);
    s.assert_cells_show(
        (0, 0),
        16,
        |x, y| {
            if y < bar_rows_doubled {
                x < bar_width_doubled
            } else {
                y < stem_end_row_doubled && stem_cols_doubled.contains(&x)
            }
        },
        GRID_WHITE,
        GRID_BLACK,
    );
}

#[test]
fn large_text_sends_what_changed_and_goes_back_to_ordinary_cells() {
    let mut s = Screen::new();
    let a = attr(GRID_WHITE, GRID_BLUE);
    big(&mut s, 2, 4, 3, "AB", a);
    let first = s.runs();
    assert_eq!(first, vec![(4, 2, 6), (5, 2, 6), (6, 2, 6)]);
    big(&mut s, 2, 4, 3, "AB", a);
    assert_eq!(s.runs(), vec![], "the same text again sent something");
    big(&mut s, 2, 4, 3, "AC", a);
    assert_eq!(
        s.runs(),
        vec![(4, 5, 3), (5, 5, 3), (6, 5, 3)],
        "another character in the same cells, same colours: only its cells go out"
    );
    assert_eq!(s.panel, s.composed_from_scratch());
    big(&mut s, 2, 4, 2, "AC", a);
    s.drain();
    assert_eq!(
        s.panel,
        s.composed_from_scratch(),
        "large text of another size over it"
    );
    s.text(2, 4, "HI", a);
    s.drain();
    assert_eq!(
        s.panel,
        s.composed_from_scratch(),
        "ordinary text over it: changed cells in their glyphs, the rest still pieces"
    );
    unsafe { grid_fill(&mut *s.g, 2, 4, 6, 3, b' ', a) };
    s.drain();
    assert_eq!(s.panel, s.composed_from_scratch());
    assert_eq!(s.panel[(4 * 8) * W + 2 * 8], pal(GRID_BLUE));
}

#[test]
fn a_piece_of_large_text_is_blank_in_the_font_so_drawing_over_it_shows_the_new_glyph() {
    let f = font::load();
    for code in GRID_BIG_FIRST..GRID_BIG_FIRST + GRID_BIG_PIECES {
        assert_eq!(f.glyphs[code as usize], [0u8; 8], "code {code}");
    }
}

#[test]
fn large_text_is_clipped_to_whole_characters_and_respects_the_guard() {
    let mut s = Screen::new();
    let a = attr(GRID_WHITE, GRID_BLUE);
    assert_eq!(
        big(&mut s, 27, 0, 3, "AB", a),
        30,
        "3 columns left: one tripled character fits"
    );
    assert_eq!(
        big(&mut s, 25, 4, 2, "ABC", a),
        29,
        "two doubled ones do not"
    );
    assert_eq!(
        big(&mut s, 29, 8, 2, "A", a),
        29,
        "no room for one character"
    );
    assert_eq!(big(&mut s, 30, 8, 2, "A", a), 30);
    assert_eq!(
        big(&mut s, 0, 28, 3, "A", a),
        3,
        "the rows below the grid are not drawn"
    );
    s.drain();
    assert_eq!(s.panel, s.composed_from_scratch());
    unsafe { grid_big_guard(&mut *s.g, 1 << 10) };
    big(&mut s, 0, 10, 2, "Z", a);
    assert_eq!(s.g.ch[10 * COLS], b' ', "a guarded row keeps what it has");
    assert_eq!(
        s.g.ch[11 * COLS],
        (GRID_BIG_FIRST + 2) as u8,
        "the row below is not guarded"
    );
}

#[test]
fn a_cell_put_on_top_shows_its_glyph_with_no_overlay_over_it() {
    let mut s = Screen::new();
    let a = attr(GRID_WHITE, GRID_BLACK);
    assert_eq!(s.overlay(4, 4, [0xff; 8], GRID_RED), 0);
    s.drain();
    assert_eq!(
        s.panel[(4 * 8) * W + 4 * 8],
        pal(GRID_RED),
        "an overlay on a blank cell shows"
    );
    unsafe { grid_put_top(&mut *s.g, 4, 4, b'A', a) };
    s.drain();
    assert_eq!(s.panel, s.composed_from_scratch());
    assert_eq!(
        s.g.ch[4 * COLS + 4],
        GRID_PIECE_PLAIN as u8,
        "the cell holds the glyph as a piece of its own"
    );
    assert_eq!(s.g.src[4 * COLS + 4], b'A');
    let f = font::load();
    s.assert_cells_show(
        (32, 32),
        8,
        |x, y| f.pixel(b'A', x, y),
        GRID_WHITE,
        GRID_BLACK,
    );
    unsafe { grid_put_top(&mut *s.g, 4, 4, b'A', a) };
    assert_eq!(s.runs(), vec![], "the same again sends nothing");
    unsafe { grid_put_top(&mut *s.g, 4, 4, b'B', a) };
    assert_eq!(s.runs(), vec![(4, 4, 1)], "another glyph is sent");
    unsafe { grid_fill_top(&mut *s.g, 10, 10, 3, 2, b'C', a) };
    assert_eq!(
        s.runs(),
        vec![(10, 10, 3), (11, 10, 3)],
        "a fill covers a block"
    );
    assert_eq!(s.panel, s.composed_from_scratch());
    let outside_col = 30;
    unsafe { grid_put_top(&mut *s.g, outside_col, 0, b'A', a) };
    assert_eq!(s.runs(), vec![], "a cell outside is ignored");
}
