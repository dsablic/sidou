// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The USER chip profiles (`app/user_profile.c`, the CHIP view's picker and save target,
//! M2a Task 6): the record on the store, the picker's order and its skipped empty slots, EDIT +
//! ALGORITHM's save target and SAVE into it (asking before an overwrite), the song's chip A,
//! CC 85, the write's retries, and M2b's schema-2 raw expert overrides and v1 migration.

use std::cell::Cell;

use sidou_sys::*;

use crate::store::{fresh, Rig as Store};
use crate::views::Ui;

const SAVE: u32 = SAVE_KEY_BUTTON;
const EDIT: u32 = PANEL_BTN_EDIT;
const ALG: u32 = PANEL_ENC_ALGORITHM;
const CC_PROFILE: u32 = 85;
const COLS: usize = GRID_COLS as usize;
/// live.h: LIVE_REG_CHAR(0) is the profile's, then age, DAC, thump and bleed.
const REG_CHAR: u32 = 0x82;

thread_local! {
    /// The force hook's last level and the number of times it was asked for on.
    static FORCE: Cell<(bool, u32)> = const { Cell::new((false, 0)) };
}

unsafe extern "C" fn force(on: i32) {
    FORCE.with(|f| {
        let (_, ons) = f.get();
        f.set((on != 0, ons + u32::from(on != 0)));
    });
}

fn key(n: u32) -> u32 {
    0x101 + n
}

/// The views on a store, the settings on it, and the store's polls between frames.
struct Prof {
    ui: Ui,
    st: Store,
    set: Box<settings>,
    _ml: Box<midi_learn>,
    quiet: bool,
}

impl Prof {
    fn on(st: Store) -> Self {
        Self::with(st, true)
    }

    fn with(st: Store, quiet: bool) -> Self {
        let mut ui = Ui::without_cards();
        let mut set = Box::<settings>::default();
        let mut ml = Box::<midi_learn>::default();
        let mut st = st;
        unsafe {
            midi_learn_init(&mut *ml);
            views_learn(&mut *ui.v, &mut *ml);
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
        FORCE.with(|f| f.set((false, 0)));
        let mut p = Prof {
            ui,
            st,
            set,
            _ml: ml,
            quiet,
        };
        for _ in 0..140 {
            p.frame(); // past the first settings save
        }
        p
    }

    fn new() -> Self {
        Self::on(fresh(7, 256))
    }

    fn frame(&mut self) {
        let forcing = self.set.forcing != 0 || self.ui.v.users.forcing != 0;
        unsafe { store_quiet(&mut *self.st.st, i32::from(self.quiet || forcing)) };
        for _ in 0..256 {
            if unsafe { store_poll(&mut *self.st.st) } == 0 {
                break;
            }
        }
        self.ui.frame();
        crate::drain(&mut self.ui.g);
    }

    fn frames(&mut self, n: u32) {
        for _ in 0..n {
            self.frame();
        }
    }

    fn settle(&mut self) {
        for _ in 0..60 {
            self.frame();
            if unsafe { screen_animating(&self.ui.v.s) } == 0 {
                return;
            }
        }
        panic!("the screen kept animating");
    }

    fn open_chip(&mut self) {
        self.ui.press(PANEL_BTN_HOME);
        self.settle();
        self.ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
        self.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_CHIP as i32);
        self.ui.turn(ALG, 1);
        self.settle();
        assert_eq!(self.ui.top(), "CHIP");
        self.frames(3);
    }

    fn save(&mut self) {
        self.ui.press(SAVE);
    }

    /// Frames until the save's toast shows (at most `ms`; others, a model change's, pass).
    fn toast_within(&mut self, ms: u32) -> Option<String> {
        for _ in 0..ms / 40 {
            self.frame();
            if let Some(t) = self.ui.toast() {
                if t.starts_with("SAVED") || t.starts_with("STORE") {
                    return Some(t);
                }
            }
        }
        None
    }

    /// EDIT held, ALGORITHM by `steps`, EDIT let go: the save target.
    fn target(&mut self, steps: i32) {
        self.ui.hold(EDIT, true);
        self.ui.turn(ALG, steps);
        self.ui.hold(EDIT, false);
        self.frames(3);
    }

    /// `n` cells of a row from `col`, as text (large text's pieces show their characters).
    fn small(&self, row: u32, col: usize, n: usize) -> String {
        self.ui.small(row as usize, col, n).trim_end().to_owned()
    }

