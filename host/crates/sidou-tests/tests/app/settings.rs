// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The settings that persist (`app/settings.c`, M2a Task 5): the record and its schema, the
//! load at boot (the full boot once after an install), the debounced background save, the
//! explicit save and its forced held operations, a foreign store, and M2b's raw expert
//! overrides and bindings with migrations from the original 42-parameter layout.

use std::cell::Cell;
use std::ffi::CStr;

use sidou_sys::*;

use crate::store::{fresh, Rig as Store};
use crate::{record, Recorder, Rig};

const VERSION: &CStr = c"V1.2.3";
const TICK: u32 = 10;

/// Byte 1 is CARDS: 0 OFF, 1 ONCE (an old record's ON), 2 ALWAYS; anything else reads as the
/// default, ONCE. No schema change.
#[test]
fn cards_byte_reads_old_and_new() {
    let mut a = Set::blank();
    let mut rec = a.gather();
    let version = rec[0];
    for (byte, want) in [(0u8, 0u8), (1, 1), (2, 2), (3, 1), (255, 1)] {
        rec[1] = byte;
        assert_eq!(
            unsafe { settings_apply(&mut *a.set, rec.as_ptr(), rec.len() as u32) },
            1
        );
        assert_eq!(*a.cards, want, "byte {byte}");
        let out = a.gather();
        assert_eq!(out[1], want, "gathered after {byte}");
        assert_eq!(out[0], version, "no schema change");
    }
    *a.cards = 2;
    assert_eq!(a.gather()[1], 2, "ALWAYS is saved as 2");
}

#[test]
fn the_last_song_survives_a_power_cycle_and_old_records_default_to_none() {
    let mut a = Set::blank();
    a.load();
    unsafe {
        live_set_theme(&mut *a.rig.live, THEME_C64);
        settings_set_song(&mut *a.set, c"NIGHT DRIVE".as_ptr());
    }
    a.ticks(SETTINGS_DEBOUNCE_MS + 100);
    let mut st = a.st;
    st.reset();
    st.mount();
    let mut b = Set::new(st, VERSION);
    b.load();
    assert_eq!(b.rig.live.theme, THEME_C64 as u8);
    assert_eq!(
        unsafe { CStr::from_ptr(b.set.last_song.as_ptr()) },
        c"NIGHT DRIVE"
    );
    let old = include_bytes!("../data/settings-v1.bin");
    unsafe { settings_apply(&mut *b.set, old.as_ptr(), old.len() as u32) };
    assert_eq!(b.set.last_song[0], 0, "v1 has no last song");
}

#[test]
fn schema_four_fixture_keeps_every_earlier_field_and_the_last_song() {
    let mut a = Set::blank();
    let v3 = include_bytes!("../data/settings-v3.bin");
    assert_eq!(
        unsafe { settings_apply(&mut *a.set, v3.as_ptr(), v3.len() as u32) },
        1
    );
    assert_eq!(a.set.last_song[0], 0);
    unsafe { settings_set_song(&mut *a.set, c"NIGHT DRIVE".as_ptr()) };
    let mut rec = a.gather();
    rec.truncate(124);
    rec[0] = 4;
    assert_eq!(rec.len(), 124);
    assert_eq!(rec[0], 4);
    assert_eq!(&rec[1..108], &v3[1..]);
    assert_eq!(&rec[108..124], b"NIGHT DRIVE\0\0\0\0\0");
    let path = fixture_path().with_file_name("settings-v4.bin");
    if std::env::var_os("SIDOU_SETTINGS_V4_FIXTURE_UPDATE").is_some() {
        assert_eq!(SETTINGS_VERSION, 4, "the v4 generator requires schema 4");
        std::fs::write(&path, &rec).unwrap();
    }
    assert_eq!(std::fs::read(path).expect("schema 4 fixture"), rec);
    let mut b = Set::blank();
    assert_eq!(
        unsafe { settings_apply(&mut *b.set, rec.as_ptr(), rec.len() as u32) },
        1
    );
    let migrated = b.gather();
    assert_eq!(
        &migrated[1..124],
        &rec[1..],
        "exact schema 4 prefix migration"
    );
    assert!(b
        .rig
        .live
        .expert
        .iter()
        .all(|&v| v == SID_EXPERT_DEFAULT as u8));
}

#[test]
fn every_schema_migrates_from_nondefaults_without_changing_its_prefix() {
    for (schema, len, rows, theme, song) in [
        (1, 106, 0, THEME_HIGH_CONTRAST, c""),
        (2, 107, 1, THEME_HIGH_CONTRAST, c""),
        (3, 108, 1, THEME_C64, c""),
        (4, 124, 1, THEME_C64, c"NIGHT DRIVE"),
    ] {
        let path = fixture_path().with_file_name(format!("settings-v{schema}.bin"));
        let rec = std::fs::read(path).unwrap();
        assert_eq!(rec.len(), len);
        assert_eq!(rec[0], schema);
        let mut s = Set::blank();
        *s.rows = 1 - rows;
        unsafe {
            live_set_theme(&mut *s.rig.live, THEME_COUNT - 1 - theme);
            settings_set_song(&mut *s.set, c"OLD SONG".as_ptr());
        }
        assert_eq!(
            unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
            1,
            "schema {schema}"
        );
        assert_eq!(*s.rows, rows, "schema {schema}: TRACKER ROWS");
        assert_eq!(u32::from(s.rig.live.theme), theme, "schema {schema}: THEME");
        assert_eq!(unsafe { CStr::from_ptr(s.set.last_song.as_ptr()) }, song);
        assert_eq!(&s.gather()[1..len], &rec[1..], "unchanged schema prefix");
        s.rig.tick();
        let mut want = Set::blank();
        want.change_everything();
        assert_eq!(s.snapshot(), want.snapshot(), "schema {schema}: v1 fields");
    }
}

