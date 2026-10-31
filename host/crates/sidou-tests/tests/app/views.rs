// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::CStr;

use sidou_sys::*;

use crate::{cutoff_after, drain, piece_part, printable, Rig, Sent};

const COLS: usize = GRID_COLS as usize;
pub(crate) const CARDS_OFF: u8 = 0;
pub(crate) const CARDS_ONCE: u8 = 1;
pub(crate) const CARDS_ALWAYS: u8 = 2;
pub(crate) const FRAME_MS: u32 = 40;
const SETTLE_FRAMES_MAX: usize = 40;
const POPUP_CLOSE_FRAMES_MAX: usize = 80;
const MENU_FIRST_ROW: usize = 4;
const MENU_LINE_ROWS: usize = 3;
const MENU_LINE_CHARS: usize = 14;

pub(crate) fn ascii_or_hash(text: String) -> String {
    text.chars()
        .map(|c| if c.is_ascii() { c } else { '#' })
        .collect()
}

fn open_song(ui: &mut Ui) {
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
}

fn song_frames(ui: &mut Ui) {
    drain(&mut ui.g);
    ui.frames_sent(40);
}

fn song_attr(ui: &Ui, row: usize, col: usize, width: usize, fg: u8, bg: u8) {
    let expected = (bg << 4) | fg;
    for c in col..col + width {
        assert_eq!(
            ui.g.attr[row * COLS + c],
            expected,
            "theme {}, row {row}, column {c}",
            ui.live().theme
        );
    }
}

#[test]
fn song_root_rebuilds_its_theme_and_moves_selection_without_reopening() {
    let mut ui = Ui::without_cards();
    open_song(&mut ui);
    song_frames(&mut ui);
    for id in [THEME_HIGH_CONTRAST, THEME_C64, THEME_HIGH_CONTRAST] {
        unsafe { live_set_theme(&mut *ui.rig.live, id) };
        song_frames(&mut ui);
        let th = unsafe { &*theme_get(id) };
        assert_eq!(ui.top(), "SONG");
        assert_eq!(ui.big(2, 2, 2, 4), "SONG");
        for row in [2, 3] {
            song_attr(&ui, row, 2, 8, th.title_fg, th.title_bg);
        }
        for row in [6, 7] {
            song_attr(&ui, row, 2, 8, th.sel_fg, th.sel_bg);
        }
        song_attr(&ui, 10, 2, 6, th.menu_item, th.bg);
        song_attr(&ui, 24, 2, 16, th.hint, th.bg);
        song_attr(&ui, 26, 2, 18, th.hint, th.bg);
        ui.turn(PANEL_ENC_SELECT, 1);
        song_frames(&mut ui);
        song_attr(&ui, 6, 2, 8, th.menu_item, th.bg);
        for row in [10, 11] {
            song_attr(&ui, row, 2, 6, th.sel_fg, th.sel_bg);
        }
        ui.turn(PANEL_ENC_SELECT, -1);
        song_frames(&mut ui);
    }
}

#[test]
fn song_directory_selection_and_hints_follow_theme_changes_after_scanning() {
    let mut ui = Ui::without_cards();
    let mut flash = crate::store::fresh(809, STORE_WINDOW_MAX);
    for name in ["FIRST", "SECOND"] {
        assert_eq!(flash.file_now(name, &[0]), STORE_OK);
    }
    let mut io = Box::new(song_io {
        st: &mut *flash.st,
        ..Default::default()
    });
    unsafe { views_song_io(&mut *ui.v, &mut *io) };
    open_song(&mut ui);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    song_frames(&mut ui);
    for id in [THEME_HIGH_CONTRAST, THEME_C64] {
        unsafe { live_set_theme(&mut *ui.rig.live, id) };
        song_frames(&mut ui);
        let th = unsafe { &*theme_get(id) };
        assert!(ui.row(6).contains("FIRST"));
        assert!(ui.row(8).contains("SECOND"));
        song_attr(&ui, 2, 2, 8, th.title_fg, th.title_bg);
        song_attr(&ui, 6, 6, 5, th.sel_fg, th.sel_bg);
        song_attr(&ui, 8, 6, 6, th.menu_item, th.bg);
        let save_choose_hint_row = 24;
        song_attr(&ui, save_choose_hint_row, 2, 22, th.hint, th.bg);
        ui.turn(PANEL_ENC_SELECT, 1);
        song_frames(&mut ui);
        song_attr(&ui, 6, 6, 5, th.menu_item, th.bg);
        song_attr(&ui, 8, 6, 6, th.sel_fg, th.sel_bg);
        ui.turn(PANEL_ENC_SELECT, -1);
        song_frames(&mut ui);
    }
}

#[test]
fn song_name_editor_and_confirmation_keep_distinct_roles_when_the_theme_changes() {
    let mut ui = Ui::without_cards();
    let mut io = Box::new(song_io {
        dirty: 1,
        ..Default::default()
    });
    unsafe { views_song_io(&mut *ui.v, &mut *io) };
    open_song(&mut ui);
    ui.turn(PANEL_ENC_SELECT, 2);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    song_frames(&mut ui);
    for id in [THEME_HIGH_CONTRAST, THEME_C64] {
        unsafe { live_set_theme(&mut *ui.rig.live, id) };
        song_frames(&mut ui);
        let th = unsafe { &*theme_get(id) };
        assert_eq!(ui.big(2, 2, 2, 7), "SAVE AS");
        song_attr(&ui, 2, 2, 14, th.title_fg, th.title_bg);
        song_attr(&ui, 8, 2, 16, th.value, th.bg);
        song_attr(&ui, 10, 2, 2, th.hot, th.bg);
        song_attr(&ui, 18, 2, 26, th.hint, th.bg);
        ui.turn(PANEL_ENC_SELECT, STORE_NAME_MAX as i32);
        song_frames(&mut ui);
        song_attr(&ui, 16, 2, 8, th.sel_fg, th.sel_bg);
        ui.turn(PANEL_ENC_SELECT, -(STORE_NAME_MAX as i32));
        song_frames(&mut ui);
    }
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, -1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    song_frames(&mut ui);
    for id in [THEME_HIGH_CONTRAST, THEME_C64] {
        unsafe { live_set_theme(&mut *ui.rig.live, id) };
        song_frames(&mut ui);
        let th = unsafe { &*theme_get(id) };
        assert!(ui.row(6).contains("DISCARD"));
        song_attr(&ui, 6, 2, 14, th.alert, th.bg);
        song_attr(&ui, 8, 2, 26, th.alert, th.bg);
        song_attr(&ui, 14, 2, 4, th.sel_fg, th.sel_bg);
        song_attr(&ui, 14, 16, 6, th.menu_item, th.bg);
        ui.turn(PANEL_ENC_SELECT, 1);
        song_frames(&mut ui);
        song_attr(&ui, 14, 2, 4, th.menu_item, th.bg);
        song_attr(&ui, 14, 16, 6, th.sel_fg, th.sel_bg);
        ui.turn(PANEL_ENC_SELECT, -1);
        song_frames(&mut ui);
    }
}

#[test]
fn song_menu_has_load_new_save_as_delete_and_home_cancels_a_page() {
    let mut ui = Ui::without_cards();
    open_song(&mut ui);
    song_frames(&mut ui);
    assert_eq!(ui.top(), "SONG");
    let text = (0..GRID_ROWS as usize)
        .map(|r| ui.row(r))
        .collect::<String>();
    for action in ["LOAD", "NEW", "SAVE AS", "DELETE"] {
        assert!(text.contains(action), "missing {action}: {text}");
    }
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    song_frames(&mut ui);
    assert!(ui.row(2).contains("LOAD"));
    assert!((0..GRID_ROWS as usize).any(|r| ui.row(r).contains("NO SONGS")));
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "SONG", "HOME should cancel LOAD, not leave SONG");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
}

