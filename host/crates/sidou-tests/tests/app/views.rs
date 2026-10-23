// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's views and the panel's routing through them (app/views.c): HOME and the menu, the
//! M1a and M1c panel mappings in every view, and the toasts and raster of what changed.

use std::ffi::CStr;

use sidou_sys::*;

use crate::piece_part;

use crate::{cutoff_after, Rig};

const COLS: usize = GRID_COLS as usize;

fn song_frames(ui: &mut Ui) {
    crate::drain(&mut ui.g);
    for _ in 0..40 {
        ui.frame();
        crate::drain(&mut ui.g);
    }
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
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
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
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
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
        song_attr(&ui, 24, 2, 22, th.hint, th.bg); // SAVE CHOOSE
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
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
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
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
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
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
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
        let sent = crate::drain(&mut ui.g);
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
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
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
    crate::drain(&mut ui.g);
    ui.press(PANEL_BTN_SEQ);
    for _ in 0..30 {
        ui.frame();
        crate::drain(&mut ui.g);
    }
    ui.press(SAVE_KEY_BUTTON);
    for _ in 0..30 {
        ui.frame();
        let sent = crate::drain(&mut ui.g);
        assert!(sent.cells <= SONG_VIEW_FRAME_CELLS as usize, "{sent:?}");
        assert!(sent.spi_ms() <= crate::SPI_BUDGET_MS, "{sent:?}");
    }
    ui.turn(PANEL_ENC_SELECT, 16);
    for _ in 0..10 {
        ui.frame();
        let sent = crate::drain(&mut ui.g);
        assert!(sent.cells <= SONG_VIEW_FRAME_CELLS as usize, "{sent:?}");
    }
    ui.press(PANEL_BTN_HOME);
    for _ in 0..30 {
        ui.frame();
        crate::drain(&mut ui.g);
    }
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
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    crate::drain(&mut ui.g);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    for _ in 0..60 {
        ui.frame();
        let sent = crate::drain(&mut ui.g);
        assert!(sent.cells <= SONG_VIEW_FRAME_CELLS as usize, "{sent:?}");
    }
    assert!(ui.row(6).contains("SONG 00"));
    assert!(ui.row(20).contains("SONG 07"));
    ui.turn(PANEL_ENC_SELECT, 8);
    for _ in 0..60 {
        ui.frame();
        crate::drain(&mut ui.g);
    }
    assert!(ui.row(6).contains("SONG 08"));
    assert!(ui.row(8).contains("SONG 09"));
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, 3);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    for _ in 0..60 {
        ui.frame();
        crate::drain(&mut ui.g);
    }
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