#[test]
fn short_records_are_rejected_before_any_field_changes_for_every_schema() {
    let mut s = Set::blank();
    s.change_everything();
    *s.rows = 1;
    unsafe {
        live_set_theme(&mut *s.rig.live, THEME_C64);
        settings_set_song(&mut *s.set, c"OLD SONG".as_ptr());
    }
    let before = s.gather();
    for (schema, len) in [
        (1, 106),
        (2, 107),
        (3, 108),
        (4, 124),
        (5, SETTINGS_BYTES),
        (6, SETTINGS_BYTES),
    ] {
        let mut rec = Set::blank().gather();
        rec[0] = schema;
        for short in 0..len {
            assert_eq!(
                unsafe { settings_apply(&mut *s.set, rec.as_ptr(), short) },
                0,
                "schema {schema}: length {short}"
            );
            assert_eq!(s.gather(), before, "rejected records apply nothing");
        }
    }
}

#[test]
fn theme_validation_applies_to_schema_three_four_and_future_prefixes() {
    for schema in [3, 4, 5, 6] {
        for theme in [THEME_HIGH_CONTRAST, THEME_C64, THEME_COUNT, 255] {
            let mut rec = Set::blank().gather();
            rec[0] = schema;
            rec[107] = theme as u8;
            if schema == 3 {
                rec.truncate(108);
            } else if schema == 4 {
                rec.truncate(124);
            } else if schema == 6 {
                rec.extend_from_slice(&[0xa5; 20]);
            }
            let mut s = Set::blank();
            unsafe { live_set_theme(&mut *s.rig.live, THEME_C64) };
            assert_eq!(
                unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
                1
            );
            let want = if theme < THEME_COUNT {
                theme
            } else {
                THEME_HIGH_CONTRAST
            };
            assert_eq!(u32::from(s.rig.live.theme), want);
            assert_eq!(s.gather()[107], want as u8);
        }
    }
}

thread_local! {
    /// The force hook's calls, in order (settings.force).
    static FORCED: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
}

unsafe extern "C" fn force(on: i32) {
    FORCED.with(|f| {
        let (ons, offs) = f.get();
        f.set(if on != 0 {
            (ons + 1, offs)
        } else {
            (ons, offs + 1)
        });
    });
}

fn forced() -> (u32, u32) {
    FORCED.with(Cell::get)
}

/// An instrument, its learn table, CARDS and the UI sounds, and the settings on a store.
struct Set {
    rig: Rig,
    ml: Box<midi_learn>,
    cards: Box<u8>,
    rows: Box<u8>,
    snd: Box<ui_sound>,
    st: Store,
    set: Box<settings>,
    now: u32,
    /// Writes the settings' ticks began.
    writes: u32,
    /// The store is quiet: maintenance runs (else only writes).
    quiet: bool,
}

impl Set {
    fn new(st: Store, version: &CStr) -> Self {
        let mut rig = Rig::new(SID_MODEL_6581);
        let mut ml = Box::<midi_learn>::default();
        let mut cards = Box::new(1u8);
        let mut rows = Box::new(0u8);
        let mut snd = Box::<ui_sound>::default();
        let rec: *mut Recorder = &mut *rig.rec;
        let mut s = Set {
            set: Box::default(),
            rig,
            ml: Box::default(),
            cards: Box::new(0),
            rows: Box::new(0),
            snd: Box::default(),
            st,
            now: 1000,
            writes: 0,
            quiet: true,
        };
        unsafe {
            midi_learn_init(&mut *ml);
            ui_sound_init(&mut *snd, &mut *s.rig.live, Some(record), rec.cast());
            settings_init(
                &mut *s.set,
                &mut *s.rig.live,
                &mut *ml,
                &mut *cards,
                &mut *rows,
                &mut *snd,
                &mut *s.st.st,
                version.as_ptr(),
                Some(force),
            );
        }
        s.ml = ml;
        s.cards = cards;
        s.rows = rows;
        s.snd = snd;
        FORCED.with(|f| f.set((0, 0)));
        s
    }

    /// On a fresh, blank store with its pool filled.
    fn blank() -> Self {
        Self::new(fresh(1, 256), VERSION)
    }

    fn load(&mut self) -> bool {
        (unsafe { settings_load(&mut *self.set, self.now) }) != 0
    }

    /// The settings' tick, then the store's polls for it (as the device's main loop runs
    /// them). A write the tick began counts in `writes`.
    fn tick(&mut self) {
        self.now += TICK;
        let busy = self.st.busy();
        unsafe { settings_tick(&mut *self.set, self.now) };
        if !busy && self.st.busy() {
            self.writes += 1;
        }
        unsafe {
            store_quiet(
                &mut *self.st.st,
                i32::from(self.quiet || self.set.forcing != 0),
            )
        };
        for _ in 0..64 {
            if unsafe { store_poll(&mut *self.st.st) } == 0 {
                break;
            }
        }
    }

    fn ticks(&mut self, ms: u32) {
        for _ in 0..ms / TICK {
            self.tick();
        }
    }

    fn gather(&self) -> Vec<u8> {
        let mut out = vec![0u8; SETTINGS_BYTES as usize];
        let n = unsafe { settings_gather(&*self.set, out.as_mut_ptr()) };
        out.truncate(n as usize);
        out
    }

    fn stored(&self) -> Option<Vec<u8>> {
        self.st.get(SETTINGS_KEY)
    }

    fn param(&self, p: u32) -> u32 {
        unsafe { live_param_get(&*self.rig.live, p) }
    }

    fn set_param(&mut self, p: u32, v: u32) {
        unsafe { live_param_set(&mut *self.rig.live, p, v) };
    }

