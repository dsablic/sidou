// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! TRACKER (app/tracker_view.c, M2d Task 2): the cells' text, the orderlists' entries, the
//! window in both modes, and the view's rows, orderlists and command line for a fixed song,
//! through the views with the fake player (song_view.rs).

use std::ffi::CStr;

use sidou_sys::*;
use sidou_tests::song::{SngSpec, Song as Model};

use crate::drain;
use crate::song_view::{attr, attr_of, ch, open_tracker, settle, Call, Song};

fn cell(c: [u8; 4]) -> String {
    let mut out = [0 as std::ffi::c_char; 9];
    unsafe { tracker_view_cell(c.as_ptr(), out.as_mut_ptr()) };
    unsafe { CStr::from_ptr(out.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn entry(e: u8) -> String {
    let mut out = [0 as std::ffi::c_char; 4];
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
    let (page, context, scroll_row) = (
        TRACKER_VIEW_PAGE as i32,
        TRACKER_VIEW_CONTEXT as i32,
        TRACKER_VIEW_SCROLL_ROW as i32,
    );
    assert_eq!(
        (page, context, TRACKER_VIEW_ROWS as i32),
        (8, 4, 12),
        "twelve rows: a page of 8 and 4 of context"
    );
    for row in 0..8 {
        assert_eq!(top(row, 0), -4, "row {row}");
    }
    assert_eq!(top(8, 0), 4);
    assert_eq!(top(0x0f, 0), 4);
    assert_eq!(top(0x10, 0), 12);
    assert_eq!(top(0x1a, 0), 0x14);
    for row in [0, 5, 12, 40, 127] {
        assert_eq!(top(row, 1), row as i32 - scroll_row, "row {row}");
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

/// The cell rows of the window's row j: the top (the note and the instrument at 2x) and the lower
/// (the number and the command, small).
fn top(j: usize) -> usize {
    TRACKER_VIEW_TOP as usize + 2 * j
}

fn th() -> &'static theme {
    unsafe { &*theme_get(THEME_HIGH_CONTRAST) }
}

#[test]
fn a_streaming_load_keeps_the_previous_canvas_until_the_model_is_complete() {
    let mut s = tracker();
    settle(&mut s.ui);
    let before = (s.ui.g.ch, s.ui.g.attr, s.ui.g.src);
    let song = s.ui.v.song.song;
    let mut io = Box::<song_io>::default();
    unsafe { views_song_io(&mut *s.ui.v, &mut *io) };
    for state in [SONG_IO_STOP, SONG_IO_READ, SONG_IO_VERIFY] {
        io.state = state as u8;
        s.ui.v.song.song = std::ptr::null();
        s.ui.v.song.built = 0;
        for _ in 0..12 {
            s.ui.frame();
            drain(&mut s.ui.g);
        }
        assert_eq!(
            (s.ui.g.ch, s.ui.g.attr, s.ui.g.src),
            before,
            "a partial load must not replace the displayed rows: state {state}"
        );
    }
    io.state = SONG_IO_IDLE as u8;
    s.ui.v.song.song = song;
    s.ui.v.song.built = 0;
    settle(&mut s.ui);
    assert_eq!(s.ui.big(0, 5, 2, 6), "PAT 00");
    assert_eq!((s.ui.g.ch, s.ui.g.attr, s.ui.g.src), before);
}

#[test]
fn the_rows_show_the_shown_voices_pattern_from_the_cursors_entry() {
    let s = tracker();
    let t = th();
    // Stopped: the cursor's entry 0 row 0: the page from row -4, the bar on row 0 (window row 4).
    // The header: V1 at 2x in voice 1's colour, its pattern, the stop square, the tempo.
    assert_eq!(s.ui.big(0, 0, 2, 2), "V1");
    assert_eq!(attr(&s.ui, 0, 0) & 15, t.voice[0]);
    assert_eq!(s.ui.big(0, 5, 2, 6), "PAT 00");
    assert_eq!(ch(&s.ui, 22, 0), FONT_SQUARE as u8);
    assert_eq!(s.ui.small(0, 26, 4), " 125");
    assert_eq!(s.ui.small(1, 26, 3), "BPM");
    // The other two voices' numbers over their strips.
    assert_eq!(s.ui.small(2, 26, 1), "2");
    assert_eq!(attr(&s.ui, 26, 2) & 15, t.voice[1]);
    assert_eq!(s.ui.small(2, 28, 1), "3");
    assert_eq!(attr(&s.ui, 28, 2) & 15, t.voice[2]);
    // Four blank rows of context above the first row.
    for j in 0..4 {
        for r in top(j)..top(j) + 2 {
            assert_eq!(s.ui.small(r, 0, 25).trim(), "", "window row {j}");
        }
    }
    // Row 0: the number small at the left of the lower row, the note and the instrument at 2x,
    // the command small.
    assert_eq!(s.ui.small(top(4) + 1, 0, 2), "00");
    assert_eq!(s.ui.big(top(4), 3, 2, 3), "C-4");
    assert_eq!(s.ui.big(top(4), 10, 2, 2), "01");
    assert_eq!(s.ui.small(top(4) + 1, 15, 3), "308");
    assert_eq!(s.ui.big(top(5), 3, 2, 3), "===");
    assert_eq!(s.ui.big(top(5), 10, 2, 2), "..");
    assert_eq!(s.ui.big(top(6), 3, 2, 3), "+++");
    assert_eq!(s.ui.big(top(7), 3, 2, 3), "---");
    assert_eq!(s.ui.big(top(8), 3, 2, 3), "G-4");
    assert_eq!(s.ui.small(top(8) + 1, 15, 3), "422");
    assert_eq!(s.ui.small(top(11) + 1, 0, 2), "07", "the 12th row");
    // The bar: a dark grey band across columns 0-17 on both cell rows of row 0, the current
    // number yellow, the note yellow, the instrument in the voice's colour, the command grey.
    for r in [top(4), top(4) + 1] {
        for c in 0..TRACKER_VIEW_BAND_W as usize {
            assert_eq!(
                u32::from(attr(&s.ui, c, r) >> 4),
                u32::from(t.cursor),
                "({c}, {r})"
            );
        }
        for c in TRACKER_VIEW_BAND_W as usize..30 {
            assert_eq!(
                u32::from(attr(&s.ui, c, r) >> 4),
                u32::from(t.bg),
                "off the bar ({c}, {r})"
            );
        }
    }
    assert_eq!(attr(&s.ui, 0, top(4) + 1) & 15, t.hot, "the current number");
    assert_eq!(attr(&s.ui, 3, top(4)) & 15, t.hot, "the current note");
    assert_eq!(attr(&s.ui, 10, top(4)) & 15, t.voice[0], "the instrument");
    assert_eq!(attr(&s.ui, 15, top(4) + 1) & 15, t.label, "the command");
    // Other rows: grey numbers, white notes, dark grey rests and blanks.
    assert_eq!(attr(&s.ui, 0, top(8) + 1) & 15, t.label);
    assert_eq!(attr(&s.ui, 3, top(8)) & 15, t.value, "a note");
    assert_eq!(attr(&s.ui, 3, top(5)) & 15, t.label, "a key off");
    assert_eq!(attr(&s.ui, 3, top(7)) & 15, t.dim, "a rest");
    assert_eq!(attr(&s.ui, 10, top(7)) & 15, t.dim, "no instrument");
    assert_eq!(attr(&s.ui, 15, top(7) + 1) & 15, t.dim, "no command");
    // On the bar the rests are lighter than the band.
    let mut quiet = tracker();
    quiet.ui.v.song.row = 1;
    settle(&mut quiet.ui);
    assert_eq!(
        attr(&quiet.ui, 3, top(5)) & 15,
        t.label,
        "a key off on the bar"
    );
    quiet.ui.v.song.row = 3;
    settle(&mut quiet.ui);
    assert_eq!(
        attr(&quiet.ui, 3, top(7)) & 15,
        t.label,
        "a rest on the bar, not dark grey"
    );
    // The strips: voice 2 (pattern 2) sounds a note on row 0 (tall), key off on row 4 and rests
    // otherwise (short); voice 3's pattern has 16 rows, so row 16 and on have none.
    let tall =
        |s: &Song, j: usize, col: usize| (ch(&s.ui, col, top(j)), ch(&s.ui, col, top(j) + 1));
    assert_eq!(
        tall(&s, 4, 26),
        (FONT_V4 as u8, FONT_V8 as u8),
        "voice 2 row 0"
    );
    assert_eq!(tall(&s, 5, 26), (b' ', FONT_V2 as u8), "a rest: short");
    assert_eq!(tall(&s, 8, 26), (b' ', FONT_V2 as u8), "a key off: short");
    assert_eq!(
        tall(&s, 4, 28),
        (FONT_V4 as u8, FONT_V8 as u8),
        "voice 3 row 0"
    );
    assert_eq!(attr(&s.ui, 26, top(4) + 1) & 15, t.voice[1]);
    assert_eq!(attr(&s.ui, 28, top(4) + 1) & 15, t.voice[2]);
    assert_eq!(
        u32::from(attr(&s.ui, 26, top(4) + 1) >> 4),
        u32::from(t.bg),
        "not on the band"
    );
    // The orderlist of the shown voice from its entry, the bar row's command spelled out, the
    // hint and the position.
    assert_eq!(s.ui.small(27, 0, 8), "OL 00 01");
    assert_eq!(
        attr(&s.ui, 3, 27),
        attr_of(u32::from(t.value), u32::from(t.cursor))
    );
    assert_eq!(s.ui.small(28, 0, 22), "V1 3 08 TONEPORTAMENTO");
    assert_eq!(s.ui.small(29, 0, 13).trim_end(), "SELECT: ROW");
    assert_eq!(s.ui.small(29, 17, 13), "ORD 00 ROW 00");
}

#[test]
fn algorithm_steps_the_shown_voice_and_round() {
    let mut s = tracker();
    let t = th();
    assert_eq!(s.ui.v.song.voice, 0);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.v.song.voice, 1);
    // V2 in cyan, its pattern (voice 2 starts at the orderlist's transpose, pattern 2 is its
    // first), its rows, the other two voices' numbers over the strips, its orderlist.
    assert_eq!(s.ui.big(0, 0, 2, 2), "V2");
    assert_eq!(attr(&s.ui, 0, 0) & 15, t.voice[1]);
    assert_eq!(s.ui.big(0, 5, 2, 6), "PAT 02");
    assert_eq!(s.ui.big(top(4), 3, 2, 3), "C-3");
    assert_eq!(s.ui.big(top(4), 10, 2, 2), "02");
    assert_eq!(
        attr(&s.ui, 10, top(4)) & 15,
        t.voice[1],
        "the instrument in voice 2's colour"
    );
    assert_eq!(s.ui.small(2, 26, 1), "1");
    assert_eq!(attr(&s.ui, 26, 2) & 15, t.voice[0]);
    assert_eq!(s.ui.small(2, 28, 1), "3");
    assert_eq!(s.ui.small(27, 0, 14).trim_end(), "OL 02 RST00");
    // Voice 1 is a strip now: tall on its row 0, its instrument shows no more.
    assert_eq!(ch(&s.ui, 26, top(4)), FONT_V4 as u8);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.big(0, 0, 2, 2), "V3");
    assert_eq!(attr(&s.ui, 0, 0) & 15, t.voice[2]);
    assert_eq!(s.ui.big(0, 5, 2, 6), "PAT 03");
    assert_eq!(s.ui.big(top(4), 3, 2, 3), "C-5");
    // Round: 1 again, and the other way: 3.
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.v.song.voice, 0);
    assert_eq!(s.ui.big(0, 0, 2, 2), "V1");
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    settle(&mut s.ui);
    assert_eq!(s.ui.v.song.voice, 2);
    s.ui.turn(PANEL_ENC_ALGORITHM, -5);
    settle(&mut s.ui);
    assert_eq!(
        s.ui.v.song.voice, 0,
        "five back from voice 3: round twice and one"
    );
    // The cursor row is the song's, not the voice's: it stays.
    assert_eq!(s.ui.v.song.row, 0);
    // ALGORITHM is TRACKER's: no toast, and the panel did nothing with it.
    assert!(s.ui.toast().is_none());
    assert_eq!(s.ui.v.s.popup_ask, 0);
}

#[test]
fn the_keys_audition_on_the_shown_voice() {
    let mut s = tracker();
    s.ui.turn(PANEL_ENC_ALGORITHM, 2);
    settle(&mut s.ui);
    s.calls();
    s.ui.note_keys(1 << 16, 1 << 16); // A4 on the panel at octave 0
    let jam: Vec<_> = s
        .calls()
        .into_iter()
        .filter(|c| matches!(c, Call::Jam(..)))
        .collect();
    assert_eq!(jam.len(), 1, "{jam:?}");
    match jam[0] {
        Call::Jam(voice, _, _) => assert_eq!(voice, 2, "the shown voice's"),
        _ => unreachable!(),
    }
}

#[test]
fn an_applied_edit_refreshes_the_stopped_trackers_rows() {
    let mut s = tracker();
    assert_eq!(s.ui.big(top(4), 3, 2, 3), "C-4");
    let song = s.ui.v.song.song.cast_mut();
    let mut edit = gt2_edit::default();
    let cell = [C4 + 2, 2, 4, 0x22];
    unsafe {
        assert_eq!(gt2_set_row(song, 0, 0, cell.as_ptr(), &mut edit), GT2_OK);
        gt2_edit_commit(song, &edit);
    }
    s.f.st.applied = 1;
    settle(&mut s.ui);
    assert_eq!(s.ui.big(top(4), 3, 2, 3), "D-4");
    assert_eq!(s.ui.big(top(4), 10, 2, 2), "02");
    assert_eq!(s.ui.small(top(4) + 1, 15, 3), "422");
    assert_eq!(s.ui.small(28, 0, 15), "V1 4 22 VIBRATO");
}

#[test]
fn a_muted_voice_is_grey_in_the_header_and_its_strip_and_says_so() {
    let mut s = tracker();
    let t = th();
    // Voice 1 (shown) muted: its number grey, MUT in red beside the pattern.
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(1, 1);
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    settle(&mut s.ui);
    assert_eq!(attr(&s.ui, 0, 0) & 15, t.dim, "V1 grey");
    assert_eq!(s.ui.small(0, 18, 3), "MUT");
    assert_eq!(attr(&s.ui, 18, 0) & 15, t.alert);
    // Its rows keep their colours (the header and the strips say the mute).
    assert_eq!(attr(&s.ui, 3, top(4)) & 15, t.hot, "the bar's note");
    assert_eq!(attr(&s.ui, 10, top(4)) & 15, t.voice[0], "its instrument");
    assert_eq!(attr(&s.ui, 3, top(8)) & 15, t.value, "another row's note");
    assert_eq!(
        s.ui.small(28, 0, 30).trim_end(),
        "",
        "voice 1's command is not said"
    );
    // Voice 2 muted too: its strip and its number turn dark grey, the bars short.
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(2, 2);
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    settle(&mut s.ui);
    assert_eq!(attr(&s.ui, 26, 2) & 15, t.dim, "its number");
    assert_eq!(attr(&s.ui, 26, top(4) + 1) & 15, t.dim, "its strip");
    assert_eq!(ch(&s.ui, 26, top(4)), b' ', "short, though a note sounds");
    assert_eq!(attr(&s.ui, 28, 2) & 15, t.voice[2], "voice 3 sounds");
    // Voice 1 soloed instead: SOL in white, the other two silent (grey strips).
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(1 | 2, 1 | 2); // both unmuted
    s.ui.note_keys(0, 0);
    s.ui.note_keys(1 << 3, 1 << 3); // solo voice 1
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(0, 18, 3), "SOL");
    assert_eq!(attr(&s.ui, 18, 0) & 15, t.value);
    assert_eq!(
        attr(&s.ui, 0, 0) & 15,
        t.voice[0],
        "V1 sounds under the solo"
    );
    assert_eq!(attr(&s.ui, 26, 2) & 15, t.dim);
    assert_eq!(attr(&s.ui, 28, 2) & 15, t.dim);
}

