// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use sidou_patch::bank::sound_song;
use sidou_patch::sounds::{patch_file, sidou_sounds, Sound};
use sidou_sys::song::Song;
use sidou_sys::*;

const SONG_NAME: &str = "SIDOU SOUNDS";
const UPDATE: &str = "SIDOU_SOUNDS_UPDATE";

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../assets/sounds")
}

fn song_bytes(sounds: &[Sound]) -> Vec<u8> {
    let list: Vec<_> = sounds
        .iter()
        .map(|s| (s.bytes, s.name.to_string()))
        .collect();
    sound_song(&list, SID_MODEL_6581, SONG_NAME)
        .expect("sixteen sounds fit one song")
        .sng
}

fn listing(sounds: &[Sound]) -> String {
    let rows: Vec<String> = sounds
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                "    {{ \"instrument\": {}, \"name\": \"{}\", \"about\": \"{}\", \"patch\": \"{}\" }}",
                i + 1,
                s.name,
                s.about,
                patch_file(s.name)
            )
        })
        .collect();
    format!(
        "{{\n  \"song\": \"sidou-sounds.sng\",\n  \"bank\": \"sidou-sounds-bank.json\",\n  \"name\": \"{SONG_NAME}\",\n  \"sounds\": [\n{}\n  ]\n}}\n",
        rows.join(",\n")
    )
}

#[test]
fn every_sound_is_valid_audible_on_both_chips_and_named_for_the_fm1() {
    let sounds = sidou_sounds();
    assert_eq!(sounds.len(), LAB_SONG_KEPT_MAX as usize, "a song's worth");
    let mut names: Vec<&str> = sounds.iter().map(|s| s.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), sounds.len(), "unique names");
    for s in &sounds {
        assert!(
            unsafe { lab_sound_valid(s.bytes.as_ptr()) } != 0,
            "{}",
            s.name
        );
        for model in [SID_MODEL_6581, SID_MODEL_8580] {
            assert!(
                unsafe { lab_audible(s.bytes.as_ptr(), model) } != 0,
                "{} on {model}",
                s.name
            );
        }
        assert!(s.name.len() <= LAB_SOUND_NAME as usize, "{}", s.name);
        assert!(
            s.name.bytes().all(|c| (b' '..=b'_').contains(&c)),
            "{}",
            s.name
        );
    }
}

#[test]
fn the_bank_song_pulls_back_every_sound_exactly() {
    let sounds = sidou_sounds();
    let mut song = Song::device();
    song.read(&song_bytes(&sounds), 0).unwrap();
    for (i, s) in sounds.iter().enumerate() {
        let mut got = [0u8; LAB_SOUND_BYTES as usize];
        let notes = unsafe { lab_song_pull(song.ptr(), i as u32 + 1, 0, got.as_mut_ptr()) };
        assert_eq!(notes, LAB_SONG_EXACT, "{}", s.name);
        assert_eq!(got, s.bytes, "{}", s.name);
    }
}

#[test]
fn the_committed_bank_is_current() {
    let sounds = sidou_sounds();
    let (sng, json) = (song_bytes(&sounds), listing(&sounds));
    if std::env::var(UPDATE).as_deref() == Ok("1") {
        std::fs::write(assets().join("sidou-sounds.sng"), &sng).unwrap();
        std::fs::write(assets().join("sidou-sounds.json"), &json).unwrap();
        return;
    }
    assert!(
        std::fs::read(assets().join("sidou-sounds.sng")).ok() == Some(sng)
            && std::fs::read_to_string(assets().join("sidou-sounds.json")).ok() == Some(json),
        "assets/sounds is stale: rerun with {UPDATE}=1"
    );
}