    fn result(&mut self) -> settings_result {
        unsafe { settings_take_result(&mut *self.set) }
    }

    /// The instrument and the settings as they stand, all of what persists.
    fn snapshot(&self) -> (Vec<u32>, u8, u8, u8, u8, Vec<u8>) {
        let params = (0..LIVE_PARAM_COUNT)
            .filter(|&p| p != LIVE_PARAM_VOLUME)
            .map(|p| self.param(p))
            .collect();
        let ccs = (0..LIVE_PARAM_COUNT)
            .map(|p| unsafe { midi_learn_cc_of(&*self.ml, p) })
            .collect();
        (
            params,
            *self.cards,
            self.snd.on,
            self.rig.live.keys_mode,
            self.rig.live.preset,
            ccs,
        )
    }

    /// Every kind of persisted value away from its default.
    fn change_everything(&mut self) {
        *self.cards = 0;
        unsafe { ui_sound_set(&mut *self.snd, 1) };
        unsafe { live_set_keys_mode(&mut *self.rig.live, LIVE_KEYS_A) };
        self.rig.set_preset(2);
        self.set_param(LIVE_PARAM_CUTOFF, 1234);
        self.set_param(LIVE_PARAM_PULSE_WIDTH, 3001);
        self.set_param(LIVE_PARAM_RESONANCE, 9);
        self.set_param(LIVE_PARAM_ENVELOPE_MODE, LIVE_ENVELOPE_AUTHENTIC);
        self.set_param(LIVE_PARAM_MODEL, 1);
        self.set_param(LIVE_PARAM_PROFILE, 2);
        self.set_param(LIVE_PARAM_AGE, 77);
        self.set_param(LIVE_PARAM_BLEED, 5);
        self.set_param(LIVE_PARAM_SLOT + LIVE_SLOT_ALGORITHM, 3);
        self.set_param(LIVE_PARAM_SLOT + 2 * LIVE_SLOT_FIELDS + LIVE_SLOT_MIX, 99);
        self.set_param(LIVE_PARAM_VIBRATO_DEPTH, 40);
        unsafe { midi_learn_bind(&mut *self.ml, 21, LIVE_PARAM_CUTOFF) };
        unsafe { midi_learn_bind(&mut *self.ml, 30, LIVE_PARAM_AGE) };
        self.rig.tick();
    }
}

#[test]
fn schema_four_last_song_migrates_without_becoming_expert_values() {
    let mut rec = std::fs::read(v3_fixture_path()).unwrap();
    rec[0] = 4;
    rec.extend_from_slice(b"NIGHT DRIVE\0\0\0\0\0");
    let mut s = Set::blank();
    s.set_param(LIVE_PARAM_EXPERT, 12);
    unsafe { midi_learn_bind(&mut *s.ml, 31, LIVE_PARAM_EXPERT) };
    assert_eq!(
        unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
        1
    );
    let migrated = s.gather();
    assert_eq!(migrated[0], 5);
    assert_eq!(&migrated[1..124], &rec[1..]);
    assert!(s
        .rig
        .live
        .expert
        .iter()
        .all(|&v| v == SID_EXPERT_DEFAULT as u8));
    assert_eq!(
        unsafe { midi_learn_cc_of(&*s.ml, LIVE_PARAM_EXPERT) },
        MIDI_MAP_NO_CC as u8
    );
    assert_eq!(migrated[124], SID_EXPERT_COUNT as u8);
    assert_eq!(migrated.len(), 125 + 2 * SID_EXPERT_COUNT as usize);
}

#[test]
fn the_record_is_v5s_layout_with_the_original_offsets_and_raw_expert_defaults() {
    let s = Set::blank();
    let rec = s.gather();
    assert_eq!(rec.len(), SETTINGS_BYTES as usize);
    assert_eq!(rec.len(), 125 + 2 * SID_EXPERT_COUNT as usize);
    assert_eq!(rec[..6], [5, 1, 0, LIVE_KEYS_AUTO as u8, 0, 42]);
    let cutoff = s.param(LIVE_PARAM_CUTOFF);
    assert_eq!(
        rec[6..8],
        [cutoff as u8, (cutoff >> 8) as u8],
        "the cutoff, two bytes"
    );
    assert_eq!(rec[8], s.param(LIVE_PARAM_RESONANCE) as u8);
    let pw = s.param(LIVE_PARAM_PULSE_WIDTH);
    assert_eq!(
        rec[9..11],
        [pw as u8, (pw >> 8) as u8],
        "the pulse width, two bytes"
    );
    assert_eq!(rec[11], 0, "the volume is the pot's");
    for p in 4..SETTINGS_PARAMS_V1 {
        assert_eq!(u32::from(rec[p as usize + 8]), s.param(p), "parameter {p}");
    }
    assert!(
        rec[50..92].iter().all(|&b| b == MIDI_MAP_NO_CC as u8),
        "no bindings"
    );
    assert_eq!(&rec[92..98], b"V1.2.3");
    assert!(rec[98..106].iter().all(|&b| b == 0), "NUL-padded");
    assert_eq!(rec[106], 0, "BAR");
    assert_eq!(rec[107], THEME_HIGH_CONTRAST as u8, "HIGH CONTRAST");
    assert_eq!(&rec[108..124], &[0; 16], "built-in song");
    assert_eq!(rec[124], SID_EXPERT_COUNT as u8);
    let end = 125 + SID_EXPERT_COUNT as usize;
    assert!(rec[125..end].iter().all(|&v| v == SID_EXPERT_DEFAULT as u8));
    assert!(rec[end..].iter().all(|&v| v == MIDI_MAP_NO_CC as u8));
}

