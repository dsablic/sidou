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
        (GROOVE, r#""bpm": 125"#, r#""bpm": 400"#, "bpm: 400 is too high. Allowed: 60 to 250."),
        (GROOVE, r#""bpm": 125"#, r#""bpm": 59"#, "bpm: 59 is too low. Allowed: 60 to 250."),
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
        (GROOVE, "G2 .", "C#9 .", "sections.A.voices[0].notes: token 7 \"C#9\" is not a note. Allowed: C0 to G#7, - . and ! or ~ after a note."),
        (GROOVE, "C2! .", "C2!! .", "sections.A.voices[0].notes: token 1 \"C2!!\" is not a note."),
        (GROOVE, "C2! .", "k .", "sections.A.voices[0].notes: token 1 \"k\" is a drum, and drums need the kit."),
        (GROOVE, "k . h", "C2 . h", "sections.A.voices[1].notes: token 1 \"C2\" is not a drum. Allowed with the kit: k kick, s snare, h closed hat"),
        (GROOVE, "C2 .\"}", "C2\"}", "sections.A.voices[0].notes: 15 tokens, rows is 16."),
        (GROOVE, "h dh\"}", "h dh k\"}", "sections.A.voices[1].notes: 17 tokens, rows is 16."),
    ];
    for (text, from, to, want) in cases {
        let err = refusal(&with(text, from, to));
        assert!(err.contains(want), "{to}: {err}");
    }
}

#[test]
fn tokens_are_separated_by_any_run_of_spaces_tabs_and_newlines() {
    let spaced = with(GROOVE, "C2! . C2~", r"  C2!\t.\n C2~ ");
    assert_eq!(
        json::song_song(&spaced, SID_MODEL_6581),
        json::song_song(GROOVE, SID_MODEL_6581)
    );
    assert!(json::song_song(GROOVE, SID_MODEL_6581).is_ok());
}

const OFF: u8 = GT2_KEYOFF as u8;
const HOLD: [u8; 4] = [GT2_REST as u8, 0, 0, 0];
const KEY_OFF: [u8; 4] = [OFF, 0, 0, 0];

fn note(n: u8) -> u8 {
    GT2_FIRSTNOTE as u8 + n
}

fn played(song: &Song, voice: u32, at: usize) -> (u32, Vec<[u8; 4]>) {
    let p = u32::from(song.order(0, voice).unwrap().0[at]);
    let mut rows = song.pattern(p).unwrap();
    rows.pop();
    (p, rows)
}

fn locks(song: &Song, p: u32, rows: usize) -> Vec<(bool, bool)> {
    let mut v = 0;
    let lock = |row: usize, param: u32, v: &mut u32| unsafe {
        song_lock_get(song.ptr(), p, row as u32, param, v) != 0 && *v != 0
    };
    (0..rows)
        .map(|r| {
            (
                lock(r, SONG_LOCK_ACCENT, &mut v),
                lock(r, SONG_LOCK_SLIDE, &mut v),
            )
        })
        .collect()
}

#[test]
fn each_token_writes_its_row_and_accent_and_slide_are_steps_locks() {
    let groove = song_of(GROOVE);
    let (p, bass) = played(&groove, 0, 0);
    let instr = bass[0][1];
    assert!(instr > 0);
    assert_eq!(
        &bass[..4],
        &[
            [note(24), instr, 0, 0],
            KEY_OFF,
            [note(24), instr, 0, 0],
            [note(27), instr, 0, 0]
        ]
    );
    assert_eq!(bass[12], [note(22), instr, 0, 0], "A#1");
    let mut want = vec![(false, false); 16];
    want[0].0 = true;
    want[2].1 = true;
    assert_eq!(locks(&groove, p, 16), want);
    let (_, kit) = played(&groove, 1, 0);
    assert_eq!(
        (kit[0], kit[2][1], kit[4][1], kit[15][1], kit[1]),
        (
            [note(36), SONG_IO_KIT as u8, 0, 0],
            SONG_IO_KIT as u8 + 2,
            SONG_IO_KIT as u8 + 1,
            SONG_IO_KIT as u8 + 11,
            KEY_OFF
        ),
        "KICK, CLOSED HAT, SNARE and DG HAT of the NEW song's kit"
    );
    let three = song_of(THREE_PARTS);
    let (_, bass) = played(&three, 0, 0);
    assert_eq!(
        (bass[0][1], bass[1], bass[2]),
        (2, HOLD, KEY_OFF),
        "the NEW song's BASS; - holds, . is a key off"
    );
}

