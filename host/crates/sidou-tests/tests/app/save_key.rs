// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::c_text;
use crate::store::{fresh, Rig as Store};
use crate::views::{Ui, CARDS_OFF};

const SAVE: u32 = SAVE_KEY_BUTTON;
const A_PRESET_CHANGE_ONLY_A_SAVE_STORES_BEFORE_THE_DEBOUNCE: u8 = 3;

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
    buttons(&mut k, s | oct, s | oct);
    assert_eq!(buttons(&mut k, 0, 0), SAVE_KEY_NONE, "in the same frame");
    let mut k = key();
    buttons(&mut k, s, s);
    unsafe { save_key_spoil(&mut k) };
    assert_eq!(buttons(&mut k, 0, 0), SAVE_KEY_NONE, "a knob");
    unsafe { save_key_spoil(&mut k) };
    buttons(&mut k, s, s);
    assert_eq!(buttons(&mut k, 0, 0), SAVE_KEY_SAVE);
    buttons(&mut k, s, s | oct);
    buttons(&mut k, 0, s);
    assert_eq!(buttons(&mut k, 0, 0), SAVE_KEY_SAVE);
}

struct Saving {
    ui: Ui,
    st: Store,
    set: Box<settings>,
    _ml: Box<midi_learn>,
    forced: bool,
    popup_source: screen_popup_fn,
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
        let popup_source = ui.v.popup.source.take();
        let mut set = Box::<settings>::default();
        let mut ml = ui.attach_learn();
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
            popup_source,
        };
        s.settle();
        s
    }

    fn new() -> Self {
        Self::on(fresh(5, 256))
    }

    fn on_a_foreign_store() -> (Self, Vec<u8>) {
        let region = foreign_region_of_one_stray_byte();
        let mut st = Store::new(
            crate::store::Sim::with_region(6, &region, [false; STORE_SECTORS as usize]),
            256,
        );
        assert_eq!(st.mount(), STORE_STATE_FOREIGN);
        (Self::on(st), region)
    }

    fn polled_quiet_frame(&mut self) {
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
        self.frames(40);
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

    fn save_and_wait_for_its_toast(&mut self) -> Option<String> {
        self.ui.press(SAVE);
        for _ in 0..25 {
            self.polled_quiet_frame();
            let settings_save_still_unanswered = self.set.asked != 0;
            if settings_save_still_unanswered {
                continue;
            }
            if let Some(t) = self.ui.toast() {
                return Some(t);
            }
        }
        None
    }

    fn store_item_value(&mut self) -> String {
        crate::views::settings_settle(&mut self.ui);
        self.ui
            .menu_line(VIEWS_SETTINGS_STORE as usize)
            .trim()
            .to_string()
    }

    fn algorithm_popup_value(&mut self) -> String {
        let mut name = [0 as std::ffi::c_char; SCREEN_POPUP_TEXT as usize];
        let mut value = [0 as std::ffi::c_char; SCREEN_POPUP_TEXT as usize];
        let (mut fill, mut max, mut accent) = (0, 0, 0);
        let ctx: *mut views = &mut *self.ui.v;
        let shown = unsafe {
            (self.popup_source.unwrap())(
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
        c_text(value.as_ptr())
    }

    fn open_store_item(&mut self) {
        crate::views::open_setting(&mut self.ui, VIEWS_SETTINGS_STORE);
        self.polled_quiet_frame();
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.polled_quiet_frame();
        }
    }

    fn ticks_for_the_debounce(&mut self) {
        self.frames((SETTINGS_DEBOUNCE_MS / 40 + 2) as usize);
    }

    fn the_next_toast_within(&mut self, frames: usize) -> Option<String> {
        for _ in 0..frames {
            self.polled_quiet_frame();
            if let Some(t) = self.ui.toast() {
                return Some(t);
            }
        }
        None
    }

    fn never_says_saved_for_ten_frames(&mut self) {
        for _ in 0..10 {
            self.polled_quiet_frame();
            assert_ne!(self.ui.toast().as_deref(), Some("SAVED"));
        }
    }
}

#[test]
fn a_tracker_rows_change_alone_is_background_saved_after_five_seconds_and_loaded_at_boot() {
    let mut s = Saving::new();
    s.frames(150);
    let before = s.stored().expect("the boot's settings saved");
    assert_eq!(s.ui.rig.live.settings_dirty, 0);
    crate::views::open_setting(&mut s.ui, VIEWS_SETTINGS_ROWS);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(s.ui.v.song.scroll, 1);
    s.polled_quiet_frame();
    let changed_at = s.set.changed_at;
    while s.ui.now - changed_at < SETTINGS_DEBOUNCE_MS {
        assert_eq!(s.stored(), Some(before.clone()), "not before five seconds");
        s.polled_quiet_frame();
    }
    s.polled_quiet_frame();
    s.polled_quiet_frame();
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
    s.frames(150);
    let before = s.stored().expect("the boot's settings saved");
    assert_eq!(s.ui.rig.live.settings_dirty, 0);
    assert_eq!(
        s.ui.rig.live.theme, THEME_HIGH_CONTRAST as u8,
        "the default"
    );
    crate::views::open_setting(&mut s.ui, VIEWS_SETTINGS_THEME);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(s.ui.rig.live.theme, THEME_C64 as u8);
    s.polled_quiet_frame();
    let changed_at = s.set.changed_at;
    while s.ui.now - changed_at < SETTINGS_DEBOUNCE_MS {
        assert_eq!(s.stored(), Some(before.clone()), "not before five seconds");
        s.polled_quiet_frame();
    }
    s.polled_quiet_frame();
    s.polled_quiet_frame();
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
    s.ui.learn_on(PANEL_ENC_K1);
    assert_eq!(s.ui.top(), "LEARN");
    let entry = unsafe { midi_learn_route(&mut *s._ml, 21) };
    assert!(!entry.is_null());
    assert_eq!(unsafe { (*entry).param } as u32, LIVE_PARAM_CUTOFF);
    let want = s.gather();
    assert_eq!(
        before.iter().zip(&want).filter(|(a, b)| a != b).count(),
        1,
        "only the learned binding changed"
    );
    s.polled_quiet_frame();
    assert_eq!(s.ui.top(), "LIVE", "learn closed after the CC");
    let changed_at = s.ui.now;
    while s.ui.now - changed_at < SETTINGS_DEBOUNCE_MS {
        assert_eq!(s.stored(), Some(before.clone()), "not before 5 s");
        s.polled_quiet_frame();
    }
    s.polled_quiet_frame();
    assert_eq!(s.stored(), Some(want));
}

#[test]
fn a_menu_cards_change_is_background_saved_without_save() {
    let mut s = Saving::new();
    s.frames(150);
    let before = s.stored();
    crate::views::open_setting(&mut s.ui, VIEWS_SETTINGS_CARDS);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.frames(120);
    assert_eq!(s.stored(), before, "the menu edit must debounce");
    s.frames(10);
    assert_ne!(s.stored(), before);
    assert_eq!(s.stored(), Some(s.gather()));
}

#[test]
fn saves_press_saves_the_settings_now_on_live_and_says_saved() {
    let mut s = Saving::new();
    let first = s.stored();
    s.ui.v.cards = 1 - s.ui.v.cards;
    assert_eq!(s.save_and_wait_for_its_toast().as_deref(), Some("SAVED"));
    assert_ne!(s.stored(), first);
    assert_eq!(
        s.stored(),
        Some(s.gather()),
        "within a frame or two, not after 5 s"
    );
    assert!(!s.forced);
}

#[test]
fn saves_press_saves_the_settings_in_scene_status_and_chip_not_in_the_menu() {
    let mut s = Saving::new();
    s.ui.press(PANEL_BTN_HOME);
    s.ui.settle();
    s.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_PRESET as i32);
    let before = s.stored();
    s.ui.rig.live.preset = A_PRESET_CHANGE_ONLY_A_SAVE_STORES_BEFORE_THE_DEBOUNCE;
    assert_eq!(
        s.save_and_wait_for_its_toast(),
        None,
        "the menu's SAVE on PRE"
    );
    assert_eq!(s.stored(), before);
    for (view, open) in [
        ("SCENE", Some(VIEWS_MENU_SCENE)),
        ("STATUS", Some(VIEWS_MENU_STATUS)),
        ("CHIP", Some(VIEWS_MENU_CHIP)),
    ] {
        let mut s = Saving::new();
        s.ui.press(PANEL_BTN_HOME);
        s.ui.settle();
        if let Some(item) = open {
            s.ui.choose_menu_item(item);
            s.ui.settle();
        }
        assert_eq!(s.ui.top(), view);
        s.ui.rig.live.preset = A_PRESET_CHANGE_ONLY_A_SAVE_STORES_BEFORE_THE_DEBOUNCE;
        assert_eq!(
            s.save_and_wait_for_its_toast().as_deref(),
            Some("SAVED"),
            "{view}"
        );
        assert_eq!(s.stored(), Some(s.gather()), "{view}");
    }
}

