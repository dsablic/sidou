// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::{c_char, c_void, CStr, CString};

use sidou_sys::*;

use crate::drain;
use crate::views::{ascii_or_hash, open_setting, open_status, settings_settle, Ui, FRAME_MS};

const ROW: usize = VIEWS_SETTINGS_BLUETOOTH as usize;
const LINE_CHARS: usize = VIEWS_DEVICE_LINE as usize;
const TITLE_ROW: usize = 1;
const HELP_ROW: usize = 24;
const SEARCH_MS: u32 = 8000;
const LIT: [u8; 3] = [FONT_V2 as u8, FONT_V5 as u8, FONT_V8 as u8];
const UNLIT: u8 = FONT_V1 as u8;

#[derive(Default)]
struct Radio {
    mode: i32,
    state: u32,
    name: CString,
    devices: Vec<(CString, u8)>,
    fresh: u32,
    picked: Option<u32>,
}

fn of<'a>(ctx: *mut c_void) -> &'a mut Radio {
    unsafe { &mut *ctx.cast::<Radio>() }
}

unsafe extern "C" fn mode(ctx: *mut c_void) -> i32 {
    of(ctx).mode
}

unsafe extern "C" fn set(ctx: *mut c_void, m: i32) {
    of(ctx).mode = m;
}

unsafe extern "C" fn report(_: *mut c_void, buf: *mut c_char) {
    let text = c"P 03 0B 07 0F 01 01 06";
    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), buf, text.count_bytes() + 1) };
}

unsafe extern "C" fn keyboard(ctx: *mut c_void, name: *mut *const c_char) -> u32 {
    let r = of(ctx);
    unsafe { *name = r.name.as_ptr() };
    r.state
}

unsafe extern "C" fn devices(
    ctx: *mut c_void,
    fresh: i32,
    names: *mut *const c_char,
    bars: *mut u8,
) -> u32 {
    let r = of(ctx);
    r.fresh += u32::from(fresh != 0);
    for (i, (name, b)) in r.devices.iter().enumerate() {
        unsafe {
            *names.add(i) = name.as_ptr();
            *bars.add(i) = *b;
        }
    }
    r.devices.len() as u32
}

unsafe extern "C" fn pick(ctx: *mut c_void, i: u32) {
    of(ctx).picked = Some(i);
}

struct Bound {
    ui: Ui,
    radio: Box<Radio>,
    _table: Box<views_radio>,
}

fn bound() -> Bound {
    let mut ui = Ui::new();
    let mut radio = Box::<Radio>::default();
    let table = Box::new(views_radio {
        ctx: std::ptr::from_mut(&mut *radio).cast(),
        mode: Some(mode),
        set: Some(set),
        report: Some(report),
        keyboard: Some(keyboard),
        devices: Some(devices),
        pick: Some(pick),
    });
    unsafe { views_radio_bind(&mut *ui.v, &*table) };
    Bound {
        ui,
        radio,
        _table: table,
    }
}

fn row_text(b: &mut Bound) -> String {
    b.ui.frame();
    settings_settle(&mut b.ui);
    b.ui.menu_line(ROW)
}

