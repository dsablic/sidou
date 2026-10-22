// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SAVE's gesture (`app/save_key.c`, M2a Task 5) and its routing in the views (`app/views.c`):
//! a clean press and release saves (the settings, now, anywhere but in MIDI learn's frame;
//! the CHIP view's USER slots and the song views come with Task 6 and M2d), a knob turned or
//! another button pressed while SAVE is down spoils that save; and the menu's STORE.

use sidou_sys::*;

use crate::store::{fresh, Rig as Store};
use crate::views::Ui;

const SAVE: u32 = SAVE_KEY_BUTTON;
const K1: u32 = PANEL_ENC_K1;

fn key() -> save_key {
    let mut k = save_key::default();
    unsafe { save_key_init(&mut k) };
    k
}

fn buttons(k: &mut save_key, pressed: u32, held: u32) -> save_key_action {
    unsafe { save_key_buttons(k, pressed, held) }
}

#[test]
fn a_clean_press_and_release_saves_at_the_release() {
    let mut k = key();
    let s = 1 << SAVE;
    assert_eq!(buttons(&mut k, s, s), SAVE_KEY_NONE, "not at the press");
    assert_eq!(unsafe { save_key_held(&k) }, 1);
    assert_eq!(buttons(&mut k, 0, s), SAVE_KEY_NONE, "nor while held");
    assert_eq!(buttons(&mut k, 0, 0), SAVE_KEY_SAVE);
    assert_eq!(unsafe { save_key_held(&k) }, 0);
    assert_eq!(buttons(&mut k, 0, 0), SAVE_KEY_NONE, "once");
    // A press and release within one input frame saves too.
    assert_eq!(buttons(&mut k, s, 0), SAVE_KEY_SAVE);
}

#[test]
fn another_button_or_a_knob_while_save_is_down_spoils_the_save() {
    let s = 1 << SAVE;
    let oct = 1 << PANEL_BTN_OCT_UP;
    let mut k = key();
    buttons(&mut k, s, s);
    buttons(&mut k, oct, s | oct);
    assert_eq!(buttons(&mut k, 0, 0), SAVE_KEY_NONE, "another button");
    // Pressed in the same frame as SAVE.
    buttons(&mut k, s | oct, s | oct);
    assert_eq!(buttons(&mut k, 0, 0), SAVE_KEY_NONE, "in the same frame");
    let mut k = key();
    buttons(&mut k, s, s);
    unsafe { save_key_spoil(&mut k) };
    assert_eq!(buttons(&mut k, 0, 0), SAVE_KEY_NONE, "a knob");
    // The next press is clean again; a spoil while SAVE is up spoils nothing.
    unsafe { save_key_spoil(&mut k) };
    buttons(&mut k, s, s);
    assert_eq!(buttons(&mut k, 0, 0), SAVE_KEY_SAVE);
    // Another button held from before SAVE's press, let go meanwhile: no spoil (only presses).
    buttons(&mut k, s, s | oct);
    buttons(&mut k, 0, s);
    assert_eq!(buttons(&mut k, 0, 0), SAVE_KEY_SAVE);
}

/// The views with the settings on a store, MIDI learn, and the store's polls between frames.
struct Saving {
    ui: Ui,
    st: Store,
    set: Box<settings>,
    _ml: Box<midi_learn>,
    forced: bool,
    /// The views' popup source, kept aside (the popup is off for these tests).
    popup: screen_popup_fn,
}

unsafe extern "C" fn force(on: i32) {
    FORCE.with(|f| f.set(on != 0));
}

