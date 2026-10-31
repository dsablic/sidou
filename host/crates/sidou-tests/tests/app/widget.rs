// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::{c_char, c_void, CStr};

use sidou_sys::*;
use sidou_tests::font;

const COLS: usize = GRID_COLS as usize;
const MENU_BUDGET_CELLS: u16 = 190;

fn attr(fg: u32, bg: u32) -> u8 {
    ((bg << 4) | fg) as u8
}

fn new_grid() -> Box<grid> {
    let mut g = Box::<grid>::default();
    unsafe { grid_init(&mut *g, b'.', attr(GRID_WHITE, GRID_BLACK)) };
    g
}

fn at(g: &grid, col: usize, row: usize) -> u8 {
    g.ch[row * COLS + col]
}

fn text(g: &grid, col: usize, row: usize, len: usize) -> String {
    (col..col + len).map(|c| at(g, c, row) as char).collect()
}

#[test]
fn a_frame_carries_its_title_in_the_top_edge() {
    let mut g = new_grid();
    let f = font::load();
    let a = attr(GRID_LIGHT_BLUE, GRID_BLUE);
    unsafe { widget_frame(&mut *g, 2, 4, 12, 5, a, c"MENU".as_ptr()) };
    assert_eq!(at(&g, 2, 4), f.code("BOX_TL"));
    assert_eq!(text(&g, 3, 4, 6), " MENU ");
    assert_eq!(at(&g, 9, 4), f.code("BOX_H"));
    assert_eq!(at(&g, 13, 4), f.code("BOX_TR"));
    assert_eq!(at(&g, 2, 8), f.code("BOX_BL"));
    assert_eq!(at(&g, 13, 8), f.code("BOX_BR"));
    assert_eq!(at(&g, 2, 6), f.code("BOX_V"));
    assert_eq!(at(&g, 5, 6), b'.', "the inside is left alone");
    assert!((3..9).all(|c| g.attr[4 * COLS + c] == a));
    unsafe { widget_frame(&mut *g, 0, 10, 6, 3, a, c"TOOLONG".as_ptr()) };
    assert_eq!(text(&g, 1, 10, 4), " TOO");
    assert_eq!(at(&g, 5, 10), f.code("BOX_TR"));
    unsafe { widget_frame(&mut *g, 0, 20, 6, 3, a, std::ptr::null()) };
    assert_eq!(at(&g, 1, 20), f.code("BOX_H"));
}

#[test]
fn a_screen_frame_is_the_outer_ring_around_a_cleared_inside() {
    let mut g = new_grid();
    let a = attr(GRID_LIGHT_BLUE, GRID_BLUE);
    unsafe { widget_screen(&mut *g, FONT_FULL as u8, a) };
    for i in 0..COLS * COLS {
        let (c, r) = (i % COLS, i / COLS);
        let ring = r == 0 || r == COLS - 1 || c == 0 || c == COLS - 1;
        assert_eq!(
            g.ch[i],
            if ring { FONT_FULL as u8 } else { b' ' },
            "({c}, {r})"
        );
        assert_eq!(g.attr[i], a);
    }
}

#[test]
fn numbers_are_right_aligned_and_signed_on_request() {
    let mut g = new_grid();
    let a = attr(1, 0);
    unsafe {
        widget_number(&mut *g, 0, 0, 5, 42, 0, a);
        widget_number(&mut *g, 0, 1, 3, -3, 1, a);
        widget_number(&mut *g, 0, 2, 3, 0, 1, a);
        widget_number(&mut *g, 0, 3, 2, 2048, 0, a);
    }
    assert_eq!(text(&g, 0, 0, 6), "   42.");
    assert_eq!(text(&g, 0, 1, 3), " -3");
    assert_eq!(text(&g, 0, 2, 3), " +0");
    assert_eq!(text(&g, 0, 3, 3), "48.", "clipped to its width");
}