#[test]
fn song_pages_use_large_labels_and_reverse_selection_without_losing_changed_glyphs() {
    let mut ui = Ui::without_cards();
    open_song(&mut ui);
    song_frames(&mut ui);
    assert_eq!(ui.row(2), ui.row(3), "the SONG heading is two rows tall");
    assert_eq!(ui.g.src[2 * COLS + 2], b'S');
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    let reverse = (th.sel_bg << 4) | th.sel_fg;
    assert_eq!(ui.g.attr[6 * COLS + 2], reverse);
    ui.turn(PANEL_ENC_SELECT, 2);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    song_frames(&mut ui);
    let before = ui.row(8);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    let mut changed = 0;
    for _ in 0..12 {
        ui.frame();
        let sent = drain(&mut ui.g);
        changed += sent.cells;
        assert!(sent.cells <= SONG_VIEW_FRAME_CELLS as usize, "{sent:?}");
        assert!(sent.spi_ms() <= crate::SPI_BUDGET_MS, "{sent:?}");
    }
    assert_ne!(
        ui.row(8),
        before,
        "a new glyph must replace the same four piece codes"
    );
    assert!(changed > 0, "the changed glyph must be sent to the LCD");
    assert_eq!(ui.row(8), ui.row(9));
    ui.turn(PANEL_ENC_SELECT, 8);
    song_frames(&mut ui);
    assert!(
        ui.row(14).contains('^'),
        "the cursor follows the second half of the name"
    );
}

#[test]
fn save_in_an_unnamed_song_opens_the_name_editor_not_settings() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_SEQ);
    ui.press(SAVE_KEY_BUTTON);
    assert_eq!(ui.top(), "SONG");
    song_frames(&mut ui);
    assert!(ui.row(2).contains("SAVE AS"));
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.turn(PANEL_ENC_SELECT, 15);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    song_frames(&mut ui);
    let name = format!("{}{}", &ui.row(8)[1..9], &ui.row(12)[1..9]);
    assert_eq!(name.len(), 16);
    assert_eq!(&name[15..], "!");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(
        ui.top(),
        "STEP",
        "SAVE AS cancellation returns to the song view"
    );
}

#[test]
fn new_requires_confirmation_and_home_keeps_the_unsaved_song() {
    let mut ui = Ui::without_cards();
    let mut io = Box::new(song_io {
        dirty: 1,
        ..Default::default()
    });
    unsafe { views_song_io(&mut *ui.v, &mut *io) };
    open_song(&mut ui);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    assert!(ui.row(6).contains("DISCARD"));
    assert!(ui.row(8).contains("UNSAVED SONG?"));
    ui.turn(PANEL_ENC_ALGORITHM, 4);
    assert_eq!(io.state, SONG_IO_IDLE as u8, "NO is selected initially");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "SONG");
    assert_eq!(io.dirty, 1);
    assert_eq!(io.state, SONG_IO_IDLE as u8);
}

#[test]
fn song_header_shows_saving_and_returns_to_tracker_after_save() {
    let mut ui = Ui::without_cards();
    let mut io = Box::<song_io>::default();
    unsafe { views_song_io(&mut *ui.v, &mut *io) };
    ui.press(PANEL_BTN_SEQ);
    song_frames(&mut ui);
    assert_eq!(ui.big(0, 5, 2, 6), "PAT --");
    io.state = SONG_IO_WRITE as u8;
    song_frames(&mut ui);
    assert_eq!(ui.big(0, 0, 2, 2), "V1");
    assert_eq!(ui.big(0, 5, 2, 6), "SAVING");
    io.state = SONG_IO_IDLE as u8;
    song_frames(&mut ui);
    assert_eq!(ui.big(0, 0, 2, 2), "V1");
    assert_eq!(ui.big(0, 5, 2, 6), "PAT --");
}

#[test]
fn song_pages_share_the_canvas_without_leaking_it_into_tracker() {
    let mut ui = Ui::without_cards();
    drain(&mut ui.g);
    ui.press(PANEL_BTN_SEQ);
    ui.frames_sent(30);
    ui.press(SAVE_KEY_BUTTON);
    for _ in 0..30 {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(sent.cells <= SONG_VIEW_FRAME_CELLS as usize, "{sent:?}");
        assert!(sent.spi_ms() <= crate::SPI_BUDGET_MS, "{sent:?}");
    }
    ui.turn(PANEL_ENC_SELECT, 16);
    for _ in 0..10 {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(sent.cells <= SONG_VIEW_FRAME_CELLS as usize, "{sent:?}");
    }
    ui.press(PANEL_BTN_HOME);
    ui.frames_sent(30);
    assert_eq!(ui.big(0, 0, 2, 2), "V1");
    assert_eq!(ui.big(0, 5, 2, 6), "PAT --");
    assert!(!(0..GRID_ROWS as usize).any(|r| ui.row(r).contains("WRITE SONG")));
}

#[test]
fn unnamed_save_works_when_the_song_view_already_fills_the_stack() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.top(), "CHIP");
    ui.press(PANEL_BTN_SEQ);
    assert_eq!(ui.v.s.depth, SCREEN_DEPTH as u8);
    ui.press(SAVE_KEY_BUTTON);
    assert_eq!(ui.top(), "SONG");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "STEP");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "CHIP");
}

#[test]
fn load_directory_pages_and_delete_waits_for_yes() {
    let mut ui = Ui::without_cards();
    let mut flash = crate::store::fresh(808, STORE_WINDOW_MAX);
    for i in (0..10).rev() {
        assert_eq!(flash.file_now(&format!("SONG {i:02}"), &[0]), STORE_OK);
    }
    let mut io = Box::new(song_io {
        st: &mut *flash.st,
        ..Default::default()
    });
    unsafe { views_song_io(&mut *ui.v, &mut *io) };
    open_song(&mut ui);
    drain(&mut ui.g);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    for _ in 0..60 {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(sent.cells <= SONG_VIEW_FRAME_CELLS as usize, "{sent:?}");
    }
    assert!(ui.row(6).contains("SONG 00"));
    assert!(ui.row(20).contains("SONG 07"));
    ui.turn(PANEL_ENC_SELECT, 8);
    ui.frames_sent(60);
    assert!(ui.row(6).contains("SONG 08"));
    assert!(ui.row(8).contains("SONG 09"));
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, 3);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.frames_sent(60);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(io.state, SONG_IO_IDLE as u8);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(io.state, SONG_IO_IDLE as u8, "default NO must not delete");
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(io.state, SONG_IO_DELETE as u8);
    assert_eq!(unsafe { CStr::from_ptr(io.target.as_ptr()) }, c"SONG 01");
}

#[test]
fn the_load_list_names_its_files_at_2x_the_selected_in_a_reverse_bar_a_long_one_ending_in_an_arrow()
{
    let mut ui = Ui::without_cards();
    let mut flash = crate::store::fresh(809, STORE_WINDOW_MAX);
    for name in ["BEAT", "SIXTEEN CHARS OK"] {
        assert_eq!(flash.file_now(name, &[0]), STORE_OK);
    }
    let mut io = Box::new(song_io {
        st: &mut *flash.st,
        dirty: 1,
        ..Default::default()
    });
    unsafe { views_song_io(&mut *ui.v, &mut *io) };
    open_song(&mut ui);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    song_frames(&mut ui);
    assert_eq!(ui.big(6, 2, 2, 14), "BEAT          ");
    assert_eq!(ui.row(6), ui.row(7), "a name is two rows tall");
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    song_attr(&ui, 6, 2, 28, th.sel_fg, th.sel_bg);
    song_attr(&ui, 8, 2, 28, th.menu_item, th.bg);
    let long = ui.big(8, 2, 2, 14);
    assert_eq!(&long[..13], "SIXTEEN CHARS");
    assert_eq!(
        u32::from(ui.g.src[8 * COLS + 28]),
        FONT_ARROW_RIGHT,
        "{long:?}"
    );
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.press(SAVE_KEY_BUTTON);
    song_frames(&mut ui);
    assert!(ui.row(6).contains("DISCARD"));
    assert_eq!(&ui.big(10, 2, 2, 13), "SIXTEEN CHARS");
}

unsafe extern "C" fn builtin_none(_: *mut std::ffi::c_void, _: u32, _: *mut u8, _: u32) -> u32 {
    0
}