thread_local! {
    static FORCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl Saving {
    fn on(st: Store) -> Self {
        let mut ui = Ui::without_cards();
        // These tests read the menu's cells; the value popup (views.rs, screen.rs) would sit on
        // them.
        let popup = ui.v.popup.source.take();
        let mut set = Box::<settings>::default();
        let mut ml = Box::<midi_learn>::default();
        unsafe {
            midi_learn_init(&mut *ml);
            views_learn(&mut *ui.v, &mut *ml);
        }
        let mut st = st;
        unsafe {
            settings_init(
                &mut *set,
                &mut *ui.rig.live,
                &mut *ml,
                &mut ui.v.cards,
                &mut ui.v.song.scroll,
                std::ptr::null_mut(),
                &mut *st.st,
                c"V1.2.3".as_ptr(),
                Some(force),
            );
            settings_load(&mut *set, ui.now);
            views_settings(&mut *ui.v, &mut *set);
        }
        FORCE.with(|f| f.set(false));
        let mut s = Saving {
            ui,
            st,
            set,
            _ml: ml,
            forced: false,
            popup,
        };
        s.settle();
        s
    }

    fn new() -> Self {
        Self::on(fresh(5, 256))
    }

    /// A frame (40 ms) with the store's polls before it, quiet.
    fn frame(&mut self) {
        unsafe { store_quiet(&mut *self.st.st, 1) };
        for _ in 0..256 {
            if unsafe { store_poll(&mut *self.st.st) } == 0 {
                break;
            }
        }
        self.ui.frame();
        self.forced = FORCE.with(std::cell::Cell::get);
    }

    fn settle(&mut self) {
        for _ in 0..40 {
            self.frame();
        }
    }

    fn stored(&self) -> Option<Vec<u8>> {
        self.st.get(SETTINGS_KEY)
    }

    fn gather(&self) -> Vec<u8> {
        let mut out = vec![0u8; SETTINGS_BYTES as usize];
        let n = unsafe { settings_gather(&*self.set, out.as_mut_ptr()) };
        out.truncate(n as usize);
        out
    }

    /// SAVE's press and release; then frames until the toast or a second.
    fn save(&mut self) -> Option<String> {
        self.ui.press(SAVE);
        for _ in 0..25 {
            self.frame();
            if let Some(t) = self.ui.toast() {
                return Some(t);
            }
        }
        None
    }

    /// The menu's STORE item's value.
    fn store_item(&mut self) -> String {
        self.ui
            .menu_line(VIEWS_MENU_STORE as usize)
            .trim()
            .to_string()
    }

    /// The value the popup shows for ALGORITHM on the menu's item under the cursor.
    fn store_popup(&mut self) -> String {
        let mut name = [0 as std::ffi::c_char; SCREEN_POPUP_TEXT as usize];
        let mut value = [0 as std::ffi::c_char; SCREEN_POPUP_TEXT as usize];
        let (mut fill, mut max, mut accent) = (0, 0, 0);
        let ctx: *mut views = &mut *self.ui.v;
        let shown = unsafe {
            (self.popup.unwrap())(
                ctx.cast(),
                PANEL_ENC_ALGORITHM,
                name.as_mut_ptr(),
                value.as_mut_ptr(),
                &mut fill,
                &mut max,
                &mut accent,
            )
        };
        assert_eq!(shown, 1);
        unsafe { std::ffi::CStr::from_ptr(value.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    fn open_store_item(&mut self) {
        self.ui.press(PANEL_BTN_HOME);
        self.ui.settle();
        assert_eq!(self.ui.top(), "MENU");
        self.ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
        self.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_STORE as i32);
        self.frame();
    }
}

#[test]
fn a_tracker_rows_change_alone_is_background_saved_after_five_seconds_and_loaded_at_boot() {
    let mut s = Saving::new();
    for _ in 0..150 {
        s.frame();
    }
    let before = s.stored().expect("the boot's settings saved");
    assert_eq!(s.ui.rig.live.settings_dirty, 0);
    s.ui.press(PANEL_BTN_HOME);
    s.ui.settle();
    s.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_ROWS as i32);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(s.ui.v.song.scroll, 1);
    s.frame();
    let changed_at = s.set.changed_at;
    while s.ui.now - changed_at < SETTINGS_DEBOUNCE_MS {
        assert_eq!(s.stored(), Some(before.clone()), "not before five seconds");
        s.frame();
    }
    s.frame();
    s.frame();
    assert_eq!(s.stored(), Some(s.gather()), "TRACKER ROWS alone must save");
    assert_ne!(s.stored(), Some(before));
    let mut st = s.st;
    st.reset();
    st.mount();
    let mut reboot = Saving::on(st);
    reboot.ui.press(PANEL_BTN_SEQ);
    reboot.ui.press(PANEL_BTN_SEQ);
    reboot.ui.settle();
    assert_eq!(reboot.ui.top(), "TRACKER");
    assert_eq!(reboot.ui.v.song.scroll, 1, "opens in the loaded mode");
    assert_eq!(
        &reboot.ui.row(14)[..2],
        "00",
        "SCROLL puts row 0 at screen row 14"
    );
}

#[test]
fn a_theme_change_alone_is_background_saved_after_five_seconds_and_loaded_at_boot() {
    let mut s = Saving::new();
    for _ in 0..150 {
        s.frame();
    }
    let before = s.stored().expect("the boot's settings saved");
    assert_eq!(s.ui.rig.live.settings_dirty, 0);
    assert_eq!(
        s.ui.rig.live.theme, THEME_HIGH_CONTRAST as u8,
        "the default"
    );
    s.ui.press(PANEL_BTN_HOME);
    s.ui.settle();
    s.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_THEME as i32);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(s.ui.rig.live.theme, THEME_C64 as u8);
    s.frame();
    let changed_at = s.set.changed_at;
    while s.ui.now - changed_at < SETTINGS_DEBOUNCE_MS {
        assert_eq!(s.stored(), Some(before.clone()), "not before five seconds");
        s.frame();
    }
    s.frame();
    s.frame();
    assert_eq!(s.stored(), Some(s.gather()), "THEME alone must save");
    assert_ne!(s.stored(), Some(before));
    let mut st = s.st;
    st.reset();
    st.mount();
    let reboot = Saving::on(st);
    assert_eq!(
        reboot.ui.rig.live.theme, THEME_C64 as u8,
        "the theme comes back at boot"
    );
}

#[test]
fn a_learned_binding_alone_is_background_saved_through_views_frame() {
    let mut s = Saving::new();
    s.ticks_for_the_debounce();
    let before = s.stored().expect("the initial settings saved");
    assert_eq!(s.ui.live().settings_dirty, 0, "no other pending change");
    s.ui.learn_on(K1);
    assert_eq!(s.ui.top(), "LEARN");
    // Route the CC through learn without a parameter setter: only the binding changes.
    let entry = unsafe { midi_learn_route(&mut *s._ml, 21) };
    assert!(!entry.is_null());
    assert_eq!(unsafe { (*entry).param } as u32, LIVE_PARAM_CUTOFF);
    let want = s.gather();
    assert_eq!(
        before.iter().zip(&want).filter(|(a, b)| a != b).count(),
        1,
        "only the learned binding changed"
    );
    s.frame();
    assert_eq!(s.ui.top(), "LIVE", "learn closed after the CC");
    let changed_at = s.ui.now;
    while s.ui.now - changed_at < SETTINGS_DEBOUNCE_MS {
        assert_eq!(s.stored(), Some(before.clone()), "not before 5 s");
        s.frame();
    }
    s.frame(); // the store polls commit the record submitted at 5 s
    assert_eq!(s.stored(), Some(want));
}

#[test]
fn a_menu_cards_change_is_background_saved_without_save() {
    let mut s = Saving::new();
    for _ in 0..150 {
        s.frame();
    }
    let before = s.stored();
    s.ui.press(PANEL_BTN_HOME);
    s.ui.settle();
    s.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_CARDS as i32);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    for _ in 0..120 {
        s.frame();
    }
    assert_eq!(s.stored(), before, "the menu edit must debounce");
    for _ in 0..10 {
        s.frame();
    }
    assert_ne!(s.stored(), before);
    assert_eq!(s.stored(), Some(s.gather()));
}