#[test]
fn the_command_line_says_the_first_command_of_a_voice_that_sounds() {
    let mut s = tracker();
    // Row 4: voice 1 has a vibrato (4 22), voice 2 a filter cutoff (C 40).
    s.ui.v.song.row = 4;
    settle(&mut s.ui);
    assert_eq!(s.ui.small(28, 0, 15), "V1 4 22 VIBRATO");
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(1, 1); // voice 1 muted: voice 2's is said
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(28, 0, 21), "V2 C 40 FILTER CUTOFF");
}

/// Playing: each voice's entry and row from the player's row events; the bar moves down the
/// page a row at a time (its two rows, a few cells), and turns the page at row 8, which the
/// copy spreads over frames, top first, each within the budget.
#[test]
fn the_bar_moves_down_the_page_and_turns_it() {
    let mut s = tracker();
    let t = th();
    s.ui.press(PANEL_BTN_PLAY);
    s.f.st.order_pos = [0, 1, 1];
    let mut most = 0;
    for row in 0..7u8 {
        s.f.st.row = [row, row, row];
        s.ui.frame();
        let sent = drain(&mut s.ui.g);
        most = most.max(sent.cells);
        // the bar's two rows (2 cell rows of 25 cells each way), and the position
        assert!(sent.cells <= 130, "row {row}: {sent:?}");
    }
    println!("the bar's step: at most {most} cells");
    assert_eq!(s.ui.small(top(4 + 6) + 1, 0, 2), "06");
    assert_eq!(attr(&s.ui, 0, top(4 + 6) + 1) & 15, t.hot);
    assert_eq!(s.ui.small(29, 17, 13), "ORD 00 ROW 06");
    // The page turn: the window from row 4 (rows 4..15), the bar on row 8.
    s.f.st.row = [8, 8, 8];
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
    println!("the page turn: {frames:?}");
    assert!(frames.len() <= 4, "the page turn took {frames:?}");
    assert_eq!(s.ui.small(top(0) + 1, 0, 2), "04");
    assert_eq!(s.ui.small(top(4) + 1, 0, 2), "08");
    assert_eq!(attr(&s.ui, 0, top(4) + 1) & 15, t.hot, "the bar on row 8");
    assert_eq!(s.ui.big(top(5), 3, 2, 3), "---", "pattern 0's row 9");
    // Voices 2 and 3 on their second entry: voice 3's pattern 3 has 16 rows.
    s.f.st.row = [0x11, 0x11, 0x11];
    settle(&mut s.ui);
    // The page of row $11 is rows $0C..$17: the bar on the window's row 5.
    assert_eq!(s.ui.small(top(5) + 1, 0, 2), "11");
    assert_eq!(s.ui.big(top(5), 3, 2, 3), "D-4", "pattern 0's row $11");
    // Row $11 is past voice 3's 16 rows: no strip bar beside it.
    assert_eq!(
        (ch(&s.ui, 28, top(5)), ch(&s.ui, 28, top(5) + 1)),
        (b' ', b' ')
    );
    assert_eq!(s.ui.small(27, 0, 14).trim_end(), "OL 00 01");
}