#[test]
fn load_lists_the_builtin_song_first_delete_does_not_and_save_as_offers_its_name() {
    let mut ui = Ui::without_cards();
    let mut flash = crate::store::fresh(810, STORE_WINDOW_MAX);
    for i in (0..8).rev() {
        assert_eq!(flash.file_now(&format!("SONG {i:02}"), &[0]), STORE_OK);
    }
    let mut io = Box::new(song_io {
        st: &mut *flash.st,
        ..Default::default()
    });
    io.hooks.builtin = Some(builtin_none);
    unsafe { views_song_io(&mut *ui.v, &mut *io) };
    open_song(&mut ui);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    song_frames(&mut ui);
    let first = ui.big(6, 2, 2, 12);
    assert_eq!(&first[..11], "SIDOU LOOP ", "{first:?}");
    assert_eq!(u32::from(ui.g.src[6 * COLS + 24]), FONT_MIDDOT, "the mark");
    assert!(ui.row(8).contains("SONG 00"));
    assert!(ui.row(20).contains("SONG 06"));
    ui.turn(PANEL_ENC_SELECT, 8);
    song_frames(&mut ui);
    assert!(
        ui.row(6).contains("SONG 07"),
        "the next page: {}",
        ui.row(6)
    );
    ui.turn(PANEL_ENC_SELECT, -1);
    song_frames(&mut ui);
    assert!(ui.row(6).contains("SIDOU LOOP"));
    ui.turn(PANEL_ENC_SELECT, -8);
    ui.press(SAVE_KEY_BUTTON);
    assert_eq!(io.state, SONG_IO_STOP as u8, "the built-in loads");
    assert_eq!(io.target[0], 0, "no file");
    assert_eq!(ui.top(), "SONG");
    ui.turn(PANEL_ENC_SELECT, 3);
    io.state = SONG_IO_IDLE as u8;
    ui.press(SAVE_KEY_BUTTON);
    song_frames(&mut ui);
    assert!(ui.row(2).contains("DELETE"));
    assert!(ui.row(6).contains("SONG 00"), "{}", ui.row(6));
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, -1);
    ui.press(SAVE_KEY_BUTTON);
    song_frames(&mut ui);
    assert_eq!(
        format!("{}{}", ui.big(8, 2, 2, 8), ui.big(12, 2, 2, 8)),
        "SIDOU LOOP      "
    );
}

#[test]
fn model_revision_alone_requires_new_confirmation() {
    let mut ui = Ui::without_cards();
    let mut model = Box::new(gt2_song {
        revision: 1,
        ..Default::default()
    });
    let mut ctl = Box::new(song_ctl {
        song: &mut *model,
        ..Default::default()
    });
    let mut io = Box::new(song_io {
        ctl: &mut *ctl,
        ..Default::default()
    });
    unsafe { views_song_io(&mut *ui.v, &mut *io) };
    open_song(&mut ui);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    assert!(ui.row(6).contains("DISCARD"));
    assert!(ui.row(8).contains("UNSAVED SONG?"));
    assert_eq!(io.state, SONG_IO_IDLE as u8);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(io.state, SONG_IO_STOP as u8);
}

#[test]
fn song_page_home_still_cancels_after_midi_learn_closes() {
    let mut ui = Ui::without_cards();
    let _ml = ui.attach_learn();
    open_song(&mut ui);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.learn_on(PANEL_ENC_K1);
    assert_eq!(ui.top(), "LEARN");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "SONG");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "SONG", "HOME should return from LOAD to SONG");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
}

#[test]
fn save_routes_named_songs_to_io_and_save_as_accepts_all_sixteen_characters() {
    let mut ui = Ui::without_cards();
    let mut model = Box::<gt2_song>::default();
    let mut ctl = Box::new(song_ctl {
        song: &mut *model,
        ..Default::default()
    });
    let mut st = Box::<store>::default();
    let mut io = Box::new(song_io {
        ctl: &mut *ctl,
        st: &mut *st,
        ..Default::default()
    });
    for (out, byte) in io.name.iter_mut().zip(b"EXISTING") {
        *out = *byte as _;
    }
    unsafe { views_song_io(&mut *ui.v, &mut *io) };
    ui.press(PANEL_BTN_SEQ);
    ui.press(SAVE_KEY_BUTTON);
    assert_eq!(ui.top(), "STEP");
    assert_eq!(io.state, SONG_IO_SAVE_WAIT as u8);
    assert_eq!(unsafe { CStr::from_ptr(io.target.as_ptr()) }, c"EXISTING");
    assert_eq!(model.frozen, 1);
    io.state = SONG_IO_IDLE as u8;
    model.frozen = 0;
    io.name[0] = 0;
    ui.press(SAVE_KEY_BUTTON);
    assert_eq!(ui.top(), "SONG");
    let original = b"NEW SONG        ";
    for (i, (was, want)) in original.iter().zip(b"ABCDEFGHIJKLMNOP").enumerate() {
        if i != 0 {
            ui.turn(PANEL_ENC_SELECT, 1);
        }
        ui.turn(PANEL_ENC_ALGORITHM, i32::from(*want) - i32::from(*was));
    }
    ui.press(SAVE_KEY_BUTTON);
    assert_eq!(ui.top(), "STEP");
    assert_eq!(io.state, SONG_IO_SAVE_WAIT as u8);
    assert_eq!(
        unsafe { CStr::from_ptr(io.target.as_ptr()) },
        c"ABCDEFGHIJKLMNOP"
    );
}

pub(crate) struct Ui {
    pub(crate) rig: Rig,
    pub(crate) p: Box<panel>,
    pub(crate) g: Box<grid>,
    pub(crate) v: Box<views>,
    pub(crate) sc: Box<scope>,
    pub(crate) now: u32,
    pub(crate) held: u32,
}

impl Ui {
    pub(crate) fn new() -> Self {
        let mut rig = Rig::new(SID_MODEL_6581);
        let mut p = Box::<panel>::default();
        let mut g = Box::<grid>::default();
        let mut v = Box::<views>::default();
        let mut sc = Box::<scope>::default();
        unsafe {
            panel_init(&mut *p);
            grid_init(&mut *g, b' ', ((GRID_BLUE << 4) | GRID_LIGHT_BLUE) as u8);
            scope_init(&mut *sc);
            views_init(
                &mut *v,
                &mut *g,
                &mut *rig.live,
                &mut *sc,
                c"V1.2.3".as_ptr(),
            );
        }
        let mut ui = Ui {
            rig,
            p,
            g,
            v,
            sc,
            now: 0,
            held: 0,
        };
        ui.frame();
        ui
    }

    pub(crate) fn without_cards() -> Self {
        let mut ui = Self::new();
        ui.v.cards = CARDS_OFF;
        ui
    }

