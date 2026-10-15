// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The screen (app/screen.c): the view stack and its focus, event routing, the toast, and the
//! wipe and raster transitions with what each frame of them sends.

use std::ffi::{c_int, c_void, CStr};

use sidou_sys::*;

const COLS: usize = GRID_COLS as usize;
const ROWS: usize = GRID_ROWS as usize;
const CELLS: usize = COLS * ROWS;
/// platform/fm1/lcd.c: SPI1 at 60 MHz / (4 + 1); lcd.h: 11 command bytes a window.
const SPI_HZ: f64 = 12e6;
const WINDOW_BYTES: usize = 11;
/// docs/art/README.md: at most about 20 ms of SPI per 40 ms frame.
const SPI_BUDGET_MS: f64 = 20.0;
const FRAME_MS: u32 = 40;
const BORDER_CELLS: usize = 2 * COLS + 2 * (ROWS - 2);

fn attr(fg: u32, bg: u32) -> u8 {
    ((bg << 4) | fg) as u8
}

fn border_attr() -> u8 {
    attr(GRID_LIGHT_BLUE, GRID_BLUE)
}

fn on_border(i: usize) -> bool {
    let (c, r) = (i % COLS, i / COLS);
    r == 0 || r == ROWS - 1 || c == 0 || c == COLS - 1
}

/// A test view: a light blue border of full blocks around `ch` in `fill`, and a log of the
/// calls it gets.
struct Probe {
    ch: u8,
    fill: u8,
    consume: bool,
    overlay: bool,
    log: Vec<String>,
}

impl Probe {
    fn new(ch: u8) -> Box<Self> {
        Box::new(Probe {
            ch,
            fill: attr(GRID_WHITE, GRID_BLUE),
            consume: false,
            overlay: false,
            log: Vec::new(),
        })
    }

    fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }
}

unsafe fn probe<'a>(state: *mut c_void) -> &'a mut Probe {
    unsafe { &mut *state.cast::<Probe>() }
}

unsafe extern "C" fn p_enter(_s: *mut screen, state: *mut c_void) {
    unsafe { probe(state) }.log.push("enter".into());
}

unsafe extern "C" fn p_exit(_s: *mut screen, state: *mut c_void) {
    unsafe { probe(state) }.log.push("exit".into());
}

unsafe extern "C" fn p_input(_s: *mut screen, state: *mut c_void, e: *const screen_event) -> c_int {
    let p = unsafe { probe(state) };
    let e = unsafe { &*e };
    p.log.push(format!(
        "input {} {} {} {:#x}",
        e.kind, e.id, e.steps, e.held
    ));
    c_int::from(p.consume)
}

unsafe extern "C" fn p_compose(_s: *mut screen, state: *mut c_void, g: *mut grid) {
    let p = unsafe { probe(state) };
    p.log.push("compose".into());
    unsafe {
        grid_fill(
            g,
            0,
            0,
            GRID_COLS,
            GRID_ROWS,
            FONT_FULL as u8,
            border_attr(),
        );
        grid_fill(g, 1, 1, GRID_COLS - 2, GRID_ROWS - 2, p.ch, p.fill);
        if p.overlay {
            grid_overlay(g, 5, 5, [0x3c; 8].as_ptr(), GRID_YELLOW as u8);
        }
    }
}

fn probe_view(name: &'static CStr) -> Box<view> {
    Box::new(view {
        name: name.as_ptr(),
        enter: Some(p_enter),
        exit: Some(p_exit),
        input: Some(p_input),
        compose: Some(p_compose),
    })
}

/// What a frame sent.
#[derive(Debug, Default, Clone, Copy)]
struct Sent {
    windows: usize,
    cells: usize,
}

impl Sent {
    fn spi_ms(self) -> f64 {
        (self.cells * 128 + self.windows * WINDOW_BYTES) as f64 * 8.0 / SPI_HZ * 1e3
    }
}

/// A screen over a grid, and what the panel shows (each cell's code and colour as sent).
struct Rig {
    g: Box<grid>,
    s: Box<screen>,
    shown: Vec<(u8, u8)>,
    now: u32,
}

impl Rig {
    fn new(root: &view, state: &mut Probe) -> Self {
        let mut r = Rig {
            g: Box::<grid>::default(),
            s: Box::<screen>::default(),
            shown: vec![(0, 0); CELLS],
            now: 0,
        };
        unsafe {
            grid_init(&mut *r.g, b' ', border_attr());
            screen_init(
                &mut *r.s,
                &mut *r.g,
                root,
                state as *mut Probe as *mut c_void,
            );
        }
        r.frame();
        r
    }