#[test]
fn expert_raw_overrides_and_learn_bindings_survive_loading_after_macros() {
    let mut a = Set::blank();
    a.change_everything();
    for e in 0..SID_EXPERT_COUNT {
        a.set_param(LIVE_PARAM_EXPERT + e, (e * 7) % 128);
    }
    a.rig.live.expert[0] = SID_EXPERT_DEFAULT as u8;
    unsafe { midi_learn_bind(&mut *a.ml, 31, LIVE_PARAM_EXPERT + SID_EXPERT_COUNT - 1) };
    let rec = a.gather();
    let mut b = Set::blank();
    assert_eq!(
        unsafe { settings_apply(&mut *b.set, rec.as_ptr(), rec.len() as u32) },
        1
    );
    assert_eq!(b.rig.live.expert, a.rig.live.expert);
    assert_ne!(b.rig.live.expert_dirty, 0);
    assert_ne!(b.rig.live.expert_active, 0);
    assert_eq!(b.gather(), rec);
}

#[test]
fn older_settings_clear_expert_overrides_and_bindings() {
    for version in 1..=4 {
        let rec = std::fs::read(fixture_path().with_file_name(format!("settings-v{version}.bin")))
            .unwrap();
        let mut s = Set::blank();
        s.set_param(LIVE_PARAM_EXPERT, 12);
        unsafe { midi_learn_bind(&mut *s.ml, 31, LIVE_PARAM_EXPERT) };
        assert_eq!(
            unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
            1
        );
        assert!(s
            .rig
            .live
            .expert
            .iter()
            .all(|&v| v == SID_EXPERT_DEFAULT as u8));
        assert_eq!(s.rig.live.expert_active, 0);
        assert_eq!(
            unsafe { midi_learn_cc_of(&*s.ml, LIVE_PARAM_EXPERT) },
            MIDI_MAP_NO_CC as u8
        );
    }
}

#[test]
fn malformed_expert_sections_apply_nothing() {
    let mut s = Set::blank();
    let before = s.gather();
    let mut rec = before.clone();
    rec[1] ^= 1;
    for len in SETTINGS_V3_BYTES as usize..rec.len() {
        assert_eq!(
            unsafe { settings_apply(&mut *s.set, rec.as_ptr(), len as u32) },
            0
        );
        assert_eq!(s.gather(), before);
    }
    rec[124] = SID_EXPERT_COUNT as u8 - 1;
    assert_eq!(
        unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
        0
    );
    assert_eq!(s.gather(), before);
}

#[test]
fn future_expert_values_are_skipped_before_the_learn_bindings() {
    let mut a = Set::blank();
    a.set_param(LIVE_PARAM_EXPERT, 17);
    unsafe { midi_learn_bind(&mut *a.ml, 31, LIVE_PARAM_EXPERT) };
    let mut rec = a.gather();
    let end = 125 + SID_EXPERT_COUNT as usize;
    rec[0] = SETTINGS_VERSION as u8 + 1;
    rec[124] += 1;
    rec.insert(end, 19);
    rec.push(MIDI_MAP_NO_CC as u8);
    let mut st = fresh(10, 256);
    assert_eq!(
        st.put_typed(SETTINGS_KEY, SETTINGS_TYPE as u8, &rec),
        STORE_OK
    );
    st.settle();
    let mut b = Set::new(st, VERSION);
    assert!(b.load());
    assert_eq!(b.rig.live.expert, a.rig.live.expert);
    assert_eq!(unsafe { midi_learn_cc_of(&*b.ml, LIVE_PARAM_EXPERT) }, 31);
}

#[test]
fn invalid_expert_overrides_fall_back_to_profile_following() {
    let mut s = Set::blank();
    let mut rec = s.gather();
    s.set_param(LIVE_PARAM_EXPERT, 17);
    rec[125] = 128;
    assert_eq!(
        unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
        1
    );
    assert_eq!(s.rig.live.expert[0], SID_EXPERT_DEFAULT as u8);
}

#[test]
fn the_v5_fixture_preserves_the_v4_prefix_and_raw_expert_values() {
    let rec: Vec<u8> = include_str!("../data/settings-v5.hex")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .flat_map(str::split_whitespace)
        .map(|v| u8::from_str_radix(v, 16).unwrap())
        .collect();
    let old = include_bytes!("../data/settings-v4.bin");
    assert_eq!(&rec[1..124], &old[1..]);
    let mut s = Set::blank();
    assert_eq!(
        unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
        1
    );
    assert_eq!(&s.rig.live.expert[..3], &[SID_EXPERT_DEFAULT as u8, 0, 127]);
    assert_eq!(s.gather(), rec);
}

#[test]
fn every_persisted_value_round_trips_and_the_volume_does_not() {
    let mut a = Set::blank();
    a.change_everything();
    let rec = a.gather();
    let mut b = Set::blank();
    b.rig.set_master(100);
    let volume = b.rig.live.volume;
    let same = unsafe { settings_apply(&mut *b.set, rec.as_ptr(), rec.len() as u32) };
    assert_eq!(same, 1, "the same firmware version");
    b.rig.tick();
    assert_eq!(b.snapshot(), a.snapshot());
    assert_eq!(b.rig.live.volume, volume, "the pot's, as it was");
    // The master pot changes no record: no background save for it.
    a.rig.set_master(1000);
    assert_eq!(a.gather(), rec);
}

#[test]
fn dice_is_saved_and_loaded() {
    let mut a = Set::blank();
    a.rig.preset_step(4);
    assert_eq!(a.rig.live.preset, LIVE_PRESET_COUNT as u8);
    a.rig.tick();
    let rec = a.gather();
    assert_eq!(
        rec[4], LIVE_PRESET_COUNT as u8,
        "the preset byte holds DICE"
    );
    let mut b = Set::blank();
    let same = unsafe { settings_apply(&mut *b.set, rec.as_ptr(), rec.len() as u32) };
    assert_eq!(same, 1);
    b.rig.tick();
    assert_eq!(b.rig.live.preset, LIVE_PRESET_COUNT as u8, "back on DICE");
    assert_eq!(
        b.snapshot(),
        a.snapshot(),
        "the roll's parameters came back"
    );
}