#[test]
fn a_bar_fills_its_cells_in_eighths() {
    let mut g = new_grid();
    let a = attr(1, 0);
    let bar = |g: &mut grid, value: u32| {
        unsafe { widget_bar(g, 0, 0, 4, value, 64, a) };
        (0..4)
            .map(|c| at(g, c, 0) - FONT_BAR0 as u8)
            .collect::<Vec<u8>>()
    };
    assert_eq!(bar(&mut g, 0), [0, 0, 0, 0]);
    assert_eq!(bar(&mut g, 64), [8, 8, 8, 8]);
    assert_eq!(bar(&mut g, 20), [8, 2, 0, 0]);
    assert_eq!(bar(&mut g, 63), [8, 8, 8, 7]);
}

#[derive(Default)]
struct Ctx {
    level: i32,
    mode: bool,
    log: Vec<String>,
}

unsafe fn ctx<'a>(c: *mut c_void) -> &'a mut Ctx {
    unsafe { &mut *c.cast::<Ctx>() }
}

unsafe fn put_value(buf: *mut c_char, s: &str) {
    assert!(s.len() < MENU_VALUE_CHARS as usize);
    for (i, b) in s.bytes().chain([0]).enumerate() {
        unsafe { *buf.add(i) = b as c_char };
    }
}

unsafe extern "C" fn level_value(c: *mut c_void, buf: *mut c_char) {
    let v = unsafe { ctx(c) }.level;
    unsafe { put_value(buf, &v.to_string()) };
}

unsafe extern "C" fn level_turn(c: *mut c_void, steps: i32) {
    let c = unsafe { ctx(c) };
    c.level += steps;
    c.log.push(format!("level {steps}"));
}

unsafe extern "C" fn mode_value(c: *mut c_void, buf: *mut c_char) {
    let on = unsafe { ctx(c) }.mode;
    unsafe { put_value(buf, if on { "ON" } else { "OFF" }) };
}

unsafe extern "C" fn mode_turn(c: *mut c_void, _steps: i32) {
    let c = unsafe { ctx(c) };
    c.mode = !c.mode;
    c.log.push("mode".into());
}

unsafe extern "C" fn open_turn(c: *mut c_void, steps: i32) {
    unsafe { ctx(c) }.log.push(format!("open {steps}"));
}

fn item(label: &'static CStr, value: ValueFn, turn: TurnFn) -> menu_item {
    menu_item {
        label: label.as_ptr(),
        value,
        turn,
    }
}

type ValueFn = Option<unsafe extern "C" fn(*mut c_void, *mut c_char)>;
type TurnFn = Option<unsafe extern "C" fn(*mut c_void, i32)>;

fn items() -> Vec<menu_item> {
    vec![
        item(c"LEVEL", Some(level_value), Some(level_turn)),
        item(c"MODE", Some(mode_value), Some(mode_turn)),
        item(c"OPEN", None, Some(open_turn)),
        item(c"FOUR", Some(level_value), Some(level_turn)),
        item(c"FIVE", Some(level_value), Some(level_turn)),
    ]
}

fn new_menu(items: &[menu_item], rows: u32, c: &mut Ctx) -> menu {
    let mut m = menu::default();
    unsafe {
        menu_init(
            &mut m,
            items.as_ptr(),
            items.len() as u32,
            rows,
            (c as *mut Ctx).cast(),
        )
    };
    m
}

#[test]
fn blank_large_quadrants_preserve_every_pixel() {
    let palette = unsafe { grid_palette };
    for label in [c"LEARN CUTOFF", c"SEND A CC", c"HOME CANCEL", c"I_ +-"] {
        let mut actual = new_grid();
        let mut expected = new_grid();
        let a = attr(GRID_YELLOW, GRID_BLACK);
        unsafe {
            grid_text_big(&mut *actual, 1, 20, 2, label.as_ptr(), a);
            grid_text_big(&mut *expected, 1, 20, 2, label.as_ptr(), a);
            widget_blank_pieces(&mut *actual, 1, 20, 28);
        }
        for row in 20..22 {
            let run = grid_run {
                row,
                col: 0,
                len: 30,
            };
            let mut a = vec![0u16; 240 * 8];
            let mut b = a.clone();
            unsafe {
                grid_render_run(&*actual, &run, palette.as_ptr(), a.as_mut_ptr());
                grid_render_run(&*expected, &run, palette.as_ptr(), b.as_mut_ptr());
            }
            assert_eq!(a, b, "{label:?}, row {row}");
        }
    }
}

