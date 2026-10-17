// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! TRACKER (app/tracker_view.c, M2d Task 2): the cells' text, the orderlists' entries, the
//! window in both modes, and the view's rows, orderlists and command line for a fixed song,
//! through the views with the fake player (song_view.rs).

use std::ffi::CStr;

use sidou_sys::*;
use sidou_tests::song::{SngSpec, Song as Model};

use crate::drain;
use crate::song_view::{attr, attr_of, open_tracker, settle, Song};

fn cell(c: [u8; 4]) -> String {
    let mut out = [0 as std::ffi::c_char; 9];
    unsafe { tracker_view_cell(c.as_ptr(), out.as_mut_ptr()) };
    unsafe { CStr::from_ptr(out.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn entry(e: u8) -> String {
    let mut out = [0 as std::ffi::c_char; 3];
    unsafe { tracker_view_order_entry(e, out.as_mut_ptr()) };
    unsafe { CStr::from_ptr(out.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

const C4: u8 = 0x60 + 4 * 12;

#[test]
fn a_cell_reads_as_goattracker_shows_it() {
    assert_eq!(cell([C4, 1, 3, 0x08]), "C-401308");
    assert_eq!(cell([0x60 + 3 * 12 + 1, 0x1f, 0xf, 0x06]), "C#31FF06");
    assert_eq!(cell([GT2_LASTNOTE as u8, 0, 0, 0]), "G#7.....");
    assert_eq!(cell([0x60, 0, 0, 0]), "C-0.....");
    assert_eq!(cell([GT2_REST as u8, 0, 0, 0]), "---.....");
    assert_eq!(cell([GT2_KEYOFF as u8, 2, 0, 0]), "===02...");
    assert_eq!(cell([GT2_KEYON as u8, 0, 4, 0x22]), "+++..422");
}

#[test]
fn an_orderlist_entry_reads_as_goattracker_shows_it() {
    assert_eq!(entry(0x05), "05");
    assert_eq!(entry(0xcf), "CF");
    assert_eq!(entry(0xd2), "R2");
    assert_eq!(entry(0xd0), "R0", "16 times");
    assert_eq!(entry(0xf0), "+0");
    assert_eq!(entry(0xf3), "+3");
    assert_eq!(entry(0xef), "-1");
    assert_eq!(entry(0xe1), "-F");
}

#[test]
fn the_window_is_the_bars_page_or_scrolls_under_it() {
    let top = |row, scroll| unsafe { tracker_view_top(row, scroll) };
    for row in 0..16 {
        assert_eq!(top(row, 0), -4, "row {row}");
    }
    assert_eq!(top(16, 0), 12);
    assert_eq!(top(0x1a, 0), 12);
    assert_eq!(top(0x20, 0), 0x1c);
    for row in [0, 5, 12, 40, 127] {
        assert_eq!(top(row, 1), row as i32 - 12, "row {row}");
    }
    assert_eq!(
        unsafe { CStr::from_ptr(tracker_view_command_name(4)) }.to_str(),
        Ok("VIBRATO")
    );
    assert_eq!(
        unsafe { CStr::from_ptr(tracker_view_command_name(0)) }.to_str(),
        Ok("")
    );
}

const R: [u8; 4] = [0xbd, 0, 0, 0];

/// Voice 1 plays patterns 0 and 1, voice 2 pattern 2 transposed +3, voice 3 pattern 3 twice;
/// pattern 0 holds each kind of cell in its first rows.
fn fixed_song() -> Model {
    let mut p0 = vec![R; 32];
    p0[0] = [C4, 1, 3, 0x08];
    p0[1] = [GT2_KEYOFF as u8, 0, 0, 0];
    p0[2] = [GT2_KEYON as u8, 0, 0, 0];
    p0[4] = [C4 + 7, 0, 4, 0x22];
    p0[17] = [C4 + 2, 2, 0, 0];
    let mut p2 = vec![R; 32];
    p2[0] = [C4 - 12, 2, 0, 0];
    p2[4] = [GT2_KEYOFF as u8, 0, 0xc, 0x40];
    let mut p3 = vec![R; 16];
    p3[0] = [C4 + 12, 1, 0, 0];
    let mut s = SngSpec::new();
    s.orders = vec![vec![
        (vec![0, 1], 0),
        (vec![0xf3, 2], 0),
        (vec![0xd2, 3], 1),
    ]];
    s.instr = vec![[0u8; 25], [0u8; 25]];
    s.patterns = vec![
        (p0, [0xff, 0, 0, 0]),
        (vec![R; 32], [0xff, 0, 0, 0]),
        (p2, [0xff, 0, 0, 0]),
        (p3, [0xff, 0, 0, 0]),
    ];
    let mut m = Model::host();
    m.read(&s.encode(), 0).unwrap();
    m
}

fn tracker() -> Song {
    let mut s = Song::playing(Some(fixed_song()));
    open_tracker(&mut s.ui);
    s
}

#[test]
fn the_rows_show_each_voices_pattern_from_the_cursors_entry() {
    let s = tracker();
    // Stopped: the cursor's entry 0 row 0: the page from row -4, the bar on row 0 (screen 6).
    for r in 2..6 {
        assert_eq!(s.ui.row(r)[..29].trim(), "", "row {r}");
    }
    assert_eq!(s.ui.row(6), "00 C-401308 C-302... C-501...#");
    assert_eq!(s.ui.row(7), "01 ===..... ---..... ---.....#");
    assert_eq!(&s.ui.row(8)[..11], "02 +++.....");
    assert_eq!(&s.ui.row(9)[..11], "03 ---.....");
    assert_eq!(&s.ui.row(10)[..20], "04 G-4..422 ===..C40");
    // Voice 3's pattern has 16 rows: blank from row 16 on; the numbers follow the bar's voice.
    assert_eq!(&s.ui.row(22)[21..29], "        ");
    assert_eq!(&s.ui.row(22)[..2], "10");
    assert_eq!(&s.ui.row(26)[..2], "14");
    // The bar: light blue on blue to column 28, its number white; the cells' colours.
    let (bar, black) = (GRID_BLUE, GRID_BLACK);
    assert_eq!(attr(&s.ui, 0, 6), attr_of(GRID_WHITE, bar));
    assert_eq!(attr(&s.ui, 3, 6), attr_of(GRID_WHITE, bar), "a note");
    assert_eq!(
        attr(&s.ui, 6, 6),
        attr_of(GRID_CYAN, bar),
        "the instrument on the bar"
    );
    assert_eq!(attr(&s.ui, 8, 6), attr_of(GRID_YELLOW, bar), "the command");
    assert_eq!(
        attr(&s.ui, 9, 6),
        attr_of(GRID_LIGHT_GREEN, bar),
        "its data"
    );
    assert_eq!(attr(&s.ui, 3, 7), attr_of(GRID_GREY, black), "a key off");
    assert_eq!(attr(&s.ui, 3, 9), attr_of(GRID_DARK_GREY, black), "a rest");
    assert_eq!(
        attr(&s.ui, 6, 9),
        attr_of(GRID_DARK_GREY, black),
        "no instrument"
    );
    assert_eq!(
        attr(&s.ui, 8, 9),
        attr_of(GRID_DARK_GREY, black),
        "no command"
    );
    assert_eq!(
        attr(&s.ui, 0, 10),
        attr_of(GRID_LIGHT_GREY, black),
        "every 4"
    );
    assert_eq!(attr(&s.ui, 0, 22), attr_of(GRID_YELLOW, black), "every 16");
    assert_eq!(attr(&s.ui, 0, 7), attr_of(GRID_GREY, black));
    // Column 29: the pip at the bar's place, row 0 of 32 at the top; row 27 a rule.
    assert_eq!(s.ui.g.ch[2 * 30 + 29], FONT_PIP as u8);
    assert_eq!(s.ui.g.ch[3 * 30 + 29], FONT_BOX_V as u8);
    assert!((0..30).all(|c| s.ui.g.ch[27 * 30 + c] == FONT_BOX_H as u8));
    // Row 28: each voice's orderlist from its entry; row 29: the bar row's first command.
    assert_eq!(s.ui.row(28), "OL 00 01    02 RST00 03 RST01 ");
    assert_eq!(attr(&s.ui, 3, 28), attr_of(GRID_WHITE, GRID_BLUE));
    assert_eq!(s.ui.row(29).trim_end(), "V1 3 08 TONEPORTAMENTO");
}

#[test]
fn an_applied_edit_refreshes_the_stopped_trackers_rows() {
    let mut s = tracker();
    assert_eq!(&s.ui.row(6)[3..11], "C-401308");
    let song = s.ui.v.song.song.cast_mut();
    let mut edit = gt2_edit::default();
    let cell = [C4 + 2, 2, 4, 0x22];
    unsafe {
        assert_eq!(gt2_set_row(song, 0, 0, cell.as_ptr(), &mut edit), GT2_OK);
        gt2_edit_commit(song, &edit);
    }
    s.f.st.applied = 1;
    settle(&mut s.ui);
    assert_eq!(&s.ui.row(6)[3..11], "D-402422");
    assert_eq!(s.ui.row(29).trim_end(), "V1 4 22 VIBRATO");
}

#[test]
fn a_silent_voice_is_dark_grey_and_says_nothing() {
    let mut s = tracker();
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(1, 1); // voice 1 muted
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    settle(&mut s.ui);
    for c in 3..11 {
        assert_eq!(
            attr(&s.ui, c, 6),
            attr_of(GRID_DARK_GREY, GRID_BLUE),
            "column {c}"
        );
        assert_eq!(
            attr(&s.ui, c, 10),
            attr_of(GRID_DARK_GREY, GRID_BLACK),
            "column {c}"
        );
    }
    assert_eq!(
        attr(&s.ui, 12, 6),
        attr_of(GRID_WHITE, GRID_BLUE),
        "voice 2 sounds"
    );
    assert_eq!(s.ui.row(29).trim_end(), "", "voice 1's command is not said");
    // On row 4 voice 2's filter command is.
    s.ui.v.song.row = 4;
    settle(&mut s.ui);
    assert_eq!(s.ui.row(29).trim_end(), "V2 C 40 FILTER CUTOFF");
}

/// Playing: each voice's entry and row from the player's row events; the bar moves down the
/// page a row at a time (its two rows, a few cells), and turns the page at row 16, which the
/// copy spreads over frames, top first, each within the budget.
#[test]
fn the_bar_moves_down_the_page_and_turns_it() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_PLAY);
    s.f.st.order_pos = [0, 1, 1];
    for row in 0..15u8 {
        s.f.st.row = [row, row, row];
        s.ui.frame();
        let sent = drain(&mut s.ui.g);
        // the bar's two rows (58 cells), the pip, the header's row, the command line
        assert!(sent.cells <= 100, "row {row}: {sent:?}");
    }
    assert_eq!(s.ui.row(6 + 14)[..2].to_string(), "0E");
    assert_eq!(attr(&s.ui, 0, 20), attr_of(GRID_WHITE, GRID_BLUE));
    // The page turn: the window from row 12.
    s.f.st.row = [16, 16, 16];
    let mut frames = Vec::new();
    for _ in 0..8 {
        s.ui.frame();
        let sent = drain(&mut s.ui.g);
        assert!(sent.cells as u32 <= SONG_VIEW_FRAME_CELLS, "{sent:?}");
        frames.push(sent.cells);
        if s.ui.v.song.dirty == 0 {
            break;
        }
    }
    assert!(frames.len() <= 4, "the page turn took {frames:?}");
    assert_eq!(&s.ui.row(2)[..2], "0C");
    assert_eq!(&s.ui.row(6)[..2], "10");
    assert_eq!(
        attr(&s.ui, 0, 6),
        attr_of(GRID_WHITE, GRID_BLUE),
        "the bar on row $10"
    );
    assert_eq!(&s.ui.row(7)[3..11], "D-402...", "pattern 0's row $11");
    // Voices 2 and 3 on their second entry: pattern 2 (after the transpose) and 3.
    assert_eq!(s.ui.row(28), "OL 00 01    02 RST00 03 RST01 ");
}

#[test]
fn scroll_keeps_the_bar_on_the_windows_row_12() {
    let mut s = tracker();
    // The menu's TRACKER ROWS: clockwise SCROLL (HOME back to LIVE, HOME the menu).
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    s.ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    s.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_ROWS as i32);
    settle(&mut s.ui);
    while s.ui.v.s.frame % SCREEN_BLINK_FRAMES != 2 {
        s.ui.frame();
    }
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(s.ui.v.song.scroll, 1);
    // The value changes without waiting for the cursor's next blink phase.
    s.ui.frame();
    assert_eq!(
        s.ui.row(4 + 1 + VIEWS_MENU_ROWS as usize)[4..27].to_string(),
        format!("{:<17}SCROLL", "TRACKER ROWS")
    );
    s.ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    s.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_TRACKER as i32);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "TRACKER");
    // Row 0 on screen row 14, the twelve rows above blank.
    assert_eq!(&s.ui.row(14)[..11], "00 C-401308");
    assert_eq!(attr(&s.ui, 0, 14), attr_of(GRID_WHITE, GRID_BLUE));
    assert_eq!(s.ui.row(13)[..29].trim(), "");
    // Playing row 5: row 5 on 14, every row moved.
    s.ui.press(PANEL_BTN_PLAY);
    s.f.st.order_pos = [0, 1, 1];
    s.f.st.row = [5, 5, 5];
    settle(&mut s.ui);
    assert_eq!(&s.ui.row(14)[..2], "05");
    assert_eq!(&s.ui.row(13)[..11], "04 G-4..422");
    assert_eq!(&s.ui.row(9)[..11], "00 C-401308");
}

#[test]
fn without_a_song_the_rows_are_numbers_alone() {
    let mut s = Song::playing(None);
    open_tracker(&mut s.ui);
    assert_eq!(s.ui.row(6)[..29].trim_end(), "00");
    assert_eq!(s.ui.row(28).trim_end(), "OL");
    assert_eq!(s.ui.row(29).trim_end(), "");
}