    fn drain(&mut self) -> Sent {
        let mut sent = Sent::default();
        let mut run = grid_run::default();
        while unsafe { grid_next_run(&mut *self.g, &mut run) } != 0 {
            sent.windows += 1;
            sent.cells += usize::from(run.len);
            let i0 = usize::from(run.row) * COLS + usize::from(run.col);
            for i in i0..i0 + usize::from(run.len) {
                self.shown[i] = (self.g.ch[i], self.g.attr[i]);
            }
        }
        sent
    }

    /// One frame, 40 ms after the last, sent in full.
    fn frame(&mut self) -> Sent {
        self.now += FRAME_MS;
        unsafe { screen_frame(&mut *self.s, self.now) };
        self.drain()
    }

    fn push(&mut self, v: &view, state: &mut Probe, w: screen_wipe) -> i32 {
        unsafe { screen_push(&mut *self.s, v, state as *mut Probe as *mut c_void, w) }
    }

    fn pop(&mut self, w: screen_wipe) -> i32 {
        unsafe { screen_pop(&mut *self.s, w) }
    }

    fn top(&self) -> *const view {
        unsafe { screen_top(&*self.s) }
    }

    fn input(&mut self, kind: screen_event_kind, id: u8, steps: i16, held: u32) -> bool {
        let e = screen_event {
            kind: kind as u8,
            id,
            steps,
            held,
        };
        unsafe { screen_input(&mut *self.s, &e) != 0 }
    }

    fn animating(&self) -> bool {
        unsafe { screen_animating(&*self.s) != 0 }
    }

    /// Rows whose every interior cell shows ch.
    fn rows_showing(&self, ch: u8) -> Vec<usize> {
        (0..ROWS)
            .filter(|&r| (1..COLS - 1).all(|c| self.shown[r * COLS + c].0 == ch))
            .collect()
    }

    fn row_text(&self, row: usize, col: usize, len: usize) -> String {
        (col..col + len)
            .map(|c| self.shown[row * COLS + c].0 as char)
            .collect()
    }
}

#[test]
fn the_root_view_is_entered_composed_every_frame_and_takes_the_events() {
    let root = probe_view(c"ROOT");
    let mut rs = Probe::new(b'R');
    let mut r = Rig::new(&root, &mut rs);
    assert_eq!(rs.take(), ["enter", "compose"]);
    r.frame();
    r.frame();
    assert_eq!(rs.take(), ["compose", "compose"]);
    assert_eq!(r.rows_showing(b'R'), (1..ROWS - 1).collect::<Vec<_>>());
    assert!(!r.input(SCREEN_TURN, 3, -2, 1 << 6), "not consumed");
    rs.consume = true;
    assert!(r.input(SCREEN_PRESS, 8, 0, 1 << 8));
    assert_eq!(rs.take(), ["input 1 3 -2 0x40", "input 0 8 0 0x100"]);
    assert_eq!(r.top(), &*root as *const view);
}

#[test]
fn push_and_pop_move_the_focus_and_call_enter_and_exit() {
    let (root, a, b) = (probe_view(c"ROOT"), probe_view(c"A"), probe_view(c"B"));
    let (mut rs, mut as_, mut bs) = (Probe::new(b'R'), Probe::new(b'A'), Probe::new(b'B'));
    let mut r = Rig::new(&root, &mut rs);
    rs.take();
    assert_eq!(r.push(&a, &mut as_, SCREEN_CUT), 0);
    assert_eq!(r.push(&b, &mut bs, SCREEN_CUT), 0);
    assert_eq!(as_.take(), ["enter"]);
    assert_eq!(bs.take(), ["enter"]);
    r.frame();
    r.input(SCREEN_PRESS, 2, 0, 0);
    assert_eq!(bs.take(), ["compose", "input 0 2 0 0x0"]);
    assert!(
        rs.take().is_empty() && as_.take().is_empty(),
        "only the top view"
    );
    assert_eq!(r.pop(SCREEN_CUT), 0);
    assert_eq!(bs.take(), ["exit"]);
    assert!(as_.take().is_empty(), "A was not entered again");
    r.frame();
    r.input(SCREEN_PRESS, 2, 0, 0);
    assert_eq!(as_.take(), ["compose", "input 0 2 0 0x0"]);
    assert_eq!(r.top(), &*a as *const view);
    assert_eq!(r.rows_showing(b'A').len(), ROWS - 2);
}