    /// The profile as the choice's second word, `n` characters of 3x text.
    fn word(&self, n: usize) -> String {
        self.ui.big(CHIP_VIEW_CHOICE_ROW as usize, 13, 3, n)
    }

    fn profile(&self) -> u32 {
        unsafe { live_param_get(&*self.ui.rig.live, LIVE_PARAM_PROFILE) }
    }

    fn controls(&self) -> [u8; 4] {
        self.ui.rig.live.character
    }

    fn fill(&mut self, slot: usize, base: u32, ctl: [u8; 4]) {
        let l = &mut *self.ui.rig.live;
        l.user_valid |= 1 << slot;
        l.user_base[slot] = base as u8;
        l.user_ctl[slot] = ctl;
    }
}

fn record(base: u32, ctl: [u8; 4]) -> Vec<u8> {
    let mut rec = vec![
        2,
        base as u8,
        4,
        ctl[0],
        ctl[1],
        ctl[2],
        ctl[3],
        SID_EXPERT_COUNT as u8,
    ];
    rec.extend(std::iter::repeat_n(
        SID_EXPERT_DEFAULT as u8,
        SID_EXPERT_COUNT as usize,
    ));
    rec
}

#[test]
fn the_record_keeps_the_v1_prefix_under_its_own_key() {
    assert_eq!(USER_PROFILE_V1_BYTES, 7);
    assert_eq!(USER_PROFILE_VERSION, 2);
    assert_eq!(USER_PROFILE_TYPE, 2);
    // Beside the settings' 0x100, one key a slot.
    assert_eq!(SETTINGS_KEY, 0x100);
    assert!((0..LIVE_USER_PROFILES).all(|n| key(n) == 0x101 + n));
}

#[test]
fn a_save_target_stores_the_chips_setup_without_changing_the_chip() {
    let mut p = Prof::new();
    p.open_chip();
    p.ui.turn(PANEL_ENC_K1, 3); // age 12
    p.ui.turn(PANEL_ENC_K1 + 3, 2); // bleed 8
    let (profile, ctl) = (p.profile(), p.controls());
    p.ui.rig.clear();
    p.target(3);
    assert_eq!(p.small(CHIP_VIEW_NAMES_ROW, 0, 16), "SAVE TO USER 3 -");
    assert_eq!(
        (p.profile(), p.controls()),
        (profile, ctl),
        "the chip is unchanged"
    );
    assert!(p.ui.rig.log().is_empty(), "no register write");
    // The target is yellow, where the names of the choice stand.
    assert_eq!(
        p.ui.g.attr[CHIP_VIEW_NAMES_ROW as usize * COLS + 4],
        ((GRID_BLACK << 4) | GRID_YELLOW) as u8,
        "the target, yellow on black"
    );
    p.save();
    let toast = p.toast_within(1000).expect("a toast");
    assert_eq!(toast, "SAVED TO USER 3");
    p.frames(5);
    assert_eq!(p.st.get(key(2)), Some(record(profile, ctl)));
    assert_eq!(p.st.get(key(0)), None, "no other slot");
    assert_eq!(p.ui.live().user_valid, 0b100);
    assert_eq!(
        p.st.get(SETTINGS_KEY).map(|r| r.len()),
        Some(SETTINGS_BYTES as usize),
        "the settings record is not the profiles' place"
    );
}

#[test]
fn a_non_empty_slot_asks_before_it_is_overwritten() {
    let mut p = Prof::new();
    p.open_chip();
    p.ui.turn(PANEL_ENC_K1, 1);
    p.target(1);
    p.save();
    assert_eq!(p.toast_within(1000).as_deref(), Some("SAVED TO USER 1"));
    p.frames(60);
    let first = p.st.get(key(0)).unwrap();
    // A different sound over it: asks, stores nothing, until SAVE confirms.
    p.ui.turn(PANEL_ENC_K1 + 1, 5);
    p.save();
    assert_eq!(p.ui.toast().as_deref(), Some("OVERWRITE USER 1?"));
    p.frames(60);
    assert_eq!(p.st.get(key(0)), Some(first.clone()), "not yet");
    p.save();
    assert_eq!(p.toast_within(1000).as_deref(), Some("SAVED TO USER 1"));
    p.frames(5);
    let second = p.st.get(key(0)).unwrap();
    assert_ne!(second, first);
    assert_eq!(second[3..7], p.controls());
}

