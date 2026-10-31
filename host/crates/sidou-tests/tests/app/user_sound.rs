// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::{CStr, CString};

use sidou_sys::*;

use crate::lab_sound::{random_sound, Sound, Xs};
use crate::store::{fresh, Rig as Store, Sim};
use crate::{hex_bytes, Rig};

const TICK: u32 = 10;
const BYTES: usize = LAB_SOUND_BYTES as usize;
const STRIDE: usize = USER_SOUND_STRIDE as usize;
const HEADER: usize = USER_SOUND_HEADER as usize;
const NAME: &CStr = c"USER SOUNDS";
const SETTINGS_AND_EDIT_SAVED_MS: u32 = SETTINGS_DEBOUNCE_MS + 500;

struct Unit {
    rig: Rig,
    st: Store,
    set: Box<settings>,
    sounds: Box<user_sounds>,
    _cards: Box<u8>,
    _rows: Box<u8>,
    now: u32,
}

impl Unit {
    fn new(st: Store) -> Self {
        let mut u = Unit {
            rig: Rig::new(SID_MODEL_6581),
            st,
            set: Box::default(),
            sounds: Box::default(),
            _cards: Box::new(1),
            _rows: Box::new(0),
            now: 1000,
        };
        unsafe {
            user_sounds_init(&mut *u.sounds, &mut *u.rig.live, &mut *u.st.st);
            settings_init(
                &mut *u.set,
                &mut *u.rig.live,
                std::ptr::null_mut(),
                &mut *u._cards,
                &mut *u._rows,
                std::ptr::null_mut(),
                &mut *u.st.st,
                c"V1.2.3".as_ptr(),
                None,
            );
            settings_sounds(&mut *u.set, &mut *u.sounds);
        }
        u
    }

    fn blank() -> Self {
        let mut u = Self::new(fresh(1, 256));
        u.boot();
        u
    }

    fn boot(&mut self) {
        unsafe {
            user_sounds_load(&mut *self.sounds);
            settings_load(&mut *self.set, self.now);
        }
    }

    fn power_cycle(self) -> Self {
        let mut st = self.st;
        st.reset();
        assert_eq!(st.mount(), STORE_STATE_SIDOU);
        st.maintain();
        let mut u = Self::new(st);
        u.boot();
        u
    }

    fn tick(&mut self) {
        self.now += TICK;
        unsafe {
            settings_tick(&mut *self.set, self.now);
            user_sounds_tick(&mut *self.sounds, self.now, None);
            store_quiet(&mut *self.st.st, 1);
        }
        for _ in 0..64 {
            if self.st.sim.dead || unsafe { store_poll(&mut *self.st.st) } == 0 {
                break;
            }
        }
    }

    fn ticks(&mut self, ms: u32) {
        for _ in 0..ms / TICK {
            self.tick();
        }
    }

    fn save(&mut self, n: u32, name: &str, s: &Sound) -> i32 {
        let name = CString::new(name).unwrap();
        unsafe { user_sound_save(&mut *self.sounds, n, name.as_ptr(), s.as_ptr(), self.now) }
    }

    fn tick_until_written(&mut self) -> (user_sound_result, u32) {
        for _ in 0..10_000 {
            if self.st.sim.dead {
                return (USER_SOUND_NOTHING, 0);
            }
            if self.sounds.result != 0 {
                let mut slot = 0;
                let r = unsafe { user_sounds_take(&mut *self.sounds, &mut slot) };
                return (r, slot);
            }
            self.tick();
        }
        panic!("the write never ended: state {}", self.sounds.state);
    }

    fn keep(&mut self, n: u32, name: &str, s: &Sound) {
        assert_eq!(self.save(n, name, s), 0, "queued");
        assert_eq!(self.tick_until_written(), (USER_SOUND_SAVED, n));
    }

    fn read(&self, n: u32) -> Option<(String, Sound)> {
        let mut name = [0 as std::ffi::c_char; LAB_SOUND_NAME as usize + 1];
        let mut s = [0u8; BYTES];
        (unsafe { user_sound_read(&*self.sounds, n, name.as_mut_ptr(), s.as_mut_ptr()) } != 0).then(
            || {
                let name = unsafe { CStr::from_ptr(name.as_ptr()) };
                (name.to_str().unwrap().to_owned(), s)
            },
        )
    }