    pub(crate) fn press(&mut self, b: u32) {
        let held = self.held | (1 << b);
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 1 << b, held) };
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 0, self.held) };
        if b == PANEL_BTN_HOME {
            unsafe { views_flow(&mut *self.v, self.now) };
        }
    }

    pub(crate) fn down(&mut self, b: u32) {
        self.held |= 1 << b;
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 1 << b, self.held) };
    }

    pub(crate) fn up(&mut self, b: u32) {
        self.held &= !(1 << b);
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 0, self.held) };
    }

    pub(crate) fn learn_on(&mut self, enc: u32) {
        self.down(SAVE_KEY_BUTTON);
        self.turn(enc, 1);
        self.up(SAVE_KEY_BUTTON);
    }

    pub(crate) fn hold(&mut self, b: u32, down: bool) {
        if down {
            self.held |= 1 << b;
        } else {
            self.held &= !(1 << b);
        }
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 0, self.held) };
    }

    pub(crate) fn note_keys(&mut self, down: u32, pressed: u32) {
        unsafe { views_note_keys(&mut *self.v, &mut *self.p, down, pressed) };
    }

    pub(crate) fn turn(&mut self, enc: u32, steps: i32) {
        unsafe { views_encoder(&mut *self.v, &mut *self.p, enc, steps) };
    }

    pub(crate) fn frame(&mut self) {
        self.now += FRAME_MS;
        unsafe { views_frame(&mut *self.v, self.now) };
        for _ in 0..GRID_CELLS {
            if unsafe { views_idle(&mut *self.v) } == 0 {
                break;
            }
        }
    }

    pub(crate) fn frames(&mut self, n: u32) {
        for _ in 0..n {
            self.frame();
        }
    }

    pub(crate) fn frame_sent(&mut self) -> Sent {
        self.frame();
        drain(&mut self.g)
    }

    pub(crate) fn frames_sent(&mut self, n: u32) {
        for _ in 0..n {
            self.frame_sent();
        }
    }

    pub(crate) fn attach_learn(&mut self) -> Box<midi_learn> {
        let mut ml = Box::<midi_learn>::default();
        unsafe {
            midi_learn_init(&mut *ml);
            views_learn(&mut *self.v, &mut *ml);
        }
        ml
    }

    pub(crate) fn highlight_menu_item(&mut self, item: u32) {
        self.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
        self.turn(PANEL_ENC_SELECT, item as i32);
    }

    pub(crate) fn choose_menu_item(&mut self, item: u32) {
        self.highlight_menu_item(item);
        self.turn(PANEL_ENC_ALGORITHM, 1);
    }

    pub(crate) fn settle(&mut self) {
        for _ in 0..SETTLE_FRAMES_MAX {
            self.frame();
            if unsafe { screen_animating(&self.v.s) } == 0 {
                self.popup_gone();
                return;
            }
        }
        panic!("the screen kept animating");
    }

    pub(crate) fn popup_gone(&mut self) {
        for _ in 0..POPUP_CLOSE_FRAMES_MAX {
            if unsafe { screen_popup_shows(&self.v.s) } == 0 {
                return;
            }
            self.frame();
        }
        panic!("the popup never closed");
    }

    pub(crate) fn top(&self) -> String {
        let v = unsafe { &*screen_top(&self.v.s) };
        unsafe { CStr::from_ptr(v.name) }
            .to_string_lossy()
            .into_owned()
    }

    pub(crate) fn menu_line(&self, i: usize) -> String {
        let top = usize::from(if self.top() == "SETTINGS" {
            self.v.setup.top
        } else {
            self.v.home.top
        });
        assert!(
            (top..top + VIEWS_MENU_SHOWN as usize).contains(&i),
            "item {i} is out of sight"
        );
        ascii_or_hash(self.big(
            MENU_FIRST_ROW + MENU_LINE_ROWS * (i - top),
            1,
            2,
            MENU_LINE_CHARS,
        ))
    }

    pub(crate) fn row(&self, row: usize) -> String {
        (0..COLS)
            .filter_map(|c| {
                let i = row * COLS + c;
                let piece = usize::from(self.g.ch[i]).wrapping_sub(GRID_BIG_FIRST as usize);
                if piece >= GRID_BIG_PIECES as usize {
                    return Some(printable(self.g.ch[i]));
                }
                let (scale, q) = piece_part(piece);
                (q % scale == 0).then(|| printable(self.g.src[i]))
            })
            .collect()
    }

    pub(crate) fn small(&self, row: usize, col: usize, n: usize) -> String {
        (col..col + n)
            .map(|c| {
                let i = row * COLS + c;
                let piece = usize::from(self.g.ch[i]).wrapping_sub(GRID_BIG_FIRST as usize);
                printable(if piece < GRID_BIG_PIECES as usize {
                    self.g.src[i]
                } else {
                    self.g.ch[i]
                })
            })
            .collect()
    }

    pub(crate) fn big(&self, row: usize, col: usize, scale: usize, n: usize) -> String {
        (0..n)
            .map(|i| {
                let k = row * COLS + col + scale * i;
                let first = if scale == 2 {
                    GRID_BIG_FIRST
                } else {
                    GRID_BIG_FIRST + 4
                };
                let blank_cell_with_stale_source = self.g.ch[k] == b' ';
                if blank_cell_with_stale_source {
                    return ' ';
                }
                assert_eq!(
                    u32::from(self.g.ch[k]),
                    first,
                    "a character's top left piece at ({row}, {})",
                    col + scale * i
                );
                self.g.src[k] as char
            })
            .collect()
    }

    pub(crate) fn toast(&self) -> Option<String> {
        (self.v.s.toast_on != 0).then(|| {
            unsafe { CStr::from_ptr(self.v.s.toast.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        })
    }

    pub(crate) fn live(&self) -> &live {
        &self.rig.live
    }
}

pub(crate) fn open_setting(ui: &mut Ui, item: u32) {
    if ui.top() != "SETTINGS" {
        if ui.top() != "MENU" {
            ui.press(PANEL_BTN_HOME);
            ui.settle();
        }
        assert_eq!(ui.top(), "MENU");
        ui.choose_menu_item(VIEWS_MENU_SETTINGS);
        ui.settle();
        assert_eq!(ui.top(), "SETTINGS");
    }
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_SETTINGS_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, item as i32);
    ui.settle();
    settings_settle(ui);
}

pub(crate) fn settings_settle(ui: &mut Ui) {
    ui.frame();
    ui.popup_gone();
    for n in 0..20 {
        ui.frame();
        drain(&mut ui.g);
        if n >= 8 && ui.v.setup.pending == 0 {
            return;
        }
    }
    panic!("SETTINGS kept lines pending");
}

fn open_from_home(ui: &mut Ui, item: u32) {
    ui.press(PANEL_BTN_HOME);
    ui.choose_menu_item(item);
    ui.settle();
}

fn open_once_the_menu_settles(ui: &mut Ui, item: u32) {
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, item as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
}

pub(crate) fn open_status(ui: &mut Ui) {
    open_from_home(ui, VIEWS_MENU_STATUS);
}

pub(crate) fn open_charset(ui: &mut Ui) {
    open_from_home(ui, VIEWS_MENU_CHARSET);
}

#[test]
fn home_opens_the_menu_and_goes_back_one_view() {
    let mut ui = Ui::without_cards();
    assert_eq!(ui.top(), "LIVE");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "LIVE");
    open_charset(&mut ui);
    assert_eq!(ui.top(), "CHARSET");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "LIVE");
    open_status(&mut ui);
    assert_eq!(ui.top(), "STATUS");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "LIVE");
    assert_eq!(ui.live().octave, 0, "HOME reached the panel");
}

#[test]
fn the_views_open_with_a_wipe_down_and_close_with_a_wipe_up() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.v.s.wipe, SCREEN_WIPE_DOWN as u8);
    ui.settle();
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.v.s.wipe, SCREEN_WIPE_UP as u8);
}

#[test]
fn the_menu_changes_the_preset_and_octave() {
    let mut ui = Ui::new();
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(
        (ui.top().as_str(), ui.live().model),
        ("MENU", SID_MODEL_6581)
    );
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_PRESET as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 2);
    assert_eq!(ui.live().preset, 2);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, -2);
    assert_eq!(ui.live().octave, -2);
    ui.turn(PANEL_ENC_ALGORITHM, -9);
    assert_eq!(ui.live().octave, LIVE_OCTAVE_MIN as i8);
    ui.turn(PANEL_ENC_ALGORITHM, 9);
    assert_eq!(ui.live().octave, LIVE_OCTAVE_MAX as i8);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_ITEMS as i32);
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(ui.top(), "MENU");
}

fn menu_settle(ui: &mut Ui) {
    for _ in 0..12 {
        ui.frame();
        if ui.v.home.pending == 0 {
            return;
        }
    }
    panic!("the menu kept lines pending");
}