#[test]
fn a_fast_spin_never_shows_two_selected_lines() {
    let labels: Vec<_> = (0..15)
        .map(|i| std::ffi::CString::new(vec![b'A' + i; 13]).unwrap())
        .collect();
    let its: Vec<_> = labels
        .iter()
        .map(|s| menu_item {
            label: s.as_ptr(),
            value: None,
            turn: None,
        })
        .collect();
    let mut c = Ctx::default();
    let mut m = new_menu(&its, 6, &mut c);
    m.scale = 2;
    m.budget = MENU_BUDGET_CELLS;
    m.sel_attr = attr(GRID_YELLOW, GRID_DARK_GREY);
    let base = attr(GRID_WHITE, GRID_BLACK);
    let mut g = new_grid();
    crate::drain(&mut g);
    for _ in 0..8 {
        unsafe { menu_draw(&mut m, &mut *g, 0, 4, 30, base, 0) };
        crate::drain(&mut g);
    }
    for step in [1, 1, 1, 1, 1, 1, -1, -1] {
        unsafe {
            menu_move(&mut m, step);
            menu_draw(&mut m, &mut *g, 0, 4, 30, base, 0);
        }
        let selected = (0..6)
            .filter(|&k| g.attr[(4 + 3 * k) * COLS] == m.sel_attr)
            .count();
        assert_eq!(selected, 1, "cursor {}, top {}", m.cursor, m.top);
        assert!(crate::drain(&mut g).cells <= usize::from(MENU_BUDGET_CELLS));
    }
    for _ in 0..3 {
        unsafe { menu_draw(&mut m, &mut *g, 0, 4, 30, base, 0) };
        crate::drain(&mut g);
    }
    let mut fresh = new_grid();
    let mut reference = new_menu(&its, 6, &mut c);
    reference.scale = 2;
    reference.sel_attr = m.sel_attr;
    reference.cursor = m.cursor;
    reference.top = m.top;
    unsafe { menu_draw(&mut reference, &mut *fresh, 0, 4, 30, base, 0) };
    assert_eq!(&g.ch[4 * COLS..22 * COLS], &fresh.ch[4 * COLS..22 * COLS]);
    assert_eq!(
        &g.attr[4 * COLS..22 * COLS],
        &fresh.attr[4 * COLS..22 * COLS]
    );
    assert_eq!(&g.src[4 * COLS..22 * COLS], &fresh.src[4 * COLS..22 * COLS]);
}

#[test]
fn colliding_menu_rows_never_keep_the_old_value() {
    unsafe extern "C" fn frames(c: *mut c_void, buf: *mut c_char) {
        let value = unsafe { ctx(c) }.level;
        unsafe { put_value(buf, &format!("{value} FR")) };
    }
    let its = [item(c"ARP RATE", Some(frames), Some(level_turn))];
    let mut c = Ctx {
        level: 19,
        ..Ctx::default()
    };
    let mut m = new_menu(&its, 1, &mut c);
    m.scale = 2;
    let mut g = new_grid();
    let a = attr(GRID_WHITE, GRID_BLACK);
    unsafe { menu_draw(&mut m, &mut *g, 0, 4, 30, a, 0) };
    assert_eq!(big_line(&g, 4, 1, 14), "ARP RATE 19 FR");
    crate::drain(&mut g);
    unsafe { menu_turn(&mut m, 13) };
    unsafe { menu_draw(&mut m, &mut *g, 0, 4, 30, a, 0) };
    assert_eq!(big_line(&g, 4, 1, 14), "ARP RATE 32 FR");
}