    fn sound(&self) -> Sound {
        self.rig.sound()
    }

    fn apply(&mut self, s: &Sound) {
        unsafe { live_sound_apply(&mut *self.rig.live, s.as_ptr()) };
    }

    fn edited(&self) -> bool {
        unsafe { live_preset_edited(&*self.rig.live) != 0 }
    }

    fn base_name(&self) -> String {
        let n: Vec<u8> = (self.rig.live.base_name.iter())
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8(n).unwrap()
    }
}

fn put_user_sounds_file(st: &mut Store, typ: u32, bytes: &[u8]) {
    st.maintain();
    assert_eq!(
        st.api(|s| unsafe { store_file_begin(s, NAME.as_ptr(), typ as u8, bytes.len() as u32) }),
        STORE_OK
    );
    assert_eq!(
        st.file_stream(bytes, STORE_WINDOW_MAX as usize, None),
        STORE_OK
    );
}

fn chunk(version: u8, extra: usize, slots: usize, held: &[(usize, &str, Sound)]) -> Vec<u8> {
    let stride = STRIDE + extra;
    let mut f = vec![version, (BYTES + extra) as u8, slots as u8];
    f.resize(HEADER + slots * stride, 0);
    for (n, name, s) in held {
        let at = HEADER + n * stride;
        f[at] = 1;
        f[at + 1..at + 1 + name.len()].copy_from_slice(name.as_bytes());
        f[at + 13..at + 13 + BYTES].copy_from_slice(s);
        for i in 0..extra {
            f[at + STRIDE + i] = 0xa0 + i as u8;
        }
    }
    f
}

fn sounds(seed: u32, n: usize) -> Vec<Sound> {
    let mut r = Xs(seed);
    (0..n).map(|_| random_sound(&mut r)).collect()
}

#[test]
fn a_slot_saves_and_reads_back() {
    let mut u = Unit::blank();
    assert_eq!(u.rig.live.sounds_valid, 0);
    let s = sounds(7, 1)[0];
    u.apply(&s);
    let kept = u.sound();
    assert_eq!(u.save(2, "LAB 03", &kept), 0, "queued");
    assert_eq!(u.rig.live.sounds_valid, 1 << 2);
    assert_eq!(u.rig.live.user_sound, 3);
    assert_eq!(u.rig.live.base, kept);
    assert_eq!(u.base_name(), "LAB 03");
    assert!(!u.edited());
    assert_eq!(u.save(5, "LAB 06", &kept), -1, "one write at a time");
    assert_eq!(u.tick_until_written(), (USER_SOUND_SAVED, 2));
    assert_eq!(u.read(2), Some(("LAB 03".to_owned(), kept)));
    assert_eq!(u.read(1), None);
    let file = u.st.file("USER SOUNDS").expect("the file");
    assert_eq!(file.len(), USER_SOUND_FILE_BYTES as usize);
    assert_eq!(file[..HEADER], [1, BYTES as u8, USER_SOUNDS as u8]);
    let at = HEADER + 2 * STRIDE;
    assert_eq!(file[at], 1);
    assert_eq!(&file[at + 1..at + 13], b"LAB 03\0\0\0\0\0\0");
    assert_eq!(file[at + 13..at + STRIDE], kept);
    assert_eq!(
        u32::from(u.st.file_open("USER SOUNDS").unwrap().type_),
        STORE_FILE_SOUNDS
    );
    let u = u.power_cycle();
    assert_eq!(u.rig.live.sounds_valid, 1 << 2);
    assert_eq!(u.read(2), Some(("LAB 03".to_owned(), kept)));
    u.st.assert_clean();
}

#[test]
fn sixteen_slots_and_a_rewrite_keep_the_others() {
    let mut u = Unit::blank();
    let all = sounds(11, 17);
    for n in 0..USER_SOUNDS {
        u.keep(n, &format!("LAB {:02}", n + 1), &all[n as usize]);
    }
    assert_eq!(u.rig.live.sounds_valid, 0xffff);
    u.keep(5, "RENAMED", &all[16]);
    let mut u = u.power_cycle();
    assert_eq!(u.rig.live.sounds_valid, 0xffff);
    for n in 0..USER_SOUNDS as usize {
        let want = if n == 5 {
            ("RENAMED".to_owned(), all[16])
        } else {
            (format!("LAB {:02}", n + 1), all[n])
        };
        assert_eq!(u.read(n as u32), Some(want), "slot {n}");
    }
    assert_eq!(u.read(USER_SOUNDS), None);
    assert_eq!(u.save(USER_SOUNDS, "X", &all[0]), -1);
    u.st.assert_clean();
}