#[test]
fn saves_press_saves_the_settings_now_on_live_and_says_saved() {
    let mut s = Saving::new();
    let first = s.stored();
    s.ui.v.cards = 1 - s.ui.v.cards;
    assert_eq!(s.save().as_deref(), Some("SAVED"));
    assert_ne!(s.stored(), first);
    assert_eq!(
        s.stored(),
        Some(s.gather()),
        "within a frame or two, not after 5 s"
    );
    assert!(!s.forced);
}

#[test]
fn saves_press_saves_the_settings_in_the_menu_scene_status_and_chip() {
    for (view, open) in [
        ("MENU", None),
        ("SCENE", Some(VIEWS_MENU_SCENE)),
        ("STATUS", Some(VIEWS_MENU_STATUS)),
        // A built-in profile and no save target (USER slots: Task 6).
        ("CHIP", Some(VIEWS_MENU_CHIP)),
    ] {
        let mut s = Saving::new();
        s.ui.press(PANEL_BTN_HOME);
        s.ui.settle();
        if let Some(item) = open {
            s.ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
            s.ui.turn(PANEL_ENC_SELECT, item as i32);
            s.ui.turn(PANEL_ENC_ALGORITHM, 1);
            s.ui.settle();
        }
        assert_eq!(s.ui.top(), view);
        s.ui.rig.live.preset = 3; // a change only an explicit save stores before 5 s
        assert_eq!(s.save().as_deref(), Some("SAVED"), "{view}");
        assert_eq!(s.stored(), Some(s.gather()), "{view}");
    }
}

