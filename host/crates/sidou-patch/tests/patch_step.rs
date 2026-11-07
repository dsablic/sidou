// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::json;
use sidou_patch::sounds::sidou_sounds;
use sidou_sys::song::Song;
use sidou_sys::*;

mod common;

use common::{ACID, BEAT, TUNE};

fn song_of(text: &str) -> Song {
    let mut song = Song::device();
    song.read(&json::step_song(text, SID_MODEL_6581).unwrap(), 0)
        .unwrap();
    song
}

fn lane(song: &Song, voice: u32) -> (u32, Vec<[u8; 4]>) {
    let order = song.order(0, voice).unwrap().0;
    let p = u32::from(order[0]);
    let mut rows = song.pattern(p).unwrap();
    rows.pop();
    (p, rows)
}

#[test]
fn a_pattern_comes_back_as_written_and_its_writing_reads_back_the_same() {
    for text in [ACID, BEAT, TUNE] {
        let written = json::write_step(&song_of(text), voice_of(text)).expect(text);
        assert_eq!(
            json::write_step(&song_of(&written), voice_of(text)).as_deref(),
            Some(written.as_str()),
            "{written}"
        );
        assert_eq!(
            json::step_song(&written, SID_MODEL_6581),
            json::step_song(text, SID_MODEL_6581)
        );
    }
}

fn voice_of(text: &str) -> u32 {
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    v["voice"].as_u64().unwrap_or(1) as u32 - 1
}

#[test]
fn the_song_is_the_new_song_with_the_lane_written_as_step_writes_it() {
    let song = song_of(ACID);
    assert!(song
        .raw()
        .name
        .iter()
        .map(|&c| c as u8)
        .collect::<Vec<_>>()
        .starts_with(b"ACID LINE"));
    let (p, rows) = lane(&song, 0);
    assert_eq!(rows.len(), 16);
    let note = |n: u8| GT2_FIRSTNOTE as u8 + n;
    let instr = rows[0][1];
    assert_eq!(rows[0], [note(24), instr, 0, 0]);
    assert_eq!(
        rows[1][0], GT2_KEYOFF as u8,
        "staccato: a key off after a note"
    );
    assert_eq!(rows[2], [note(24), instr, 0, 0]);
    assert_eq!(
        rows[3],
        [note(27), instr, 0, 0],
        "a note right after keeps no key off between"
    );
    assert_eq!(rows[5][0], GT2_REST as u8);
    let mut sound = [0u8; LAB_SOUND_BYTES as usize];
    let notes = unsafe {
        lab_song_pull(
            song.ptr(),
            u32::from(instr),
            SID_MODEL_6581,
            sound.as_mut_ptr(),
        )
    };
    assert_eq!(
        (notes, sound),
        (LAB_SONG_EXACT, sidou_sounds()[1].bytes),
        "the 303 BASS as a patch"
    );
    let mut v = 0;
    let lock = |row: u32, param: u32, v: &mut u32| unsafe {
        song_lock_get(song.ptr(), p, row, param, v) != 0
    };
    assert!(lock(0, SONG_LOCK_ACCENT, &mut v) && lock(2, SONG_LOCK_SLIDE, &mut v));
    assert!(lock(8, SONG_LOCK_CHANCE, &mut v) && v == 75);
    assert!(!lock(3, SONG_LOCK_SLIDE, &mut v));
    let mut sc = song_scene::default();
    assert!(unsafe { song_ext_get_scene(song.ptr(), &mut sc) } != 0);
    assert_eq!(sc.role[0], SONG_ROLE_ACID as u8);
    let beat = song_of(BEAT);
    let (_, drums) = lane(&beat, 2);
    assert_eq!(drums.len(), 32);
    assert_eq!(
        (drums[0][1], drums[4][1], drums[28][1]),
        (3, 4, 12),
        "KICK, SNARE, DG SNARE of the NEW song's kit"
    );
    let (_, other) = lane(&beat, 0);
    assert_eq!(other.len(), 64, "the other lanes as a NEW song has them");
}

#[test]
fn a_patterns_refusal_says_which_step_and_why() {
    let cases = [
        (r#"{"format": "sidou-step/1", "steps": [{"step": 17}]}"#, "steps[0]: is past the last page: add pages (16 steps each)."),
        (r#"{"format": "sidou-step/1", "steps": [{"step": 2}, {"step": 2}]}"#, "steps[1]: is a step already given: one entry a step."),
        (r#"{"format": "sidou-step/1", "steps": [{"step": 1, "drum": "kick"}]}"#, "steps[0]: names a drum: a drum needs \"instrument\": \"kit\"."),
        (r#"{"format": "sidou-step/1", "instrument": "kit", "steps": [{"step": 1}]}"#, "steps[0]: needs a drum: the instrument is the kit."),
        (r#"{"format": "sidou-step/1", "steps": [{"step": 1, "note": "H2"}]}"#, "steps[0].note: \"H2\" is not allowed. Allowed: a note \"C0\" to \"G#7\" (\"C3\" is the middle of STEP's range)."),
        (r#"{"format": "sidou-step/1", "steps": [{"note": "C2"}]}"#, "steps[0]: has no step. Allowed: \"step\": 1 to 64."),
        (r#"{"format": "sidou-step/1", "role": "funk"}"#, "role: \"funk\" is not allowed. Allowed: auto, drums, bass, arp, lead, none, bass+drums or acid."),
        (r#"{"format": "sidou-step/1", "instrument": {"wave": "sawtooth"}}"#, "instrument.wave: \"sawtooth\" is not allowed. Allowed: triangle, saw, pulse and noise, one or several joined with + (like \"triangle+saw\"), or none."),
        (r#"{"format": "sidou-step/1", "tempo": 120}"#, "tempo: no such field. Allowed here: format, name, voice, role, gate, pages, instrument, steps."),
        (r#"{"steps": []}"#, "format: is missing. Allowed: \"sidou-step/1\"."),
    ];
    for (text, want) in cases {
        assert_eq!(
            json::step_song(text, SID_MODEL_6581).map(|_| ()),
            Err(want.to_string()),
            "{text}"
        );
    }
}

#[test]
fn a_text_is_told_a_patch_a_bank_or_a_pattern() {
    assert_eq!(json::kind(ACID), PATCH_JSON_STEP_TEXT);
    assert_eq!(
        json::kind(r#"{"format": "sidou-bank/1"}"#),
        PATCH_JSON_BANK_TEXT
    );
    assert_eq!(
        json::kind(r#"{"format": "sidou-patch/1"}"#),
        PATCH_JSON_PATCH_TEXT
    );
}
