// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::json;
use sidou_sys::song::Song;
use sidou_sys::*;

mod common;

use common::{with, ACID, GROOVE, THREE_PARTS};

fn song_of(text: &str) -> Song {
    let mut song = Song::device();
    song.read(&json::song_song(text, SID_MODEL_6581).unwrap(), 0)
        .unwrap();
    song
}

#[test]
fn a_song_text_becomes_a_song_the_fm1_reads() {
    for text in [GROOVE, THREE_PARTS] {
        let song = song_of(text);
        assert_eq!(song.order(0, 0).map(|o| o.0.is_empty()), Some(false));
    }
}

#[test]
fn a_song_is_told_from_a_patch_a_bank_and_a_pattern() {
    assert_eq!(
        [
            GROOVE,
            ACID,
            r#"{"format": "sidou-bank/1"}"#,
            r#"{"format": "sidou-patch/1"}"#
        ]
        .map(json::kind),
        [
            PATCH_JSON_SONG_TEXT,
            PATCH_JSON_STEP_TEXT,
            PATCH_JSON_BANK_TEXT,
            PATCH_JSON_PATCH_TEXT
        ]
    );
}

fn refusal(text: &str) -> String {
    json::check_song(text, SID_MODEL_6581).expect_err(text)
}

fn listed(prefix: &str, entries: impl Iterator<Item = String>) -> String {
    format!("{prefix}{}", entries.collect::<Vec<_>>().join(", "))
}

#[test]
fn every_mistake_is_refused_in_words_a_model_can_act_on() {
    let lead = r#""instruments": {"lead": {"name": "LEAD", "wave": "pulse"}}"#;
    let seventeen = listed(
        r#""instruments": {"#,
        (0..17).map(|k| format!(r#""i{k}": {{}}"#)),
    ) + "}";
    let sixty_five = listed(r#""order": ["#, (0..65).map(|_| r#""A""#.to_string())) + "]";
    let cases = [
        (GROOVE, r#""bpm": 125"#, r#""tempo": 125"#, "tempo: no such field. Allowed here: format, name, bpm, instruments, sections, order."),
        (GROOVE, r#""format": "sidou-song/1", "#, "", "format: is missing. Allowed: \"sidou-song/1\"."),
        (GROOVE, "sidou-song/1", "sidou-song/2", "format: is not this format. Allowed: \"sidou-song/1\"."),
        (GROOVE, r#""name": "GROOVE""#, r#""name": "A NAME FAR TOO LONG""#, "name: is too long. Allowed: 1 to 16 characters"),
        (GROOVE, r#""bpm": 125"#, r#""bpm": 400"#, "bpm: 400 is too high. Allowed: 60 to 300."),
        (GROOVE, r#""bpm": 125"#, r#""bpm": 59"#, "bpm: 59 is too low. Allowed: 60 to 300."),
        (GROOVE, r#""wave": "saw""#, r#""wave": "sawtooth""#, "instruments.bass.wave: \"sawtooth\" is not allowed."),
        (GROOVE, r#""bass": {"name""#, r#""bass": [], "b": {"name""#, "instruments.bass: must be a patch { }."),
        (THREE_PARTS, lead, &seventeen, "instruments: has too many. Allowed: up to 16 patches."),
        (THREE_PARTS, r#""lead": {"#, r#""a_long_name": {"#, "instruments.a_long_name: is too long a name. Allowed: 1 to 8 characters."),
        (THREE_PARTS, r#""C": {"rows": 4, "#, r#""C": {"#, "sections.C: has no rows. Allowed: \"rows\": 1 to 128."),
        (THREE_PARTS, r#""C": {"rows": 4"#, r#""A": {"rows": 4"#, "sections.A: is a name already given."),
        (GROOVE, r#""rows": 16"#, r#""rows": 0"#, "sections.A.rows: 0 is too low. Allowed: 1 to 128."),
        (GROOVE, r#""rows": 16"#, r#""rows": 129"#, "sections.A.rows: 129 is too high. Allowed: 1 to 128."),
        (GROOVE, r#""rows": 16"#, r#""bars": 4"#, "sections.A.bars: no such field. Allowed here: rows, voices."),
        (GROOVE, "null]", "null, null]", "sections.A.voices: has too many. Allowed: up to 3 voices, the first chip's."),
        (GROOVE, r#"{"instrument": "bass""#, r#"{"instrument": "lead2""#, "sections.A.voices[0].instrument: no instrument \"lead2\". Instruments: bass, kit, pulse."),
        (GROOVE, r#"{"instrument": "kit", "#, "{", "sections.A.voices[1]: has no instrument. Allowed: the name of one of instruments, or \"pulse\", \"bass\" or \"kit\"."),
        (GROOVE, r#", "notes": "k . h . s . h . k k h . s . h dh""#, "", "sections.A.voices[1]: has no notes. Allowed: one token a row, separated by spaces."),
        (GROOVE, r#""order": ["A"]"#, r#""order": []"#, "order: is empty. Allowed: a list of 1 to 64 section names."),
        (THREE_PARTS, r#""A", "C"]"#, r#""A", "D"]"#, "order[4]: no section \"D\". Sections: A, B, C."),
        (GROOVE, r#""order": ["A"]"#, &sixty_five, "order: has too many. Allowed: a list of 1 to 64 section names."),
    ];
    for (text, from, to, want) in cases {
        let err = refusal(&with(text, from, to));
        assert!(err.contains(want), "{to}: {err}");
    }
}