#[test]
fn a_knob_turned_while_save_is_down_saves_nothing() {
    let mut s = Saving::new();
    let first = s.stored();
    s.ui.v.cards = 0;
    // With learn: the knob opens it; HOME cancels it.
    s.ui.down(SAVE);
    s.ui.turn(K1, 1);
    s.ui.up(SAVE);
    assert_eq!(s.ui.top(), "LEARN");
    s.ui.press(PANEL_BTN_HOME);
    // Without learn: the knob turns as usual.
    unsafe { views_learn(&mut *s.ui.v, std::ptr::null_mut()) };
    let cutoff = s.ui.live().cutoff;
    s.ui.down(SAVE);
    s.ui.turn(K1, 1);
    s.ui.up(SAVE);
    assert_ne!(s.ui.live().cutoff, cutoff);
    // SELECT too.
    s.ui.down(SAVE);
    s.ui.turn(PANEL_ENC_SELECT, 1);
    s.ui.up(SAVE);
    for _ in 0..10 {
        s.frame();
        assert_ne!(s.ui.toast().as_deref(), Some("SAVED"));
    }
    assert_eq!(s.stored(), first, "no save before the debounce");
}

#[test]
fn another_button_while_save_is_down_acts_and_saves_nothing() {
    let mut s = Saving::new();
    let first = s.stored();
    s.ui.down(SAVE);
    s.ui.press(PANEL_BTN_OCT_UP);
    s.ui.up(SAVE);
    assert_eq!(s.ui.live().octave, 1, "OCT+ acts as usual");
    s.ui.down(SAVE);
    s.ui.press(PANEL_BTN_SCL);
    s.ui.up(SAVE);
    assert_eq!(s.ui.live().fx_select, 1);
    for _ in 0..10 {
        s.frame();
        assert_ne!(s.ui.toast().as_deref(), Some("SAVED"));
    }
    assert_eq!(s.stored(), first);
}