#[test]
fn a_cut_at_every_program_leaves_the_old_chunk_or_the_new() {
    let all = sounds(23, 5);
    let mut u = Unit::blank();
    for n in 0..4u32 {
        u.keep(n, &format!("OLD {n}"), &all[n as usize]);
    }
    u.ticks(SETTINGS_AND_EDIT_SAVED_MS);
    let region = u.st.sim.region().to_vec();
    let old: Vec<_> = (0..4).map(|n| u.read(n)).collect();
    let on_region = |seed| {
        let mut st = Store::new(
            Sim::with_region(seed, &region, [false; STORE_SECTORS as usize]),
            256,
        );
        assert_eq!(st.mount(), STORE_STATE_SIDOU);
        st.maintain();
        let mut u = Unit::new(st);
        u.boot();
        u
    };
    let mut clean = on_region(1);
    let ops = clean.st.sim.ops;
    clean.keep(1, "NEW 1", &all[4]);
    let n = clean.st.sim.ops - ops;
    assert!(n > 4, "{n} operations");
    let mut new = old.clone();
    new[1] = Some(("NEW 1".to_owned(), all[4]));
    let (mut olds, mut news) = (0, 0);
    for k in 0..=n {
        let mut u = on_region(100 + k);
        u.st.sim.cut_at = Some(u.st.sim.ops + k);
        assert_eq!(u.save(1, "NEW 1", &all[4]), 0);
        let (r, _) = u.tick_until_written();
        let dead = u.st.sim.dead;
        assert!(dead || r == USER_SOUND_SAVED, "cut {k}: {r}");
        let mut st = u.st;
        st.sim.dead = false;
        st.sim.cut_at = None;
        st.reset();
        assert_eq!(st.mount(), STORE_STATE_SIDOU, "cut {k}");
        st.maintain();
        let mut after = Unit::new(st);
        after.boot();
        let got: Vec<_> = (0..4).map(|n| after.read(n)).collect();
        assert!(got == old || got == new, "cut {k}: {got:?}");
        if got == old {
            olds += 1;
        } else {
            news += 1;
        }
        after.keep(1, "NEW 1", &all[4]);
        assert_eq!(after.read(1), new[1], "cut {k}: kept again");
        after.st.assert_clean();
    }
    println!("a keep in {n} operations: a cut left the old file {olds} times, the new {news}");
    assert!(olds > 0 && news > 0, "{olds} old, {news} new");
}

