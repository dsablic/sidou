// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::json::{self, Sound};
use sidou_patch::sounds::sidou_sounds;
use sidou_patch::user_sounds::{self, UserSound, SLOTS};
use sidou_sys::*;

const HEADER: usize = USER_SOUND_HEADER as usize;
const STRIDE: usize = USER_SOUND_STRIDE as usize;
const NAME_AT: usize = 1;
const SOUND_AT: usize = 1 + LAB_SOUND_NAME as usize;

fn file_with(held: &[(usize, &str, Sound)]) -> Vec<u8> {
    let mut file = vec![USER_SOUND_VERSION as u8, LAB_SOUND_BYTES as u8, SLOTS as u8];
    file.resize(HEADER + SLOTS * STRIDE, 0);
    for (slot, name, sound) in held {
        let at = HEADER + slot * STRIDE;
        file[at] = 1;
        file[at + NAME_AT..at + NAME_AT + name.len()].copy_from_slice(name.as_bytes());
        file[at + SOUND_AT..at + STRIDE].copy_from_slice(sound);
    }
    file
}

fn two_sounds() -> (Sound, Sound) {
    let sounds = sidou_sounds();
    (sounds[0].bytes, sounds[1].bytes)
}

#[test]
fn a_file_reads_as_sixteen_slots_with_the_used_ones_named() {
    let (a, b) = two_sounds();
    let read = user_sounds::read(&file_with(&[(0, "FIRST", a), (15, "LAST ONE", b)]));
    assert_eq!(read.len(), SLOTS);
    assert_eq!(
        read[0],
        Some(UserSound {
            name: "FIRST".into(),
            sound: a
        })
    );
    assert_eq!(
        read[15],
        Some(UserSound {
            name: "LAST ONE".into(),
            sound: b
        })
    );
    assert_eq!(read.iter().flatten().count(), 2);
}

#[test]
fn a_damaged_header_or_a_cut_file_reads_as_sixteen_empty_slots() {
    let (a, _) = two_sounds();
    let good = file_with(&[(3, "KEPT", a)]);
    let mut no_version = good.clone();
    no_version[0] = 0;
    let mut too_few_slots = good.clone();
    too_few_slots[2] = SLOTS as u8 - 1;
    let mut too_few_bytes = good.clone();
    too_few_bytes[1] = LAB_SOUND_BYTES_1_0 as u8 - 1;
    let cut = good[..good.len() - 1].to_vec();
    for (what, file) in [
        ("version 0", no_version),
        ("fifteen slots", too_few_slots),
        ("a sound under 45 bytes", too_few_bytes),
        ("one byte short", cut),
        ("empty", Vec::new()),
    ] {
        assert!(
            user_sounds::read(&file).iter().all(Option::is_none),
            "{what}"
        );
    }
}

#[test]
fn a_damaged_slot_is_empty_and_its_neighbours_still_read() {
    let (a, b) = two_sounds();
    let mut file = file_with(&[(1, "BAD NAME", a), (2, "BAD SOUND", a), (4, "GOOD", b)]);
    file[HEADER + STRIDE + NAME_AT] = 0x07;
    file[HEADER + 2 * STRIDE + SOUND_AT + LAB_SOUND_CUTOFF as usize + 1] = 0x0F;
    let read = user_sounds::read(&file);
    assert!(read[1].is_none() && read[2].is_none());
    assert_eq!(read[4].as_ref().map(|s| s.sound), Some(b));
}

#[test]
fn a_kept_sound_opens_as_the_patch_its_json_describes() {
    let (a, _) = two_sounds();
    let read = user_sounds::read(&file_with(&[(0, "MY LEAD", a)]));
    let kept = read[0].as_ref().unwrap();
    let text = json::write(&kept.sound, &kept.name);
    assert_eq!(json::check(&text).unwrap().sound, a);
}
