// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Widgets (app/widget.c): the frame with a title, numbers, bars and the menu.

use std::ffi::{c_char, c_void, CStr};

use sidou_sys::*;
use sidou_tests::font;

const COLS: usize = GRID_COLS as usize;

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
    // A title longer than the edge is cut before the corner; none leaves the edge plain.
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

/// A menu's context: two values and a log of what ran.
#[derive(Default)]
struct Ctx {
    level: i32,
    mode: bool,
    log: Vec<String>,
}

unsafe fn ctx<'a>(c: *mut c_void) -> &'a mut Ctx {
    unsafe { &mut *c.cast::<Ctx>() }
}

/// Writes s and its terminator into a MENU_VALUE_CHARS buffer.
unsafe fn put(buf: *mut c_char, s: &str) {
    assert!(s.len() < MENU_VALUE_CHARS as usize);
    for (i, b) in s.bytes().chain([0]).enumerate() {
        unsafe { *buf.add(i) = b as c_char };
    }
}

unsafe extern "C" fn level_value(c: *mut c_void, buf: *mut c_char) {
    let v = unsafe { ctx(c) }.level;
    unsafe { put(buf, &v.to_string()) };
}

unsafe extern "C" fn level_turn(c: *mut c_void, steps: i32) {
    let c = unsafe { ctx(c) };
    c.level += steps;
    c.log.push(format!("level {steps}"));
}

unsafe extern "C" fn mode_value(c: *mut c_void, buf: *mut c_char) {
    let on = unsafe { ctx(c) }.mode;
    unsafe { put(buf, if on { "ON" } else { "OFF" }) };
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
        menu_draw(&m, &mut *g, 3, 5, 12, a, 1);
    }
    assert_eq!(text(&g, 3, 5, 12), " LEVEL   -12");
    assert_eq!(at(&g, 3, 6), FONT_FULL as u8, "the cursor on MODE");
    assert_eq!(text(&g, 4, 6, 11), "MODE    OFF");
    assert_eq!(text(&g, 3, 7, 11), " OPEN      ");
    assert_eq!(at(&g, 14, 7), f.code("ARROW_RIGHT"));
    assert_eq!(at(&g, 15, 6), b'.', "drew past its width");
    assert!((3..15).all(|col| (5..8).all(|row| g.attr[row * COLS + col] == a)));
    assert_eq!(at(&g, 3, 8), b'.', "drew past its rows");
    // The cursor off; scrolled, the rows show the items from the top one.
    unsafe {
        menu_move(&mut m, 3);
        menu_draw(&m, &mut *g, 3, 5, 12, a, 0);
    }
    assert_eq!(text(&g, 3, 5, 11), " OPEN      ");
    assert_eq!(text(&g, 3, 7, 12), " FIVE    -12");
}