#[test]
fn the_edit_comes_back_after_a_power_cycle() {
    let s = sounds(31, 3);
    let mut u = Unit::blank();
    u.rig.preset_step(1);
    let mut edit = u.sound();
    edit[LAB_SOUND_TUNE as usize] = -12i8 as u8;
    edit[LAB_SOUND_PWM_DEPTH as usize] = 200;
    edit[LAB_SOUND_SLOTS as usize] = 2;
    edit[LAB_SOUND_SYNC_SEMIS as usize] = 0;
    edit[(LAB_SOUND_ARP + LIVE_ARP_RATE) as usize] = 5;
    u.apply(&edit);
    let edit = u.sound();
    assert!(u.edited());
    u.ticks(SETTINGS_AND_EDIT_SAVED_MS);
    let rec = u.st.get(USER_SOUND_EDIT_KEY).expect("the unsaved edit");
    assert_eq!(rec.len(), USER_SOUND_EDIT_BYTES as usize);
    assert_eq!(
        rec[..4],
        [USER_SOUND_EDIT_VERSION as u8, 1, 1, b'P'],
        "version, PWM BASS, edited, its name"
    );
    assert_eq!(rec[15], BYTES as u8);
    assert_eq!(rec[16..], edit);
    let u = u.power_cycle();
    assert_eq!(u.rig.live.preset, 1);
    assert_eq!(u.sound(), edit);
    assert!(u.edited(), "the star comes back");
    assert_eq!(u.base_name(), "PWM BASS");
    let mut u = u;
    u.apply(&s[0]);
    let playing = u.sound();
    assert_eq!(playing, s[0]);
    u.keep(3, "LAB 04", &playing);
    assert!(!u.edited(), "a keep is of the sound playing: no star");
    let mut tweak = s[0];
    tweak[LAB_SOUND_RESONANCE as usize] ^= 1;
    u.apply(&tweak);
    assert!(u.edited());
    u.ticks(SETTINGS_AND_EDIT_SAVED_MS);
    let stored = u.st.get(SETTINGS_KEY).unwrap();
    assert_eq!(
        stored[4],
        LIVE_PRESET_SLOTS as u8 + 3,
        "the settings' preset byte"
    );
    let u = u.power_cycle();
    assert_eq!(u.rig.live.user_sound, 4);
    assert_eq!(u.rig.live.base, s[0]);
    assert_eq!(u.base_name(), "LAB 04");
    assert_eq!(u.sound(), tweak);
    assert!(u.edited());
    let mut u = u;
    u.apply(&s[0]);
    u.ticks(SETTINGS_AND_EDIT_SAVED_MS);
    let u = u.power_cycle();
    assert_eq!((u.rig.live.user_sound, u.sound()), (4, s[0]));
    assert!(!u.edited());
    let mut u = u;
    let mut rec = u.st.get(SETTINGS_KEY).unwrap();
    rec[4] = LIVE_PRESET_SLOTS as u8 + 9;
    assert_eq!(
        unsafe { settings_apply(&mut *u.set, rec.as_ptr(), rec.len() as u32) },
        1
    );
    assert_eq!(
        (u.rig.live.preset, u.rig.live.user_sound),
        (0, 0),
        "SAW LEAD"
    );
    let mut u = Unit::blank();
    u.rig.preset_step(4);
    unsafe { live_dice_roll(&mut *u.rig.live, 99) };
    let rolled = u.sound();
    u.ticks(SETTINGS_AND_EDIT_SAVED_MS);
    let u = u.power_cycle();
    assert_eq!(u.rig.live.preset, LIVE_PRESET_DICE as u8);
    assert_eq!(u.sound(), rolled);
    assert_eq!(u.rig.live.base, rolled);
    assert!(!u.edited());
    let mut u = u;
    let mut rec = u.st.get(USER_SOUND_EDIT_KEY).unwrap();
    rec[1] = 2;
    assert_eq!(
        unsafe { user_sound_edit_apply(&mut *u.rig.live, rec.as_ptr(), rec.len() as u32) },
        0
    );
    u.st.assert_clean();
}

#[test]
fn a_cut_during_save_never_brings_back_an_older_sound() {
    let mut u = Unit::blank();
    u.ticks(SETTINGS_AND_EDIT_SAVED_MS);
    assert!(u.st.get(USER_SOUND_EDIT_KEY).is_some());
    let region = u.st.sim.region().to_vec();
    let old = u.sound();
    let on_region = |seed| {
        let mut st = Store::new(
            Sim::with_region(seed, &region, [false; STORE_SECTORS as usize]),
            256,
        );
        assert_eq!(st.mount(), STORE_STATE_SIDOU);
        st.maintain();
        let mut u = Unit::new(st);
        u.boot();
        u
    };
    let turn = |u: &mut Unit| unsafe {
        let c = live_param_get(&*u.rig.live, LIVE_PARAM_CUTOFF);
        live_param_set(&mut *u.rig.live, LIVE_PARAM_CUTOFF, c - 200);
    };
    let mut clean = on_region(1);
    turn(&mut clean);
    let new = clean.sound();
    assert_ne!(new, old);
    let ops = clean.st.sim.ops;
    unsafe { settings_save_now(&mut *clean.set, clean.now) };
    let mut saved = false;
    for _ in 0..1000 {
        clean.tick();
        if unsafe { settings_take_result(&mut *clean.set) } == SETTINGS_SAVED {
            saved = true;
            break;
        }
    }
    assert!(saved, "SAVE never said SAVED");
    let n = clean.st.sim.ops - ops;
    let (mut olds, mut news) = (0, 0);
    for k in 0..=n + 4 {
        let mut u = on_region(200 + k);
        turn(&mut u);
        u.st.sim.cut_at = Some(u.st.sim.ops + k);
        unsafe { settings_save_now(&mut *u.set, u.now) };
        let mut said = false;
        for _ in 0..1000 {
            if u.st.sim.dead {
                break;
            }
            u.tick();
            said |= unsafe { settings_take_result(&mut *u.set) } == SETTINGS_SAVED;
        }
        let mut st = u.st;
        st.sim.dead = false;
        st.sim.cut_at = None;
        st.reset();
        assert_eq!(st.mount(), STORE_STATE_SIDOU, "cut {k}");
        st.maintain();
        let mut after = Unit::new(st);
        after.boot();
        let got = after.sound();
        assert!(
            got == old || got == new,
            "cut {k}: a sound neither old nor new"
        );
        assert!(
            !said || got == new,
            "cut {k}: SAVED, and the old sound came back"
        );
        if got == old {
            olds += 1;
        } else {
            news += 1;
        }
    }
    println!("SAVE in {n} operations: a cut left the old sound {olds} times, the new {news}");
    assert!(olds > 0 && news > 0, "{olds} old, {news} new");
}