#[test]
fn home_keeps_the_slot_and_forgets_the_question_and_the_target() {
    let mut p = Prof::new();
    p.open_chip();
    p.target(2);
    p.save();
    p.toast_within(1000).unwrap();
    p.frames(60);
    let kept = p.st.get(key(1)).unwrap();
    p.ui.turn(PANEL_ENC_K1 + 2, 4);
    p.save();
    assert_eq!(p.ui.toast().as_deref(), Some("OVERWRITE USER 2?"));
    p.ui.press(PANEL_BTN_HOME); // keeps it
    p.settle();
    p.frames(60);
    assert_eq!(p.st.get(key(1)), Some(kept));
    // Back on the page: no target, no question pending.
    p.ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    p.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_CHIP as i32);
    p.ui.turn(ALG, 1);
    p.settle();
    p.frames(3);
    assert_eq!(p.ui.top(), "CHIP");
    assert_eq!(p.ui.v.chip.target, 0);
    assert_eq!(
        p.small(CHIP_VIEW_NAMES_ROW, 0, 5),
        "MODEL",
        "the names are back"
    );
}

#[test]
fn the_target_steps_through_the_slots_and_off_and_clears_when_the_focus_leaves() {
    let mut p = Prof::new();
    p.open_chip();
    for (steps, want) in [(1, 1), (1, 2), (7, 8), (3, 8), (-9, 0), (-1, 0)] {
        p.target(steps);
        assert_eq!(p.ui.v.chip.target, want, "{steps}");
    }
    p.target(2);
    assert_eq!(p.ui.v.chip.target, 2);
    p.ui.turn(PANEL_ENC_SELECT, 1); // onto a slot: the target goes
    assert_eq!(p.ui.v.chip.target, 0);
    p.ui.turn(PANEL_ENC_SELECT, -1);
    p.target(2);
    p.ui.turn(PANEL_ENC_SELECT, -1); // onto the tabs
    assert_eq!(p.ui.v.chip.target, 0);
}

#[test]
fn save_with_no_target_and_a_built_in_profile_saves_the_settings() {
    let mut p = Prof::new();
    p.open_chip();
    p.ui.turn(PANEL_ENC_K1, 1);
    p.save();
    assert_eq!(p.toast_within(1000).as_deref(), Some("SAVED"));
    assert_eq!(p.ui.live().user_valid, 0);
    for n in 0..LIVE_USER_PROFILES {
        assert_eq!(p.st.get(key(n)), None);
    }
}

#[test]
fn save_with_a_user_slot_applied_stores_back_into_it_asking_first() {
    let mut p = Prof::new();
    p.fill(3, SID_CHIP_8580_R5, [10, 20, 30, 40]);
    p.open_chip();
    // USER 4 is the only filled slot: the step from R4AR passes 8580 and worn, then picks it.
    p.ui.turn(ALG, 3);
    assert_eq!(p.ui.live().user_applied, 4);
    p.ui.turn(PANEL_ENC_K1, 2);
    p.save();
    assert_eq!(p.ui.toast().as_deref(), Some("OVERWRITE USER 4?"));
    p.save();
    assert_eq!(p.toast_within(1000).as_deref(), Some("SAVED TO USER 4"));
    p.frames(5);
    assert_eq!(
        p.st.get(key(3)),
        Some(record(SID_CHIP_8580_R5, [18, 20, 30, 40]))
    );
}