#[test]
fn the_menu_draws_six_items_in_large_text_with_the_selected_one_in_reverse_video() {
    let mut ui = Ui::new();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    let heading = |ui: &Ui| ascii_or_hash(ui.big(1, 1, 2, MENU_LINE_CHARS));
    assert_eq!(heading(&ui), format!("MENU   #  1/{VIEWS_MENU_ITEMS}"));
    assert!((0..14).all(|i| ui.g.src[COLS + 1 + 2 * i] == ui.g.src[2 * COLS + 1 + 2 * i]));
    let t = unsafe { &*theme_get(THEME_HIGH_CONTRAST) };
    assert_eq!(
        ui.g.attr[0],
        (t.title_bg << 4) | t.title_fg,
        "the title bar in the theme's"
    );
    let at_top = |ui: &Ui| -> Vec<String> { (0..6).map(|i| ui.menu_line(i)).collect() };
    assert_eq!(
        at_top(&ui),
        [
            format!("{:<13}#", "CHIP"),
            format!("{:<13}#", "TRACKER"),
            format!("{:<13}#", "SCENE"),
            format!("{:<13}#", "SONG"),
            format!("{:<13}#", "LAB"),
            format!("{:<13}#", "SIDS"),
        ]
    );
    assert_eq!(ui.g.src[4 * COLS + 28], FONT_ARROW_RIGHT as u8);
    assert_eq!(ui.g.src[7 * COLS + 28], FONT_ARROW_RIGHT as u8);
    assert_eq!(ui.g.src[COLS + 15], FONT_ARROW_DOWN as u8);
    assert_eq!(ui.g.src[COLS + 13], b' ');
    ui.turn(PANEL_ENC_SELECT, 99);
    menu_settle(&mut ui);
    let end_top = VIEWS_MENU_ITEMS - VIEWS_MENU_SHOWN;
    assert_eq!(u32::from(ui.v.home.top), end_top);
    assert_eq!(
        heading(&ui),
        format!("MENU  #  {VIEWS_MENU_ITEMS}/{VIEWS_MENU_ITEMS}")
    );
    let at_end: Vec<String> = (end_top..VIEWS_MENU_ITEMS)
        .map(|i| ui.menu_line(i as usize))
        .collect();
    assert_eq!(
        at_end,
        [
            format!("{:<6}{:>8}", "OCTAVE", "+0"),
            format!("{:<13}#", "SETTINGS"),
            format!("{:<13}#", "STATUS"),
            format!("{:<13}#", "MIDI"),
            format!("{:<13}#", "CHARSET"),
            format!("{:<13}#", "ABOUT"),
        ]
    );
    assert_eq!(ui.g.src[COLS + 13], FONT_ARROW_UP as u8);
    assert_eq!(ui.g.src[COLS + 15], b' ');
    assert_eq!(ui.big(24, 1, 2, 12), "SELECT  MOVE");
    assert_eq!(ui.big(26, 1, 2, 12), "SAVE    OPEN");
}

#[test]
fn settings_draws_its_seven_items_like_the_menu_and_home_goes_back() {
    let mut ui = Ui::new();
    open_setting(&mut ui, VIEWS_SETTINGS_KEYS);
    let heading = ascii_or_hash(ui.big(1, 1, 2, MENU_LINE_CHARS));
    assert_eq!(heading, format!("SETTINGS #1/{VIEWS_SETTINGS_ITEMS}"));
    assert_eq!(
        (0..6).map(|i| ui.menu_line(i)).collect::<Vec<_>>(),
        [
            format!("{:<4}{:>10}", "KEYS", "AUTO"),
            format!("{:<6}{:>8}", "SOUNDS", "OFF"),
            format!("{:<4}{:>10}", "ROWS", "BAR"),
            format!("{:<5}{:>9}", "CARDS", "ONCE"),
            format!("{:<5}{:>9}", "STORE", "NONE"),
            format!("{:<5}{:>9}", "THEME", "HIGH"),
        ]
    );
    assert_eq!(ui.big(26, 1, 2, 12), "ALG   CHANGE");
    open_setting(&mut ui, VIEWS_SETTINGS_REC);
    assert_eq!(ui.menu_line(6), format!("{:<3}{:>11}", "REC", "NEAREST"));
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    assert_eq!(ui.top(), "MENU");
    assert_eq!(u32::from(ui.v.home.cursor), VIEWS_MENU_SETTINGS);
}

#[test]
fn save_opens_the_menus_item_and_does_nothing_on_a_value() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.press(SAVE_KEY_BUTTON);
    ui.settle();
    assert_eq!(ui.top(), "CHIP", "SAVE on CHIP opens CHIP");
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    assert_eq!(ui.top(), "MENU");
    ui.turn(
        PANEL_ENC_SELECT,
        (VIEWS_MENU_OCTAVE - VIEWS_MENU_CHIP) as i32,
    );
    let octave = ui.live().octave;
    ui.rig.live.settings_dirty = 0;
    ui.press(SAVE_KEY_BUTTON);
    ui.settle();
    assert_eq!(ui.top(), "MENU", "SAVE on a value opens nothing");
    assert_eq!(ui.live().octave, octave, "and changes nothing");
    assert_eq!(ui.toast(), None, "no SAVED: the menu's SAVE is its OK");
    ui.turn(
        PANEL_ENC_SELECT,
        (VIEWS_MENU_SETTINGS - VIEWS_MENU_OCTAVE) as i32,
    );
    ui.press(SAVE_KEY_BUTTON);
    ui.settle();
    assert_eq!(ui.top(), "SETTINGS");
    let cards = ui.v.cards;
    ui.press(SAVE_KEY_BUTTON);
    ui.settle();
    assert_eq!(
        (ui.top(), ui.v.cards),
        ("SETTINGS".to_owned(), cards),
        "nothing here"
    );
}

#[test]
fn every_menu_item_is_reachable_and_readable_as_the_cursor_walks_down_and_up() {
    let mut ui = Ui::new();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    let labels = [
        "CHIP", "TRACKER", "SCENE", "SONG", "LAB", "SIDS", "PRE", "OCTAVE", "SETTINGS", "STATUS",
        "MIDI", "CHARSET", "ABOUT",
    ];
    assert_eq!(labels.len(), VIEWS_MENU_ITEMS as usize);
    let walk = |ui: &mut Ui, i: usize| {
        assert_eq!(usize::from(ui.v.home.cursor), i);
        menu_settle(ui);
        let top = usize::from(ui.v.home.top);
        assert!(
            (top..top + VIEWS_MENU_SHOWN as usize).contains(&i),
            "item {i} is out of the {} rows shown from {top}",
            VIEWS_MENU_SHOWN
        );
        for (k, label) in labels
            .iter()
            .enumerate()
            .skip(top)
            .take(VIEWS_MENU_SHOWN as usize)
        {
            assert!(
                ui.menu_line(k).starts_with(label),
                "item {k}: {}",
                ui.menu_line(k)
            );
        }
        let up_hint = ui.g.src[COLS + 13] == FONT_ARROW_UP as u8;
        let down_hint = ui.g.src[COLS + 15] == FONT_ARROW_DOWN as u8;
        assert_eq!(up_hint, top > 0, "the hint up at item {i}");
        assert_eq!(
            down_hint,
            top + (VIEWS_MENU_SHOWN as usize) < labels.len(),
            "the hint down at item {i}"
        );
    };
    walk(&mut ui, 0);
    for i in 1..labels.len() {
        ui.turn(PANEL_ENC_SELECT, 1);
        walk(&mut ui, i);
    }
    for i in (0..labels.len() - 1).rev() {
        ui.turn(PANEL_ENC_SELECT, -1);
        walk(&mut ui, i);
    }
    assert_eq!(ui.v.home.top, 0);
    let th = unsafe { &*theme_get(THEME_HIGH_CONTRAST) };
    let reverse = (th.sel_bg << 4) | th.sel_fg;
    let normal = (th.bg << 4) | th.menu_item;
    for c in 0..COLS {
        assert_eq!(
            ui.g.attr[4 * COLS + c],
            reverse,
            "item 0, row 4, column {c}"
        );
        assert_eq!(ui.g.attr[5 * COLS + c], reverse);
        assert_eq!(ui.g.attr[7 * COLS + c], normal, "item 1");
    }
}

#[test]
fn select_and_algorithm_do_nothing_outside_the_menu() {
    let mut ui = Ui::without_cards();
    for view in ["LIVE", "STATUS", "CHARSET"] {
        if view == "STATUS" {
            open_status(&mut ui);
        }
        if view == "CHARSET" {
            ui.press(PANEL_BTN_HOME);
            ui.press(PANEL_BTN_HOME);
            ui.settle();
            open_charset(&mut ui);
        }
        ui.rig.clear();
        ui.turn(PANEL_ENC_SELECT, 3);
        ui.turn(PANEL_ENC_ALGORITHM, -2);
        assert!(ui.rig.log().is_empty(), "{view}: the instrument changed");
        assert_eq!(ui.top(), view);
    }
}