#[test]
fn scroll_keeps_the_bar_on_the_windows_row_5() {
    let mut s = tracker();
    // The menu's TRACKER ROWS: clockwise SCROLL (HOME back to LIVE, HOME the menu).
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    s.ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    s.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_ROWS as i32);
    settle(&mut s.ui);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(s.ui.v.song.scroll, 1);
    s.ui.frame();
    assert_eq!(
        s.ui.menu_line(VIEWS_MENU_ROWS as usize),
        format!("ROWS{:>10}", "SCROLL")
    );
    s.ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    s.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_TRACKER as i32);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "TRACKER");
    // Row 0 on window row 5, the five rows above blank.
    assert_eq!(s.ui.big(top(5), 3, 2, 3), "C-4");
    assert_eq!(s.ui.small(top(5) + 1, 0, 2), "00");
    assert_eq!(attr(&s.ui, 0, top(5) + 1) & 15, th().hot);
    for j in 0..5 {
        assert_eq!(s.ui.small(top(j), 0, 25).trim(), "", "window row {j}");
    }
    // Playing row 5: row 5 on window row 5, every row moved.
    s.ui.press(PANEL_BTN_PLAY);
    s.f.st.order_pos = [0, 1, 1];
    s.f.st.row = [5, 5, 5];
    settle(&mut s.ui);
    assert_eq!(s.ui.small(top(5) + 1, 0, 2), "05");
    assert_eq!(s.ui.big(top(4), 3, 2, 3), "G-4", "row 4 above");
    assert_eq!(s.ui.big(top(0), 3, 2, 3), "C-4", "row 0 at the top");
    assert_eq!(s.ui.small(top(11) + 1, 0, 2), "0B");
}