#[test]
fn save_ends_the_boot_and_saves_nothing() {
    let mut s = Saving::new();
    let first = s.stored();
    unsafe { views_boot(&mut *s.ui.v, 0, 0) };
    s.frame();
    assert_eq!(s.ui.top(), "BOOT");
    s.ui.v.cards = 0;
    s.ui.press(SAVE);
    s.frame();
    s.ui.settle();
    assert_eq!(s.ui.top(), "LIVE", "SAVE ended the boot");
    for _ in 0..10 {
        s.frame();
        assert_ne!(s.ui.toast().as_deref(), Some("SAVED"));
    }
    assert_eq!(s.stored(), first);
}

#[test]
fn the_menus_store_shows_the_free_space() {
    let mut s = Saving::new();
    s.open_store_item();
    assert_eq!(s.store_item(), format!("STORE{:>9}", "193K"));
    assert_eq!(STORE_CAPACITY / 1024, 193);
    // ALGORITHM on it does nothing (and buzzes).
    s.ui.turn(PANEL_ENC_ALGORITHM, 2);
    s.frame();
    assert_eq!(s.store_item(), format!("STORE{:>9}", "193K"));
}

/// A region with one stray byte: not blank, no SIDOU headers.
fn foreign_region() -> Vec<u8> {
    let lo = STORE_LO as usize;
    let mut region = vec![0xFFu8; STORE_HI as usize - lo];
    region[9 * STORE_SECTOR as usize + 300] = 0x17;
    region
}

#[test]
fn a_foreign_store_reads_foreign_a_clockwise_turn_arms_and_save_formats_it() {
    let region = foreign_region();
    let mut st = Store::new(
        crate::store::Sim::with_region(6, &region, [false; STORE_SECTORS as usize]),
        256,
    );
    assert_eq!(st.mount(), STORE_STATE_FOREIGN);
    let mut s = Saving::on(st);
    assert_eq!(s.save().as_deref(), Some("STORE FOREIGN: NOT SAVED"));
    s.open_store_item();
    assert_eq!(s.store_item(), format!("STORE{:>9}", "FOREIGN"));
    // Once arms; SELECT away and back disarms; counter-clockwise disarms.
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.frame();
    assert_eq!(s.store_item(), format!("STORE{:>9}", "FORMAT?"));
    assert_eq!(
        s.store_popup(),
        "SAVE FORMATS",
        "the popup names the confirm"
    );
    s.ui.turn(PANEL_ENC_SELECT, 1);
    s.ui.turn(PANEL_ENC_SELECT, -1);
    s.frame();
    assert_eq!(s.store_item(), format!("STORE{:>9}", "FOREIGN"));
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    s.frame();
    assert_eq!(
        s.store_item(),
        format!("STORE{:>9}", "FOREIGN"),
        "counter-clockwise"
    );
    // Armed, a second clockwise turn is not the confirm.
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.frame();
    assert_eq!(s.store_item(), format!("STORE{:>9}", "FORMAT?"));
    assert_eq!(
        s.st.sim.region(),
        &region[..],
        "not a byte before the confirm"
    );
    s.ui.press(SAVE);
    assert!(FORCE.with(std::cell::Cell::get), "the erases forced");
    s.ui.frame(); // no polls yet
    let item = s.store_item();
    assert!(
        item.starts_with("STORE") && item.ends_with("ERASE 73"),
        "{item}"
    );
    let mut frames = 0;
    while s.ui.toast().as_deref() != Some("STORE FORMATTED") {
        s.frame();
        frames += 1;
        assert!(frames < 200, "the format never ended: {}", s.store_item());
    }
    assert!(!s.forced, "the force ends with the format");
    assert_eq!(s.store_item(), format!("STORE{:>9}", "193K"));
    // The settings are written again once the store takes them.
    s.ticks_for_the_debounce();
    assert_eq!(s.stored(), Some(s.gather()));
    let mut st = s.st;
    st.reset();
    assert_eq!(st.mount(), STORE_STATE_SIDOU);
}