#[test]
fn tracker_rows_round_trips_both_modes() {
    for rows in [0, 1] {
        let mut a = Set::blank();
        *a.rows = rows;
        let rec = a.gather();
        assert_eq!(rec.len(), SETTINGS_BYTES as usize);
        assert_eq!(rec[0], SETTINGS_VERSION as u8);
        assert_eq!(rec[106], rows);
        let mut b = Set::blank();
        *b.rows = 1 - rows;
        assert_eq!(
            unsafe { settings_apply(&mut *b.set, rec.as_ptr(), rec.len() as u32) },
            1
        );
        assert_eq!(*b.rows, rows);
    }
}

#[test]
fn invalid_tracker_rows_falls_back_to_bar() {
    for (schema, len) in [(2, 107), (3, 108), (4, 124), (5, SETTINGS_BYTES as usize)] {
        for rows in [2, 255] {
            let mut s = Set::blank();
            let mut rec = s.gather();
            rec.resize(len, 0);
            rec[0] = schema;
            rec[106] = rows;
            *s.rows = 1;
            assert_eq!(
                unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
                1
            );
            assert_eq!(*s.rows, 0, "schema {schema}: invalid TRACKER ROWS");
        }
    }
}

#[test]
fn filename_validation_clears_invalid_records_but_keeps_a_full_length_name() {
    for schema in [4, 5, 6] {
        for name in [
            b" SIXTEEN CHARS!!".as_slice(),
            b"LOWERcase",
            b"BAD\x1f",
            b"BAD`",
        ] {
            let mut s = Set::blank();
            unsafe { settings_set_song(&mut *s.set, c"OLD SONG".as_ptr()) };
            let mut rec = s.gather();
            rec[0] = schema;
            rec[108..124].fill(0);
            rec[108..108 + name.len()].copy_from_slice(name);
            assert_eq!(
                unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
                1
            );
            assert_eq!(
                s.set.last_song[0], 0,
                "invalid filename selects the built-in song"
            );
        }
    }
    let mut s = Set::blank();
    let mut rec = s.gather();
    rec[108..124].copy_from_slice(b"SIXTEEN CHARS!!!");
    assert_eq!(
        unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
        1
    );
    assert_eq!(
        unsafe { CStr::from_ptr(s.set.last_song.as_ptr()) },
        c"SIXTEEN CHARS!!!"
    );
    assert_eq!(s.gather(), rec);
}

#[test]
fn a_first_boot_plays_the_full_boot_and_saves_the_version_then_the_short_one() {
    let mut s = Set::blank();
    let before = s.snapshot();
    assert!(!s.load(), "no record: the full boot");
    assert_eq!(s.snapshot(), before, "M1's defaults");
    s.ticks(SETTINGS_DEBOUNCE_MS - 20);
    assert_eq!(s.stored(), None, "not before the debounce");
    s.ticks(40);
    assert_eq!(s.stored(), Some(s.gather()), "the version stored");
    assert_eq!(s.writes, 1);
    // The next boot, from the flash.
    let mut st = s.st;
    st.reset();
    assert_eq!(st.mount(), STORE_STATE_SIDOU);
    let mut s = Set::new(st, VERSION);
    assert!(s.load(), "the short boot");
    s.ticks(2 * SETTINGS_DEBOUNCE_MS);
    assert_eq!(s.writes, 0, "nothing changed: nothing written");
}

#[test]
fn settings_survive_a_power_cycle_and_a_new_version_plays_the_full_boot_once() {
    let mut s = Set::blank();
    s.load();
    s.change_everything();
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    let want = s.snapshot();
    let mut st = s.st;
    st.reset();
    st.mount();
    let mut s = Set::new(st, c"V1.3.0");
    assert!(!s.load(), "another version: the full boot");
    s.rig.tick();
    assert_eq!(s.snapshot(), want, "the settings still apply");
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    assert_eq!(s.writes, 1, "the new version stored");
    let mut st = s.st;
    st.reset();
    st.mount();
    let mut s = Set::new(st, c"V1.3.0");
    assert!(s.load(), "then the short boot");
    s.rig.tick();
    assert_eq!(s.snapshot(), want);
}

#[test]
fn an_unchanged_frame_does_not_gather_and_a_setter_change_is_saved() {
    let mut s = Set::blank();
    s.load();
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    let seen = s.set.seen;
    let cutoff = s.rig.live.cutoff;
    // Bypass the read-only live API to detect an unwanted gather on an idle frame.
    s.rig.live.cutoff ^= 1;
    s.tick();
    assert_eq!(
        s.set.seen, seen,
        "an unchanged frame must not gather or CRC"
    );
    s.rig.live.cutoff = cutoff;
    s.set_param(LIVE_PARAM_CUTOFF, u32::from(cutoff ^ 1));
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    assert_eq!(s.writes, 2);
    assert_eq!(s.stored(), Some(s.gather()));
}

#[test]
fn every_parameter_setter_marks_a_change_for_the_background_save() {
    for p in 0..LIVE_PARAM_COUNT {
        if p == LIVE_PARAM_VOLUME {
            continue;
        }
        let mut s = Set::blank();
        s.load();
        s.ticks(SETTINGS_DEBOUNCE_MS + 100);
        let value = (s.param(p) + 1) % (unsafe { live_param_max(p) } + 1);
        s.set_param(p, value);
        s.ticks(SETTINGS_DEBOUNCE_MS - 20);
        assert_eq!(s.writes, 1, "parameter {p}: must debounce");
        s.ticks(120);
        assert_eq!(s.writes, 2, "parameter {p}: must save");
        assert_eq!(s.stored(), Some(s.gather()), "parameter {p}");
    }
}