#[test]
fn without_a_song_the_rows_are_numbers_alone() {
    let mut s = Song::playing(None);
    open_tracker(&mut s.ui);
    assert_eq!(s.ui.small(top(4) + 1, 0, 2), "00");
    assert_eq!(s.ui.small(top(4), 3, 22).trim_end(), "");
    assert_eq!(s.ui.small(27, 0, 14).trim_end(), "OL");
    assert_eq!(s.ui.small(28, 0, 30).trim_end(), "");
}

#[test]
fn a_theme_change_redraws_the_tracker_in_the_new_colours() {
    let mut s = tracker();
    let hc = th();
    assert_eq!(u32::from(attr(&s.ui, 5, top(8)) >> 4), u32::from(hc.bg));
    unsafe { live_set_theme(&mut *s.ui.rig.live, THEME_C64) };
    s.ui.frame();
    settle(&mut s.ui);
    let c64 = unsafe { &*theme_get(THEME_C64) };
    assert_eq!(u32::from(attr(&s.ui, 5, top(8)) >> 4), u32::from(c64.bg));
    assert_eq!(attr(&s.ui, 0, top(8) + 1) & 15, c64.label);
    assert_eq!(s.ui.big(top(4), 3, 2, 3), "C-4", "the same rows");
}

/// The canvas as the view holds it: its cells, colours and the characters of its pieces.
fn canvas(s: &Song) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let cv = &s.ui.v.song.cv;
    (
        cv.ch.iter().flatten().copied().collect(),
        cv.attr.iter().flatten().copied().collect(),
        cv.src.iter().flatten().copied().collect(),
    )
}

