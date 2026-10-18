// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The screen (app/screen.c): the view stack and its focus, event routing, the toast, and the
//! wipe and raster transitions with what each frame of them sends.

use std::ffi::{c_int, c_void, CStr};

use sidou_sys::*;

use crate::piece_part;

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
    /// What its changed function answers (a view made with quiet_view); compose clears it.
    dirty: bool,
    log: Vec<String>,
}

impl Probe {
    fn new(ch: u8) -> Box<Self> {
        Box::new(Probe {
            ch,
            fill: attr(GRID_WHITE, GRID_BLUE),
            consume: false,
            overlay: false,
            dirty: false,
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
    p.dirty = false;
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

unsafe extern "C" fn p_changed(_s: *mut screen, state: *mut c_void) -> c_int {
    c_int::from(unsafe { probe(state) }.dirty)
}

/// A probe composed every frame (no changed function).
fn probe_view(name: &'static CStr) -> Box<view> {
    Box::new(view {
        name: name.as_ptr(),
        enter: Some(p_enter),
        exit: Some(p_exit),
        input: Some(p_input),
        compose: Some(p_compose),
        changed: None,
    })
}

/// A probe composed only when its `dirty` is set or the screen forces it.
fn quiet_view(name: &'static CStr) -> Box<view> {
    Box::new(view {
        changed: Some(p_changed),
        ..*probe_view(name)
    })
}

/// What a frame sent.
#[derive(Debug, Default, Clone, Copy)]
struct Sent {
    windows: usize,
    cells: usize,
    /// Rows with a window.
    rows: usize,
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
        let mut rows = 0u32;
        while unsafe { grid_next_run(&mut *self.g, &mut run) } != 0 {
            rows |= 1 << run.row;
            sent.windows += 1;
            sent.cells += usize::from(run.len);
            let i0 = usize::from(run.row) * COLS + usize::from(run.col);
            for i in i0..i0 + usize::from(run.len) {
                self.shown[i] = (self.g.ch[i], self.g.attr[i]);
            }
        }
        sent.rows = rows.count_ones() as usize;
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

    /// `len` characters of what the panel shows on a row from `col`: large text (a character
    /// over several cells) as one character, from the grid's record of it.
    fn row_text(&self, row: usize, col: usize, len: usize) -> String {
        (col..COLS)
            .filter_map(|c| {
                let i = row * COLS + c;
                let ch = self.shown[i].0;
                let piece = usize::from(ch).wrapping_sub(GRID_BIG_FIRST as usize);
                if piece >= GRID_BIG_PIECES as usize {
                    return Some(ch as char);
                }
                let (scale, q) = piece_part(piece);
                (q % scale == 0).then(|| self.g.src[i] as char)
            })
            .take(len)
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

/// A banded cut (the title cards' end, app/title_card.h): the view below in bands of
/// SCREEN_BAND_ROWS rows a frame, top first, with no edge, each frame inside the SPI budget.
#[test]
fn a_banded_cut_reveals_the_view_below_top_first_with_no_edge() {
    let (root, a) = (probe_view(c"ROOT"), probe_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_CUT);
    r.frame();
    r.pop(SCREEN_BANDS);
    assert!(r.animating());
    let n = SCREEN_BAND_ROWS as usize;
    let mut frames = 0;
    loop {
        let sent = r.frame();
        frames += 1;
        assert!(
            sent.spi_ms() <= SPI_BUDGET_MS,
            "a band took {:.1} ms of SPI ({sent:?})",
            sent.spi_ms()
        );
        assert!(sent.windows <= n, "{sent:?}: one window a row");
        let revealed = (frames * n).min(ROWS);
        assert_eq!(
            r.rows_showing(b'R'),
            (1..revealed.min(ROWS - 1)).collect::<Vec<_>>(),
            "frame {frames}"
        );
        // No edge: the rows below still show A, all of them.
        assert_eq!(
            r.rows_showing(b'A'),
            (revealed.max(1)..ROWS - 1).collect::<Vec<_>>(),
            "frame {frames}"
        );
        if !r.animating() {
            break;
        }
    }
    assert_eq!(frames, ROWS.div_ceil(n), "frames for the whole screen");
    assert_eq!(r.g.hold, 0, "rows left held");
    assert_eq!(r.frame().cells, 0, "the cut left work behind");
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
fn a_reveal_wipes_the_same_view_in_anew() {
    // A view that changes its whole look (the boot's title card) shows it behind a wipe,
    // as a switch would: the stack, the focus and the capture stay as they are.
    let (root, a) = (probe_view(c"ROOT"), probe_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_CUT);
    unsafe { screen_capture(&mut *r.s, Some(consume_all), std::ptr::null_mut()) };
    r.frame();
    as_.overlay = true;
    as_.ch = b'B';
    as_.take();
    unsafe { screen_reveal(&mut *r.s, SCREEN_WIPE_DOWN) };
    assert!(r.animating());
    let rest = wipe_frames(&mut r, b'B');
    assert_eq!(rest.len(), ROWS.div_ceil(SCREEN_WIPE_ROWS as usize));
    assert_eq!(r.rows_showing(b'B').len(), ROWS - 2);
    assert_eq!(r.top(), &*a as *const view);
    assert!(
        as_.take().iter().all(|c| c == "compose"),
        "no enter or exit"
    );
    assert!(r.input(SCREEN_PRESS, 3, 0, 0), "the capture went");
    assert_eq!(
        r.g.ovl[5 * COLS + 5],
        1,
        "the overlay drawn after the reveal"
    );
}

unsafe extern "C" fn consume_all(_ctx: *mut c_void, _e: *const screen_event) -> c_int {
    1
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

/// A view pushed over the one on top (SCREEN_OVER: MIDI learn's frame, which composes the
/// view under it) comes in at once, and the overlays of the view under it stay on the grid,
/// there and back: that view goes on showing, and a switch that cleared them would send every
/// cell they were on.
#[test]
fn a_view_pushed_over_another_keeps_its_overlays() {
    let (root, a) = (probe_view(c"ROOT"), probe_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    rs.overlay = true;
    let mut r = Rig::new(&root, &mut rs);
    assert_eq!(r.g.ovl[5 * COLS + 5], 1);
    assert_eq!(r.push(&a, &mut as_, SCREEN_OVER), 0);
    assert!(!r.animating(), "no wipe");
    assert_eq!(r.g.ovl[5 * COLS + 5], 1, "the push cleared the overlays");
    assert_eq!(r.g.hold, 0, "nothing held");
    r.frame();
    assert_eq!(r.pop(SCREEN_OVER), 0);
    assert_eq!(r.g.ovl[5 * COLS + 5], 1, "the pop cleared the overlays");
    assert_eq!(
        r.frame().cells,
        CELLS - BORDER_CELLS,
        "A's cells, back to the root's"
    );
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
    // Large text: 9 characters of 2 x 2 cells, in two rows.
    assert_eq!((sent.windows, sent.cells), (2, 36));
    assert_eq!(r.row_text(row, col, 9), "OCTAVE +1");
    assert_eq!(r.row_text(row + 1, col, 9), "OCTAVE +1");
    let a = r.shown[row * COLS + col].1;
    let th = unsafe { &*theme_get(THEME_HIGH_CONTRAST) };
    assert_eq!(
        a,
        attr(u32::from(th.toast_fg), u32::from(th.toast_bg)),
        "the theme's toast colours"
    );
    assert_eq!(r.frame().cells, 0, "a toast that stays costs nothing");
    r.frame(); // 80 ms after its first frame
    assert_eq!(r.row_text(row, col, 9), "OCTAVE +1");
    r.frame(); // 120 ms after its first frame
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

#[test]
fn a_toast_runs_from_its_first_frame_whenever_it_was_set() {
    let root = probe_view(c"ROOT");
    let mut rs = Probe::new(b'R');
    let mut r = Rig::new(&root, &mut rs);
    let (row, col) = (SCREEN_TOAST_ROW as usize, SCREEN_TOAST_COL as usize);
    // Set after a long pause (no frames for 10 s, say during an update): it still shows for
    // its whole time.
    unsafe { screen_toast(&mut *r.s, c"HELLO".as_ptr(), 100) };
    r.now += 10_000;
    r.frame();
    assert_eq!(r.row_text(row, col, 5), "HELLO", "expired at once");
    r.frame();
    r.frame();
    assert_eq!(r.row_text(row, col, 5), "HELLO");
    r.frame();
    assert_eq!(
        r.row_text(row, col, 5),
        "RRRRR",
        "100 ms after its first frame"
    );
}

#[test]
fn a_toast_set_before_the_first_frame_shows_for_its_time() {
    let root = probe_view(c"ROOT");
    let mut rs = Probe::new(b'R');
    let mut g = Box::<grid>::default();
    let mut s = Box::<screen>::default();
    unsafe {
        grid_init(&mut *g, b' ', border_attr());
        screen_init(
            &mut *s,
            &mut *g,
            &*root,
            &mut *rs as *mut Probe as *mut c_void,
        );
        screen_toast(&mut *s, c"BOOT".as_ptr(), 100);
        // The first frame comes late, at 5 s.
        screen_frame(&mut *s, 5_000);
    }
    assert_eq!(s.toast_on, 1, "the toast expired before it showed");
    unsafe { screen_frame(&mut *s, 5_080) };
    assert_eq!(s.toast_on, 1);
    unsafe { screen_frame(&mut *s, 5_100) };
    assert_eq!(s.toast_on, 0);
}

static GLOBAL: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

/// A global handler that takes presses of button 8 (HOME, say).
unsafe extern "C" fn global_home(_ctx: *mut c_void, e: *const screen_event) -> c_int {
    let e = unsafe { &*e };
    if e.kind != SCREEN_PRESS as u8 || e.id != 8 {
        return 0;
    }
    GLOBAL.lock().unwrap().push(e.id);
    1
}

/// A capture that takes everything, HOME included.
unsafe extern "C" fn capture_all(_ctx: *mut c_void, _e: *const screen_event) -> c_int {
    1
}

#[test]
fn the_capture_sees_home_before_the_global_handler_and_the_global_before_the_view() {
    let root = probe_view(c"ROOT");
    let mut rs = Probe::new(b'R');
    let mut r = Rig::new(&root, &mut rs);
    rs.take();
    rs.consume = true;
    GLOBAL.lock().unwrap().clear();
    unsafe { screen_global(&mut *r.s, Some(global_home), std::ptr::null_mut()) };
    assert!(r.input(SCREEN_PRESS, 8, 0, 1 << 8));
    assert_eq!(GLOBAL.lock().unwrap().as_slice(), [8]);
    assert!(
        rs.take().is_empty(),
        "the view saw what the global handler took"
    );
    assert!(r.input(SCREEN_PRESS, 3, 0, 1 << 3));
    assert_eq!(rs.take(), ["input 0 3 0 0x8"], "the rest goes to the view");
    unsafe { screen_capture(&mut *r.s, Some(capture_all), 0x5150 as *mut c_void) };
    assert!(r.input(SCREEN_PRESS, 8, 0, 1 << 8));
    assert_eq!(
        GLOBAL.lock().unwrap().as_slice(),
        [8],
        "HOME reached the global handler past the capture"
    );
    assert!(rs.take().is_empty());
}

#[test]
fn a_capture_is_released_when_the_stack_falls_below_its_depth() {
    let (root, a, b) = (probe_view(c"ROOT"), probe_view(c"A"), probe_view(c"B"));
    let (mut rs, mut as_, mut bs) = (Probe::new(b'R'), Probe::new(b'A'), Probe::new(b'B'));
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_CUT);
    // A captures (MIDI learn, say), then opens B over itself.
    unsafe { screen_capture(&mut *r.s, Some(capture_all), 0x5150 as *mut c_void) };
    r.push(&b, &mut bs, SCREEN_CUT);
    r.pop(SCREEN_CUT);
    assert!(r.s.capture.is_some(), "released when a view above it went");
    assert!(r.input(SCREEN_PRESS, 2, 0, 0));
    assert!(as_.take().iter().all(|l| !l.starts_with("input")));
    // A goes: so does its capture, and the events reach the root again.
    r.pop(SCREEN_CUT);
    assert!(r.s.capture.is_none(), "the capture outlived its view");
    assert!(r.s.capture_ctx.is_null());
    rs.take();
    assert!(!r.input(SCREEN_PRESS, 2, 0, 0));
    assert_eq!(rs.take(), ["input 0 2 0 0x0"]);
    // A capture set on the root stays: the root is never popped.
    unsafe { screen_capture(&mut *r.s, Some(capture_all), 0x5150 as *mut c_void) };
    r.push(&a, &mut as_, SCREEN_CUT);
    r.pop(SCREEN_CUT);
    assert!(r.s.capture.is_some());
}

static REENTRY: std::sync::Mutex<Vec<i32>> = std::sync::Mutex::new(Vec::new());

/// enter and exit that try to push and pop.
unsafe extern "C" fn reenter(s: *mut screen, state: *mut c_void) {
    let v = unsafe { screen_top(&*s) };
    let pushed = unsafe { screen_push(s, v, state, SCREEN_CUT) };
    let popped = unsafe { screen_pop(s, SCREEN_CUT) };
    REENTRY.lock().unwrap().extend([pushed, popped]);
}

#[test]
fn enter_and_exit_cannot_push_or_pop() {
    let root = probe_view(c"ROOT");
    let a = Box::new(view {
        enter: Some(reenter),
        exit: Some(reenter),
        ..*probe_view(c"A")
    });
    let mut rs = Probe::new(b'R');
    let mut as_ = Probe::new(b'A');
    let mut r = Rig::new(&root, &mut rs);
    REENTRY.lock().unwrap().clear();
    assert_eq!(r.push(&a, &mut as_, SCREEN_CUT), 0);
    assert_eq!(r.s.depth, 2, "enter changed the stack");
    assert_eq!(r.pop(SCREEN_CUT), 0);
    assert_eq!(r.s.depth, 1, "exit changed the stack");
    assert_eq!(REENTRY.lock().unwrap().as_slice(), [-1, -1, -1, -1]);
    // And the screen takes pushes again afterwards.
    assert_eq!(r.push(&a, &mut as_, SCREEN_CUT), 0);
}

#[test]
fn a_view_composes_only_when_it_changed_or_the_screen_needs_it() {
    let (root, a) = (quiet_view(c"ROOT"), quiet_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    let mut r = Rig::new(&root, &mut rs);
    assert_eq!(rs.take(), ["enter", "compose"], "the first frame composes");
    for _ in 0..3 {
        assert_eq!(r.frame().cells, 0);
    }
    assert!(rs.take().is_empty(), "an idle view composed");
    rs.dirty = true;
    r.frame();
    assert_eq!(rs.take(), ["compose"]);
    r.frame();
    assert!(rs.take().is_empty());
    // A toast: composed while it shows and on the frame it goes, so the view's row is back.
    unsafe { screen_toast(&mut *r.s, c"HI".as_ptr(), 80) };
    for _ in 0..5 {
        r.frame();
    }
    let row = SCREEN_TOAST_ROW as usize;
    assert_eq!(r.row_text(row, SCREEN_TOAST_COL as usize, 2), "RR");
    let composes = rs.take().len();
    assert!(
        (3..=4).contains(&composes),
        "{composes} composes for a toast"
    );
    r.frame();
    assert!(rs.take().is_empty());
    // A raster: composed through it and once after, and the border is the view's again.
    unsafe { screen_raster(&mut *r.s, 2) };
    for _ in 0..2 * SCREEN_RASTER_FRAMES + 2 {
        r.frame();
    }
    assert_eq!(rs.take().len(), 2 * SCREEN_RASTER_FRAMES as usize + 1);
    assert!(r
        .shown
        .iter()
        .enumerate()
        .filter(|&(i, _)| on_border(i))
        .all(|(_, &(c, a))| (c, a) == (FONT_FULL as u8, border_attr())));
    // A switch composes the new view; screen_redraw the same one.
    r.push(&a, &mut as_, SCREEN_CUT);
    r.frame();
    r.frame();
    assert_eq!(as_.take(), ["enter", "compose"]);
    r.pop(SCREEN_CUT);
    r.frame();
    assert_eq!(rs.take(), ["compose"]);
    unsafe { screen_redraw(&mut *r.s) };
    r.frame();
    r.frame();
    assert_eq!(rs.take(), ["compose"]);
    assert_eq!(r.rows_showing(b'R').len(), ROWS - 2);
}

#[test]
fn a_wipe_composes_every_frame_until_it_ends() {
    let (root, a) = (quiet_view(c"ROOT"), quiet_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_WIPE_DOWN);
    let frames = wipe_frames(&mut r, b'A').len();
    assert_eq!(as_.take().len(), 1 + frames, "enter and a compose a frame");
    assert_eq!(r.rows_showing(b'A').len(), ROWS - 2);
    assert_eq!(r.frame().cells, 0);
    assert!(as_.take().is_empty());
}

/// What `p`'s compose draws, cell by cell.
fn composed(p: &Probe) -> Vec<(u8, u8)> {
    (0..CELLS)
        .map(|i| {
            if on_border(i) {
                (FONT_FULL as u8, border_attr())
            } else {
                (p.ch, p.fill)
            }
        })
        .collect()
}

/// A transition's frames until nothing runs: each inside the SPI budget and sending at most
/// the wipe's rows, its edge and `extra` rows more. Then the panel shows `p`'s compose with
/// nothing held.
fn finish(r: &mut Rig, p: &Probe, extra: usize) {
    for f in 0..40 {
        let sent = r.frame();
        assert!(
            sent.spi_ms() <= SPI_BUDGET_MS,
            "frame {f}: {:.1} ms of SPI ({sent:?})",
            sent.spi_ms()
        );
        assert!(
            sent.rows <= SCREEN_WIPE_ROWS as usize + 1 + extra,
            "frame {f}: {sent:?}"
        );
        if !r.animating() && r.s.toast_on == 0 {
            break;
        }
    }
    assert!(!r.animating(), "the transition did not end");
    r.frame();
    assert_eq!(r.g.hold, 0, "rows left held");
    assert_eq!(
        r.shown,
        composed(p),
        "the panel does not show the last view"
    );
    assert_eq!(r.frame().cells, 0, "work left behind");
}

/// Three frames of a transition, inside the budgets.
fn frames3(r: &mut Rig) {
    for _ in 0..3 {
        let sent = r.frame();
        assert!(sent.spi_ms() <= SPI_BUDGET_MS, "{sent:?}");
        assert!(sent.rows <= SCREEN_WIPE_ROWS as usize + 1, "{sent:?}");
    }
}

#[test]
fn a_wipe_up_interrupting_a_wipe_down_ends_on_the_view_below() {
    let (root, a) = (quiet_view(c"ROOT"), quiet_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_WIPE_DOWN);
    frames3(&mut r);
    assert!(r.animating());
    r.pop(SCREEN_WIPE_UP);
    finish(&mut r, &rs, 0);
}

#[test]
fn a_push_inside_a_wipe_ends_on_the_new_view() {
    let (root, a, b) = (quiet_view(c"ROOT"), quiet_view(c"A"), quiet_view(c"B"));
    let (mut rs, mut as_, mut bs) = (Probe::new(b'R'), Probe::new(b'A'), Probe::new(b'B'));
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_WIPE_DOWN);
    frames3(&mut r);
    r.push(&b, &mut bs, SCREEN_WIPE_DOWN);
    finish(&mut r, &bs, 0);
    // And back to back the other way: a pop and at once another pop.
    r.pop(SCREEN_WIPE_UP);
    r.frame();
    r.pop(SCREEN_WIPE_UP);
    finish(&mut r, &rs, 0);
}

#[test]
fn a_toast_during_a_wipe_shows_once_its_row_is_revealed() {
    let (root, a) = (quiet_view(c"ROOT"), quiet_view(c"A"));
    let (mut rs, mut as_) = (Probe::new(b'R'), Probe::new(b'A'));
    let mut r = Rig::new(&root, &mut rs);
    r.push(&a, &mut as_, SCREEN_WIPE_DOWN);
    r.frame();
    unsafe { screen_toast(&mut *r.s, c"OCTAVE +1".as_ptr(), 100_000) };
    let row = SCREEN_TOAST_ROW as usize;
    let col = SCREEN_TOAST_COL as usize;
    for _ in 0..40 {
        let sent = r.frame();
        assert!(sent.spi_ms() <= SPI_BUDGET_MS, "{sent:?}");
        assert!(sent.rows <= SCREEN_WIPE_ROWS as usize + 1, "{sent:?}");
        if r.g.hold & (1 << row) != 0 {
            assert_eq!(r.row_text(row, col, 2), "RR", "a held row was sent");
        }
        if !r.animating() {
            break;
        }
    }
    assert_eq!(r.row_text(row, col, 9), "OCTAVE +1");
    // Then it goes, and the panel shows A alone.
    unsafe { screen_toast(&mut *r.s, c"".as_ptr(), 0) };
    finish(&mut r, &as_, 0);
}

/// What the view on top draws over the view under it (screen_cover) and the toast's text while
/// it shows are covered: a view holding to a budget leaves them alone and does not count them
/// (screen_pending); the cover's cells count with the layers' (screen_layer_cells) for the
/// frame it was set in.
#[test]
fn the_cover_and_a_showing_toast_are_left_out_of_a_views_budget() {
    let root = probe_view(c"ROOT");
    let mut rs = Probe::new(b'R');
    let mut r = Rig::new(&root, &mut rs);
    let covered = |r: &Rig, c: u32, row: u32| unsafe { screen_covered(&*r.s, c, row) } != 0;
    assert!(!covered(&r, 5, 20));
    unsafe { screen_cover(&mut *r.s, 2, 20, 4, 3, 7) };
    for (c, row, want) in [(2, 20, true), (5, 22, true), (6, 20, false), (2, 23, false)] {
        assert_eq!(covered(&r, c, row), want, "({c}, {row})");
    }
    let layers = unsafe { screen_layer_cells(&*r.s) };
    assert_eq!(layers, 7, "the cover's cells count with the layers'");
    // What the view drew under the cover is not pending.
    unsafe {
        grid_put(&mut *r.g, 3, 21, b'X', 0x12);
        grid_put(&mut *r.g, 9, 21, b'X', 0x12);
    }
    assert_eq!(unsafe { grid_pending(&*r.g) }, 2);
    assert_eq!(unsafe { screen_pending(&*r.s) }, 1);
    // A frame clears the cover.
    r.frame();
    assert!(!covered(&r, 2, 20), "the cover outlived its frame");
    assert_eq!(unsafe { screen_layer_cells(&*r.s) }, 0);
    // The toast's text, from its first frame to its last, and not past its text.
    let row = SCREEN_TOAST_ROW;
    let col = SCREEN_TOAST_COL;
    unsafe { screen_toast(&mut *r.s, c"HELLO".as_ptr(), 120) };
    // Large text: 5 characters of 2 x 2 cells, on the toast's row and the one below.
    assert!(covered(&r, col, row) && covered(&r, col + 9, row + 1));
    assert!(!covered(&r, col + 10, row) && !covered(&r, col - 1, row));
    assert!(!covered(&r, col, row - 1) && !covered(&r, col, row + 2));
    r.frame(); // its first frame: it shows until 120 ms from now
    let first = r.now;
    r.s.now = first + 80;
    assert!(covered(&r, col, row), "the toast's last frame");
    r.s.now = first + 120;
    assert!(
        !covered(&r, col, row),
        "the frame it ends in: the view draws there again"
    );
    // A toast of no time never shows.
    unsafe { screen_toast(&mut *r.s, c"HELLO".as_ptr(), 0) };
    assert!(!covered(&r, col, row));
}