#[test]
fn the_cursor_moves_clamped_and_the_rows_shown_follow_it() {
    let its = items();
    let mut c = Ctx::default();
    let mut m = new_menu(&its, 3, &mut c);
    assert_eq!((m.cursor, m.top), (0, 0));
    let mv = |m: &mut menu, steps| {
        unsafe { menu_move(m, steps) };
        (m.cursor, m.top)
    };
    assert_eq!(mv(&mut m, -1), (0, 0));
    assert_eq!(mv(&mut m, 2), (2, 0));
    assert_eq!(mv(&mut m, 1), (3, 1));
    assert_eq!(mv(&mut m, 9), (4, 2));
    assert_eq!(mv(&mut m, -2), (2, 2));
    assert_eq!(mv(&mut m, -1), (1, 1));
    assert_eq!(mv(&mut m, -9), (0, 0));
}

#[test]
fn turning_changes_the_value_under_the_cursor_and_opens_an_action_clockwise() {
    let its = items();
    let mut c = Ctx::default();
    let mut m = new_menu(&its, 5, &mut c);
    unsafe {
        menu_turn(&mut m, 3);
        menu_turn(&mut m, -1);
        menu_move(&mut m, 1);
        menu_turn(&mut m, 1);
        menu_move(&mut m, 1);
        menu_turn(&mut m, -2);
        menu_turn(&mut m, 1);
    }
    assert_eq!(c.level, 2);
    assert!(c.mode);
    assert_eq!(c.log, ["level 3", "level -1", "mode", "open 1"]);
}

#[test]
fn a_menu_draws_its_rows_with_values_on_the_right_and_a_block_cursor() {
    let its = items();
    let f = font::load();
    let mut c = Ctx {
        level: -12,
        ..Ctx::default()
    };
    let mut m = new_menu(&its, 3, &mut c);
    let mut g = new_grid();
    let a = attr(GRID_LIGHT_BLUE, GRID_BLUE);
    unsafe {
        menu_move(&mut m, 1);
        menu_draw(&mut m, &mut *g, 3, 5, 12, a, 1);
    }
    assert_eq!(text(&g, 3, 5, 12), " LEVEL   -12");
    assert_eq!(at(&g, 3, 6), FONT_FULL as u8, "the cursor on MODE");
    assert_eq!(text(&g, 4, 6, 11), "MODE    OFF");
    assert_eq!(text(&g, 3, 7, 11), " OPEN      ");
    assert_eq!(at(&g, 14, 7), f.code("ARROW_RIGHT"));
    assert_eq!(at(&g, 15, 6), b'.', "drew past its width");
    assert!((3..15).all(|col| (5..8).all(|row| g.attr[row * COLS + col] == a)));
    assert_eq!(at(&g, 3, 8), b'.', "drew past its rows");
    unsafe {
        menu_move(&mut m, 3);
        menu_draw(&mut m, &mut *g, 3, 5, 12, a, 0);
    }
    assert_eq!(text(&g, 3, 5, 11), " OPEN      ");
    assert_eq!(text(&g, 3, 7, 12), " FIVE    -12");
}

fn big_line(g: &grid, row: usize, col: usize, chars: usize) -> String {
    (0..chars)
        .map(|k| g.src[row * COLS + col + 2 * k] as char)
        .collect()
}