#[test]
fn the_picker_runs_the_built_in_profiles_then_the_filled_users_skipping_empty_ones() {
    let mut p = Prof::new();
    p.fill(0, SID_CHIP_6581_R2, [1, 2, 3, 4]);
    p.fill(2, SID_CHIP_8580_R5, [5, 6, 7, 8]);
    p.open_chip();
    p.ui.turn(ALG, 2); // R4AR -> 8580 R5 -> worn
    assert_eq!(p.profile(), SID_CHIP_6581_WORN);
    assert_eq!(p.ui.live().user_applied, 0);
    p.ui.turn(ALG, 1); // USER 1
    assert_eq!(p.ui.live().user_applied, 1);
    assert_eq!(
        (p.profile(), p.controls()),
        (SID_CHIP_6581_R2, [1, 2, 3, 4])
    );
    p.settle();
    assert_eq!(p.word(5), "USR 1");
    p.ui.turn(ALG, 1); // skips the empty USER 2
    assert_eq!(p.ui.live().user_applied, 3);
    assert_eq!(
        (p.profile(), p.controls()),
        (SID_CHIP_8580_R5, [5, 6, 7, 8])
    );
    p.frames(3);
    assert_eq!(p.word(5), "USR 3");
    // Nothing filled beyond: it stays, and says which slot it passed.
    p.ui.turn(ALG, 1);
    assert_eq!(p.ui.live().user_applied, 3);
    assert_eq!(p.ui.toast().as_deref(), Some("USER 4 EMPTY"));
    // Back through the empty one, and out of the users onto the built-ins.
    p.ui.turn(ALG, -1);
    assert_eq!(p.ui.live().user_applied, 1);
    p.ui.turn(ALG, -1);
    assert_eq!(p.ui.live().user_applied, 0);
    assert_eq!(p.profile(), SID_CHIP_6581_WORN);
    // With none filled the step off the last built-in profile names the first slot.
    let mut q = Prof::new();
    q.open_chip();
    q.ui.turn(ALG, 9);
    assert_eq!(q.profile(), SID_CHIP_6581_WORN);
    assert_eq!(q.ui.toast().as_deref(), Some("USER 1 EMPTY"));
}

#[test]
fn picking_a_user_goes_through_the_profile_change_and_the_character_ramp() {
    let mut p = Prof::new();
    p.fill(0, SID_CHIP_8580_R5, [30, 40, 50, 60]);
    p.open_chip();
    p.ui.turn(ALG, 3); // 8580, worn, USER 1
    assert_eq!(p.ui.live().user_applied, 1);
    // Asked for, as a profile step is: the chip has it at the next tick (a reset).
    assert_eq!(p.ui.live().profile_req, SID_CHIP_8580_R5 as u8);
    assert_ne!(p.ui.live().model, SID_MODEL_8580);
    p.ui.rig.clear();
    p.ui.rig.tick();
    assert_eq!(p.ui.live().model, SID_MODEL_8580);
    assert_eq!(p.ui.live().profile, SID_CHIP_8580_R5 as u8);
    let reg = |r: u32| {
        p.ui.rig
            .log()
            .iter()
            .filter(|(reg, _)| u32::from(*reg) == r)
            .map(|(_, v)| *v)
            .next_back()
    };
    assert_eq!(reg(REG_CHAR + 1), Some(30));
    assert_eq!(reg(REG_CHAR + 4), Some(60));
    // Any other profile or the model forgets which slot it came from.
    p.ui.rig.button(LIVE_BUTTON_FX);
    assert_eq!(p.ui.live().user_applied, 0);
}

#[test]
fn an_empty_slot_never_applies_from_a_cc() {
    let mut p = Prof::new();
    p.fill(0, SID_CHIP_8580_R5, [9, 9, 9, 9]);
    let before = p.profile();
    // CC 85 spans 0..12 in steps of 128/13: 60 is 6 (USER 2, empty), 50 is 5 (USER 1).
    p.ui.rig.cc(CC_PROFILE, 60);
    assert_eq!((p.profile(), p.ui.live().user_applied), (before, 0));
    p.ui.rig.cc(CC_PROFILE, 50);
    assert_eq!(p.ui.live().user_applied, 1);
    assert_eq!(p.profile(), SID_CHIP_8580_R5);
    assert_eq!(p.controls(), [9, 9, 9, 9]);
    p.ui.rig.cc(CC_PROFILE, 127); // USER 8, empty
    assert_eq!(p.ui.live().user_applied, 1, "nothing moved");
    p.ui.rig.cc(CC_PROFILE, 0);
    assert_eq!(p.profile(), SID_CHIP_6581_R2);
    assert_eq!(p.ui.live().user_applied, 0);
}

