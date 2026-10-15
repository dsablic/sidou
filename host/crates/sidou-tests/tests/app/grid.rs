// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The cell-grid renderer (app/grid.c): which cells go out, in which runs, as which pixels.

use sidou_sys::*;
use sidou_tests::font;

const COLS: usize = GRID_COLS as usize;
const ROWS: usize = GRID_ROWS as usize;
const W: usize = COLS * 8;

fn attr(fg: u32, bg: u32) -> u8 {
    ((bg << 4) | fg) as u8
}

/// A grid and a simulated panel that the runs are drawn into.
struct Screen {
    g: Box<grid>,
    panel: Vec<u16>,
}

impl Screen {
    /// Initialised and fully sent, so the panel shows the grid.
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

    /// The next run, drawn into the panel.
    fn next(&mut self) -> Option<(u8, u8, u8)> {
        let mut r = grid_run::default();
        if unsafe { grid_next_run(&mut *self.g, &mut r) } == 0 {
            return None;
        }
        assert!(r.len >= 1 && usize::from(r.col) + usize::from(r.len) <= COLS);
        assert!(usize::from(r.row) < ROWS);
        let mut px = vec![0u16; GRID_RUN_PIXELS as usize];
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

    fn drain(&mut self) {
        let n = self.runs().len();
        assert!(n <= COLS * ROWS);
    }

    /// The composed frame drawn from scratch: what the panel must show once drained.
    fn expected(&self) -> Vec<u16> {
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
                    let mut c = if f.pixel(g.ch[i], x, y) {
                        pal[usize::from(a & 15)]
                    } else {
                        pal[usize::from(a >> 4)]
                    };
                    if ovl != 0 && g.ovl_mask[ovl - 1][y] & (0x80 >> x) != 0 {
                        c = pal[usize::from(g.ovl_color[ovl - 1] & 15)];
                    }
                    out[(row * 8 + y) * W + col * 8 + x] = c;
                }
            }
        }
        out
    }
}

#[test]
fn a_new_grid_sends_every_row_as_one_run() {
    let mut s = Screen::new();
    unsafe { grid_init(&mut *s.g, b'A', attr(GRID_WHITE, GRID_BLACK)) };
    let runs = s.runs();
    let want: Vec<_> = (0..ROWS as u8).map(|r| (r, 0, COLS as u8)).collect();
    assert_eq!(runs, want);
    assert_eq!(s.panel, s.expected());
    assert_eq!(s.runs(), vec![], "sent twice");
}

#[test]
fn an_unchanged_frame_sends_nothing() {
    let mut s = Screen::new();
    // Rewritten with the same content, as a full compose does every frame.
    for row in 0..ROWS as u32 {
        for col in 0..COLS as u32 {
            s.put(col, row, b'X', attr(1, 0));
            s.put(col, row, b' ', attr(GRID_LIGHT_BLUE, GRID_BLUE));
        }
    }
    assert_eq!(s.runs(), vec![]);
    assert_eq!(s.g.rows, 0, "rows left marked");
}

#[test]
fn adjacent_changes_on_a_row_go_out_as_one_run() {
    let mut s = Screen::new();
    s.text(3, 7, "ABC", attr(1, 6));
    s.put(9, 7, b'Z', attr(1, 6));
    s.put(0, 2, b'Q', attr(1, 6));
    assert_eq!(s.runs(), vec![(2, 0, 1), (7, 3, 3), (7, 9, 1)]);
    assert_eq!(s.panel, s.expected());
}

#[test]
fn a_colour_change_alone_resends_the_cell() {
    let mut s = Screen::new();
    s.put(5, 5, b' ', attr(GRID_LIGHT_BLUE, GRID_RED));
    assert_eq!(s.runs(), vec![(5, 5, 1)]);
    assert_eq!(s.panel, s.expected());
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
    // Behind the scan on its own row, and on a row above it.
    s.put(2, 5, b'D', attr(1, 0));
    s.put(0, 0, b'E', attr(1, 0));
    let rest = s.runs();
    let mut sorted = rest.clone();
    sorted.sort();
    assert_eq!(sorted, vec![(0, 0, 1), (5, 2, 1), (5, 20, 1), (8, 5, 1)]);
    assert_eq!(s.panel, s.expected());
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
    assert_eq!(s.panel, s.expected());
}

