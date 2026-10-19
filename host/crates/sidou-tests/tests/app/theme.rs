// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The theme (app/theme.c): the colour roles every view takes its colours from, the menu's
//! THEME item, and the menu, toast and popup following it.

use sidou_sys::*;

use crate::views::{open_status, Ui};

const COLS: usize = GRID_COLS as usize;

fn theme(id: u32) -> &'static theme {
    unsafe { &*theme_get(id) }
}

fn fg(attr: u8) -> u8 {
    attr & 15
}

fn bg(attr: u8) -> u8 {
    attr >> 4
}

#[test]
fn theme_roles_pass_contrast() {
    let luminance = unsafe { grid_palette }.map(|c| {
        let (r, g, b) = ((c >> 11) as u8 & 31, (c >> 5) as u8 & 63, c as u8 & 31);
        let rgb = [r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2].map(|v| {
            let s = f64::from(v) / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
    });
    for id in 0..THEME_COUNT {
        let t = theme(id);
        let check = |fg: u8, bg: u8, minimum: f64| {
            let a = luminance[usize::from(fg)];
            let b = luminance[usize::from(bg)];
            let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert!(
                ratio >= minimum,
                "theme {id}: {fg} on {bg}: {ratio:.3} < {minimum}"
            );
        };
        for colour in [t.value, t.label, t.hint, t.hot]
            .into_iter()
            .chain(t.knob_text)
            .chain(t.voice_text)
        {
            check(colour, t.bg, 4.5);
        }
        for colour in [t.alert, t.menu_item, t.menu_value]
            .into_iter()
            .chain(t.voice)
            .chain(t.knob)
        {
            check(colour, t.bg, 3.0);
        }
        check(t.sel_fg, t.sel_bg, 3.0);
        check(t.title_fg, t.title_bg, 3.0);
        check(t.toast_fg, t.toast_bg, 4.5);
        check(t.value, t.cursor, 4.5);
        check(t.hot, t.cursor, 4.5);
        for accent in t.knob {
            let name = if t.popup_name == THEME_ACCENT as u8 {
                accent
            } else {
                t.popup_name
            };
            check(name, t.popup_bg, 3.0);
        }
    }
}

#[test]
fn high_contrast_is_the_default_and_has_the_agreed_roles() {
    let t = theme(THEME_HIGH_CONTRAST);
    assert_eq!(u32::from(t.bg), GRID_BLACK);
    assert_eq!(u32::from(t.value), GRID_WHITE);
    assert_eq!(u32::from(t.label), GRID_LIGHT_GREY);
    assert_eq!(u32::from(t.dim), GRID_DARK_GREY);
    assert_eq!(u32::from(t.cursor), GRID_DARK_GREY);
    assert_eq!(u32::from(t.hot), GRID_YELLOW);
    let voices = t.voice.map(u32::from);
    assert_eq!(voices, [GRID_LIGHT_RED, GRID_CYAN, GRID_LIGHT_GREEN]);
    let knobs = t.knob.map(u32::from);
    assert_eq!(
        knobs,
        [GRID_YELLOW, GRID_CYAN, GRID_LIGHT_GREEN, GRID_LIGHT_RED]
    );
    // An id the firmware does not know is the default.
    assert!(std::ptr::eq(theme(77), t));
    assert!(std::ptr::eq(theme(THEME_COUNT), t));
    // A new instrument has it.
    let ui = Ui::new();
    assert_eq!(u32::from(ui.live().theme), THEME_HIGH_CONTRAST);
    assert_eq!(THEME_COUNT, LIVE_THEMES);
}

#[test]
fn every_role_is_a_palette_colour_and_reads_on_the_background_and_the_cursor_row() {
    for id in 0..THEME_COUNT {
        let t = theme(id);
        let on_bg = [
            t.value,
            t.label,
            t.hint,
            t.dim,
            t.hot,
            t.alert,
            t.voice[0],
            t.voice[1],
            t.voice[2],
            t.knob[0],
            t.knob[1],
            t.knob[2],
            t.knob[3],
            t.menu_item,
            t.menu_value,
        ];
        for c in on_bg {
            assert!(u32::from(c) < 16, "theme {id}: {c}");
            assert_ne!(
                c, t.bg,
                "theme {id}: a role that vanishes into the background"
            );
        }
        // What the current row of TRACKER draws over `cursor`.
        let on_cursor = [
            t.value, t.hot, t.voice[0], t.voice[1], t.voice[2], t.label, t.alert,
        ];
        for c in on_cursor {
            assert_ne!(
                c, t.cursor,
                "theme {id}: a role that vanishes on the cursor row"
            );
        }
        for (fg, bg) in [
            (t.toast_fg, t.toast_bg),
            (t.title_fg, t.title_bg),
            (t.sel_fg, t.sel_bg),
        ] {
            assert!(u32::from(fg) < 16 && u32::from(bg) < 16);
            assert_ne!(fg, bg, "theme {id}: an unreadable pair");
        }
        assert!(!t.name.is_null() && !t.menu_name.is_null());
    }
}

#[test]
fn the_c64_theme_keeps_blue_and_readable_reverse_video() {
    let t = theme(THEME_C64);
    assert_eq!(u32::from(t.bg), GRID_BLUE);
    assert_eq!(u32::from(t.label), GRID_CYAN);
    assert_eq!(
        (u32::from(t.toast_fg), u32::from(t.toast_bg)),
        (GRID_BLUE, GRID_WHITE)
    );
    assert_eq!(
        (u32::from(t.title_fg), u32::from(t.title_bg)),
        (GRID_BLACK, GRID_LIGHT_BLUE)
    );
    assert_eq!(u32::from(t.popup_frame), GRID_LIGHT_BLUE);
    assert_eq!(u32::from(t.popup_name), GRID_YELLOW);
    assert_eq!(u32::from(t.popup_bar), GRID_LIGHT_GREEN);
    assert_eq!(u32::from(t.popup_track), GRID_DARK_GREY);
    // Cyan and light green retain their voice and knob accents on blue.
    let hc = theme(THEME_HIGH_CONTRAST);
    assert_eq!(t.voice[1..], hc.voice[1..]);
    assert_eq!(t.knob[..3], hc.knob[..3]);
}

fn to_theme_item(ui: &mut Ui) {
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_THEME as i32);
}

#[test]
fn the_menu_item_turns_the_theme_and_the_menu_follows_it() {
    let mut ui = Ui::without_cards();
    ui.settle();
    to_theme_item(&mut ui);
    ui.settle();
    assert_eq!(ui.menu_line(VIEWS_MENU_THEME as usize), "THEME     HIGH");
    // High contrast: the title bar grey on black, the selected line yellow on dark grey, the
    // values cyan.
    let hc = theme(THEME_HIGH_CONTRAST);
    assert_eq!(fg(ui.g.attr[COLS + 1]), hc.title_fg);
    assert_eq!(bg(ui.g.attr[COLS + 1]), hc.title_bg);
    let item_row = 4 + 3 * (VIEWS_MENU_THEME as usize - usize::from(ui.v.home.top));
    let sel = ui.g.attr[item_row * COLS + 2];
    assert_eq!((fg(sel), bg(sel)), (hc.sel_fg, hc.sel_bg));
    // Clockwise: C64, saved dirty, every view in its old colours.
    ui.rig.live.settings_dirty = 0;
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    assert_eq!(u32::from(ui.live().theme), THEME_C64);
    assert_eq!(
        ui.live().settings_dirty,
        1,
        "marked for the settings' debounce"
    );
    assert_eq!(ui.menu_line(VIEWS_MENU_THEME as usize), "THEME      C64");
    let c64 = theme(THEME_C64);
    assert_eq!(fg(ui.g.attr[COLS + 1]), c64.title_fg);
    assert_eq!(bg(ui.g.attr[COLS + 1]), c64.title_bg);
    let sel = ui.g.attr[item_row * COLS + 2];
    assert_eq!((fg(sel), bg(sel)), (c64.sel_fg, c64.sel_bg));
    // Counter-clockwise: back; and it stops there (a value item turns both ways).
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    ui.settle();
    assert_eq!(u32::from(ui.live().theme), THEME_HIGH_CONTRAST);
    ui.turn(PANEL_ENC_ALGORITHM, -3);
    ui.settle();
    assert_eq!(u32::from(ui.live().theme), THEME_HIGH_CONTRAST);
}

#[test]
fn the_popup_names_the_theme_in_full() {
    let mut ui = Ui::without_cards();
    ui.settle();
    to_theme_item(&mut ui);
    ui.settle();
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    for _ in 0..4 {
        ui.frame();
    }
    let row = |ui: &Ui, r: usize| ui.row(r)[3..].split('#').next().unwrap().trim().to_owned();
    assert_eq!(row(&ui, SCREEN_POPUP_ROW as usize + 1), "THEME");
    assert_eq!(row(&ui, SCREEN_POPUP_ROW as usize + 4), "C64");
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    for _ in 0..4 {
        ui.frame();
    }
    assert_eq!(row(&ui, SCREEN_POPUP_ROW as usize + 4), "HI-CONTRAST");
}

#[test]
fn the_popup_bar_and_name_take_the_knobs_colour_in_both_themes() {
    let bar_row = SCREEN_POPUP_ROW as usize + 7;
    let name_row = SCREEN_POPUP_ROW as usize + 1;
    for id in 0..THEME_COUNT {
        for k in 0..4u32 {
            let mut ui = Ui::without_cards();
            unsafe { live_set_theme(&mut *ui.rig.live, id) };
            ui.settle();
            // STATUS keeps its popups (LIVE and CHIP highlight the knob instead).
            open_status(&mut ui);
            ui.settle();
            let t = theme(id);
            // K1..K3 turn a parameter with a bar; K4 the preset, which has none.
            ui.turn(PANEL_ENC_K1 + k, 1);
            for _ in 0..6 {
                ui.frame();
            }
            let accent = t.knob[k as usize];
            let name = ui.g.attr[name_row * COLS + 3];
            let want_name = if t.popup_name == THEME_ACCENT as u8 {
                accent
            } else {
                t.popup_name
            };
            assert_eq!(fg(name), want_name, "theme {id}, K{}", k + 1);
            if k < 3 {
                let bar = ui.g.attr[bar_row * COLS + 3];
                let want = if t.popup_bar == THEME_ACCENT as u8 {
                    accent
                } else {
                    t.popup_bar
                };
                assert_eq!(fg(bar), want, "theme {id}, K{}", k + 1);
                assert_eq!(bg(bar), t.popup_track);
            }
        }
    }
}

#[test]
fn the_toast_takes_the_themes_colours() {
    for id in 0..THEME_COUNT {
        let mut ui = Ui::without_cards();
        unsafe { live_set_theme(&mut *ui.rig.live, id) };
        ui.settle();
        unsafe { screen_toast(&mut ui.v.s, c"HELLO".as_ptr(), 600) };
        ui.frame();
        ui.frame();
        let t = theme(id);
        let a = ui.g.attr[SCREEN_TOAST_ROW as usize * COLS + SCREEN_TOAST_COL as usize];
        assert_eq!((fg(a), bg(a)), (t.toast_fg, t.toast_bg), "theme {id}");
    }
}