#[test]
fn a_knob_turned_while_save_is_down_saves_nothing() {
    let mut s = Saving::new();
    let first = s.stored();
    s.ui.v.cards = CARDS_OFF;
    s.ui.down(SAVE);
    s.ui.turn(PANEL_ENC_K1, 1);
    s.ui.up(SAVE);
    assert_eq!(s.ui.top(), "LEARN");
    s.ui.press(PANEL_BTN_HOME);
    unsafe { views_learn(&mut *s.ui.v, std::ptr::null_mut()) };
    let cutoff = s.ui.live().cutoff;
    s.ui.down(SAVE);
    s.ui.turn(PANEL_ENC_K1, 1);
    s.ui.up(SAVE);
    assert_ne!(s.ui.live().cutoff, cutoff);
    s.ui.down(SAVE);
    s.ui.turn(PANEL_ENC_SELECT, 1);
    s.ui.up(SAVE);
    s.never_says_saved_for_ten_frames();
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
    s.never_says_saved_for_ten_frames();
    assert_eq!(s.stored(), first);
}

#[test]
fn save_ends_the_boot_and_saves_nothing() {
    let mut s = Saving::new();
    let first = s.stored();
    unsafe { views_boot(&mut *s.ui.v, 0, 0) };
    s.polled_quiet_frame();
    assert_eq!(s.ui.top(), "BOOT");
    s.ui.v.cards = CARDS_OFF;
    s.ui.press(SAVE);
    s.polled_quiet_frame();
    s.ui.settle();
    assert_eq!(s.ui.top(), "LIVE", "SAVE ended the boot");
    s.never_says_saved_for_ten_frames();
    assert_eq!(s.stored(), first);
}