#[test]
fn render_lays_glyph_bits_out_in_the_cell_colours() {
    let mut s = Screen::new();
    s.text(0, 0, "A", attr(GRID_YELLOW, GRID_BROWN));
    s.drain();
    let f = font::load();
    let pal = unsafe { grid_palette };
    for y in 0..8 {
        for x in 0..8 {
            let want = if f.pixel(b'A', x, y) {
                pal[GRID_YELLOW as usize]
            } else {
                pal[GRID_BROWN as usize]
            };
            assert_eq!(s.panel[y * W + x], want, "({x}, {y})");
        }
    }
}

#[test]
fn the_palette_is_the_art_directions() {
    // docs/art/README.md
    let want = [
        0x0000, 0xFFFF, 0x69A5, 0x7536, 0x69F0, 0x5C68, 0x314F, 0xBE2D, 0x6A64, 0x41C0, 0x9B2B,
        0x4228, 0x6B6D, 0x9E90, 0x6AF6, 0x94B2,
    ];
    assert_eq!(unsafe { grid_palette }, want);
}

#[test]
fn an_overlay_draws_over_the_glyph_and_is_sent_only_when_it_changes() {
    let mut s = Screen::new();
    let sine = [0x18, 0x24, 0x42, 0x81, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(s.overlay(6, 9, sine, GRID_CYAN), 0);
    assert_eq!(s.runs(), vec![(9, 6, 1)]);
    assert_eq!(s.panel, s.expected());
    let pal = unsafe { grid_palette };
    assert_eq!(s.panel[(9 * 8 + 3) * W + 6 * 8], pal[GRID_CYAN as usize]);

    assert_eq!(s.overlay(6, 9, sine, GRID_CYAN), 0);
    assert_eq!(s.runs(), vec![], "the same overlay was sent again");

    assert_eq!(s.overlay(6, 9, sine, GRID_WHITE), 0);
    assert_eq!(s.runs(), vec![(9, 6, 1)], "a colour change");
    let mut moved = sine;
    moved[7] = 0xff;
    assert_eq!(s.overlay(6, 9, moved, GRID_WHITE), 0);
    assert_eq!(s.runs(), vec![(9, 6, 1)], "a mask change");
    assert_eq!(s.panel, s.expected());

    s.clear_overlay(6, 9);
    assert_eq!(s.runs(), vec![(9, 6, 1)]);
    assert_eq!(s.panel, s.expected());
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
    assert_eq!(s.panel, s.expected());
}

#[test]
fn a_slot_reused_by_another_cell_resends_both() {
    let mut s = Screen::new();
    assert_eq!(s.overlay(1, 1, [0xf0; 8], 1), 0);
    s.drain();
    s.clear_overlay(1, 1);
    assert_eq!(s.overlay(2, 1, [0x0f; 8], 2), 0);
    assert_eq!(s.runs(), vec![(1, 1, 2)]);
    assert_eq!(s.panel, s.expected());
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

/// xorshift32: a fixed, dependency-free sequence.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

#[test]
fn random_edits_interleaved_with_partial_sends_end_on_the_composed_frame() {
    let mut s = Screen::new();
    let mut rng = Rng(0x5eed_1234);
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
                    let m = rng.next().to_le_bytes();
                    let mask = [m[0], m[1], m[2], m[3], m[3], m[2], m[1], m[0]];
                    let _ = s.overlay(c, r, mask, rng.below(16));
                }
                1 => s.clear_overlay(c, r),
                _ => s.put(c, r, (rng.below(8) as u8) + b'A', rng.below(4) as u8),
            }
        }
        // The panel takes a few runs between frames, as the main loop sends them.
        for _ in 0..rng.below(12) {
            if s.next().is_none() {
                break;
            }
        }
    }
    s.drain();
    assert_eq!(s.panel, s.expected());
    assert_eq!(s.runs(), vec![]);
}

#[test]
fn every_run_is_maximal_and_holds_only_changed_cells() {
    let mut s = Screen::new();
    let mut rng = Rng(42);
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
