// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's views and the panel's routing through them (app/views.c): HOME and the menu, the
//! M1a and M1c panel mappings in every view, and the toasts and raster of what changed.

use std::ffi::CStr;

use sidou_sys::*;

use crate::Rig;

const COLS: usize = GRID_COLS as usize;

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

    /// Presses button b (and lets it go, unless it is held with hold()).
    pub(crate) fn press(&mut self, b: u32) {
        let held = self.held | (1 << b);
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 1 << b, held) };
        unsafe { views_buttons(&mut *self.v, &mut *self.p, 0, self.held) };
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

    /// Frames until no transition runs.
    pub(crate) fn settle(&mut self) {
        for _ in 0..40 {
            self.frame();
            if unsafe { screen_animating(&self.v.s) } == 0 {
                return;
            }
        }
        panic!("the screen kept animating");
    }

    pub(crate) fn top(&self) -> String {
        let v = unsafe { &*screen_top(&self.v.s) };
        unsafe { CStr::from_ptr(v.name) }
            .to_string_lossy()
            .into_owned()
    }

    pub(crate) fn row(&self, row: usize) -> String {
        (0..COLS)
            .map(|c| self.g.ch[row * COLS + c])
            .map(|b| {
                if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    '#'
                }
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
    ui.turn(PANEL_ENC_SELECT, 2); // past TRACKER, which ALGORITHM would open
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
    ui.turn(PANEL_ENC_SELECT, 9);
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(ui.top(), "MENU");
}

#[test]
fn the_menu_draws_its_items_in_a_frame_with_a_blinking_cursor() {
    let mut ui = Ui::new();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    let rows: Vec<String> = (4..19).map(|r| ui.row(r)).collect();
    let body: Vec<String> = rows.iter().map(|r| r[3..27].to_string()).collect();
    assert_eq!(rows[0][2..10].to_string(), "# MENU #");
    // Labels after the cursor cell, values right-aligned against the frame, an arrow for a view.
    assert_eq!(body[1][..23], format!(" {:<22}", "CHIP"));
    // M2d Task 1: TRACKER, the song views.
    assert_eq!(body[2][..23], format!(" {:<22}", "TRACKER"));
    assert_eq!(body[3], format!(" {:<15}SAW LEAD", "PRESET"));
    assert_eq!(body[4], format!(" {:<21}+0", "OCTAVE"));
    assert_eq!(body[5], format!(" {:<21}ON", "CARDS"));
    // M2a Task 5: STORE, the store's free space (NONE: no store in these views).
    assert_eq!(body[6], format!(" {:<19}NONE", "STORE"));
    // M2b Task 4: KEYS, which chip the keys play during a song (AUTO at power-on).
    assert_eq!(body[7], format!(" {:<19}AUTO", "KEYS"));
    // M2b Task 7: SOUNDS, the UI sounds (OFF at power-on).
    assert_eq!(body[8], format!(" {:<20}OFF", "SOUNDS"));
    // M2d Task 2: TRACKER ROWS, the TRACKER's moving bar or scrolling (BAR at power-on).
    assert_eq!(body[9], format!(" {:<20}BAR", "TRACKER ROWS"));
    assert_eq!(body[10][..23], format!(" {:<22}", "STATUS"));
    assert_eq!(body[11][..23], format!(" {:<22}", "SCENE"));
    assert_eq!(body[12][..23], format!(" {:<22}", "CHARSET"));
    assert_eq!(body[13][..23], format!(" {:<22}", "ABOUT"));
    for row in [5, 6, 14, 15, 16, 17] {
        assert_eq!(
            ui.g.ch[row * COLS + 26],
            FONT_ARROW_RIGHT as u8,
            "row {row}"
        );
    }
    assert_eq!(
        rows[14][2..26].chars().filter(|&c| c == '#').count(),
        24,
        "the frame's bottom"
    );
    // The cursor: a block on the selected row for 8 frames, then a space for 8.
    let cursor = |ui: &Ui| ui.g.ch[5 * COLS + 3];
    let mut seen = Vec::new();
    for _ in 0..16 {
        ui.frame();
        seen.push(cursor(&ui));
    }
    assert!(seen.contains(&(FONT_FULL as u8)) && seen.contains(&b' '));
    ui.turn(PANEL_ENC_SELECT, 2);
    for _ in 0..16 {
        ui.frame();
        assert_eq!(cursor(&ui), b' ', "the cursor stayed on CHIP");
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
        l0.2 - 2 * LIVE_CUTOFF_DETENT as u16,
        "{view}: K1"
    );
    ui.turn(PANEL_ENC_K1, 2);
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
        ui.turn(PANEL_ENC_K1 + k as u32, 1);
        assert_eq!(
            ui.live().slot[s][field],
            before + LIVE_SLOT_DETENT as u8,
            "{view}: EDIT + K{}",
            k + 1
        );
    }
    let cutoff = ui.live().cutoff;
    assert_eq!(
        cutoff, l0.2,
        "{view}: K1 moved the cutoff while EDIT was held"
    );
    ui.hold(PANEL_BTN_EDIT, false);
    ui.turn(PANEL_ENC_K1, -1);
    assert_eq!(ui.live().cutoff, cutoff - LIVE_CUTOFF_DETENT as u16);
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
    assert_eq!(&ui.row(2)[2..7], "SIDOU");
    assert_eq!(
        &ui.row(2)[22..28],
        "V1.2.3",
        "the version ends before column 28"
    );
    assert_eq!(&ui.row(5)[2..13], "CHIP   6581");
    ui.press(PANEL_BTN_ENV);
    ui.frame();
    assert_eq!(&ui.row(19)[2..12], ">DIST TAPE");
    assert_eq!(&ui.row(26)[2..8], "READY.");
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
    assert_eq!(&ui.row(0)[1..5], "LIVE");
    ui.rig.live.keys_chip = 1;
    ui.frame();
    assert_eq!(
        &ui.row(0)[1..7],
        "KEYS:B",
        "LIVE's header with the keys on chip B"
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
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_KEYS as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(u32::from(ui.live().keys_mode), LIVE_KEYS_A);
    ui.settle();
    assert_eq!(
        ui.row(11)[4..27],
        format!("{:<22}A", "KEYS"),
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
    // Whatever the status screen shows: a note, a knob, a slot field.
    type Change = fn(&mut Ui);
    let changes: [(&str, Change); 4] = [
        ("note on", |ui| ui.rig.note_on(60)),
        ("note off", |ui| ui.rig.note_off(60)),
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
}

#[test]
fn the_menu_composes_when_its_cursor_blinks_or_moves() {
    let mut ui = Ui::new();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    let mut composes = 0;
    for _ in 0..4 * SCREEN_BLINK_FRAMES {
        mark(&mut ui);
        ui.frame();
        composes += usize::from(composed(&ui));
    }
    assert!(
        (3..=5).contains(&composes),
        "{composes} composes in 4 blinks"
    );
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
        assert_eq!(&ui.row(26)[2..8], "      ", "READY. under the box");
    }
    // Back in the main loop the next frame puts the view back.
    ui.frame();
    assert_eq!(&ui.row(26)[2..8], "READY.");
    assert_eq!(&ui.row(20)[3..6], "MOD");
}