#[test]
fn a_menu_at_scale_two_draws_large_lines_a_quota_at_a_time_and_only_what_changed() {
    let its = items();
    let mut c = Ctx {
        level: 7,
        ..Ctx::default()
    };
    let mut m = new_menu(&its, 4, &mut c);
    m.scale = 2;
    let sel = attr(GRID_BLUE, GRID_LIGHT_BLUE);
    m.sel_attr = sel;
    let mut g = new_grid();
    let a = attr(GRID_LIGHT_BLUE, GRID_BLUE);
    let draw = |m: &mut menu, g: &mut grid| unsafe { menu_draw(m, g, 0, 4, 30, a, 0) };
    crate::drain(&mut g);
    m.budget = MENU_BUDGET_CELLS;
    draw(&mut m, &mut g);
    assert_eq!(
        (m.pending, m.valid),
        (1, 0b0011),
        "the selected line and the first"
    );
    assert_eq!(big_line(&g, 4, 1, 14), format!("{:<5}{:>9}", "LEVEL", "7"));
    assert_eq!(
        g.attr[4 * COLS],
        sel,
        "the selected line's bar, margin included"
    );
    assert_eq!(g.attr[4 * COLS + 29], sel);
    assert_eq!(g.ch[10 * COLS + 2], b'.', "the third line is not drawn yet");
    crate::drain(&mut g);
    draw(&mut m, &mut g);
    assert_eq!((m.pending, m.valid), (0, 0b1111));
    let before = (g.ch, g.attr);
    draw(&mut m, &mut g);
    assert!((g.ch, g.attr) == before && m.pending == 0);
    c.level = 12;
    crate::drain(&mut g);
    draw(&mut m, &mut g);
    assert_eq!(big_line(&g, 4, 1, 14), format!("{:<5}{:>9}", "LEVEL", "12"));
    crate::drain(&mut g);
    unsafe { menu_move(&mut m, 1) };
    draw(&mut m, &mut g);
    assert_eq!(g.attr[4 * COLS], a, "the old line lost its bar");
    assert_eq!(g.attr[7 * COLS], sel, "the new one has it");
    assert_eq!(m.pending, 0);
    crate::drain(&mut g);
    unsafe { menu_move(&mut m, 9) };
    draw(&mut m, &mut g);
    assert_eq!(m.pending, 1);
    assert_eq!(
        big_line(&g, 13, 1, 14),
        format!("{:<4}{:>10}", "FIVE", "12"),
        "the cursor's line first"
    );
    crate::drain(&mut g);
    draw(&mut m, &mut g);
    assert_eq!(m.pending, 0);
    assert_eq!(
        big_line(&g, 4, 1, 14),
        format!("{:<4}{:>10}", "MODE", "OFF")
    );
    g.ch[7 * COLS + 2] = b'#';
    m.force_rows = 0b11 << 7;
    m.budget = 1;
    draw(&mut m, &mut g);
    let line = big_line(&g, 7, 1, 14);
    assert!(line.starts_with("OPEN"), "{line}");
    assert_eq!(line.chars().last().unwrap() as u32, FONT_ARROW_RIGHT);
}

#[test]
fn a_menu_with_a_line_limit_draws_that_many_a_call_with_the_pixels_of_one_without() {
    let its = items();
    let a = attr(GRID_LIGHT_BLUE, GRID_BLUE);
    let sel = attr(GRID_BLUE, GRID_LIGHT_BLUE);
    let mut c1 = Ctx {
        level: 7,
        ..Ctx::default()
    };
    let mut c2 = Ctx {
        level: 7,
        ..Ctx::default()
    };
    let mut free = new_menu(&its, 5, &mut c1);
    let mut capped = new_menu(&its, 5, &mut c2);
    for m in [&mut free, &mut capped] {
        m.scale = 2;
        m.sel_attr = sel;
    }
    capped.lines = 2;
    let mut gf = new_grid();
    let mut gc = new_grid();
    unsafe { menu_draw(&mut free, &mut *gf, 0, 4, 30, a, 0) };
    assert_eq!(free.pending, 0);
    let mut calls = 0;
    loop {
        capped.budget = MENU_BUDGET_CELLS;
        unsafe { menu_draw(&mut capped, &mut *gc, 0, 4, 30, a, 0) };
        let drawn = capped.valid.count_ones();
        calls += 1;
        assert_eq!(drawn as usize, (2 * calls).min(5), "call {calls}");
        if capped.pending == 0 {
            break;
        }
        assert!(calls < 4);
    }
    assert_eq!(calls, 3, "five lines, two a call");
    let palette = unsafe { grid_palette };
    for row in 4..19u8 {
        let run = grid_run {
            row,
            col: 0,
            len: 30,
        };
        let mut x = vec![0u16; 240 * 8];
        let mut y = x.clone();
        unsafe {
            grid_render_run(&*gf, &run, palette.as_ptr(), x.as_mut_ptr());
            grid_render_run(&*gc, &run, palette.as_ptr(), y.as_mut_ptr());
        }
        assert_eq!(x, y, "row {row}");
    }
}