#[test]
fn a_later_chunk_version_is_read_and_its_extra_bytes_skipped() {
    let s = sounds(41, 3);
    let mut st = fresh(2, 256);
    put_user_sounds_file(
        &mut st,
        STORE_FILE_SOUNDS,
        &chunk(
            2,
            2,
            17,
            &[(0, "LATER A", s[0]), (3, "LATER B", s[1]), (16, "X", s[2])],
        ),
    );
    let mut u = Unit::new(st);
    u.boot();
    assert_eq!(
        u.rig.live.sounds_valid,
        1 | 1 << 3,
        "the slots this firmware has"
    );
    assert_eq!(u.read(0), Some(("LATER A".to_owned(), s[0])));
    assert_eq!(u.read(3), Some(("LATER B".to_owned(), s[1])));
    u.keep(5, "NOW", &s[2]);
    let file = u.st.file("USER SOUNDS").unwrap();
    assert_eq!(file.len(), USER_SOUND_FILE_BYTES as usize);
    assert_eq!(file[..HEADER], [1, BYTES as u8, USER_SOUNDS as u8]);
    let u = u.power_cycle();
    assert_eq!(u.rig.live.sounds_valid, 1 | 1 << 3 | 1 << 5);
    assert_eq!(u.read(0), Some(("LATER A".to_owned(), s[0])));
    assert_eq!(u.read(3), Some(("LATER B".to_owned(), s[1])));
    assert_eq!(u.read(5), Some(("NOW".to_owned(), s[2])));
}

#[test]
fn a_1_0_file_reads_with_the_new_fields_at_their_defaults_and_a_keep_rewrites_them_with_the_defaults(
) {
    const OLD: usize = LAB_SOUND_BYTES_1_0 as usize;
    let mut s = sounds(47, 3);
    for x in &mut s {
        x[OLD..].fill(0);
    }
    let stride = 1 + LAB_SOUND_NAME as usize + OLD;
    let mut f = vec![1u8, OLD as u8, USER_SOUNDS as u8];
    f.resize(HEADER + USER_SOUNDS as usize * stride, 0);
    for (n, name, x) in [(0usize, "OLD A", s[0]), (15, "OLD LAST", s[1])] {
        let at = HEADER + n * stride;
        f[at] = 1;
        f[at + 1..at + 1 + name.len()].copy_from_slice(name.as_bytes());
        f[at + 13..at + 13 + OLD].copy_from_slice(&x[..OLD]);
    }
    let mut st = fresh(5, 256);
    put_user_sounds_file(&mut st, STORE_FILE_SOUNDS, &f);
    let mut u = Unit::new(st);
    u.boot();
    assert_eq!(u.rig.live.sounds_valid, 1 | 1 << 15);
    assert_eq!(u.read(0), Some(("OLD A".to_owned(), s[0])));
    assert_eq!(
        u.read(15),
        Some(("OLD LAST".to_owned(), s[1])),
        "the file's last slot"
    );
    let mut new = s[2];
    new[LAB_SOUND_FILTER_LFO as usize] = 9;
    new[LAB_SOUND_RESO_LINK as usize] = -3i8 as u8;
    u.keep(4, "NEW", &new);
    let u = u.power_cycle();
    assert_eq!(u.rig.live.sounds_valid, 1 | 1 << 4 | 1 << 15);
    assert_eq!(u.read(0), Some(("OLD A".to_owned(), s[0])));
    assert_eq!(u.read(4), Some(("NEW".to_owned(), new)));
    assert_eq!(u.read(15), Some(("OLD LAST".to_owned(), s[1])));
    let mut u = u;
    let mut on = u.sound();
    on[LAB_SOUND_FILTER_LFO as usize] = 20;
    on[LAB_SOUND_PWM_LINK as usize] = 5;
    u.apply(&on);
    let mut rec = vec![1u8, user_sound_preset_byte_of(&u), 1];
    rec.extend_from_slice(b"SAW LEAD\0\0\0\0");
    rec.push(OLD as u8);
    rec.extend_from_slice(&s[0][..OLD]);
    assert_eq!(
        unsafe { user_sound_edit_apply(&mut *u.rig.live, rec.as_ptr(), rec.len() as u32) },
        1
    );
    assert_eq!(u.sound(), s[0], "the tail at its defaults");
    rec[15] = OLD as u8 - 1;
    rec.pop();
    assert_eq!(
        unsafe { user_sound_edit_apply(&mut *u.rig.live, rec.as_ptr(), rec.len() as u32) },
        0
    );
}