fn popup(ui: &Ui) -> (String, String) {
    let text = |t: &[c_char]| {
        unsafe { CStr::from_ptr(t.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    };
    (text(&ui.v.popup.name), text(&ui.v.popup.value))
}

fn keyboard_with(b: &mut Bound, state: u32, name: &str) {
    b.radio.mode = VIEWS_RADIO_KEYBOARD as i32;
    b.radio.state = state;
    b.radio.name = CString::new(name).unwrap();
}

fn open_devices(b: &mut Bound, listed: &[(&str, u8)]) {
    keyboard_with(b, VIEWS_KEYBOARD_LOOKING, "");
    b.radio.devices = listed
        .iter()
        .map(|(n, bars)| (CString::new(*n).unwrap(), *bars))
        .collect();
    open_setting(&mut b.ui, VIEWS_SETTINGS_BLUETOOTH);
    b.ui.press(SAVE_KEY_BUTTON);
    b.ui.settle();
    assert_eq!(b.ui.top(), "DEVICES");
    devices_settle(&mut b.ui);
}

fn devices_settle(ui: &mut Ui) {
    for _ in 0..20 {
        ui.frame();
        drain(&mut ui.g);
        if ui.v.devices.pending == 0 {
            return;
        }
    }
    panic!("DEVICES kept lines pending");
}

fn device_line(b: &Bound, i: usize) -> Vec<u8> {
    let row = 4 + 3 * i;
    (0..LINE_CHARS)
        .map(|c| b.ui.g.src[row * GRID_COLS as usize + 1 + 2 * c])
        .collect()
}

fn help(b: &Bound) -> (String, String) {
    (
        b.ui.big(HELP_ROW, 1, 2, 12),
        b.ui.big(HELP_ROW + 2, 1, 2, 12),
    )
}

#[test]
fn bluetooth_is_the_last_setting_only_with_a_radio_and_algorithm_steps_off_on_keyboard() {
    let mut ui = Ui::new();
    open_setting(&mut ui, VIEWS_SETTINGS_RESET);
    assert_eq!(
        u32::from(ui.v.setup.count),
        VIEWS_SETTINGS_ITEMS,
        "no radio, no row"
    );
    let mut b = bound();
    open_setting(&mut b.ui, VIEWS_SETTINGS_BLUETOOTH);
    let heading = ascii_or_hash(b.ui.big(TITLE_ROW, 1, 2, LINE_CHARS));
    let items = VIEWS_SETTINGS_ITEMS + 1;
    assert!(
        heading.starts_with("SETTINGS") && heading.ends_with(&format!("{items}/{items}")),
        "{heading}"
    );
    assert_eq!(row_text(&mut b), format!("{:<9}{:>5}", "BLUETOOTH", "OFF"));
    for (want, row) in [
        (VIEWS_RADIO_ON, format!("{:<9}{:>5}", "BLUETOOTH", "ON")),
        (VIEWS_RADIO_KEYBOARD, format!("{:<14}", "KEYBOARD NONE")),
        (VIEWS_RADIO_KEYBOARD, format!("{:<14}", "KEYBOARD NONE")),
    ] {
        b.ui.turn(PANEL_ENC_ALGORITHM, 1);
        b.ui.frame();
        assert_eq!(b.radio.mode, want as i32);
        let word = ["OFF", "ON", "KEYBOARD"][want as usize];
        assert_eq!(popup(&b.ui), ("BLUETOOTH".into(), word.into()), "the popup");
        assert_eq!(row_text(&mut b), row);
    }
    for want in [VIEWS_RADIO_ON, VIEWS_RADIO_OFF, VIEWS_RADIO_OFF] {
        b.ui.turn(PANEL_ENC_ALGORITHM, -1);
        assert_eq!(
            b.radio.mode, want as i32,
            "a detent back at a time, OFF the first"
        );
    }
}

#[test]
fn with_keyboard_the_row_says_none_looking_the_keyboards_name_or_needs_pairing() {
    let mut b = bound();
    open_setting(&mut b.ui, VIEWS_SETTINGS_BLUETOOTH);
    for (state, row) in [
        (VIEWS_KEYBOARD_NONE, "KEYBOARD NONE"),
        (VIEWS_KEYBOARD_LOOKING, "LOOKING"),
        (VIEWS_KEYBOARD_CONNECTED, "A KEYBOARD NAME!"),
        (VIEWS_KEYBOARD_PAIRING, "NEEDS PAIRING"),
    ] {
        keyboard_with(&mut b, state, "A KEYBOARD NAME!");
        assert_eq!(
            row_text(&mut b).trim_end(),
            &row[..row.len().min(LINE_CHARS)],
            "state {state}"
        );
    }
}

#[test]
fn save_on_the_row_opens_devices_only_with_keyboard() {
    let mut b = bound();
    open_setting(&mut b.ui, VIEWS_SETTINGS_BLUETOOTH);
    for m in [VIEWS_RADIO_OFF, VIEWS_RADIO_ON] {
        b.radio.mode = m as i32;
        b.ui.press(SAVE_KEY_BUTTON);
        b.ui.settle();
        assert_eq!(b.ui.top(), "SETTINGS", "mode {m}");
    }
    open_devices(&mut b, &[]);
    assert_eq!(b.radio.fresh, 1, "a fresh scan as it opens");
}

#[test]
fn devices_lists_names_with_bars_moves_with_select_or_algorithm_and_save_connects_the_chosen() {
    let mut b = bound();
    open_devices(
        &mut b,
        &[("microKEY Air", 3), ("PADS", 1), ("MIDI 3F2A", 2)],
    );
    let line = |name: &str, bars: usize| -> Vec<u8> {
        let mut l = format!("{name:<11}").into_bytes();
        l.extend((0..3).map(|k| if k < bars { LIT[k] } else { UNLIT }));
        l
    };
    assert_eq!(device_line(&b, 0), line("microKEY A", 3), "cut to ten");
    assert_eq!(device_line(&b, 1), line("PADS", 1));
    assert_eq!(device_line(&b, 2), line("MIDI 3F2A", 2));
    assert!(
        ascii_or_hash(b.ui.big(TITLE_ROW, 1, 2, LINE_CHARS)).starts_with("DEVICES"),
        "the title"
    );
    assert_eq!(help(&b), ("SELECT  MOVE".into(), "SAVE CONNECT".into()));
    b.ui.turn(PANEL_ENC_SELECT, 1);
    b.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(b.ui.v.devices.cursor, 2);
    b.ui.press(SAVE_KEY_BUTTON);
    b.ui.settle();
    assert_eq!(b.radio.picked, Some(2));
    assert_eq!(b.ui.top(), "SETTINGS");
}

#[test]
fn home_backs_out_of_devices_unchanged() {
    let mut b = bound();
    open_devices(&mut b, &[("microKEY Air", 3)]);
    b.ui.press(PANEL_BTN_HOME);
    b.ui.settle();
    assert_eq!(b.ui.top(), "SETTINGS");
    assert_eq!(b.radio.picked, None);
}

#[test]
fn an_empty_list_scans_then_says_none_found_and_save_scans_again() {
    let mut b = bound();
    open_devices(&mut b, &[]);
    let says = |b: &Bound| {
        (
            b.ui.big(4, 1, 2, 12).trim_end().to_owned(),
            b.ui.big(7, 1, 2, 12).trim_end().to_owned(),
        )
    };
    assert_eq!(says(&b), ("SCANNING".into(), String::new()));
    assert_eq!(help(&b), ("SAVE    SCAN".into(), "HOME    BACK".into()));
    b.ui.frames(SEARCH_MS / FRAME_MS);
    assert_eq!(says(&b), ("NO KEYBOARDS".into(), "FOUND".into()));
    b.ui.press(SAVE_KEY_BUTTON);
    b.ui.frames(2);
    assert_eq!(b.radio.fresh, 2, "SAVE scans again");
    assert_eq!(says(&b), ("SCANNING".into(), String::new()));
    assert_eq!(b.ui.top(), "DEVICES");
}

#[test]
fn the_keyboards_events_are_toasts() {
    let mut b = bound();
    for (event, toast) in [
        (VIEWS_KEYBOARD_CONNECTED, "KEYBOARD microKEY Air"),
        (VIEWS_KEYBOARD_LOST, "KEYBOARD LOST"),
        (VIEWS_KEYBOARD_PAIRING, "NEEDS PAIRING"),
    ] {
        unsafe { views_keyboard_event(&mut *b.ui.v, event, c"microKEY Air".as_ptr()) };
        assert_eq!(b.ui.toast().as_deref(), Some(toast));
    }
}

#[test]
fn a_radio_build_shows_its_power_report_where_status_says_ready() {
    let mut b = bound();
    open_status(&mut b.ui);
    assert_eq!(&b.ui.row(28)[2..24], "P 03 0B 07 0F 01 01 06");
}