#[test]
fn the_stack_keeps_its_root_and_has_a_ceiling() {
    let root = probe_view(c"ROOT");
    let v = probe_view(c"V");
    let mut rs = Probe::new(b'R');
    let mut states: Vec<Box<Probe>> = (0..SCREEN_DEPTH).map(|_| Probe::new(b'V')).collect();
    let mut r = Rig::new(&root, &mut rs);
    assert_eq!(r.pop(SCREEN_CUT), -1, "popped the root");
    assert_eq!(r.top(), &*root as *const view);
    for st in states.iter_mut().take(SCREEN_DEPTH as usize - 1) {
        assert_eq!(r.push(&v, st, SCREEN_CUT), 0);
    }
    let last = states.last_mut().unwrap();
    assert_eq!(r.push(&v, last, SCREEN_CUT), -1, "past SCREEN_DEPTH");
    assert!(last.take().is_empty(), "the refused view was entered");
    for _ in 1..SCREEN_DEPTH {
        assert_eq!(r.pop(SCREEN_CUT), 0);
    }
    assert_eq!(r.pop(SCREEN_CUT), -1);
}

static CAPTURED: std::sync::Mutex<Vec<(u8, u8, i16)>> = std::sync::Mutex::new(Vec::new());

unsafe extern "C" fn capture_turns(ctx: *mut c_void, e: *const screen_event) -> c_int {
    assert_eq!(ctx as usize, 0x5150);
    let e = unsafe { &*e };
    if e.kind != SCREEN_TURN as u8 {
        return 0;
    }
    CAPTURED.lock().unwrap().push((e.kind, e.id, e.steps));
    1
}

#[test]
fn a_capture_sees_the_events_first_until_it_is_released() {
    let root = probe_view(c"ROOT");
    let mut rs = Probe::new(b'R');
    let mut r = Rig::new(&root, &mut rs);
    rs.take();
    unsafe { screen_capture(&mut *r.s, Some(capture_turns), 0x5150 as *mut c_void) };
    assert!(
        r.input(SCREEN_TURN, 2, 1, 1 << 5),
        "the capture consumes turns"
    );
    assert!(!r.input(SCREEN_PRESS, 5, 0, 1 << 5));
    assert_eq!(
        rs.take(),
        ["input 0 5 0 0x20"],
        "a press passed on to the view"
    );
    unsafe { screen_capture(&mut *r.s, None, std::ptr::null_mut()) };
    assert!(!r.input(SCREEN_TURN, 2, 1, 0));
    assert_eq!(rs.take(), ["input 1 2 1 0x0"]);
    assert_eq!(
        CAPTURED.lock().unwrap().as_slice(),
        [(SCREEN_TURN as u8, 2, 1)]
    );
}

/// Frames until the wipe ends, with the rows each one shows of the new view; checks the
/// budget on the way.
fn wipe_frames(r: &mut Rig, ch: u8) -> Vec<Vec<usize>> {
    let mut seen = Vec::new();
    for _ in 0..20 {
        let sent = r.frame();
        assert!(
            sent.spi_ms() <= SPI_BUDGET_MS,
            "a wipe frame took {:.1} ms of SPI ({sent:?})",
            sent.spi_ms()
        );
        assert!(
            sent.windows <= SCREEN_WIPE_ROWS as usize + 1,
            "{sent:?}: one window a row"
        );
        seen.push(r.rows_showing(ch));
        if !r.animating() {
            return seen;
        }
    }
    panic!("the wipe did not end");
}