/// LOAD's (and DELETE's) files are large text, as the menu's lines: each name at 2x from column
/// 2, the selected one a reverse bar of 14 cells; a name longer than 14 characters ends in an
/// arrow. The confirmation names the file at 2x too.
#[test]
fn the_load_list_names_its_files_at_2x() {
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
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
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

/// A fresh install's LOAD has something to load: the built-in SIDOU LOOP, first and marked
/// built-in (a dot after its name), loaded as a file is (song_io_load_builtin); DELETE does not
/// list it. It takes the first place of the first page: the store's files follow, eight entries a
/// page. Unnamed after it, SAVE AS offers its name, and saving makes a store copy.
#[test]
fn load_lists_the_builtin_song_first_and_delete_does_not() {
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
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
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
    // DELETE: the store's files only.
    ui.turn(PANEL_ENC_SELECT, 3);
    io.state = SONG_IO_IDLE as u8;
    ui.press(SAVE_KEY_BUTTON);
    song_frames(&mut ui);
    assert!(ui.row(2).contains("DELETE"));
    assert!(ui.row(6).contains("SONG 00"), "{}", ui.row(6));
    // Unnamed and unchanged, the song is the built-in: SAVE AS offers its name.
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
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
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
    let mut ml = Box::<midi_learn>::default();
    unsafe {
        midi_learn_init(&mut *ml);
        views_learn(&mut *ui.v, &mut *ml);
    }
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
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
    model.frozen = 0; // Complete the mocked save, including its model-freeze release.
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

    /// As new(), with the title cards off (the menu's CARDS): for the tests of the views'
    /// routing and transitions, which the cards would sit over (title_card.rs tests them).
    pub(crate) fn without_cards() -> Self {
        let mut ui = Self::new();
        ui.v.cards = 0;
        ui
    }

    /// Presses button b (and lets it go, unless it is held with hold()). HOME acts on its release,
    /// in the frame: a tap runs the frame's flow step at once, as the next frame would, so that
    /// the view has changed when this returns.
    pub(crate) fn press(&mut self, b: u32) {
        let held = self.held | (1 << b);
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 1 << b, held) };
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 0, self.held) };
        if b == PANEL_BTN_HOME {
            unsafe { views_flow(&mut *self.v, self.now) };
        }
    }

    /// Button b's press edge, and it stays held until up(b): as the key scan reports it.
    pub(crate) fn down(&mut self, b: u32) {
        self.held |= 1 << b;
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 1 << b, self.held) };
    }

    /// Button b let go (after down).
    pub(crate) fn up(&mut self, b: u32) {
        self.held &= !(1 << b);
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 0, self.held) };
    }

    /// SAVE held while knob enc turns a detent, then let go: MIDI learn on the knob's parameter.
    pub(crate) fn learn_on(&mut self, enc: u32) {
        self.down(SAVE_KEY_BUTTON);
        self.turn(enc, 1);
        self.up(SAVE_KEY_BUTTON);
    }

    /// Button b's level only, without a press edge.
    pub(crate) fn hold(&mut self, b: u32, down: bool) {
        if down {
            self.held |= 1 << b;
        } else {
            self.held &= !(1 << b);
        }
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 0, self.held) };
    }

    /// The note keys down and pressed since the last call, through the views to the panel.
    pub(crate) fn note_keys(&mut self, down: u32, pressed: u32) {
        unsafe { views_note_keys(&mut *self.v, &mut *self.p, down, pressed) };
    }

    pub(crate) fn turn(&mut self, enc: u32, steps: i32) {
        unsafe { views_encoder(&mut *self.v, &mut *self.p, enc, steps) };
    }

    /// A frame, then the view's work between frames (views_idle: RED MOON RITE's scan) until
    /// it has none, as platform/fm1/ui.c's calls with nothing to send run it.
    pub(crate) fn frame(&mut self) {
        self.now += 40;
        unsafe { views_frame(&mut *self.v, self.now) };
        for _ in 0..GRID_CELLS {
            if unsafe { views_idle(&mut *self.v) } == 0 {
                break;
            }
        }
    }

    /// Frames until no transition runs and the value popup (app/screen.h) has closed.
    pub(crate) fn settle(&mut self) {
        for _ in 0..40 {
            self.frame();
            if unsafe { screen_animating(&self.v.s) } == 0 {
                self.popup_gone();
                return;
            }
        }
        panic!("the screen kept animating");
    }

    /// Frames until the popup is gone (a second and the frames it takes to close), the view
    /// under it composed again.
    pub(crate) fn popup_gone(&mut self) {
        for _ in 0..80 {
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

    /// Item `i`'s line of the menu (large text, a line every 3 rows from row 4, MENU_ROW in
    /// app/views.c): its 14 characters between the margins.
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
        self.big(4 + 3 * (i - top), 1, 2, 14)
            .chars()
            .map(|c| if c.is_ascii() { c } else { '#' })
            .collect()
    }

    /// A row's text, one character a cell; large text (grid_text_big) one a character, whatever
    /// its size and the row of its character, from the grid's record of the character each
    /// piece belongs to.
    pub(crate) fn row(&self, row: usize) -> String {
        let text = |b: u8| {
            if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                '#'
            }
        };
        (0..COLS)
            .filter_map(|c| {
                let i = row * COLS + c;
                let piece = usize::from(self.g.ch[i]).wrapping_sub(GRID_BIG_FIRST as usize);
                if piece >= GRID_BIG_PIECES as usize {
                    return Some(text(self.g.ch[i]));
                }
                let (scale, q) = piece_part(piece);
                (q % scale == 0).then(|| text(self.g.src[i]))
            })
            .collect()
    }

    /// `n` cells of a row from `col` as ordinary text: a large character's piece shows the glyph it is
    /// a piece of.
    pub(crate) fn small(&self, row: usize, col: usize, n: usize) -> String {
        (col..col + n)
            .map(|c| {
                let i = row * COLS + c;
                let piece = usize::from(self.g.ch[i]).wrapping_sub(GRID_BIG_FIRST as usize);
                let b = if piece < GRID_BIG_PIECES as usize {
                    self.g.src[i]
                } else {
                    self.g.ch[i]
                };
                if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    '#'
                }
            })
            .collect()
    }

    /// `n` characters of text at `scale` times the font from (`row`, `col`): one per `scale` cells,
    /// each read from the character's own piece (and its top left piece checked).
    pub(crate) fn big(&self, row: usize, col: usize, scale: usize, n: usize) -> String {
        (0..n)
            .map(|i| {
                let k = row * COLS + col + scale * i;
                let first = if scale == 2 {
                    GRID_BIG_FIRST
                } else {
                    GRID_BIG_FIRST + 4
                };
                if self.g.ch[k] == b' ' {
                    return self.g.src[k] as char;
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

/// Opens the status screen from the menu: HOME, SELECT to STATUS, ALGORITHM clockwise.
/// Opens SETTINGS from the menu (HOME first unless the menu or SETTINGS shows) and selects its
/// item `item` (VIEWS_SETTINGS_*), settled.
pub(crate) fn open_setting(ui: &mut Ui, item: u32) {
    if ui.top() != "SETTINGS" {
        if ui.top() != "MENU" {
            ui.press(PANEL_BTN_HOME);
            ui.settle();
        }
        assert_eq!(ui.top(), "MENU");
        ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
        ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SETTINGS as i32);
        ui.turn(PANEL_ENC_ALGORITHM, 1);
        ui.settle();
        assert_eq!(ui.top(), "SETTINGS");
    }
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_SETTINGS_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, item as i32);
    ui.settle();
    settings_settle(ui);
}

/// Frames until SETTINGS has drawn every line it shows (a few a frame, as the MIDI page), with
/// no popup over them.
pub(crate) fn settings_settle(ui: &mut Ui) {
    ui.frame();
    ui.popup_gone();
    for n in 0..20 {
        ui.frame();
        crate::drain(&mut ui.g);
        if n >= 8 && ui.v.setup.pending == 0 {
            return;
        }
    }
    panic!("SETTINGS kept lines pending");
}

pub(crate) fn open_status(ui: &mut Ui) {
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_STATUS as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
}

/// Opens the glyph sheet from the menu: HOME, SELECT to CHARSET, ALGORITHM clockwise.
pub(crate) fn open_charset(ui: &mut Ui) {
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_CHARSET as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
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
    // CHIP opens the CHIP view (chip_view.rs); counter-clockwise on it does nothing.
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(
        (ui.top().as_str(), ui.live().model),
        ("MENU", SID_MODEL_6581)
    );
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_PRESET as i32); // past TRACKER, which ALGORITHM would open
    ui.turn(PANEL_ENC_ALGORITHM, 2);
    assert_eq!(ui.live().preset, 2);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, -2);
    assert_eq!(ui.live().octave, -2);
    ui.turn(PANEL_ENC_ALGORITHM, -9);
    assert_eq!(ui.live().octave, LIVE_OCTAVE_MIN as i8);
    ui.turn(PANEL_ENC_ALGORITHM, 9);
    assert_eq!(ui.live().octave, LIVE_OCTAVE_MAX as i8);
    // SELECT stops at the last item, and turning the action back does nothing.
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_ITEMS as i32);
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(ui.top(), "MENU");
}