/// Ergonomics review E01 (M2 review 1.5): a fast spin of ALGORITHM on a foreign store, in one
/// event or in many, arms the format and never formats; nor does SAVE away from STORE.
#[test]
fn a_fast_algorithm_spin_on_a_foreign_store_never_formats_it() {
    let region = foreign_region();
    let mut st = Store::new(
        crate::store::Sim::with_region(6, &region, [false; STORE_SECTORS as usize]),
        256,
    );
    assert_eq!(st.mount(), STORE_STATE_FOREIGN);
    let mut s = Saving::on(st);
    s.open_store_item();
    s.ui.turn(PANEL_ENC_ALGORITHM, 7); // one event of many detents
    for _ in 0..12 {
        s.ui.turn(PANEL_ENC_ALGORITHM, 1); // and a stream of single ones
    }
    for _ in 0..30 {
        s.frame();
    }
    assert!(!FORCE.with(std::cell::Cell::get));
    assert_eq!(s.store_item(), format!("STORE{:>9}", "FORMAT?"));
    assert_eq!(s.st.sim.region(), &region[..], "nothing erased");
    // Armed, then SELECT moves off STORE: SAVE there saves the settings (refused), no format.
    s.ui.turn(PANEL_ENC_SELECT, 1);
    assert_eq!(s.save().as_deref(), Some("STORE FOREIGN: NOT SAVED"));
    assert_eq!(s.st.sim.region(), &region[..], "nothing erased");
    // HOME away and back: the arm is gone too.
    s.ui.turn(PANEL_ENC_SELECT, -1);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.press(PANEL_BTN_HOME);
    s.ui.settle();
    s.open_store_item();
    assert_eq!(s.store_item(), format!("STORE{:>9}", "FOREIGN"));
    assert_eq!(s.save().as_deref(), Some("STORE FOREIGN: NOT SAVED"));
    assert_eq!(s.st.sim.region(), &region[..], "nothing erased");
}

/// A newer SIDOU's store is foreign to this one, but the menu says which kind it is.
#[test]
fn a_newer_sidou_store_reads_newer() {
    let mut r = fresh(11, 256);
    assert_eq!(r.put_now(1, b"x"), STORE_OK);
    let mut region = r.sim.region().to_vec();
    let sector = STORE_SECTOR as usize;
    let s = (0..STORE_SECTORS as usize)
        .find(|&s| region[s * sector..s * sector + 8] == *b"SIDOUSTR")
        .unwrap();
    let h = &mut region[s * sector..s * sector + 20];
    h[8] = STORE_FORMAT as u8 + 1;
    let crc = crate::store::crc32(&h[..16]);
    h[16..20].copy_from_slice(&crc.to_le_bytes());
    let mut st = Store::new(
        crate::store::Sim::with_region(11, &region, [false; STORE_SECTORS as usize]),
        256,
    );
    assert_eq!(st.mount(), STORE_STATE_FOREIGN);
    let mut s = Saving::on(st);
    s.open_store_item();
    assert_eq!(s.store_item(), format!("STORE{:>9}", "NEWER"));
    s.ui.turn(PANEL_ENC_ALGORITHM, -1); // the popup's ask; counter-clockwise arms nothing
    assert_eq!(s.store_popup(), "NEWER SIDOU");
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.frame();
    assert_eq!(s.store_item(), format!("STORE{:>9}", "FORMAT?"));
}

impl Saving {
    fn ticks_for_the_debounce(&mut self) {
        for _ in 0..(SETTINGS_DEBOUNCE_MS / 40 + 2) {
            self.frame();
        }
    }
}

#[test]
fn no_store_reads_none_and_save_does_nothing_visible() {
    let mut ui = Ui::without_cards();
    ui.press(SAVE);
    for _ in 0..10 {
        ui.frame();
    }
    assert_eq!((ui.top().as_str(), ui.toast()), ("LIVE", None));
}