#[test]
fn the_menus_store_shows_the_free_space() {
    let mut s = Saving::new();
    s.open_store_item();
    assert_eq!(s.store_item_value(), store_line("193K"));
    assert_eq!(STORE_CAPACITY / 1024, 193);
    s.ui.turn(PANEL_ENC_ALGORITHM, 2);
    s.polled_quiet_frame();
    assert_eq!(s.store_item_value(), store_line("193K"));
}

fn store_line(value: &str) -> String {
    format!("STORE{value:>9}")
}

fn foreign_region_of_one_stray_byte() -> Vec<u8> {
    let lo = STORE_LO as usize;
    let mut region = vec![0xFFu8; STORE_HI as usize - lo];
    region[9 * STORE_SECTOR as usize + 300] = 0x17;
    region
}

#[test]
fn a_foreign_store_reads_foreign_a_clockwise_turn_arms_and_save_formats_it() {
    let (mut s, region) = Saving::on_a_foreign_store();
    assert_eq!(
        s.save_and_wait_for_its_toast().as_deref(),
        Some("STORE FOREIGN: NOT SAVED")
    );
    s.open_store_item();
    assert_eq!(s.store_item_value(), store_line("FOREIGN"));
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.polled_quiet_frame();
    assert_eq!(s.store_item_value(), store_line("FORMAT?"));
    assert_eq!(
        s.algorithm_popup_value(),
        "SAVE FORMATS",
        "the popup names the confirm"
    );
    s.ui.turn(PANEL_ENC_SELECT, 1);
    s.ui.turn(PANEL_ENC_SELECT, -1);
    s.polled_quiet_frame();
    assert_eq!(s.store_item_value(), store_line("FOREIGN"));
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    s.polled_quiet_frame();
    assert_eq!(
        s.store_item_value(),
        store_line("FOREIGN"),
        "counter-clockwise"
    );
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.polled_quiet_frame();
    assert_eq!(s.store_item_value(), store_line("FORMAT?"));
    assert_eq!(
        s.st.sim.region(),
        &region[..],
        "not a byte before the confirm"
    );
    s.ui.press(SAVE);
    assert!(FORCE.with(std::cell::Cell::get), "the erases forced");
    s.ui.frame();
    let item = s.store_item_value();
    assert!(
        item.starts_with("STORE") && item.ends_with("ERASE 73"),
        "{item}"
    );
    let mut frames = 0;
    while s.ui.toast().as_deref() != Some("STORE FORMATTED") {
        s.polled_quiet_frame();
        frames += 1;
        assert!(
            frames < 200,
            "the format never ended: {}",
            s.store_item_value()
        );
    }
    assert!(!s.forced, "the force ends with the format");
    assert_eq!(s.store_item_value(), store_line("193K"));
    s.ticks_for_the_debounce();
    assert_eq!(s.stored(), Some(s.gather()));
    let mut st = s.st;
    st.reset();
    assert_eq!(st.mount(), STORE_STATE_SIDOU);
}