/// Frames until the menu has drawn every line (a scroll takes a few frames: app/widget.h budget).
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
    // The selected item's line (item 0) and the title bar (rows 0-2) are in reverse video:
    // MENU, and which item of how many.
    let heading = |ui: &Ui| {
        ui.big(1, 1, 2, 14)
            .chars()
            .map(|c| if c.is_ascii() { c } else { '#' })
            .collect::<String>()
    };
    assert_eq!(heading(&ui), format!("MENU   #  1/{VIEWS_MENU_ITEMS}"));
    assert!((0..14).all(|i| ui.g.src[COLS + 1 + 2 * i] == ui.g.src[2 * COLS + 1 + 2 * i]));
    let t = unsafe { &*theme_get(THEME_HIGH_CONTRAST) };
    assert_eq!(
        ui.g.attr[0],
        (t.title_bg << 4) | t.title_fg,
        "the title bar in the theme's"
    );
    // Items a line every 3 rows: a label after the cursor's column, the value right-aligned
    // (15 characters a line), an arrow for a view. PRE is PRESET, ROWS TRACKER ROWS.
    let at_top = |ui: &Ui| -> Vec<String> { (0..6).map(|i| ui.menu_line(i)).collect() };
    assert_eq!(
        at_top(&ui),
        [
            format!("{:<13}#", "CHIP"),
            format!("{:<13}#", "TRACKER"),
            format!("{:<13}#", "SCENE"),
            format!("{:<13}#", "SONG"),
            format!("{:<3}{:>11}", "PRE", "SAW LEAD"),
            format!("{:<6}{:>8}", "OCTAVE", "+0"),
        ]
    );
    // The arrow of an item that opens a view, in the line's last character (2 cells).
    assert_eq!(ui.g.src[4 * COLS + 28], FONT_ARROW_RIGHT as u8);
    assert_eq!(ui.g.src[7 * COLS + 28], FONT_ARROW_RIGHT as u8);
    // Hints in the title bar: the items below are out of sight (an arrow down), none above.
    assert_eq!(ui.g.src[COLS + 15], FONT_ARROW_DOWN as u8);
    assert_eq!(ui.g.src[COLS + 13], b' ');
    // Scrolled to the end: the last 6 items, an arrow up and none down (the lines come two a
    // frame, the selected first).
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
    // The help under the items, large text as well: SAVE is the menu's OK.
    assert_eq!(ui.big(24, 1, 2, 12), "SELECT  MOVE");
    assert_eq!(ui.big(26, 1, 2, 12), "SAVE    OPEN");
}