#[test]
fn on_a_songs_chip_a_the_save_target_stores_the_songs_setup_not_the_keys() {
    let mut p = Prof::new();
    p.open_chip();
    p.ui.rig.live.two_chips = 1;
    p.ui.rig.live.keys_chip = 1;
    let mut kr = Box::<keys_route>::default();
    kr.l = &mut *p.ui.rig.live;
    kr.song = 1;
    kr.song_want = chip_setup {
        profile: SID_CHIP_6581_R3 as u8,
        controls: [11, 22, 33, 44],
        gate_reset: 1,
        expert: [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize],
    };
    unsafe { chip_view_set_keys(&mut p.ui.v.chip, &mut *kr) };
    p.frames(3);
    assert_eq!(unsafe { chip_view_song_tab(&p.ui.v.chip) }, 1);
    let keys = (p.profile(), p.controls());
    p.target(5);
    assert_eq!(p.small(CHIP_VIEW_NAMES_ROW, 0, 16), "SAVE TO USER 5 -");
    p.save();
    assert_eq!(p.toast_within(1000).as_deref(), Some("SAVED TO USER 5"));
    p.frames(5);
    assert_eq!(
        p.st.get(key(4)),
        Some(record(SID_CHIP_6581_R3, [11, 22, 33, 44]))
    );
    assert_eq!(
        (p.profile(), p.controls()),
        keys,
        "the keys' setup is untouched"
    );
    assert_eq!(kr.song_want.controls, [11, 22, 33, 44], "nor the song's");
    // Picking a USER on the song's tab sets the song's setup, as an edit does.
    p.fill(1, SID_CHIP_8580_R5, [1, 3, 5, 7]);
    p.ui.turn(ALG, 4); // R3 -> R4AR -> 8580 -> worn -> USER 2 (USER 1 is empty)
    assert_eq!(unsafe { (*p.ui.v.chip.kr).song_user }, 2);
    assert_eq!(kr.song_want.profile, SID_CHIP_8580_R5 as u8);
    assert_eq!(kr.song_want.controls, [1, 3, 5, 7]);
    assert_eq!(
        (p.profile(), p.controls()),
        keys,
        "the keys' setup is untouched"
    );
    unsafe { chip_view_set_keys(&mut p.ui.v.chip, std::ptr::null_mut()) };
}

#[test]
fn the_slots_load_at_boot_and_a_record_that_is_not_a_profile_is_an_empty_slot() {
    let mut st = fresh(11, 256);
    let good = record(SID_CHIP_8580_R5, [1, 2, 3, 4]);
    let mut longer = record(SID_CHIP_6581_R3, [9, 8, 7, 6]);
    longer[0] = USER_PROFILE_VERSION as u8 + 1;
    longer.extend_from_slice(&[0xAA, 0xBB]); // a later schema's fields
    let cases: [(u32, Vec<u8>); 8] = [
        (0, good.clone()),
        (1, longer),
        (2, vec![0, 1, 4, 0, 0, 0, 0]), // schema 0
        (3, record(SID_CHIPS, [0; 4])), // a base profile that is not one
        (4, record(0, [0, 0, 0, 128])), // a control past 127
        (5, vec![1, 0, 4, 1, 2, 3]),    // short
        (6, vec![1, 0, 3, 1, 2, 3, 4]), // too few controls
        (7, record(SID_CHIP_6581_WORN, [127; 4])),
    ];
    for (n, rec) in &cases {
        assert_eq!(st.put_typed(key(*n), 2, rec), STORE_OK);
        st.settle();
    }
    let p = Prof::on(st);
    let l = p.ui.live();
    assert_eq!(l.user_valid, 0b1000_0011);
    assert_eq!(
        (l.user_base[0], l.user_ctl[0]),
        (SID_CHIP_8580_R5 as u8, [1, 2, 3, 4])
    );
    assert_eq!(
        (l.user_base[1], l.user_ctl[1]),
        (SID_CHIP_6581_R3 as u8, [9, 8, 7, 6])
    );
    assert_eq!(l.user_ctl[7], [127; 4]);
}

#[test]
fn the_v1_fixture_loads_with_profile_following_expert_defaults() {
    let text = include_str!("../data/profile-v1.hex");
    let rec: Vec<u8> = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .flat_map(str::split_whitespace)
        .map(|v| u8::from_str_radix(v, 16).unwrap())
        .collect();
    let mut st = fresh(11, 256);
    assert_eq!(
        st.put_typed(key(2), USER_PROFILE_TYPE as u8, &rec),
        STORE_OK
    );
    st.settle();
    let p = Prof::on(st);
    assert_eq!(p.ui.live().user_valid, 1 << 2);
    assert_eq!(p.ui.live().user_base[2], SID_CHIP_8580_R5 as u8);
    assert_eq!(p.ui.live().user_ctl[2], [11, 22, 33, 44]);
    assert!(p.ui.live().user_expert[2]
        .iter()
        .all(|&v| v == SID_EXPERT_DEFAULT as u8));
}