#[test]
fn status_shows_the_stores_health_only_when_it_is_not_healthy() {
    let mut s = Saving::new();
    crate::views::open_status(&mut s.ui);
    s.polled_quiet_frame();
    assert_eq!(s.ui.top(), "STATUS");
    let ready = |s: &Saving| {
        s.ui.row(28)
            .trim_matches(|c| c == '#' || c == ' ')
            .to_string()
    };
    assert_eq!(ready(&s), "READY.");
    s.st.st.errors = 3;
    s.polled_quiet_frame();
    assert!(
        ready(&s).starts_with("READY.") && ready(&s).ends_with("STORE ERR 3"),
        "{}",
        ready(&s)
    );
    s.st.st.retired = 2;
    s.st.st.errors = 12;
    s.polled_quiet_frame();
    assert!(
        ready(&s).starts_with("READY.") && ready(&s).ends_with("RETIRED 2 ERR 12"),
        "{}",
        ready(&s)
    );
    s.st.st.errors = 0;
    s.st.st.retired = 0;
    s.polled_quiet_frame();
    assert_eq!(ready(&s), "READY.");
}

#[test]
fn reset_asks_first_only_save_confirms_and_saves_at_once_and_cancels_write_nothing() {
    let mut s = Saving::new();
    s.frames(150);
    let reset_item = |s: &mut Saving| {
        crate::views::settings_settle(&mut s.ui);
        s.ui.menu_line(VIEWS_SETTINGS_RESET as usize)
            .trim()
            .to_string()
    };
    crate::views::open_setting(&mut s.ui, VIEWS_SETTINGS_ROWS);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    unsafe { live_param_set(&mut *s.ui.rig.live, LIVE_PARAM_CUTOFF, 1500) };
    s.ui.rig.tick();
    s.polled_quiet_frame();
    s.ticks_for_the_debounce();
    let before = s.stored();
    assert_eq!(s.ui.v.song.scroll, 1);
    crate::views::open_setting(&mut s.ui, VIEWS_SETTINGS_RESET);
    s.polled_quiet_frame();
    assert_eq!(reset_item(&mut s), "RESET");
    s.ui.press(SAVE);
    s.polled_quiet_frame();
    assert_eq!(s.ui.v.song.scroll, 1);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.polled_quiet_frame();
    assert_eq!(reset_item(&mut s), format!("{:<5}{:>9}", "RESET", "RESET?"));
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    s.ui.press(SAVE);
    s.ticks_for_the_debounce();
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.turn(PANEL_ENC_SELECT, -1);
    s.ui.turn(PANEL_ENC_SELECT, 1);
    s.ui.press(SAVE);
    s.ticks_for_the_debounce();
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.press(PANEL_BTN_HOME);
    s.ui.settle();
    crate::views::open_setting(&mut s.ui, VIEWS_SETTINGS_RESET);
    s.ui.press(SAVE);
    s.ticks_for_the_debounce();
    assert_eq!(s.ui.v.song.scroll, 1, "nothing was reset");
    assert_eq!(s.ui.live().cutoff, 1500);
    assert_eq!(s.stored(), before, "nothing was written");
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.polled_quiet_frame();
    assert_eq!(s.algorithm_popup_value(), "SAVE RESETS");
    s.ui.press(SAVE);
    assert_eq!(
        s.the_next_toast_within(40).as_deref(),
        Some("RESET AND SAVED")
    );
    assert_eq!(s.ui.v.song.scroll, 0);
    assert_ne!(s.ui.live().cutoff, 1500);
    assert_eq!(
        s.stored(),
        Some(s.gather()),
        "saved now, not after the debounce"
    );
    assert_ne!(s.stored(), before);
}