/// Ergonomics review E31: SETTINGS holds the set-and-forget settings, a page like the menu's,
/// its title SETTINGS and the help ALG CHANGE; HOME goes back to the menu.
#[test]
fn settings_draws_its_seven_items_like_the_menu_and_home_goes_back() {
    let mut ui = Ui::new();
    open_setting(&mut ui, VIEWS_SETTINGS_KEYS);
    let heading = ui
        .big(1, 1, 2, 14)
        .chars()
        .map(|c| if c.is_ascii() { c } else { '#' })
        .collect::<String>();
    assert_eq!(heading, format!("SETTINGS # 1/{VIEWS_SETTINGS_ITEMS}"));
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

/// SAVE is the menu's OK: on an item that opens a view it opens it (CHIP, SETTINGS); on a value
/// item it changes nothing and saves nothing; ALGORITHM still opens and changes.
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
        "CHIP", "TRACKER", "SCENE", "SONG", "PRE", "OCTAVE", "SETTINGS", "STATUS", "MIDI",
        "CHARSET", "ABOUT",
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
    // The cursor scrolls by one at the edges of the window, on the way back as well.
    for i in (0..labels.len() - 1).rev() {
        ui.turn(PANEL_ENC_SELECT, -1);
        walk(&mut ui, i);
    }
    assert_eq!(ui.v.home.top, 0);
    // The selected item's whole line, both rows, is in reverse video, and no other line is.
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

/// M1a's and M1c's panel mappings, in view `view`.
fn the_panel_plays(ui: &mut Ui, view: &str) {
    let l0 = (ui.live().octave, ui.live().model, ui.live().cutoff);
    ui.press(PANEL_BTN_OCT_UP);
    assert_eq!(ui.live().octave, l0.0 + 1, "{view}: OCT+");
    ui.press(PANEL_BTN_OCT_DOWN);
    assert_eq!(ui.live().octave, l0.0, "{view}: OCT-");
    ui.press(PANEL_BTN_FX);
    assert_ne!(ui.live().model, l0.1, "{view}: FX");
    ui.press(PANEL_BTN_FX);
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
    let alg = ui.live().slot[s][LIVE_SLOT_ALGORITHM as usize];
    ui.press(PANEL_BTN_ENV);
    assert_eq!(
        ui.live().slot[s][LIVE_SLOT_ALGORITHM as usize],
        alg + 1,
        "{view}: ENV"
    );
    // The EDIT layer: K1..K4 are the selected slot's Amount, A, B and Mix while EDIT is held.
    ui.hold(PANEL_BTN_EDIT, true);
    for k in 0..4 {
        let field = LIVE_SLOT_AMOUNT as usize + k;
        let before = ui.live().slot[s][field];
        // a control the algorithm's defaults put at the top turns down
        let steps = if before > LIVE_SLOT_MAX as u8 - LIVE_SLOT_DETENT as u8 {
            -1
        } else {
            1
        };
        ui.turn(PANEL_ENC_K1 + k as u32, steps);
        assert_eq!(
            ui.live().slot[s][field],
            before.wrapping_add_signed(steps as i8 * LIVE_SLOT_DETENT as i8),
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
    let mut ui = Ui::new();
    assert_eq!(ui.toast(), None, "a toast at boot");
    ui.press(PANEL_BTN_OCT_UP);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("OCTAVE +1"));
    // LIVE says the slot and its algorithm in its band: no toast for them there.
    for _ in 0..SCREEN_TOAST_MS.div_ceil(40) {
        ui.frame();
    }
    assert_eq!(ui.toast(), None);
    ui.rig.cc(LIVE_CC_SLOT_FIRST + 10, 127);
    ui.press(PANEL_BTN_SCL);
    ui.frame();
    assert_eq!(ui.toast(), None, "LIVE's band says it");
    ui.press(PANEL_BTN_SCL);
    ui.press(PANEL_BTN_SCL);
    ui.rig.cc(LIVE_CC_SLOT_FIRST + 10, 0);
    ui.frame();
    // Over another view they toast.
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_STATUS as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    ui.press(PANEL_BTN_SCL);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("SLOT MOD"));
    ui.press(PANEL_BTN_ENV);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("MOD CHORUS"));
    // Whatever made the change: here MIDI, the FX slot's algorithm CC.
    ui.rig.cc(LIVE_CC_SLOT_FIRST + 10, 127);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("FX LUSH"));
    assert_eq!(
        ui.row(SCREEN_TOAST_ROW as usize)[2..9].to_string(),
        "FX LUSH"
    );
    // It goes SCREEN_TOAST_MS after its first frame.
    for _ in 0..SCREEN_TOAST_MS.div_ceil(40) {
        ui.frame();
    }
    assert_eq!(ui.toast(), None);
}

