// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::json;
use sidou_sys::song::Song;
use sidou_sys::*;

mod common;

use common::{ACID, GROOVE, THREE_PARTS};

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