fn user_sound_preset_byte_of(u: &Unit) -> u8 {
    unsafe { user_sound_preset_byte(&*u.rig.live) }
}

#[test]
fn a_bad_slot_reads_as_empty() {
    let s = sounds(43, 2);
    let mut bad_sound = s[0];
    let cutoff_past_2047 = 0x0f;
    bad_sound[LAB_SOUND_CUTOFF as usize + 1] = cutoff_past_2047;
    let mut st = fresh(3, 256);
    let mut file = chunk(
        1,
        0,
        16,
        &[
            (4, "BAD SOUND", bad_sound),
            (5, "lower", s[0]),
            (6, "USED 2", s[0]),
            (7, "GOOD", s[1]),
        ],
    );
    file[HEADER + 6 * STRIDE] = 2;
    put_user_sounds_file(&mut st, STORE_FILE_SOUNDS, &file);
    let mut u = Unit::new(st);
    u.boot();
    assert_eq!(u.rig.live.sounds_valid, 1 << 7);
    for n in [4, 5, 6] {
        assert_eq!(u.read(n), None, "slot {n}");
        assert_eq!(unsafe { user_sound_load(&mut *u.sounds, n) }, 0);
    }
    assert_eq!(u.read(7), Some(("GOOD".to_owned(), s[1])));
    for (typ, bytes) in [
        (STORE_FILE_SOUNDS, chunk(0, 0, 16, &[(1, "A", s[0])])),
        (STORE_FILE_SOUNDS, chunk(1, 0, 15, &[(1, "A", s[0])])),
        (STORE_FILE_SONG, chunk(1, 0, 16, &[(1, "A", s[0])])),
    ] {
        let mut st = fresh(4, 256);
        put_user_sounds_file(&mut st, typ, &bytes);
        let mut u = Unit::new(st);
        u.boot();
        assert_eq!(u.rig.live.sounds_valid, 0, "type {typ}: {:?}", &bytes[..3]);
    }
}