#[test]
fn a_chip_change_shows_a_toast_and_runs_the_raster() {
    let mut ui = Ui::new();
    ui.press(PANEL_BTN_FX);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("CHIP 8580"));
    assert!(unsafe { screen_animating(&ui.v.s) } != 0, "no raster");
    ui.settle();
    // From MIDI as well: the model's CC, at the next tick.
    ui.rig.cc(25, 0);
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
    assert_eq!(
        ui.small(2, 22, 6),
        "V1.2.3",
        "the version ends before column 28"
    );
    assert_eq!(&ui.row(5)[2..13], "CHIP   6581");
    // The parameters: the name small, the number at 2x, a bar; two rows each from row 10.
    for (i, (name, num)) in [
        ("CUTOFF", ui.live().cutoff.to_string()),
        ("RESO", "6".into()),
    ]
    .into_iter()
    .enumerate()
    {
        let row = 10 + 2 * i;
        assert_eq!(ui.small(row + 1, 2, name.len()), name);
        assert_eq!(ui.big(row, 9, 2, num.len()), num);
    }
    assert_eq!(&ui.row(27)[2..8], "READY.");
    ui.press(PANEL_BTN_ENV);
    ui.frame();
    assert_eq!(ui.small(22, 2, 10), ">DIST TAPE");
}