#[test]
fn a_wipe_down_reveals_the_new_view_top_first_behind_an_edge() {
    let (root, a) = (probe_view(c"ROOT"), probe_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_WIPE_DOWN);
    assert!(r.animating());
    let first = r.frame();
    let n = SCREEN_WIPE_ROWS as usize;
    assert_eq!(r.rows_showing(b'A'), (1..n).collect::<Vec<_>>());
    // The edge: the next row, a white bar across the screen.
    let edge: Vec<(u8, u8)> = r.shown[n * COLS..(n + 1) * COLS].to_vec();
    assert!(edge
        .iter()
        .all(|&(c, a)| c == FONT_FULL as u8 && a & 15 == GRID_WHITE as u8));
    // The rows below still show the root.
    assert_eq!(r.rows_showing(b'R'), (n + 1..ROWS - 1).collect::<Vec<_>>());
    assert!(first.spi_ms() <= SPI_BUDGET_MS);
    let rest = wipe_frames(&mut r, b'A');
    assert_eq!(
        rest.len() + 1,
        ROWS.div_ceil(n),
        "frames for the whole screen"
    );
    for (k, rows) in rest.iter().enumerate() {
        let revealed = ((k + 2) * n).min(ROWS);
        assert_eq!(
            rows,
            &(1..revealed.min(ROWS - 1)).collect::<Vec<_>>(),
            "frame {}",
            k + 2
        );
    }
    assert_eq!(r.g.hold, 0, "rows left held");
    assert_eq!(r.rows_showing(b'A').len(), ROWS - 2);
    assert!(r
        .shown
        .iter()
        .enumerate()
        .all(|(i, &(c, a))| on_border(i) == (c == FONT_FULL as u8 && a == border_attr())));
    assert_eq!(r.frame().cells, 0, "the wipe left work behind");
}

#[test]
fn a_wipe_up_reveals_the_view_below_bottom_first() {
    let (root, a) = (probe_view(c"ROOT"), probe_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_CUT);
    r.frame();
    r.pop(SCREEN_WIPE_UP);
    r.frame();
    let n = SCREEN_WIPE_ROWS as usize;
    assert_eq!(
        r.rows_showing(b'R'),
        (ROWS - n..ROWS - 1).collect::<Vec<_>>()
    );
    let edge = ROWS - n - 1;
    assert!(r.shown[edge * COLS..(edge + 1) * COLS]
        .iter()
        .all(|&(c, _)| c == FONT_FULL as u8));
    assert_eq!(r.rows_showing(b'A'), (1..edge).collect::<Vec<_>>());
    wipe_frames(&mut r, b'R');
    assert_eq!(r.rows_showing(b'R').len(), ROWS - 2);
}

#[test]
fn a_cut_sends_the_whole_change_in_one_frame() {
    let (root, a) = (probe_view(c"ROOT"), probe_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_CUT);
    assert!(!r.animating());
    let sent = r.frame();
    assert_eq!(
        sent.cells,
        (COLS - 2) * (ROWS - 2),
        "the inside, the border unchanged"
    );
    assert_eq!(r.rows_showing(b'A').len(), ROWS - 2);
}

#[test]
fn a_switch_clears_the_old_views_overlays() {
    let (root, a) = (probe_view(c"ROOT"), probe_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    as_.overlay = true;
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_CUT);
    r.frame();
    assert_eq!(r.g.ovl[5 * COLS + 5], 1);
    r.pop(SCREEN_CUT);
    r.frame();
    assert_eq!(r.g.ovl[5 * COLS + 5], 0, "A's overlay stayed on the root");
    assert_eq!(r.g.ovl_nfree, GRID_OVERLAYS as u8);
}

#[test]
fn the_raster_sweeps_the_border_every_other_frame_and_gives_it_back() {
    let root = probe_view(c"ROOT");
    let mut rs = Probe::new(b'R');
    let mut r = Rig::new(&root, &mut rs);
    let steps = 3;
    unsafe { screen_raster(&mut *r.s, steps) };
    assert!(r.animating());
    let ramp = unsafe { screen_ramp };
    let mut sends = Vec::new();
    let mut prev_colours: Option<Vec<u8>> = None;
    for _ in 0..steps * SCREEN_RASTER_FRAMES {
        let sent = r.frame();
        assert!(sent.spi_ms() <= SPI_BUDGET_MS, "{:.1} ms", sent.spi_ms());
        sends.push(sent.cells);
        // Every border cell takes a ramp colour by its row, in bands; the inside is untouched.
        let colours: Vec<u8> = (0..ROWS).map(|row| r.shown[row * COLS].1 & 15).collect();
        for (i, &(c, a)) in r.shown.iter().enumerate() {
            let row = i / COLS;
            if on_border(i) {
                assert_eq!(c, FONT_FULL as u8);
                assert_eq!(a & 15, colours[row], "cell {i}: one colour a row");
                assert_eq!(a >> 4, GRID_BLUE as u8, "the background is kept");
            } else {
                assert_eq!((c, a), (b'R', attr(GRID_WHITE, GRID_BLUE)));
            }
        }
        for row in 1..ROWS {
            let k = ramp.iter().position(|&x| x == colours[row - 1]).unwrap();
            assert_eq!(
                colours[row],
                ramp[(k + 1) % ramp.len()],
                "bands follow the ramp"
            );
        }
        if let Some(p) = &prev_colours {
            if sent.cells > 0 {
                assert_eq!(&colours[1..], &p[..ROWS - 1], "the bands move down a row");
            }
        }
        prev_colours = Some(colours);
    }
    // A step every other frame recolours the border; the frames between send nothing. The
    // first step leaves the rows that take the view's own light blue alone; every later step
    // changes every border cell.
    for (f, &cells) in sends.iter().enumerate() {
        let step = f % SCREEN_RASTER_FRAMES as usize == 0;
        match (step, f) {
            (false, _) => assert_eq!(cells, 0, "frame {f}"),
            (true, 0) => assert!(cells > 0 && cells <= BORDER_CELLS, "frame {f}: {cells}"),
            (true, _) => assert_eq!(cells, BORDER_CELLS, "frame {f}"),
        }
    }
    // Then the border is the view's again.
    let back = r.frame().cells;
    assert!(back > 0 && back <= BORDER_CELLS, "{back}");
    assert!(!r.animating());
    assert!(r
        .shown
        .iter()
        .enumerate()
        .filter(|&(i, _)| on_border(i))
        .all(|(_, &(c, a))| (c, a) == (FONT_FULL as u8, border_attr())));
    assert_eq!(r.frame().cells, 0);
}