#[test]
fn direct_persisted_setters_are_background_saved_but_controllers_are_not() {
    let changes: &[fn(&mut Set)] = &[
        |s| unsafe { live_knob(&mut *s.rig.live, LIVE_KNOB_CUTOFF, 1) },
        |s| unsafe { live_knob(&mut *s.rig.live, LIVE_KNOB_RESONANCE, 1) },
        |s| unsafe { live_knob(&mut *s.rig.live, LIVE_KNOB_RELEASE, 1) },
        |s| s.rig.set_preset(1),
        |s| unsafe { live_button(&mut *s.rig.live, LIVE_BUTTON_FX) },
        |s| unsafe { live_slot_set(&mut *s.rig.live, 0, LIVE_SLOT_MIX, 100) },
        |s| unsafe { live_set_keys_mode(&mut *s.rig.live, LIVE_KEYS_A) },
        |s| unsafe { live_bend_range_rpn(&mut *s.rig.live, 12) },
        |s| unsafe { ui_sound_set(&mut *s.snd, 1) },
        |s| unsafe { live_set_theme(&mut *s.rig.live, THEME_C64) },
    ];
    for (i, change) in changes.iter().enumerate() {
        let mut s = Set::blank();
        s.load();
        s.ticks(SETTINGS_DEBOUNCE_MS + 100);
        change(&mut s);
        s.ticks(SETTINGS_DEBOUNCE_MS + 100);
        assert_eq!(s.writes, 2, "setter {i}");
        assert_eq!(s.stored(), Some(s.gather()), "setter {i}");
    }
    let mut s = Set::blank();
    s.load();
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    unsafe {
        live_set_master(&mut *s.rig.live, 10);
        live_bend(&mut *s.rig.live, 4000);
        live_pressure(&mut *s.rig.live, 80);
        live_button(&mut *s.rig.live, LIVE_BUTTON_OCT_UP);
        live_note_on(&mut *s.rig.live, 60, 100);
    }
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    assert_eq!(s.writes, 1, "transient controls must not save");
    assert_eq!(s.rig.live.settings_dirty, 0);
}

#[test]
fn binding_forgetting_and_clearing_are_background_saved() {
    let mut s = Set::blank();
    s.load();
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    unsafe { midi_learn_bind(&mut *s.ml, 21, LIVE_PARAM_CUTOFF) };
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    assert_eq!(s.writes, 2);
    assert_eq!(s.stored(), Some(s.gather()));
    unsafe { midi_learn_forget(&mut *s.ml, LIVE_PARAM_CUTOFF) };
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    assert_eq!(s.writes, 3);
    assert_eq!(s.stored(), Some(s.gather()));
    unsafe { midi_learn_bind(&mut *s.ml, 22, LIVE_PARAM_RESONANCE) };
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    unsafe { midi_learn_clear(&mut *s.ml) };
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    assert_eq!(s.writes, 5);
    assert_eq!(s.stored(), Some(s.gather()));
}

#[test]
fn a_burst_of_changes_is_one_write_five_seconds_after_the_last() {
    let mut s = Set::blank();
    assert!(!s.load());
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    assert_eq!(s.writes, 1, "the first boot's version");
    for i in 0..40 {
        s.set_param(LIVE_PARAM_CUTOFF, 100 + i * 10);
        s.ticks(100);
    }
    assert_eq!(s.writes, 1, "nothing while the knob turns");
    s.ticks(SETTINGS_DEBOUNCE_MS - 200);
    assert_eq!(s.writes, 1, "nor before 5 s");
    s.ticks(300);
    assert_eq!(s.writes, 2, "one write for the burst");
    assert_eq!(s.stored(), Some(s.gather()));
    // Back to a value already stored and away again within the debounce: still one write.
    s.set_param(LIVE_PARAM_CUTOFF, 5);
    s.ticks(1000);
    s.set_param(LIVE_PARAM_CUTOFF, 490);
    s.ticks(2 * SETTINGS_DEBOUNCE_MS);
    assert_eq!(s.writes, 2, "the stored value again: nothing to write");
    let result = s.result();
    assert_eq!(result, SETTINGS_NOTHING, "background saves say nothing");
}

#[test]
fn save_now_skips_the_debounce_and_tells_when_it_commits() {
    let mut s = Set::blank();
    s.load();
    *s.cards = 0;
    unsafe { settings_save_now(&mut *s.set, s.now) };
    assert_eq!(s.result(), SETTINGS_NOTHING, "not committed yet");
    s.tick();
    s.tick();
    assert_eq!(s.result(), SETTINGS_SAVED);
    assert_eq!(s.result(), SETTINGS_NOTHING, "said once");
    assert_eq!(s.stored(), Some(s.gather()));
    assert_eq!(forced(), (0, 0), "a store with its pool needs no force");
    s.ticks(2 * SETTINGS_DEBOUNCE_MS);
    assert_eq!(
        s.writes, 0,
        "the debounce has nothing left (the version is in that save)"
    );
    // Nothing changed: an explicit save still says SAVED.
    unsafe { settings_save_now(&mut *s.set, s.now) };
    s.ticks(30);
    assert_eq!(s.result(), SETTINGS_SAVED);
}