/// Frames until the window is whole in the canvas (a few rows are built a frame), each sent.
fn finish_building(s: &mut Song) {
    for _ in 0..4 {
        s.ui.frame();
        drain(&mut s.ui.g);
        if s.ui.v.song.unbuilt == 0 {
            return;
        }
    }
    panic!("the window was not built in four frames");
}

/// Only the bar's step redraws its two rows and the footer; the canvas is the one a whole build
/// gives, at every row of a page and across the turns, whichever voice is shown and whatever is
/// muted.
#[test]
fn a_step_of_the_bar_draws_what_a_whole_build_would() {
    for voice in 0..3u32 {
        let mut s = tracker();
        s.ui.turn(PANEL_ENC_ALGORITHM, voice as i32);
        settle(&mut s.ui);
        s.ui.press(PANEL_BTN_PLAY);
        s.f.st.order_pos = [0, 1, 1];
        for row in (0..40u8).chain((0..40u8).rev()) {
            s.f.st.row = [row % 32, row % 32, row % 32];
            if row == 20 {
                s.ui.hold(PANEL_BTN_GLO, true);
                s.ui.note_keys(2, 2); // voice 2 muted
                s.ui.note_keys(0, 0);
                s.ui.hold(PANEL_BTN_GLO, false);
            }
            finish_building(&mut s);
            let stepped = canvas(&s);
            s.ui.v.song.built = 0;
            finish_building(&mut s);
            assert!(
                stepped == canvas(&s),
                "voice {voice}, row {row}: the step's canvas is not a whole build's"
            );
        }
    }
}