/// With two chips (app/keys_route.h sets live.two_chips): the status screen shows both
/// chips' profiles and marks the keys' chip; LIVE's header says KEYS:B while the keys play
/// chip B; the menu's KEYS item switches AUTO and A.
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
    assert_eq!(&ui.row(5)[2..24], "CHIP A 8580 R5        ");
    assert_eq!(&ui.row(6)[2..24], "CHIP B 6581 R4AR  KEYS");
    assert_eq!(&ui.row(7)[2..17], "PRESET SAW LEAD");
    ui.rig.live.keys_chip = 0;
    ui.frame();
    assert_eq!(&ui.row(5)[2..24], "CHIP A 6581 R4AR  KEYS");
    assert_eq!(&ui.row(6)[2..24], "CHIP B 8580 R5        ");
    ui.rig.live.other_profile = LIVE_PROFILE_NONE as u8;
    ui.frame();
    assert_eq!(
        &ui.row(6)[2..24],
        "CHIP B --             ",
        "chip B not made yet"
    );
    // The menu's KEYS (HOME from the status screen goes back to the menu): clockwise A,
    // counter-clockwise AUTO.
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

/// A cell compose draws as a space on every view (inside the border, below the toast row).
const PROBE_CELL: usize = 28 * COLS + 20;

/// Marks a cell the views draw, so a frame that composes shows by putting it back.
fn mark(ui: &mut Ui) {
    ui.g.ch[PROBE_CELL] = b'@';
}

fn composed(ui: &Ui) -> bool {
    ui.g.ch[PROBE_CELL] != b'@'
}

#[test]
fn an_idle_view_is_not_composed_and_a_change_is() {
    let mut ui = Ui::new();
    open_status(&mut ui);
    mark(&mut ui);
    for _ in 0..30 {
        ui.frame();
    }
    assert!(!composed(&ui), "the idle status screen was composed");
    // Whatever the status screen shows: a knob, a slot field.
    type Change = fn(&mut Ui);
    let changes: [(&str, Change); 2] = [
        ("cutoff", |ui| ui.turn(PANEL_ENC_K1, -1)),
        ("slot amount", |ui| ui.rig.cc(LIVE_CC_SLOT_FIRST + 1, 99)),
    ];
    for (what, change) in changes {
        ui.settle();
        mark(&mut ui);
        change(&mut ui);
        ui.frame();
        assert!(composed(&ui), "{what}: not composed");
        mark(&mut ui);
        ui.frame();
        assert!(!composed(&ui), "{what}: composed again with nothing new");
    }
    // A note changes the notes row alone, not the screen: a cell of that row is drawn again and
    // the probe, elsewhere, is not (a held key costs a row).
    let notes = 19 * COLS + 25;
    for (what, change) in [
        ("note on", (|ui| ui.rig.note_on(60)) as Change),
        ("note off", |ui| ui.rig.note_off(60)),
    ] {
        ui.settle();
        mark(&mut ui);
        ui.g.ch[notes] = b'@';
        change(&mut ui);
        ui.frame();
        assert_eq!(ui.g.ch[notes], b' ', "{what}: the notes row was not drawn");
        assert!(
            !composed(&ui),
            "{what}: the whole screen was composed for a note"
        );
    }
    assert_eq!(ui.small(19, 9, 3), "   ", "the note is off");
}