#[test]
fn the_raster_waits_for_a_wipe() {
    let (root, a) = (probe_view(c"ROOT"), probe_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_WIPE_DOWN);
    unsafe { screen_raster(&mut *r.s, 1) };
    let full = SCREEN_RASTER_FRAMES as u8;
    let mut frames = 0;
    while r.g.hold != 0 || frames == 0 {
        r.frame();
        frames += 1;
        assert_eq!(r.s.raster_frames, full, "the raster ran during the wipe");
    }
    assert!(r.animating(), "the raster was dropped");
    r.frame();
    assert_eq!(
        r.s.raster_frames,
        full - 1,
        "the raster did not start after the wipe"
    );
    let top = r.shown[0].1 & 15;
    assert_ne!(top, GRID_LIGHT_BLUE as u8, "the top row takes the ramp");
}

#[test]
fn a_toast_shows_on_its_row_until_it_times_out() {
    let root = probe_view(c"ROOT");
    let mut rs = Probe::new(b'R');
    let mut r = Rig::new(&root, &mut rs);
    let row = SCREEN_TOAST_ROW as usize;
    let col = SCREEN_TOAST_COL as usize;
    unsafe { screen_toast(&mut *r.s, c"OCTAVE +1".as_ptr(), 100) };
    let sent = r.frame();
    assert_eq!((sent.windows, sent.cells), (1, 9));
    assert_eq!(r.row_text(row, col, 9), "OCTAVE +1");
    let a = r.shown[row * COLS + col].1;
    assert_eq!(a, attr(GRID_BLUE, GRID_LIGHT_BLUE), "reverse video");
    assert_eq!(r.frame().cells, 0, "a toast that stays costs nothing");
    r.frame(); // 120 ms after the toast
    assert_eq!(r.row_text(row, col, 9), "RRRRRRRRR");
    // A long one is clipped inside the frame's margin, and a new one replaces the last.
    unsafe {
        screen_toast(
            &mut *r.s,
            c"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ".as_ptr(),
            1000,
        )
    };
    r.frame();
    let n = SCREEN_TOAST_CHARS as usize;
    assert_eq!(r.row_text(row, col, n), "0123456789ABCDEFGHIJKLMNOP");
    assert_eq!(r.shown[row * COLS + col + n].0, b'R');
    unsafe { screen_toast(&mut *r.s, c"HI".as_ptr(), 1000) };
    r.frame();
    assert_eq!(r.row_text(row, col, 3), "HIR");
}

#[test]
fn the_block_cursor_blinks_every_eight_frames() {
    let root = probe_view(c"ROOT");
    let mut rs = Probe::new(b'R');
    let mut r = Rig::new(&root, &mut rs);
    let mut runs: Vec<(bool, u32)> = Vec::new();
    for _ in 0..40 {
        r.frame();
        let on = unsafe { screen_blink(&*r.s) } != 0;
        match runs.last_mut() {
            Some((v, n)) if *v == on => *n += 1,
            _ => runs.push((on, 1)),
        }
    }
    assert!(runs.len() >= 4);
    for &(_, n) in &runs[1..runs.len() - 1] {
        assert_eq!(n, SCREEN_BLINK_FRAMES);
    }
}