#[test]
fn saving_expert_overrides_keeps_raw_sentinels_across_a_reboot() {
    let mut p = Prof::new();
    let mut expert = [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize];
    for (e, value) in expert.iter_mut().enumerate().skip(1) {
        *value = (e * 7 % 128) as u8;
    }
    let controls = [11, 22, 33, 44];
    unsafe {
        user_profile_save(
            &mut p.ui.v.users,
            2,
            SID_CHIP_8580_R5,
            controls.as_ptr(),
            expert.as_ptr(),
            p.ui.now,
        );
    }
    for _ in 0..5 {
        unsafe { user_profiles_tick(&mut p.ui.v.users, p.ui.now, Some(force)) };
        p.frame();
    }
    let rec =
        p.st.get(key(2))
            .expect("the explicitly ticked save commits");
    assert_eq!(rec[0], 2);
    assert_eq!(&rec[1..7], &[3, 4, 11, 22, 33, 44]);
    assert_eq!(rec[7], SID_EXPERT_COUNT as u8);
    assert_eq!(&rec[8..], &expert);
    let mut st = p.st;
    st.reset();
    st.mount();
    let p = Prof::on(st);
    assert_eq!(p.ui.live().user_expert[2], expert);
}

#[test]
fn malformed_v2_expert_sections_leave_the_slot_empty() {
    let good = record(SID_CHIP_8580_R5, [11, 22, 33, 44]);
    let mut invalid = vec![good[..7].to_vec(), good[..good.len() - 1].to_vec()];
    let mut short_count = good.clone();
    short_count[7] -= 1;
    invalid.push(short_count);
    let mut invalid_value = good;
    invalid_value[8] = 128;
    invalid.push(invalid_value);
    for rec in invalid {
        let mut st = fresh(12, 256);
        assert_eq!(
            st.put_typed(key(0), USER_PROFILE_TYPE as u8, &rec),
            STORE_OK
        );
        st.settle();
        let mut p = Prof::on(st);
        assert_eq!(p.ui.live().user_valid, 0);
        p.ui.rig.live.user_base[0] = SID_CHIP_6581_R4AR as u8;
        p.ui.rig.live.user_ctl[0] = [9; 4];
        p.ui.rig.live.user_expert[0] = [17; SID_EXPERT_COUNT as usize];
        unsafe { user_profiles_load(&mut p.ui.v.users) };
        assert_eq!(p.ui.live().user_valid, 0);
        assert_eq!(p.ui.live().user_base[0], SID_CHIP_6581_R4AR as u8);
        assert_eq!(p.ui.live().user_ctl[0], [9; 4]);
        assert_eq!(p.ui.live().user_expert[0], [17; SID_EXPERT_COUNT as usize]);
    }
}

#[test]
fn the_v2_fixture_loads_raw_overrides_and_picking_it_restores_them() {
    let rec: Vec<u8> = include_str!("../data/profile-v2.hex")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .flat_map(str::split_whitespace)
        .map(|v| u8::from_str_radix(v, 16).unwrap())
        .collect();
    let mut st = fresh(11, 256);
    assert_eq!(
        st.put_typed(key(2), USER_PROFILE_TYPE as u8, &rec),
        STORE_OK
    );
    st.settle();
    let mut p = Prof::on(st);
    assert_eq!(&p.ui.live().user_expert[2][..], &rec[8..]);
    p.ui.rig.cc(CC_PROFILE, 69); // USER 3
    assert_eq!(p.ui.live().user_applied, 3);
    assert_eq!(p.ui.live().expert, p.ui.live().user_expert[2]);
}

#[test]
fn future_profile_counts_skip_unknown_controls_and_experts() {
    let mut rec = record(SID_CHIP_8580_R5, [11, 22, 33, 44]);
    rec[0] = USER_PROFILE_VERSION as u8 + 1;
    rec[2] += 1;
    rec[7] += 1;
    rec.insert(7, 99);
    rec[9] = 17;
    rec.push(19);
    let mut st = fresh(11, 256);
    assert_eq!(
        st.put_typed(key(2), USER_PROFILE_TYPE as u8, &rec),
        STORE_OK
    );
    st.settle();
    let p = Prof::on(st);
    assert_eq!(p.ui.live().user_valid, 1 << 2);
    assert_eq!(p.ui.live().user_ctl[2], [11, 22, 33, 44]);
    assert_eq!(p.ui.live().user_expert[2][0], 17);
}

