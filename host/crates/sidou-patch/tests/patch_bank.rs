// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::json;
use sidou_patch::sounds::sidou_sounds;
use sidou_sys::song::Song;
use sidou_sys::*;

fn sixteen() -> String {
    let s = sidou_sounds();
    let patches: Vec<(json::Sound, &str)> = s.iter().map(|s| (s.bytes, s.name)).collect();
    json::write_bank("SIDOU SOUNDS", &patches)
}

#[test]
fn the_sixteen_as_a_bank_are_the_song_the_site_sends() {
    let bank = json::read_bank(&sixteen()).unwrap();
    assert_eq!(bank.name, "SIDOU SOUNDS");
    let want: Vec<(json::Sound, String)> = sidou_sounds()
        .iter()
        .map(|s| (s.bytes, s.name.to_string()))
        .collect();
    assert_eq!(bank.patches, want);
    let sng = json::bank_song(&sixteen(), SID_MODEL_6581).unwrap();
    let mut song = Song::device();
    song.read(&sng, 0).unwrap();
    for (i, s) in sidou_sounds().iter().enumerate() {
        let mut got = [0u8; LAB_SOUND_BYTES as usize];
        let notes =
            unsafe { lab_song_pull(song.ptr(), i as u32 + 1, SID_MODEL_6581, got.as_mut_ptr()) };
        assert_eq!((notes, got), (LAB_SONG_EXACT, s.bytes), "{}", s.name);
    }
    let site = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/sounds/sidou-sounds.sng"),
    )
    .unwrap();
    assert_eq!(
        sng, site,
        "the bank song is the sixteen's song byte for byte"
    );
}

#[test]
fn a_bank_takes_short_patches_without_their_format() {
    let text = r#"{"format": "sidou-bank/1", "name": "two", "patches": [{"name": "a"}, {"format": "sidou-patch/1", "wave": "pulse"}]}"#;
    let bank = json::read_bank(text).unwrap();
    assert_eq!(bank.name, "TWO");
    assert_eq!(bank.patches.len(), 2);
    assert_eq!(bank.patches[0], (json::defaults(), "A".to_string()));
    assert_eq!(bank.patches[1].1, "PATCH");
}

#[test]
fn a_banks_refusal_names_the_patch_and_its_field() {
    let cases = [
        (r#"{"format": "sidou-bank/1", "patches": [{}, {"filter": {"resonance": 20}}]}"#,
         "patches[1].filter.resonance: 20 is too high. Allowed: 0 to 15."),
        (r#"{"format": "sidou-bank/1", "patches": []}"#, "patches: is empty. Allowed: a list of 1 to 16 patches."),
        (r#"{"format": "sidou-bank/1", "name": "A NAME FAR TOO LONG", "patches": [{}]}"#,
         "name: is too long. Allowed: 1 to 16 characters: A to Z, 0 to 9, space and !\"#$%&'()*+,-./:;<=>?@[\\]^_, not starting with a space."),
        (r#"{"format": "sidou-bank/1", "sounds": []}"#, "sounds: no such field. Allowed here: format, name, patches."),
        (r#"{"name": "X", "patches": [{}]}"#, "format: is missing. Allowed: \"sidou-bank/1\"."),
        (r#"{"format": "sidou-bank/1", "patches": [{"format": "sidou-bank/1"}]}"#,
         "patches[0].format: \"sidou-bank/1\" is not allowed. Allowed: \"sidou-patch/1\"."),
        ("{\"format\": \"sidou-bank/1\",\n \"patches\": [{\"wave\": saw}]}",
         "the text is not valid JSON at line 2, column 23: expected a value."),
    ];
    for (text, want) in cases {
        assert_eq!(
            json::read_bank(text).map(|b| b.name),
            Err(want.to_string()),
            "{text}"
        );
    }
    let mut many = vec!["{}"; 17].join(", ");
    many = format!(r#"{{"format": "sidou-bank/1", "patches": [{many}]}}"#);
    assert_eq!(
        json::read_bank(&many).map(|b| b.name),
        Err("patches: has too many. Allowed: a list of 1 to 16 patches.".to_string())
    );
}

#[test]
fn a_patch_or_a_bank_is_told_apart_by_its_format() {
    assert!(json::is_bank(&sixteen()));
    assert!(!json::is_bank(
        r#"{"name": "sidou-bank/1", "format": "sidou-patch/1"}"#
    ));
    assert!(!json::is_bank("no json"));
}