fn the_panel_plays(ui: &mut Ui, view: &str) {
    let l0 = (ui.live().octave, ui.live().model, ui.live().cutoff);
    ui.press(PANEL_BTN_OCT_UP);
    assert_eq!(ui.live().octave, l0.0 + 1, "{view}: OCT+");
    ui.press(PANEL_BTN_OCT_DOWN);
    assert_eq!(ui.live().octave, l0.0, "{view}: OCT-");
    let s0 = usize::from(ui.live().fx_select);
    let alg0 = ui.live().slot[s0][LIVE_SLOT_ALGORITHM as usize];
    for b in [PANEL_BTN_FX, PANEL_BTN_ENV] {
        ui.press(b);
        assert_eq!(
            ui.live().model,
            l0.1,
            "{view}: button {b} toggled the model"
        );
        assert_eq!(
            ui.live().slot[s0][LIVE_SLOT_ALGORITHM as usize],
            alg0,
            "{view}: button {b} stepped the algorithm"
        );
        ui.settle();
        if view == "LIVE" {
            assert_eq!(ui.top(), "LAB", "{view}: button {b} opens LAB");
            ui.press(PANEL_BTN_HOME);
            ui.settle();
        }
        assert_eq!(ui.top(), view, "{view}: button {b}");
    }
    ui.turn(PANEL_ENC_K1, -2);
    assert_eq!(
        ui.live().cutoff,
        cutoff_after(ui.live().model, l0.2, -2),
        "{view}: K1"
    );
    ui.turn(PANEL_ENC_K1, 2);
    let before_edit = ui.live().cutoff;
    let sel = ui.live().fx_select;
    ui.press(PANEL_BTN_SCL);
    assert_eq!(
        ui.live().fx_select,
        (sel + 1) % LIVE_SLOTS as u8,
        "{view}: SCL"
    );
    let s = usize::from(ui.live().fx_select);
    ui.hold(PANEL_BTN_EDIT, true);
    for k in 0..4 {
        let field = LIVE_SLOT_AMOUNT as usize + k;
        let before = ui.live().slot[s][field];
        let steps = if before == LIVE_SLOT_MAX as u8 { -1 } else { 1 };
        let w = unsafe {
            live_walk_slot(
                s as u32,
                u32::from(ui.live().slot[s][LIVE_SLOT_ALGORITHM as usize]),
                field as u32,
            )
        };
        ui.turn(PANEL_ENC_K1 + k as u32, steps);
        assert_eq!(
            u32::from(ui.live().slot[s][field]),
            unsafe { live_walk_step(w, u32::from(before), steps) },
            "{view}: EDIT + K{}",
            k + 1
        );
    }
    let cutoff = ui.live().cutoff;
    assert_eq!(
        cutoff, before_edit,
        "{view}: K1 moved the cutoff while EDIT was held"
    );
    ui.hold(PANEL_BTN_EDIT, false);
    ui.turn(PANEL_ENC_K1, -1);
    assert_eq!(ui.live().cutoff, cutoff_after(ui.live().model, cutoff, -1));
    ui.turn(PANEL_ENC_K1, 1);
}

#[test]
fn the_panel_plays_the_instrument_as_before_in_every_view() {
    let mut ui = Ui::new();
    the_panel_plays(&mut ui, "LIVE");
    open_status(&mut ui);
    the_panel_plays(&mut ui, "STATUS");
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    the_panel_plays(&mut ui, "MENU");
    ui.turn(
        PANEL_ENC_SELECT,
        (VIEWS_MENU_CHARSET - VIEWS_MENU_STATUS) as i32,
    );
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    assert_eq!(ui.top(), "CHARSET");
    the_panel_plays(&mut ui, "CHARSET");
}

#[test]
fn a_change_of_octave_slot_or_algorithm_shows_a_toast() {
    let mod_algorithm_cc = LIVE_CC_SLOT_FIRST + 5;
    let mod_chorus = 32;
    let fx_algorithm_cc = LIVE_CC_SLOT_FIRST + 10;
    let mut ui = Ui::new();
    assert_eq!(ui.toast(), None, "a toast at boot");
    ui.press(PANEL_BTN_OCT_UP);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("OCTAVE +1"));
    ui.frames(SCREEN_TOAST_MS.div_ceil(FRAME_MS));
    assert_eq!(ui.toast(), None);
    ui.rig.cc(fx_algorithm_cc, 127);
    ui.press(PANEL_BTN_SCL);
    ui.frame();
    assert_eq!(ui.toast(), None, "LIVE's band says it");
    ui.press(PANEL_BTN_SCL);
    ui.press(PANEL_BTN_SCL);
    ui.rig.cc(fx_algorithm_cc, 0);
    ui.frame();
    open_once_the_menu_settles(&mut ui, VIEWS_MENU_STATUS);
    ui.press(PANEL_BTN_SCL);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("SLOT MOD"));
    ui.rig.cc(mod_algorithm_cc, mod_chorus);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("MOD CHORUS"));
    ui.rig.cc(fx_algorithm_cc, 127);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("FX LUSH"));
    assert_eq!(
        ui.row(SCREEN_TOAST_ROW as usize)[2..9].to_string(),
        "FX LUSH"
    );
    ui.frames(SCREEN_TOAST_MS.div_ceil(FRAME_MS));
    assert_eq!(ui.toast(), None);
}

#[test]
fn a_chip_change_shows_a_toast_and_runs_the_raster() {
    let mut ui = Ui::new();
    ui.rig.button(LIVE_BUTTON_FX);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("CHIP 8580"));
    assert!(unsafe { screen_animating(&ui.v.s) } != 0, "no raster");
    ui.settle();
    let chip_model_cc = 25;
    ui.rig.cc(chip_model_cc, 0);
    ui.rig.tick();
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("CHIP 6581"));
    assert!(ui.v.s.raster_frames > 0);
}

#[test]
fn the_status_view_shows_the_instrument() {
    let mut ui = Ui::new();
    open_status(&mut ui);
    assert_eq!(ui.big(2, 2, 2, 5), "SIDOU", "the title at 2x");
    assert_eq!(ui.big(2, 22, 2, 3), "1/3", "the page");
    assert_eq!(
        ui.small(2, 15, 6),
        "V1.2.3",
        "the version ends before column 21"
    );
    for (i, (name, num)) in [
        ("CUTOFF", ui.live().cutoff.to_string()),
        ("RESO", "6".into()),
        ("PULSE", ui.live().pulse_width.to_string()),
        ("VOLUME", ui.live().volume.to_string()),
    ]
    .into_iter()
    .enumerate()
    {
        let row = 5 + 6 * i;
        assert_eq!(ui.big(row, 2, 2, name.len()), name);
        assert_eq!(ui.big(row, 28 - 2 * num.len(), 2, num.len()), num);
    }
    assert_eq!(&ui.row(28)[2..8], "READY.");
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.settle();
    assert_eq!(ui.big(2, 22, 2, 3), "2/3");
    assert_eq!(ui.big(5, 2, 2, 4), "CHIP");
    assert_eq!(ui.big(8, 2, 2, 12), "6581 CLASSIC");
    assert_eq!(ui.big(11, 2, 2, 6), "PRESET");
    assert_eq!(ui.big(11, 16, 2, 3), "OCT");
    assert_eq!(ui.big(11, 24, 2, 2), "+0");
    assert_eq!(ui.big(14, 2, 2, 8), "SAW LEAD");
    assert_eq!(ui.big(17, 2, 2, 5), "NOTES");
    assert_eq!(&ui.row(28)[2..8], "READY.");
    ui.rig.note_on(60);
    ui.settle();
    assert_eq!(ui.big(20, 2, 2, 2), "C4", "a held note on the notes line");
    ui.rig.note_off(60);
    ui.settle();
    assert_eq!(ui.small(20, 2, 4), "    ", "the note is off");
    ui.turn(PANEL_ENC_SELECT, 1);
    unsafe { live_slot_next_algorithm(&mut *ui.rig.live, 0) };
    ui.settle();
    assert_eq!(ui.big(2, 22, 2, 3), "3/3");
    assert_eq!(ui.big(5, 2, 2, 5), ">DIST");
    assert_eq!(ui.big(5, 14, 2, 3), "MIX");
    assert_eq!(ui.big(8, 4, 2, 4), "TAPE");
    assert_eq!(ui.big(11, 4, 2, 3), "MOD");
    assert_eq!(ui.big(17, 4, 2, 2), "FX");
    assert_eq!(ui.big(23, 12, 2, 3), "AMT");
    assert_eq!(ui.big(26, 2, 2, 1), "A");
    assert_eq!(ui.big(26, 14, 2, 1), "B");
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.settle();
    assert_eq!(ui.big(2, 22, 2, 3), "1/3", "round to the first");
    ui.turn(PANEL_ENC_SELECT, -1);
    ui.settle();
    assert_eq!(ui.big(2, 22, 2, 3), "3/3", "back round to the last");
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    assert_eq!(ui.top(), "STATUS");
    assert_eq!(
        ui.big(2, 22, 2, 3),
        "1/3",
        "a new opening starts on the first page"
    );
}