/// A store whose pool is short (not quiet since its mount: the device was never quiet) takes
/// no write; an explicit save waits, and after SETTINGS_FORCE_MS forces the held operations
/// (the store refills while forced), says SAVED (GAPS), and the force ends.
#[test]
fn an_explicit_save_on_a_short_pool_forces_the_held_operations_and_says_so() {
    let mut st = Store::new(crate::store::Sim::new(2), 256);
    assert_eq!(st.mount(), STORE_STATE_BLANK);
    let mut s = Set::new(st, VERSION);
    s.quiet = false;
    s.load();
    unsafe { settings_save_now(&mut *s.set, s.now) };
    s.ticks(SETTINGS_FORCE_MS - 20);
    assert_eq!((s.stored(), forced()), (None, (0, 0)), "waits, unforced");
    s.ticks(40);
    assert_eq!(forced().0, 1, "forced after SETTINGS_FORCE_MS");
    for _ in 0..1000 {
        s.tick();
        if s.result() == SETTINGS_SAVED_GAPS {
            break;
        }
        assert!(s.set.asked != 0, "it ended without saying so");
    }
    assert_eq!(s.stored(), Some(s.gather()));
    assert_eq!(forced(), (1, 1), "the force ends with the save");
}

#[test]
fn a_foreign_store_keeps_nothing_and_an_explicit_save_says_so() {
    let mut region = vec![0xFFu8; (STORE_HI - STORE_LO) as usize];
    region[5 * STORE_SECTOR as usize + 7] = 0x42;
    let mut st = Store::new(
        crate::store::Sim::with_region(3, &region, [false; STORE_SECTORS as usize]),
        256,
    );
    assert_eq!(st.mount(), STORE_STATE_FOREIGN);
    let mut s = Set::new(st, VERSION);
    assert!(!s.load(), "nothing read: the full boot");
    s.change_everything();
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    unsafe { settings_save_now(&mut *s.set, s.now) };
    s.tick();
    assert_eq!(s.result(), SETTINGS_FOREIGN);
    assert_eq!(s.st.sim.region(), &region[..], "not a byte written");
    assert_eq!(forced(), (0, 0));
}

#[test]
fn a_record_that_is_not_one_applies_nothing() {
    let mut s = Set::blank();
    let before = s.snapshot();
    let mut good = Set::blank();
    good.change_everything();
    let rec = good.gather();
    for bad in [
        rec[..SETTINGS_V1_BYTES as usize - 1].to_vec(), // short
        [&[0u8][..], &rec[1..]].concat(),               // schema 0
        {
            let mut r = rec.clone();
            r[5] = 41; // fewer parameters than v1's
            r
        },
    ] {
        let same = unsafe { settings_apply(&mut *s.set, bad.as_ptr(), bad.len() as u32) };
        assert_eq!(same, 0);
        assert_eq!(s.snapshot(), before);
    }
}

/// A later schema appends fields: this firmware reads a newer record's v1 part.
#[test]
fn a_newer_schemas_record_applies_its_v1_part() {
    let mut good = Set::blank();
    good.change_everything();
    *good.rows = 1;
    unsafe {
        live_set_theme(&mut *good.rig.live, THEME_C64);
        settings_set_song(&mut *good.set, c"NIGHT DRIVE".as_ptr());
    }
    let prefix = good.gather();
    let mut rec = good.gather();
    rec[0] = SETTINGS_VERSION as u8 + 1;
    rec.extend_from_slice(&[0xA5; 20]);
    let mut s = Set::blank();
    let same = unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) };
    assert_eq!(same, 1);
    s.rig.tick();
    assert_eq!(s.snapshot(), good.snapshot());
    assert_eq!(s.gather(), prefix, "the entire known prefix survives");
}

/// An out-of-range value (a corrupt field the record's CRC still matched, or a range a later
/// firmware narrowed) is skipped: the parameter keeps its value.
#[test]
fn an_out_of_range_value_is_skipped() {
    let mut s = Set::blank();
    let mut rec = s.gather();
    let reso = s.param(LIVE_PARAM_RESONANCE);
    rec[8] = 200;
    unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) };
    assert_eq!(s.param(LIVE_PARAM_RESONANCE), reso);
}

fn fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/settings-v1.bin")
}

/// `tests/data/settings-v1.bin` is a schema 1 record, checked in: every later schema must load
/// it to the same values, the fields it lacks at their defaults.
/// `SIDOU_SETTINGS_FIXTURE_UPDATE=1` writes it, for schema 1 only.
#[test]
fn a_schema_1_record_loads_to_the_same_values() {
    let path = fixture_path();
    let mut want = Set::blank();
    want.change_everything();
    if std::env::var_os("SIDOU_SETTINGS_FIXTURE_UPDATE").is_some() {
        assert_eq!(
            SETTINGS_VERSION, 1,
            "the v1 fixture is written by schema 1 only"
        );
        std::fs::write(&path, want.gather()).unwrap();
    }
    let rec = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(rec.len(), 106);
    let mut s = Set::blank();
    *s.rows = 1;
    let same = unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) };
    assert_eq!(same, 1, "written by V1.2.3");
    s.rig.tick();
    assert_eq!(s.snapshot(), want.snapshot());
    assert_eq!(s.rig.live.octave, 0, "the octave does not persist");
    assert_eq!(*s.rows, 0, "schema 1 defaults to BAR, even after SCROLL");
}

#[test]
fn a_schema_2_record_loads_scroll_and_keeps_the_v1_prefix() {
    let mut rec = std::fs::read(fixture_path()).unwrap();
    rec[0] = 2;
    rec.push(1);
    let path = fixture_path().with_file_name("settings-v2.bin");
    let fixture = std::fs::read(path).unwrap();
    assert_eq!(
        fixture, rec,
        "only the version and appended byte differ from v1"
    );
    let mut s = Set::blank();
    assert_eq!(
        unsafe { settings_apply(&mut *s.set, fixture.as_ptr(), fixture.len() as u32) },
        1
    );
    assert_eq!(*s.rows, 1);
    let mut again = fixture.clone();
    again[0] = SETTINGS_VERSION as u8;
    again.push(THEME_HIGH_CONTRAST as u8);
    again.extend_from_slice(&[0; 16]);
    again.push(SID_EXPERT_COUNT as u8);
    again.extend(std::iter::repeat_n(
        SID_EXPERT_DEFAULT as u8,
        SID_EXPERT_COUNT as usize,
    ));
    again.extend(std::iter::repeat_n(
        MIDI_MAP_NO_CC as u8,
        SID_EXPERT_COUNT as usize,
    ));
    assert_eq!(
        s.gather(),
        again,
        "schema 5 preserves schema 2's bytes and appends default theme, song and expert fields"
    );
    s.rig.tick();
    let mut want = Set::blank();
    want.change_everything();
    assert_eq!(s.snapshot(), want.snapshot());
}