#[test]
fn select_moves_rows_and_edit_select_moves_a_readable_page() {
    let mut s = tracker();
    s.ui.turn(PANEL_ENC_SELECT, 3);
    settle(&mut s.ui);
    assert_eq!(s.ui.v.song.row, 3);
    assert_eq!(s.ui.v.song.voice, 0);
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.turn(PANEL_ENC_SELECT, 1);
    s.ui.hold(PANEL_BTN_EDIT, false);
    settle(&mut s.ui);
    assert_eq!(s.ui.v.song.row, 3 + TRACKER_VIEW_PAGE as u8);
    s.ui.turn(PANEL_ENC_SELECT, 1000);
    assert_eq!(s.ui.v.song.row, 31, "the end row cannot be edited");
    s.ui.turn(PANEL_ENC_SELECT, -1000);
    assert_eq!(s.ui.v.song.row, 0);
}

fn row(s: &Song, pattern: u32, row: usize) -> [u8; 4] {
    let data = unsafe { gt2_pattern_data(s.ui.v.song.song, pattern) };
    unsafe { std::slice::from_raw_parts(data.add(row * 4), 4) }
        .try_into()
        .unwrap()
}

#[test]
fn rec_keys_before_compose_follow_tracker_not_the_borrowed_canvas() {
    for borrowed in ["CHIP", "SONG", "SAVE AS"] {
        for covered in [false, true] {
            let mut s = tracker();
            let mut ml = Box::<midi_learn>::default();
            unsafe {
                midi_learn_init(&mut *ml);
                views_learn(&mut *s.ui.v, &mut *ml);
            }
            s.ui.press(PANEL_BTN_REC);
            if borrowed == "SAVE AS" {
                s.ui.press(SAVE_KEY_BUTTON);
                assert_eq!(s.ui.top(), "SONG");
                settle(&mut s.ui);
                s.ui.press(PANEL_BTN_HOME);
            } else {
                s.ui.press(PANEL_BTN_HOME);
                s.ui.press(PANEL_BTN_HOME);
                let item = if borrowed == "CHIP" {
                    VIEWS_MENU_CHIP
                } else {
                    VIEWS_MENU_SONG
                };
                s.ui.turn(PANEL_ENC_SELECT, item as i32);
                s.ui.turn(PANEL_ENC_ALGORITHM, 1);
                settle(&mut s.ui);
                assert_eq!(s.ui.top(), borrowed);
                s.ui.note_keys(1 << 16, 1 << 16);
                s.ui.note_keys(0, 0);
                assert_eq!(s.f.cell_writes, 0, "REC outside TRACKER only auditions");
                s.ui.press(PANEL_BTN_HOME);
                s.ui.press(PANEL_BTN_HOME);
                s.ui.press(PANEL_BTN_SEQ);
                s.ui.press(PANEL_BTN_SEQ);
            }
            assert_eq!(s.ui.top(), "TRACKER");
            if covered {
                s.ui.learn_on(PANEL_ENC_K1);
                assert_eq!(s.ui.top(), "LEARN");
            }
            s.ui.note_keys(1 << 16, 1 << 16);
            s.ui.note_keys(0, 0);
            assert_eq!(
                s.ui.v.song.row, 1,
                "{borrowed}, learn {covered}: the first note must advance before compose"
            );
            settle(&mut s.ui);
            assert_eq!(row(&s, 0, 0), [GT2_FIRSTNOTE as u8 + 57, 1, 3, 8]);
            assert_eq!(s.f.cell_writes, 1);
        }
    }
}

#[test]
fn rec_keys_survive_a_nested_tracker_above_midi_learn() {
    let mut s = tracker();
    let mut ml = Box::<midi_learn>::default();
    unsafe {
        midi_learn_init(&mut *ml);
        views_learn(&mut *s.ui.v, &mut *ml);
    }
    s.ui.press(PANEL_BTN_REC);
    s.ui.learn_on(PANEL_ENC_K1);
    assert_eq!(s.ui.top(), "LEARN");
    s.ui.press(PANEL_BTN_SEQ);
    s.ui.press(PANEL_BTN_SEQ);
    assert_eq!(s.ui.top(), "TRACKER");
    settle(&mut s.ui);
    s.ui.press(PANEL_BTN_HOME);
    assert_eq!(s.ui.top(), "LEARN");
    s.ui.press(PANEL_BTN_HOME);
    assert_eq!(s.ui.top(), "TRACKER");
    settle(&mut s.ui);
    s.ui.note_keys(1 << 16, 1 << 16);
    s.ui.note_keys(0, 0);
    settle(&mut s.ui);
    assert_eq!(row(&s, 0, 0), [GT2_FIRSTNOTE as u8 + 57, 1, 3, 8]);
    assert_eq!(s.ui.v.song.row, 1);
    assert_eq!(s.f.cell_writes, 1);
}