#[test]
fn a_lane_as_a_song_section_is_the_lane_step_writes() {
    let sound = r#"{"name": "303", "wave": "saw", "filter": {"cutoff_hz": 2077, "resonance": 12}}"#;
    let step = format!(
        r#"{{"format": "sidou-step/1", "voice": 2, "gate": "legato", "instrument": {sound},
        "steps": [{{"step": 1, "note": "C2", "accent": true}}, {{"step": 3, "note": "C2", "slide": true}},
          {{"step": 4, "note": "D#2", "accent": true, "slide": true}}, {{"step": 11, "note": "A#1"}}]}}"#
    );
    let song = format!(
        r#"{{"format": "sidou-song/1", "instruments": {{"acid": {sound}}},
        "sections": {{"A": {{"rows": 16, "voices": [null,
          {{"instrument": "acid", "notes": "C2! - C2~ D#2!~ - - - - - - A#1 - - - - -"}}]}}}},
        "order": ["A"]}}"#
    );
    let mut lane = Song::device();
    lane.read(&json::step_song(&step, SID_MODEL_6581).unwrap(), 0)
        .unwrap();
    let section = song_of(&song);
    let (sp, step_rows) = played(&lane, 1, 0);
    let (p, rows) = played(&section, 1, 0);
    assert_eq!(rows, step_rows);
    assert_eq!(locks(&section, p, 16), locks(&lane, sp, 16));
    assert!(locks(&section, p, 16)[3] == (true, true));
}

#[test]
fn sections_follow_the_order_share_equal_patterns_and_loop() {
    let song = song_of(THREE_PARTS);
    let orders: Vec<(Vec<u8>, u8)> = (0..3).map(|v| song.order(0, v).unwrap()).collect();
    for (entries, restart) in &orders {
        assert_eq!(
            (entries.len(), *restart),
            (5, 0),
            "five sections played, looped to the first"
        );
        assert_eq!(
            (entries[0], entries[1], entries[3]),
            (entries[0], entries[0], entries[0]),
            "A each time"
        );
        assert_ne!(entries[0], entries[4]);
    }
    let kit = &orders[1].0;
    assert_eq!(kit[0], kit[2], "the kit lane is the same in A and B");
    assert_ne!(orders[0].0[0], orders[0].0[2], "the bass lane is not");
    assert_eq!(
        orders[0].0[4], orders[2].0[4],
        "C's two voices left out share one pattern"
    );
    let (_, rest) = played(&song, 0, 4);
    assert_eq!(
        rest,
        vec![KEY_OFF, HOLD, HOLD, HOLD],
        "a voice left out rests"
    );
    let used: std::collections::BTreeSet<u8> = orders.iter().flat_map(|o| o.0.clone()).collect();
    assert_eq!(
        used.into_iter().collect::<Vec<_>>(),
        (0..7).collect::<Vec<_>>(),
        "seven patterns, numbered from 0"
    );
    assert!(song.pattern(7).is_none());
}

#[test]
fn the_song_is_named_as_written_in_capitals() {
    let song = song_of(&with(THREE_PARTS, "THREE PARTS", "three parts"));
    let name: Vec<u8> = song
        .raw()
        .name
        .iter()
        .map(|&c| c as u8)
        .take_while(|&c| c != 0)
        .collect();
    assert_eq!(name, b"THREE PARTS");
}