#[test]
fn the_menu_composes_when_its_cursor_moves_and_not_while_it_waits() {
    let mut ui = Ui::new();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    // The selection is a line in reverse video: it does not blink, so a menu that waits
    // composes nothing and sends nothing (the 2 x 2 cell block of a cursor would cost 2 windows
    // a blink: emu-gate screen.rs asserts at most 2 in 16 frames).
    crate::drain(&mut ui.g);
    for _ in 0..4 * SCREEN_BLINK_FRAMES {
        mark(&mut ui);
        ui.frame();
        assert!(!composed(&ui), "the menu composed while it waited");
        assert_eq!(crate::drain(&mut ui.g).windows, 0);
    }
    mark(&mut ui);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.frame();
    assert!(
        composed(&ui),
        "the cursor moved and the menu was not composed"
    );
    // The glyph sheet never changes.
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_CHARSET as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    ui.g.ch[PROBE_CELL] = b'@';
    for _ in 0..20 {
        ui.frame();
    }
    assert_eq!(
        ui.g.ch[PROBE_CELL], b'@',
        "the glyph sheet was composed idle"
    );
}

/// A capture that takes every event.
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

/// The update box's text line, trimmed.
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
        assert_eq!(&ui.row(27)[2..8], "      ", "READY. under the box");
    }
    // Back in the main loop the next frame puts the view back.
    ui.frame();
    assert_eq!(&ui.row(27)[2..8], "READY.");
    assert_eq!(&ui.row(23)[3..6], "MOD");
}

/// M2e Task 3: the MIDI page edits MIDI's mode (app/midi_route.h) item by item, clamped at
/// both ends, and shows "-" with no router.
#[test]
fn the_midi_page_uses_six_large_rows_within_190_cells() {
    let mut ui = Ui::without_cards();
    for _ in 0..20 {
        ui.frame();
        crate::drain(&mut ui.g);
    }
    ui.press(PANEL_BTN_HOME);
    for _ in 0..12 {
        ui.frame();
        crate::drain(&mut ui.g);
    }
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
            let sent = crate::drain(&mut ui.g);
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

/// The MIDI page opens as a ui_poll of 23k instructions allows (app/views.c midi_compose): the
/// first frame the blank, the title bar and the help, then two lines a frame, none at once; the
/// cell the frame's first run is has the title bar's background (it is not seen).
#[test]
fn the_midi_page_builds_up_two_lines_a_frame_after_its_title_and_help() {
    let mut ui = Ui::without_cards();
    for _ in 0..20 {
        ui.frame();
        crate::drain(&mut ui.g);
    }
    ui.press(PANEL_BTN_HOME);
    for _ in 0..12 {
        ui.frame();
        crate::drain(&mut ui.g);
    }
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_MIDI as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    let mut valid = Vec::new();
    for frame in 0..6 {
        ui.frame();
        let sent = crate::drain(&mut ui.g);
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
    for _ in 0..8 {
        ui.frame();
        crate::drain(&mut ui.g);
    }
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
fn the_midi_page_edits_midis_mode() {
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
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_MIDI as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
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

/// M2e Task 2: the MIDI page's CLOCK and CLOCK OUT set MIDI's clock (app/midi_sync.h).
#[test]
fn the_midi_pages_clock_items_set_the_clock() {
    let mut ui = Ui::without_cards();
    let mut sync = Box::<midi_sync>::default();
    unsafe { midi_sync_init(&mut *sync) };
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_MIDI as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
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
