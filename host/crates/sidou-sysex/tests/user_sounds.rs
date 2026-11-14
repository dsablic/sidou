// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use std::path::PathBuf;
use std::process::Command;

use common::{client, Fake, USER_SOUNDS};
use sidou_patch::json;
use sidou_patch::sounds::sidou_sounds;
use sidou_sys::*;
use sidou_sysex::send::FILE_USER_SOUNDS;
use sidou_sysex::user_sounds::{fetch, patch_files};

const HEADER: usize = USER_SOUND_HEADER as usize;
const STRIDE: usize = USER_SOUND_STRIDE as usize;
const NAME_AT: usize = 1;
const SOUND_AT: usize = 1 + LAB_SOUND_NAME as usize;

fn store_file(held: &[(usize, &str, usize)]) -> Vec<u8> {
    let sounds = sidou_sounds();
    let mut file = vec![
        USER_SOUND_VERSION as u8,
        LAB_SOUND_BYTES as u8,
        USER_SOUNDS_COUNT as u8,
    ];
    file.resize(HEADER + USER_SOUNDS_COUNT * STRIDE, 0);
    for (slot, name, sound) in held {
        let at = HEADER + slot * STRIDE;
        file[at] = 1;
        file[at + NAME_AT..at + NAME_AT + name.len()].copy_from_slice(name.as_bytes());
        file[at + SOUND_AT..at + STRIDE].copy_from_slice(&sounds[*sound].bytes);
    }
    file
}

const USER_SOUNDS_COUNT: usize = sidou_patch::user_sounds::SLOTS;

fn fm1_holding(file: &[u8]) -> Fake {
    let mut fake = Fake::default();
    fake.stored
        .insert(USER_SOUNDS.into(), (FILE_USER_SOUNDS, file.to_vec()));
    fake
}

#[test]
fn the_file_comes_over_in_several_packets_byte_for_byte() {
    let file = store_file(&[(0, "FIRST", 0), (7, "SEVENTH", 1)]);
    assert!(file.len() > 4 * 256);
    let mut c = client(fm1_holding(&file));
    c.inquire().unwrap();
    assert_eq!(fetch(&mut c), Ok(Some(file)));
}

#[test]
fn an_fm1_with_no_file_answers_not_found_which_is_no_sounds_yet() {
    let mut c = client(Fake::default());
    c.inquire().unwrap();
    assert_eq!(fetch(&mut c), Ok(None));
}

#[test]
fn what_an_old_sidou_and_a_file_being_rewritten_mean_is_said_in_words() {
    let file = store_file(&[(0, "FIRST", 0)]);
    const OLD: &str = "update SIDOU on the FM-1";
    const SAVING: &str = "the FM-1 was saving a sound: try again";
    let cases = [(1, false, OLD), (7, false, SAVING), (7, true, SAVING)];
    for (reason, after_meta, words) in cases {
        let mut fake = fm1_holding(&file);
        if after_meta {
            fake.nak_after_meta = Some(reason);
        } else {
            fake.nak_get = Some(reason);
        }
        let mut c = client(fake);
        c.inquire().unwrap();
        let e = fetch(&mut c).unwrap_err();
        assert!(e.contains(words), "{e}");
    }
}

#[test]
fn a_damaged_transfer_is_refused_by_its_crc() {
    let mut fake = fm1_holding(&store_file(&[(0, "FIRST", 0)]));
    fake.corrupt = true;
    let mut c = client(fake);
    c.inquire().unwrap();
    let e = fetch(&mut c).unwrap_err();
    assert!(e.contains("damaged"), "{e}");
}

#[test]
fn another_file_type_is_not_taken_for_the_sounds() {
    let mut fake = Fake::default();
    fake.stored
        .insert(USER_SOUNDS.into(), (1, store_file(&[(0, "A", 0)])));
    let mut c = client(fake);
    c.inquire().unwrap();
    assert!(fetch(&mut c)
        .unwrap_err()
        .contains("not a USER sounds file"));
}

#[test]
fn each_used_slot_becomes_a_numbered_patch_that_reads_back_to_its_sound() {
    let sounds = sidou_sounds();
    let file = store_file(&[(0, "LAB 01", 0), (9, "A/B: ACID", 1), (15, "", 2)]);
    let files = patch_files(&file);
    let names: Vec<&str> = files.iter().map(|f| f.file_name.as_str()).collect();
    assert_eq!(names, ["U01-LAB-01.json", "U10-A-B-ACID.json", "U16.json"]);
    for (file, sound) in files.iter().zip([0, 1, 2]) {
        assert_eq!(json::check(&file.json).unwrap().sound, sounds[sound].bytes);
    }
    assert_eq!(json::check(&files[1].json).unwrap().name, "A/B: ACID");
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sidou-sysex-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_sidou-sysex"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_cli_writes_one_json_patch_per_used_slot_from_a_saved_file() {
    let dir = scratch("files");
    let saved = dir.join("USER SOUNDS.bin");
    std::fs::write(&saved, store_file(&[(2, "KEPT", 0), (5, "ALSO", 1)])).unwrap();
    let out = dir.join("out");
    let (ok, stdout, stderr) = run(&[
        "user-sounds",
        "--file",
        saved.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(ok, "{stderr}");
    let mut written: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    written.sort();
    assert_eq!(written, ["U03-KEPT.json", "U06-ALSO.json"]);
    assert!(stdout.contains("U03 KEPT"), "{stdout}");
    let text = std::fs::read_to_string(out.join("U03-KEPT.json")).unwrap();
    assert_eq!(json::check(&text).unwrap().sound, sidou_sounds()[0].bytes);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_cli_says_so_when_the_file_holds_no_sounds() {
    let dir = scratch("empty");
    let saved = dir.join("empty.bin");
    std::fs::write(&saved, store_file(&[])).unwrap();
    let out = dir.join("out");
    let (ok, stdout, stderr) = run(&[
        "user-sounds",
        "--file",
        saved.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(ok, "{stderr}");
    assert!(stdout.contains("No USER sounds yet: keep one with SAVE in LAB."));
    assert!(!out.exists() || std::fs::read_dir(&out).unwrap().next().is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}