#[test]
fn a_song_too_big_for_the_fm1_is_refused_naming_where() {
    let names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let section = |s: usize| {
        let voice = |v: usize| {
            let notes: Vec<String> = (0..128)
                .map(|r| format!("{}{}!", names[(r + v) % 12], (s + r) % 7))
                .collect();
            format!(
                r#"{{"instrument": "pulse", "notes": "{}"}}"#,
                notes.join(" ")
            )
        };
        format!(
            r#""S{s}": {{"rows": 128, "voices": [{}, {}, {}]}}"#,
            voice(0),
            voice(1),
            voice(2)
        )
    };
    let sections: Vec<String> = (0..16).map(section).collect();
    let order: Vec<String> = (0..16).map(|s| format!(r#""S{s}""#)).collect();
    let text = format!(
        r#"{{"format": "sidou-song/1", "sections": {{{}}}, "order": [{}]}}"#,
        sections.join(", "),
        order.join(", ")
    );
    let err = refusal(&text);
    assert!(
        err.starts_with("sections.S") && err.contains("SONG FULL"),
        "{err}"
    );
}

const ROWS_A_BEAT: u32 = 4;
const PAL: u32 = 0;
const ONE_CALL_A_FRAME: u32 = 1;
const GROOVE_ROWS: u32 = 16;
const SPEEDS: std::ops::RangeInclusive<u32> = 3..=32;

unsafe extern "C" fn discard(_: *mut std::ffi::c_void, _: u8, _: u8) {}

fn bpm10(speed_sum: u32) -> u32 {
    unsafe { song_view_bpm10(speed_sum, ONE_CALL_A_FRAME, PAL, ROWS_A_BEAT) }
}

fn frames_for_rows(sng: &[u8], rows: u32) -> u32 {
    let mut song = Song::device();
    song.read(sng, 0).unwrap();
    let mut options = gt2_options::default();
    let mut analysis = Box::<gt2_analysis>::default();
    let mut shape = Box::<gt2_shape>::default();
    let mut player = Box::<gt2_player>::default();
    unsafe {
        gt2_options_default(&mut options);
        assert_eq!(
            gt2_shape_analyse(&mut *analysis, song.ptr(), 0, &options, &mut *shape),
            0
        );
        gt2_play_start(&mut *player, song.ptr(), &*shape, std::ptr::null());
    }
    let (mut read, mut first, mut last) = (0, None, player.chn[0].pattptr);
    for frame in 0..4000 {
        unsafe { gt2_play_frame(&mut *player, Some(discard), std::ptr::null_mut()) };
        if player.chn[0].pattptr != last {
            last = player.chn[0].pattptr;
            let start = *first.get_or_insert(frame);
            if read == rows {
                return frame - start;
            }
            read += 1;
        }
    }
    panic!("the song did not play {rows} rows");
}

#[test]
fn the_bpm_plays_as_the_nearest_speed_pair_and_is_reported() {
    let pairs: Vec<u32> = SPEEDS.flat_map(|a| [2 * a, 2 * a + 1]).map(bpm10).collect();
    for bpm in (60..=250).step_by(5) {
        let made = json::check_song(
            &with(GROOVE, r#""bpm": 125"#, &format!(r#""bpm": {bpm}"#)),
            SID_MODEL_6581,
        )
        .unwrap();
        let frames = frames_for_rows(&made.sng, GROOVE_ROWS);
        let played = bpm10(2 * frames / GROOVE_ROWS);
        let nearest = pairs.iter().map(|&p| p.abs_diff(bpm * 10)).min().unwrap();
        assert_eq!((made.bpm, made.bpm_played10), (bpm, played), "asked {bpm}");
        assert_eq!(
            played.abs_diff(bpm * 10),
            nearest,
            "asked {bpm}, plays {played}"
        );
    }
}