#[test]
fn the_views_show_both_chips_and_which_the_keys_play() {
    let mut ui = Ui::new();
    ui.rig.live.two_chips = 1;
    ui.rig.live.other_profile = SID_CHIP_8580_R5 as u8;
    ui.frame();
    assert_eq!(&ui.row(0)[..8], "SAW LEAD");
    assert_ne!(&ui.row(21)[24..30], "KEYS:B");
    ui.rig.live.keys_chip = 1;
    ui.frame();
    assert_eq!(
        &ui.row(21)[24..30],
        "KEYS:B",
        "LIVE's bottom band with the keys on chip B"
    );
    open_status(&mut ui);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.settle();
    let chips = |ui: &Ui| [5, 8, 11, 14].map(|r| ui.big(r, 2, 2, if r % 6 == 5 { 6 } else { 12 }));
    let named = |got: [String; 4], want: [&str; 4]| {
        for (g, w) in got.iter().zip(want) {
            assert!(g.starts_with(w), "{g:?} for {w:?}");
        }
    };
    let keys = |ui: &Ui, row: usize| {
        if ui.g.ch[row * COLS + 20] == 32 {
            "    ".to_string()
        } else {
            ui.big(row, 20, 2, 4)
        }
    };
    named(
        chips(&ui),
        ["CHIP A", "8580 CLEAN", "CHIP B", "6581 CLASSIC"],
    );
    assert_eq!(keys(&ui, 11), "KEYS", "the keys' chip is marked");
    assert_eq!(keys(&ui, 5), "    ");
    assert_eq!(ui.big(17, 2, 2, 6), "PRESET");
    assert_eq!(ui.big(20, 2, 2, 8), "SAW LEAD");
    ui.rig.live.keys_chip = 0;
    ui.frame();
    named(
        chips(&ui),
        ["CHIP A", "6581 CLASSIC", "CHIP B", "8580 CLEAN"],
    );
    assert_eq!(keys(&ui, 5), "KEYS");
    ui.rig.live.other_profile = LIVE_PROFILE_NONE as u8;
    ui.frame();
    assert!(chips(&ui)[3].starts_with("--"), "chip B not made yet");
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    assert_eq!(ui.top(), "MENU");
    open_setting(&mut ui, VIEWS_SETTINGS_KEYS);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(u32::from(ui.live().keys_mode), LIVE_KEYS_A);
    ui.settle();
    settings_settle(&mut ui);
    assert_eq!(
        ui.menu_line(VIEWS_SETTINGS_KEYS as usize),
        format!("{:<4}{:>10}", "KEYS", "A"),
        "after the cursor"
    );
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(u32::from(ui.live().keys_mode), LIVE_KEYS_AUTO);
}

const PROBE_CELL: usize = 28 * COLS + 20;

fn mark_probe_cell(ui: &mut Ui) {
    ui.g.ch[PROBE_CELL] = b'@';
}

fn probe_cell_composed(ui: &Ui) -> bool {
    ui.g.ch[PROBE_CELL] != b'@'
}

#[test]
fn an_idle_view_is_not_composed_and_a_change_is() {
    let mut ui = Ui::new();
    open_status(&mut ui);
    mark_probe_cell(&mut ui);
    ui.frames(30);
    assert!(
        !probe_cell_composed(&ui),
        "the idle status screen was composed"
    );
    type Change = fn(&mut Ui);
    let changes: [(&str, Change); 2] = [
        ("cutoff", |ui| ui.turn(PANEL_ENC_K1, -1)),
        ("slot amount", |ui| ui.rig.cc(LIVE_CC_SLOT_FIRST + 1, 99)),
    ];
    for (what, change) in changes {
        ui.settle();
        mark_probe_cell(&mut ui);
        change(&mut ui);
        ui.frame();
        assert!(probe_cell_composed(&ui), "{what}: not composed");
        mark_probe_cell(&mut ui);
        ui.frame();
        assert!(
            !probe_cell_composed(&ui),
            "{what}: composed again with nothing new"
        );
    }
    ui.turn(PANEL_ENC_SELECT, 1);
    let notes = 20 * COLS + 25;
    for (what, change) in [
        ("note on", (|ui| ui.rig.note_on(60)) as Change),
        ("note off", |ui| ui.rig.note_off(60)),
    ] {
        ui.settle();
        mark_probe_cell(&mut ui);
        ui.g.ch[notes] = b'@';
        change(&mut ui);
        ui.frame();
        assert_eq!(ui.g.ch[notes], b' ', "{what}: the notes row was not drawn");
        assert!(
            !probe_cell_composed(&ui),
            "{what}: the whole screen was composed for a note"
        );
    }
    ui.settle();
    assert_eq!(ui.small(20, 2, 6), "      ", "the note is off");
}

#[test]
fn the_menu_composes_when_its_cursor_moves_and_not_while_it_waits() {
    let mut ui = Ui::new();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    drain(&mut ui.g);
    for _ in 0..4 * SCREEN_BLINK_FRAMES {
        mark_probe_cell(&mut ui);
        ui.frame();
        assert!(
            !probe_cell_composed(&ui),
            "the menu composed while it waited"
        );
        assert_eq!(drain(&mut ui.g).windows, 0);
    }
    mark_probe_cell(&mut ui);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.frame();
    assert!(
        probe_cell_composed(&ui),
        "the cursor moved and the menu was not composed"
    );
    ui.choose_menu_item(VIEWS_MENU_CHARSET);
    ui.settle();
    mark_probe_cell(&mut ui);
    ui.frames(20);
    assert!(
        !probe_cell_composed(&ui),
        "the glyph sheet was composed idle"
    );
}

unsafe extern "C" fn capture_all(_ctx: *mut std::ffi::c_void, _e: *const screen_event) -> i32 {
    1
}

#[test]
fn a_capture_takes_home_before_the_views() {
    let mut ui = Ui::new();
    unsafe { screen_capture(&mut ui.v.s, Some(capture_all), std::ptr::null_mut()) };
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "LIVE", "HOME opened the menu past the capture");
    unsafe { screen_capture(&mut ui.v.s, None, std::ptr::null_mut()) };
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
}

#[test]
fn a_press_of_the_top_button_is_routed_without_running_past_it() {
    let mut ui = Ui::new();
    unsafe { views_buttons(&mut *ui.v, &mut *ui.p, 1 << 31, 1 << 31) };
    assert_eq!(ui.top(), "LIVE");
}

fn update_line(ui: &Ui) -> String {
    ui.row(20)[4..27].trim_end().to_string()
}

#[test]
fn the_update_box_shows_each_step_and_the_result() {
    let mut ui = Ui::new();
    open_status(&mut ui);
    for (step, code, text) in [
        (1, 0, "PACKAGE"),
        (2, 0, "CHECK HEAD"),
        (3, 0, "LOADER"),
        (4, 0, "CONFIRM"),
        (5, 0, "RESTART"),
        (9, 1, "DRY RUN OK"),
        (9, -15, "FAILED -15"),
    ] {
        unsafe { views_update(&mut *ui.v, step, code) };
        assert_eq!(update_line(&ui), text, "step {step}, code {code}");
        assert_eq!(&ui.row(18)[2..12], "# UPDATE #");
        assert_eq!(&ui.row(28)[2..8], "      ", "READY. under the box");
    }
    ui.frame();
    assert_eq!(&ui.row(28)[2..8], "READY.");
    assert_eq!(ui.big(5, 2, 2, 6), "CUTOFF");
}