fn v3_fixture_path() -> std::path::PathBuf {
    fixture_path().with_file_name("settings-v3.bin")
}

/// `tests/data/settings-v3.bin` is a schema 3 record, checked in: schema 2's with the version
/// bumped and the THEME byte appended (C64).
#[test]
fn a_schema_3_record_loads_the_c64_theme_and_keeps_the_v2_prefix() {
    let mut rec = std::fs::read(fixture_path().with_file_name("settings-v2.bin")).unwrap();
    rec[0] = 3;
    rec.push(THEME_C64 as u8);
    let fixture = std::fs::read(v3_fixture_path()).unwrap();
    assert_eq!(
        fixture, rec,
        "only the version and the appended byte differ from v2"
    );
    let mut s = Set::blank();
    assert_eq!(
        unsafe { settings_apply(&mut *s.set, fixture.as_ptr(), fixture.len() as u32) },
        1
    );
    assert_eq!(s.rig.live.theme, THEME_C64 as u8);
    assert_eq!(*s.rows, 1);
    let gathered = s.gather();
    assert_eq!(
        &gathered[1..108],
        &fixture[1..],
        "schema 3's prefix survives"
    );
    assert!(gathered[108..124].iter().all(|&b| b == 0));
}

#[test]
fn the_theme_round_trips_and_an_older_record_loads_high_contrast() {
    for theme in [THEME_HIGH_CONTRAST, THEME_C64] {
        let mut a = Set::blank();
        unsafe { live_set_theme(&mut *a.rig.live, theme) };
        let rec = a.gather();
        assert_eq!(rec.len(), SETTINGS_BYTES as usize);
        assert_eq!(rec[107], theme as u8);
        let mut b = Set::blank();
        unsafe { live_set_theme(&mut *b.rig.live, THEME_COUNT - 1 - theme) };
        assert_eq!(
            unsafe { settings_apply(&mut *b.set, rec.as_ptr(), rec.len() as u32) },
            1
        );
        assert_eq!(u32::from(b.rig.live.theme), theme);
    }
    // Schemas 1 and 2 have no theme: it loads as the default, after any other choice.
    for v in ["settings-v1.bin", "settings-v2.bin"] {
        let rec = std::fs::read(fixture_path().with_file_name(v)).unwrap();
        let mut s = Set::blank();
        unsafe { live_set_theme(&mut *s.rig.live, THEME_C64) };
        assert_eq!(
            unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
            1
        );
        assert_eq!(s.rig.live.theme, THEME_HIGH_CONTRAST as u8, "{v}");
    }
    // A theme the firmware does not know is the default, and a schema 3 record needs its byte.
    let mut s = Set::blank();
    let mut rec = s.gather();
    rec[0] = 3;
    rec.truncate(108);
    rec[107] = 9;
    unsafe { live_set_theme(&mut *s.rig.live, THEME_C64) };
    assert_eq!(
        unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
        1
    );
    assert_eq!(s.rig.live.theme, THEME_HIGH_CONTRAST as u8);
    rec.truncate(107);
    assert_eq!(
        unsafe { settings_apply(&mut *s.set, rec.as_ptr(), rec.len() as u32) },
        0,
        "a schema 3 record that stops short is no record"
    );
}

/// A store that was never mounted (an unknown flash part, review 1.1) keeps nothing, and an
/// explicit save says so at once instead of waiting for ever.
#[test]
fn an_unmounted_store_keeps_nothing_and_an_explicit_save_says_it_failed() {
    let st = Store::new(crate::store::Sim::new(4), 256);
    let mut s = Set::new(st, VERSION);
    assert!(!s.load());
    s.change_everything();
    unsafe { settings_save_now(&mut *s.set, s.now) };
    s.tick();
    assert_eq!(s.result(), SETTINGS_FAILED);
    assert_eq!(s.set.asked, 0);
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    assert_eq!(s.st.sim.ops, 0, "the part is never touched");
    assert_eq!(forced(), (0, 0));
}

/// A program error after the store took an explicit save leaves the record undecided: the
/// save says it failed, not SAVED (review 1.4), and the debounce writes the record again.
#[test]
fn a_program_error_after_an_explicit_save_is_taken_says_failed_and_retries() {
    let mut s = Set::blank();
    s.load();
    s.ticks(SETTINGS_DEBOUNCE_MS + 100);
    *s.cards = 0;
    s.st.sim.fail = 100;
    unsafe { settings_save_now(&mut *s.set, s.now) };
    let mut result = SETTINGS_NOTHING;
    for _ in 0..100 {
        s.tick();
        result = s.result();
        if result != SETTINGS_NOTHING {
            break;
        }
    }
    assert_eq!(result, SETTINGS_FAILED);
    assert!(s.st.stats().errors > 0);
    s.st.sim.fail = 0;
    s.ticks(2 * SETTINGS_DEBOUNCE_MS);
    assert_eq!(
        s.stored(),
        Some(s.gather()),
        "written again by the debounce"
    );
    assert_eq!(s.result(), SETTINGS_NOTHING, "background saves say nothing");
}