#[test]
fn learn_completion_under_a_nested_tracker_waits_to_pop_its_own_view() {
    for save in [false, true] {
        let mut s = tracker();
        let mut ml = Box::<midi_learn>::default();
        unsafe {
            midi_learn_init(&mut *ml);
            midi_learn_bind(&mut *ml, 21, LIVE_PARAM_CUTOFF);
            views_learn(&mut *s.ui.v, &mut *ml);
        }
        s.ui.learn_on(PANEL_ENC_K1);
        assert_eq!(s.ui.top(), "LEARN");
        s.ui.press(PANEL_BTN_SEQ);
        s.ui.press(PANEL_BTN_SEQ);
        assert_eq!(s.ui.top(), "TRACKER");
        if save {
            s.ui.press(SAVE_KEY_BUTTON);
        } else {
            unsafe { midi_learn_route(&mut *ml, 22) };
        }
        assert_eq!(u32::from(ml.state), MIDI_LEARN_IDLE);
        assert_eq!(s.ui.top(), "TRACKER", "learn must not pop its child");
        assert!(!s.ui.v.learn.s.is_null());
        s.ui.press(PANEL_BTN_HOME);
        assert_eq!(s.ui.top(), "LEARN", "HOME belongs to the child");
        settle(&mut s.ui);
        assert_eq!(s.ui.top(), "TRACKER");
        assert!(s.ui.v.learn.s.is_null());
        assert!(s.ui.v.s.capture.is_none());
        assert_eq!(
            unsafe { midi_learn_cc_of(&*ml, LIVE_PARAM_CUTOFF) },
            if save { MIDI_MAP_NO_CC as u8 } else { 22 }
        );
    }
}

#[test]
fn rec_keys_and_knobs_edit_the_shown_pattern_with_one_step_undo() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    s.ui.note_keys(1 << 16, 1 << 16);
    s.ui.note_keys(0, 0);
    settle(&mut s.ui);
    assert_eq!(row(&s, 0, 0), [GT2_FIRSTNOTE as u8 + 57, 1, 3, 8]);
    assert_eq!(s.ui.v.song.row, 1);
    s.ui.turn(PANEL_ENC_K1, 2);
    settle(&mut s.ui);
    assert_eq!(row(&s, 0, 1), [GT2_KEYOFF as u8, 2, 0, 0]);
    assert_eq!(
        unsafe { CStr::from_ptr(s.ui.v.popup.name.as_ptr()) }.to_str(),
        Ok("INSTRUMENT")
    );
    s.ui.turn(PANEL_ENC_K1 + 1, 4);
    settle(&mut s.ui);
    s.ui.turn(PANEL_ENC_K1 + 2, 2);
    settle(&mut s.ui);
    s.ui.turn(PANEL_ENC_K4, 3);
    settle(&mut s.ui);
    assert_eq!(row(&s, 0, 1), [GT2_KEYOFF as u8, 2, 4, 0x23]);
    s.ui.press(PANEL_BTN_REC);
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.press(PANEL_BTN_LFO);
    s.ui.hold(PANEL_BTN_EDIT, false);
    settle(&mut s.ui);
    assert_eq!(row(&s, 0, 1)[3], 0x20);
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.press(PANEL_BTN_LFO);
    s.ui.hold(PANEL_BTN_EDIT, false);
    settle(&mut s.ui);
    assert_eq!(row(&s, 0, 1)[3], 0x23);
    assert_eq!(s.f.cell_writes, 7);
    assert_eq!(s.f.index_swaps, 0);
}

#[test]
fn rec_dispatch_uses_the_active_view_before_the_first_compose_not_the_canvas_owner() {
    let mut s = Song::playing(Some(fixed_song()));
    s.ui.press(PANEL_BTN_SEQ);
    s.ui.press(PANEL_BTN_SEQ);
    assert_ne!(s.ui.v.song.which, SONG_VIEW_TRACKER as u8);
    s.ui.press(PANEL_BTN_REC);
    s.ui.note_keys(1 << 16, 1 << 16);
    s.ui.note_keys(0, 0);
    settle(&mut s.ui);
    assert_eq!(row(&s, 0, 0)[0], GT2_FIRSTNOTE as u8 + 57);
    s.ui.press(PANEL_BTN_SEQ);
    s.ui.press(PANEL_BTN_SEQ);
    assert_eq!(s.ui.top(), "INSTR");
    let writes = s.f.cell_writes;
    s.ui.note_keys(1, 1);
    s.ui.note_keys(0, 0);
    settle(&mut s.ui);
    assert_eq!(
        s.f.cell_writes, writes,
        "REC on INSTR auditions without pattern writes"
    );
}

#[test]
fn high_octave_keys_reach_the_record_range_refusal_without_sending_an_invalid_jam() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    s.ui.rig.live.octave = 3;
    s.calls();
    let before = row(&s, 0, 0);
    s.ui.note_keys(1 << 26, 1 << 26);
    assert_eq!(s.ui.toast().as_deref(), Some("NOTE OUT OF RANGE"));
    assert_eq!(row(&s, 0, 0), before);
    assert_eq!(s.ui.v.song.row, 0);
    assert!(!s
        .calls()
        .iter()
        .any(|call| matches!(call, Call::Jam(_, 96.., _))));
}