#[test]
fn an_older_firmware_ignores_the_chunk_and_the_record() {
    use crate::store::{
        decode_runs, fixture_file, fixture_path, fixture_value, FIXTURE_DELETED, FIXTURE_FILES,
        FIXTURE_KEYS, FIXTURE_ROUNDS,
    };
    assert!(!(0x100..=0x108).contains(&USER_SOUND_EDIT_KEY));
    assert!(![STORE_FILE_SONG, STORE_FILE_SID, STORE_FILE_GTI].contains(&STORE_FILE_SOUNDS));
    let region = decode_runs(&std::fs::read(fixture_path()).unwrap());
    let mut st = Store::new(
        Sim::with_region(5, &region, [false; STORE_SECTORS as usize]),
        256,
    );
    assert_eq!(st.mount(), STORE_STATE_SIDOU);
    st.maintain();
    let v7 = hex_bytes(include_str!("../data/settings-v7.hex"));
    assert_eq!(st.put_now(SETTINGS_KEY, &v7), STORE_OK);
    let profile = [1, SID_CHIP_6581_WORN as u8, 4, 1, 2, 3, 4];
    assert_eq!(st.put_now(0x103, &profile), STORE_OK);
    let before = |st: &Store| instrument_0_5_0_boots_from(st, &v7);
    let instrument = before(&st);
    let mut u = Unit::new(st);
    u.boot();
    u.keep(0, "LAB 01", &sounds(47, 1)[0]);
    u.rig.knob(LIVE_KNOB_RESONANCE, 1);
    u.ticks(SETTINGS_AND_EDIT_SAVED_MS);
    let mut st = u.st;
    assert!(st.get(USER_SOUND_EDIT_KEY).is_some() && st.file("USER SOUNDS").is_some());
    assert_eq!(st.put_now(SETTINGS_KEY, &v7), STORE_OK);
    st.reset();
    assert_eq!(st.mount(), STORE_STATE_SIDOU);
    for k in 0..FIXTURE_KEYS {
        let want = (!FIXTURE_DELETED.contains(&k)).then(|| fixture_value(k, FIXTURE_ROUNDS - 1));
        assert_eq!(st.get(k), want, "key {k}");
    }
    for (name, seed, len) in FIXTURE_FILES {
        assert_eq!(st.file(name), Some(fixture_file(seed, len)), "{name}");
    }
    let mut f = store_file::default();
    let mut listed = Vec::new();
    while unsafe { store_dir_next(&*st.st, &mut f) } == 1 {
        let name = unsafe { CStr::from_ptr(f.name.as_ptr()) }
            .to_str()
            .unwrap()
            .to_owned();
        if name == "USER SOUNDS" {
            assert_eq!(u32::from(f.type_), STORE_FILE_SOUNDS);
            assert_eq!(unsafe { song_io_listed(&f) }, 0);
        }
        listed.push(name);
    }
    assert!(listed.contains(&"USER SOUNDS".to_owned()));
    let mut f = store_file::default();
    assert_eq!(
        unsafe { sid_io_next(&*st.st, &mut f) },
        0,
        "SIDS lists no tune"
    );
    assert_eq!(before(&st), instrument);
}

fn instrument_0_5_0_boots_from(st: &Store, v7: &[u8]) -> (Vec<u32>, u8, u8, Vec<u8>) {
    let mut rig = Rig::new(SID_MODEL_6581);
    let mut cards = Box::new(1u8);
    let mut rows = Box::new(0u8);
    let mut set = Box::<settings>::default();
    let mut users = Box::<user_profiles>::default();
    let rec = st.get(SETTINGS_KEY).unwrap();
    assert_eq!(rec, v7);
    unsafe {
        settings_init(
            &mut *set,
            &mut *rig.live,
            std::ptr::null_mut(),
            &mut *cards,
            &mut *rows,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            c"V1.2.3".as_ptr(),
            None,
        );
        settings_apply(&mut *set, rec.as_ptr(), rec.len() as u32);
        user_profiles_init(
            &mut *users,
            &mut *rig.live,
            (&*st.st as *const store).cast_mut(),
        );
        user_profiles_load(&mut *users);
    }
    let params = (0..LIVE_PARAM_COUNT)
        .map(|p| unsafe { live_param_get(&*rig.live, p) })
        .collect();
    (
        params,
        rig.live.preset,
        rig.live.user_valid,
        rig.live.user_ctl.as_flattened().to_vec(),
    )
}

#[test]
fn store_full_keeps_the_sound_in_ram_and_says_so() {
    let mut st = fresh(6, 256);
    let mut i = 0;
    for size in [60_000usize, 20_000, 8_000, 3_000, 1_000] {
        loop {
            let rc = st.file_now(&format!("FILL {i}"), &vec![i as u8; size]);
            i += 1;
            if rc == STORE_FULL {
                break;
            }
            assert_eq!(rc, STORE_OK, "file {i} of {size}");
            assert!(i < 40, "the store never filled");
        }
    }
    let mut u = Unit::new(st);
    u.boot();
    let s = sounds(53, 1)[0];
    assert_eq!(u.save(1, "LAB 02", &s), 0);
    let (r, slot) = u.tick_until_written();
    assert_eq!((r, slot), (USER_SOUND_FULL, 1), "said so");
    assert_eq!(u.rig.live.sounds_valid, 1 << 1, "kept in RAM");
    assert_eq!((u.rig.live.base, u.rig.live.user_sound), (s, 2));
    assert_eq!(u.read(1), None, "not on the store");
}