#[test]
fn a_store_that_waits_for_quiet_is_asked_again_then_forced_and_says_gaps() {
    let mut st = Store::new(crate::store::Sim::new(5), 256);
    assert_eq!(st.mount(), STORE_STATE_BLANK);
    // A pool that is short needs quiet or a force for a new head, and is never quiet here.
    let mut p = Prof::with(st, false);
    p.open_chip();
    p.target(1);
    p.save();
    p.frames(40);
    assert_eq!(p.st.get(key(0)), None, "waits, unforced");
    assert_eq!(FORCE.with(Cell::get).1, 0);
    assert!(
        p.ui.v.users.queued != 0 || p.ui.v.users.sent != 0,
        "still queued"
    );
    // After USER_PROFILE_FORCE_MS the held operations are forced.
    let mut toast = None;
    for _ in 0..400 {
        p.frame();
        if let Some(t) = p.ui.toast() {
            if t.starts_with("SAVED TO") {
                toast = Some(t);
                break;
            }
        }
    }
    assert_eq!(toast.as_deref(), Some("SAVED TO USER 1 (GAPS)"));
    assert!(FORCE.with(Cell::get).1 >= 1);
    p.frames(5);
    assert!(!FORCE.with(Cell::get).0, "the force ends with the save");
    assert!(p.st.get(key(0)).is_some());
}

#[test]
fn a_foreign_store_keeps_nothing_and_says_so_but_the_slot_serves_until_reboot() {
    let mut region = vec![0xFFu8; (STORE_HI - STORE_LO) as usize];
    region[5 * STORE_SECTOR as usize + 7] = 0x42;
    let mut st = Store::new(
        crate::store::Sim::with_region(3, &region, [false; STORE_SECTORS as usize]),
        256,
    );
    assert_eq!(st.mount(), STORE_STATE_FOREIGN);
    let mut p = Prof::on(st);
    p.open_chip();
    p.target(1);
    p.save();
    assert_eq!(
        p.toast_within(1000).as_deref(),
        Some("STORE FOREIGN: NOT SAVED")
    );
    assert_eq!(p.st.sim.region(), &region[..], "not a byte written");
    assert_eq!(p.ui.live().user_valid, 1, "it applies for now");
}

#[test]
fn with_no_store_the_slot_is_kept_in_the_instrument_and_says_saved() {
    let mut p = Prof::on(fresh(1, 256));
    unsafe { views_settings(&mut *p.ui.v, std::ptr::null_mut()) };
    p.open_chip();
    p.target(2);
    p.save();
    assert_eq!(p.ui.toast().as_deref(), Some("SAVED TO USER 2"));
    assert_eq!(p.ui.live().user_valid, 0b10);
}

#[test]
fn a_format_empties_every_slot() {
    let mut p = Prof::new();
    p.fill(0, 1, [1, 1, 1, 1]);
    p.fill(5, 2, [2, 2, 2, 2]);
    unsafe { user_profiles_forget(&mut p.ui.v.users) };
    assert_eq!(p.ui.live().user_valid, 0);
    assert_eq!(p.ui.live().user_applied, 0);
}

/// On a store never mounted (an unknown flash part, review 1.1) a slot's save says it was not
/// saved at once; it still applies for now.
#[test]
fn an_unmounted_store_keeps_no_slot_and_says_so() {
    let mut p = Prof::on(Store::new(crate::store::Sim::new(8), 256));
    p.open_chip();
    p.target(1);
    p.save();
    assert_eq!(
        p.toast_within(1000).as_deref(),
        Some("STORE ERROR: NOT SAVED")
    );
    assert_eq!(p.st.sim.ops, 0, "the part is never touched");
    assert_eq!(p.ui.live().user_valid, 1, "it applies for now");
}

/// A program error after the store took a slot's record: not SAVED (review 1.4).
#[test]
fn a_program_error_after_a_slot_is_taken_says_not_saved() {
    let mut p = Prof::new();
    p.open_chip();
    p.target(2);
    p.st.sim.fail = 100;
    p.save();
    assert_eq!(
        p.toast_within(1000).as_deref(),
        Some("STORE ERROR: NOT SAVED")
    );
    assert!(p.st.stats().errors > 0);
}