#[test]
fn the_midi_page_uses_six_large_rows_within_190_cells() {
    let mut ui = Ui::without_cards();
    ui.frames_sent(20);
    ui.press(PANEL_BTN_HOME);
    ui.frames_sent(12);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_MIDI as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.v.midi.scale, 2);
    assert_eq!(u32::from(ui.v.midi.rows), VIEWS_MENU_SHOWN);
    for step in 0..10 {
        if step != 0 {
            ui.turn(PANEL_ENC_SELECT, 1);
        }
        for frame in 0..12 {
            ui.frame();
            let sent = drain(&mut ui.g);
            assert!(sent.cells <= 190, "step {step}, frame {frame}: {sent:?}");
            assert!(sent.spi_ms() <= crate::SPI_BUDGET_MS, "{sent:?}");
        }
        let row = 4 + 3 * (usize::from(ui.v.midi.cursor) - usize::from(ui.v.midi.top));
        assert_eq!(u32::from(ui.g.ch[row * COLS + 1]), GRID_BIG_FIRST);
        assert_eq!(ui.v.midi.pending, 0);
    }
    assert!(ui.row(1).contains("MIDI"));
    assert!(midi_line(&mut ui, 9).contains("CLOCK OUT"));
}

#[test]
fn the_midi_page_builds_up_two_lines_a_frame_after_its_title_and_help() {
    let mut ui = Ui::without_cards();
    ui.frames_sent(20);
    ui.press(PANEL_BTN_HOME);
    ui.frames_sent(12);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_MIDI as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    let mut valid = Vec::new();
    for frame in 0..6 {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(sent.cells <= 190, "frame {frame}: {sent:?}");
        valid.push(ui.v.midi.valid.count_ones());
        if frame == 0 {
            assert!(ui.row(1).contains("MIDI"), "{:?}", ui.row(1));
            assert_eq!(ui.big(24, 1, 2, 12), "SELECT  MOVE");
            assert_eq!(ui.v.midi.valid, 0, "no line in the first frame");
        }
        let head_bg = ui.g.attr[COLS] >> 4;
        assert_eq!(
            ui.g.attr[0] >> 4,
            head_bg,
            "frame {frame}: the first cell shows the bar"
        );
        assert_eq!(ui.g.ch[0], b' ');
    }
    assert_eq!(valid, [0, 2, 4, 6, 6, 6]);
    assert_eq!(ui.v.midi.pending, 0);
}

fn midi_line(ui: &mut Ui, item: usize) -> String {
    ui.frame();
    ui.popup_gone();
    ui.frames_sent(8);
    ui.big(4 + 3 * (item - usize::from(ui.v.midi.top)), 1, 2, 14)
}

#[test]
fn the_midi_page_sounds_moves_changes_and_refusals() {
    unsafe extern "C" fn silent(_: *mut std::ffi::c_void, _: u8, _: u8) {}
    let mut ui = Ui::without_cards();
    let mut snd = Box::<ui_sound>::default();
    let mut route = Box::<midi_route>::default();
    unsafe {
        ui_sound_init(
            &mut *snd,
            &mut *ui.rig.live,
            Some(silent),
            std::ptr::null_mut(),
        );
        ui_sound_set(&mut *snd, 1);
        views_set_sounds(&mut *ui.v, &mut *snd);
        midi_route_init(
            &mut *route,
            &mut *ui.rig.live,
            std::ptr::null_mut(),
            std::ptr::null(),
        );
    }
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_MIDI as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.turn(PANEL_ENC_SELECT, -1);
    assert_eq!(u32::from(snd.id), UI_SOUND_BUZZ);
    ui.turn(PANEL_ENC_SELECT, 1);
    assert_eq!(u32::from(snd.id), UI_SOUND_BLIP);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(u32::from(snd.id), UI_SOUND_BUZZ, "no router: no change");
    unsafe { views_route(&mut *ui.v, &mut *route) };
    ui.turn(PANEL_ENC_ALGORITHM, 99);
    assert_eq!(u32::from(snd.id), UI_SOUND_CONFIRM);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(u32::from(snd.id), UI_SOUND_BUZZ, "already at channel 16");
}

#[test]
fn the_midi_page_edits_midis_mode_clamped_and_shows_a_dash_without_a_router() {
    let mut ui = Ui::without_cards();
    let mut route = Box::<midi_route>::default();
    unsafe {
        midi_route_init(
            &mut *route,
            &mut *ui.rig.live,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    open_once_the_menu_settles(&mut ui, VIEWS_MENU_MIDI);
    assert_eq!(ui.top(), "MIDI");
    let row = midi_line(&mut ui, 0);
    assert!(
        row.starts_with("MODE") && row.ends_with(" -"),
        "no router: {row:?}"
    );
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    unsafe { views_route(&mut *ui.v, &mut *route) };
    ui.frame();
    let want = [
        ("MODE", "OMNI"),
        ("CHANNEL", "1"),
        ("OPTION", "POLY"),
        ("LEGATO", "OFF"),
        ("CHORD 2", "+4"),
        ("CHORD 3", "+7"),
        ("ARP RATE", "3 FR"),
        ("ROWS/BEAT", "4"),
    ];
    for (i, (label, v)) in want.iter().enumerate() {
        ui.turn(PANEL_ENC_SELECT, i as i32 - i32::from(ui.v.midi.cursor));
        let row = midi_line(&mut ui, i);
        assert!(
            row.starts_with(label) && row.ends_with(v),
            "item {i}: {row:?}"
        );
    }
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MIDI_ITEMS as i32));
    ui.turn(PANEL_ENC_ALGORITHM, -3);
    assert_eq!(route.setup.mode, MIDI_MODE_MULTI as u8);
    ui.turn(PANEL_ENC_ALGORITHM, -9);
    assert_eq!(route.setup.mode, MIDI_MODE_LIVE as u8, "clamped at LIVE");
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 20);
    assert_eq!(route.setup.base, 15, "clamped at channel 16");
    ui.frame();
    assert!(midi_line(&mut ui, 1).trim_end().ends_with("16"));
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 2);
    assert_eq!(route.setup.option, MIDI_WHOLE_ARP as u8);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(route.setup.legato, 1);
    ui.turn(PANEL_ENC_SELECT, 3);
    ui.turn(PANEL_ENC_ALGORITHM, -5);
    assert_eq!(route.setup.arp_rate, 0);
    ui.frame();
    assert!(midi_line(&mut ui, 6).trim_end().ends_with("CLOCK"));
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, -9);
    assert_eq!(route.setup.rows_per_beat, 1);
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    assert_eq!(ui.top(), "MENU");
}

#[test]
fn the_midi_pages_clock_items_set_the_clock() {
    let mut ui = Ui::without_cards();
    let mut sync = Box::<midi_sync>::default();
    unsafe { midi_sync_init(&mut *sync) };
    open_once_the_menu_settles(&mut ui, VIEWS_MENU_MIDI);
    ui.turn(PANEL_ENC_SELECT, 8);
    ui.frame();
    let row = midi_line(&mut ui, 8);
    assert!(row.starts_with("CLOCK") && row.trim_end().ends_with(" -"));
    unsafe { views_sync(&mut *ui.v, &mut *sync) };
    ui.frame();
    assert!(midi_line(&mut ui, 8).trim_end().ends_with("AUTO"));
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(u32::from(sync.mode), MIDI_SYNC_FOLLOW);
    ui.turn(PANEL_ENC_ALGORITHM, 5);
    assert_eq!(u32::from(sync.mode), MIDI_SYNC_OFF, "clamped");
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.popup_gone();
    ui.frame();
    assert!(midi_line(&mut ui, 9).trim_end().ends_with("OFF"));
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(sync.clock_out, 1);
    ui.popup_gone();
    ui.frame();
    assert!(midi_line(&mut ui, 9).trim_end().ends_with("ON"));
}