#[test]
fn a_forced_reset_says_reset_and_saved_with_gaps() {
    let mut s = Saving::new();
    s.frames(150);
    unsafe { live_param_set(&mut *s.ui.rig.live, LIVE_PARAM_CUTOFF, 1500) };
    s.ui.rig.tick();
    s.polled_quiet_frame();
    s.ticks_for_the_debounce();
    crate::views::open_setting(&mut s.ui, VIEWS_SETTINGS_RESET);
    s.polled_quiet_frame();
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.polled_quiet_frame();
    assert_eq!(s.algorithm_popup_value(), "SAVE RESETS");
    s.ui.press(SAVE);
    s.set.forcing = 1;
    assert_eq!(
        s.the_next_toast_within(40).as_deref(),
        Some("RESET AND SAVED (GAPS)")
    );
}

#[test]
fn a_fast_algorithm_spin_on_a_foreign_store_arms_and_never_formats_it_nor_does_save_off_store() {
    let (mut s, region) = Saving::on_a_foreign_store();
    s.open_store_item();
    s.ui.turn(PANEL_ENC_ALGORITHM, 7);
    for _ in 0..12 {
        s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    }
    s.frames(30);
    assert!(!FORCE.with(std::cell::Cell::get));
    assert_eq!(s.store_item_value(), store_line("FORMAT?"));
    assert_eq!(s.st.sim.region(), &region[..], "nothing erased");
    s.ui.turn(PANEL_ENC_SELECT, 1);
    assert_eq!(s.save_and_wait_for_its_toast(), None);
    assert_eq!(s.st.sim.region(), &region[..], "nothing erased");
    s.ui.turn(PANEL_ENC_SELECT, -1);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.press(PANEL_BTN_HOME);
    s.ui.settle();
    s.open_store_item();
    assert_eq!(s.store_item_value(), store_line("FOREIGN"));
    assert_eq!(s.save_and_wait_for_its_toast(), None);
    assert_eq!(s.st.sim.region(), &region[..], "nothing erased");
}

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
    assert_eq!(s.store_item_value(), store_line("NEWER"));
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(s.algorithm_popup_value(), "NEWER SIDOU");
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.polled_quiet_frame();
    assert_eq!(s.store_item_value(), store_line("FORMAT?"));
}

#[test]
fn no_store_reads_none_and_save_does_nothing_visible() {
    let mut ui = Ui::without_cards();
    ui.press(SAVE);
    ui.frames(10);
    assert_eq!((ui.top().as_str(), ui.toast()), ("LIVE", None));
}