#[test]
fn a_second_lfo_on_the_same_row_clears_the_whole_row_and_arp_sets_keyoff() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    s.ui.press(PANEL_BTN_LFO);
    settle(&mut s.ui);
    assert_eq!(row(&s, 0, 0), [GT2_REST as u8, 1, 3, 8]);
    s.ui.press(PANEL_BTN_LFO);
    settle(&mut s.ui);
    assert_eq!(row(&s, 0, 0), R);
    s.ui.press(PANEL_BTN_ARP);
    settle(&mut s.ui);
    assert_eq!(row(&s, 0, 0), [GT2_KEYOFF as u8, 0, 0, 0]);
}

#[test]
fn saving_and_full_refuse_an_edit_with_a_large_toast() {
    for (guard, message) in [
        (SONG_EDIT_GUARD_SAVING, "SAVING"),
        (SONG_EDIT_GUARD_FULL, "SONG FULL"),
    ] {
        let mut s = tracker();
        s.f.edit_guard = guard as i32;
        s.ui.press(PANEL_BTN_REC);
        s.ui.turn(PANEL_ENC_K1, 1);
        settle(&mut s.ui);
        assert_eq!(row(&s, 0, 0), [C4, 1, 3, 8]);
        assert_eq!(s.ui.toast().as_deref(), Some(message));
        assert!(s.ui.v.s.toast_h >= 2);
        assert_eq!(s.f.cell_writes, 0);
    }
}

#[test]
fn the_edit_cursor_and_each_edited_field_are_visible_at_two_times() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    settle(&mut s.ui);
    assert_eq!(
        attr(&s.ui, 0, top(4) + 1),
        attr_of(th().bg.into(), th().hot.into())
    );
    assert_eq!(
        attr(&s.ui, 3, top(4)),
        attr_of(th().bg.into(), th().hot.into())
    );
    s.ui.turn(PANEL_ENC_K1, 1);
    s.ui.frame();
    s.ui.now += 1500;
    settle(&mut s.ui);
    assert_eq!(s.ui.big(top(4), 10, 2, 2), "02");
    assert_eq!(
        attr(&s.ui, 10, top(4)),
        attr_of(th().bg.into(), th().hot.into())
    );
    s.ui.turn(PANEL_ENC_K1 + 1, 1);
    s.ui.frame();
    s.ui.now += 1500;
    settle(&mut s.ui);
    assert_eq!(s.ui.big(top(4), 15, 2, 3), "408");
    assert_eq!(
        attr(&s.ui, 15, top(4)),
        attr_of(th().bg.into(), th().hot.into())
    );
}

#[test]
fn presets_step_size_and_shift_keep_the_length_and_undo_the_lost_tail() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    s.ui.turn(6, 2);
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    s.ui.hold(PANEL_BTN_EDIT, false);
    assert_eq!(s.ui.v.song.instr, 3);
    assert_eq!(s.ui.v.song.step, 0);
    s.ui.note_keys(1 << 16, 1 << 16);
    s.ui.note_keys(0, 0);
    settle(&mut s.ui);
    assert_eq!(s.ui.v.song.row, 0);
    assert_eq!(row(&s, 0, 0)[1], 3);
    let before: Vec<_> = (0..32).map(|r| row(&s, 0, r)).collect();
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.turn(6, 1);
    s.ui.hold(PANEL_BTN_EDIT, false);
    settle(&mut s.ui);
    assert_eq!(row(&s, 0, 0), R);
    assert_eq!(row(&s, 0, 1), before[0]);
    assert_eq!(unsafe { gt2_pattern_rows(s.ui.v.song.song, 0) }, 33);
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.press(PANEL_BTN_LFO);
    s.ui.hold(PANEL_BTN_EDIT, false);
    settle(&mut s.ui);
    assert_eq!((0..32).map(|r| row(&s, 0, r)).collect::<Vec<_>>(), before);
    assert_eq!(s.f.index_swaps, 2);
}

#[test]
fn a_pending_edit_finishes_after_home_and_cannot_lose_its_binding() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    s.f.refuse = true;
    s.ui.turn(PANEL_ENC_K1, 1);
    assert_ne!(s.ui.v.editor.pending, 0);
    let old = s.ui.v.song.t;
    unsafe { song_view_set_transport(&mut s.ui.v.song, std::ptr::null()) };
    assert_eq!(
        s.ui.v.song.t, old,
        "pending ownership keeps its original binding"
    );
    s.ui.press(PANEL_BTN_HOME);
    s.f.refuse = false;
    settle(&mut s.ui);
    assert_eq!(s.ui.v.editor.pending, 0);
    assert_eq!(row(&s, 0, 0)[1], 2);
    assert_eq!(s.f.cell_writes, 1);
}
