// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::{json, Value};
use sidou_patch::json::{self, Sound};
use sidou_patch::sounds::sidou_sounds;
use sidou_sys::song::Song;
use sidou_sys::*;

const BASE: &str = r#"{"format": "sidou-patch/1", "wave": "saw",
  "envelope": {"attack": 0, "decay": 0, "sustain": 15, "release": 4}, "filter": {"voices": []}}"#;

fn base() -> Value {
    serde_json::from_str(BASE).unwrap()
}

fn read(v: &Value) -> Result<Sound, String> {
    json::read(&v.to_string()).map(|(s, _)| s)
}

fn set(v: &mut Value, path: &str, x: Value) {
    let mut at = v;
    let parts: Vec<&str> = path.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        let (key, list) = match part.strip_suffix("[]") {
            Some(k) => (k, true),
            None => (*part, false),
        };
        let slot = &mut at[key];
        if list {
            if !slot.is_array() {
                *slot = json!([{}]);
            }
            at = &mut slot[0];
            if last {
                *at = x;
                return;
            }
        } else if last {
            *slot = x;
            return;
        } else {
            if !slot.is_object() {
                *slot = json!({});
            }
            at = slot;
        }
    }
}

fn with(path: &str, x: Value) -> Value {
    let mut v = base();
    if path.starts_with("table.") {
        set(&mut v, "table.steps", json!(vec![json!({}); 16]));
    }
    set(&mut v, path, x);
    v
}

fn milli(word: &str) -> i64 {
    let (whole, frac) = word.split_once('.').unwrap_or((word, ""));
    let sign = if whole.starts_with('-') { -1 } else { 1 };
    let frac = format!("{frac:0<3}").parse::<i64>().unwrap();
    whole.parse::<i64>().unwrap() * 1000 + sign * frac
}

fn number(m: i64) -> Value {
    let (sign, m) = if m < 0 { ("-", -m) } else { ("", m) };
    let frac = format!("{:03}", m % 1000);
    let frac = frac.trim_end_matches('0');
    let text = if frac.is_empty() {
        format!("{sign}{}", m / 1000)
    } else {
        format!("{sign}{}.{frac}", m / 1000)
    };
    serde_json::from_str(&text).unwrap()
}

fn bounds(allowed: &str) -> (i64, i64) {
    let words: Vec<&str> = allowed.split(' ').collect();
    let at = words.iter().position(|w| *w == "to").expect(allowed);
    let num = |w: &str| milli(w.trim_end_matches(','));
    (num(words[at - 1]), num(words[at + 1]))
}

fn round_trips(s: &Sound, name: &str) {
    let text = json::write(s, name);
    assert_eq!(json::read(&text), Ok((*s, name.to_string())), "{text}");
}

fn heard_or_silent(r: &Result<Sound, String>, what: &str) {
    match r {
        Ok(s) => round_trips(s, json::read(BASE).unwrap().1.as_str()),
        Err(e) => assert!(e.starts_with("the patch is silent"), "{what}: {e}"),
    }
}

#[test]
fn the_sixteen_sounds_round_trip_through_json() {
    for s in sidou_sounds() {
        let text = json::write(&s.bytes, s.name);
        let v: Value = serde_json::from_str(&text).expect(&text);
        assert_eq!(v["format"], json::FORMAT);
        assert_eq!(v["name"], s.name);
        round_trips(&s.bytes, s.name);
    }
}

#[test]
fn a_short_patch_is_the_defaults_with_what_it_names() {
    let (s, name) = json::read(r#"{"format": "sidou-patch/1"}"#).unwrap();
    assert_eq!((s, name.as_str()), (json::defaults(), "PATCH"));
    let (s, name) =
        json::read(r#"{"format": "sidou-patch/1", "name": "Acid", "wave": "pulse"}"#).unwrap();
    let mut want = json::defaults();
    want[LAB_SOUND_CONTROL as usize] = 0x40;
    assert_eq!((s, name.as_str()), (want, "ACID"));
}

#[test]
fn what_a_part_leaves_out_follows_the_sound() {
    let v = json!({"format": "sidou-patch/1", "wave": "pulse", "pulse_width": 25,
        "envelope": {"attack": 1, "decay": 2, "sustain": 12, "release": 3},
        "layers": [{"detune": 7}, {"wave": "saw", "envelope": {"release": 9}}],
        "table": {"layer": 3, "steps": [{"note": 12}, {"wave": "noise"}]},
        "lfos": [{"to": "cutoff", "rate_hz": 2, "depth": 10}],
        "fx": {"effect": {"type": "reverb", "mix": 20}}});
    let s = read(&v).unwrap();
    let layer = |n: u32| {
        let at = (LAB_SOUND_LAYER_VOICE + (n - 2) * LAB_SOUND_LAYER_VOICE_BYTES) as usize;
        s[at..at + 5].to_vec()
    };
    assert_eq!(layer(2), s[..5].to_vec(), "a layer is a copy of the first");
    assert_eq!(
        layer(3),
        vec![0x20, 0x12, 0xc9, 0x00, 0x04],
        "but what it names"
    );
    let step = LAB_SOUND_TABLE_STEPS as usize;
    assert_eq!(
        s[LAB_SOUND_TABLE as usize],
        0x80 | 2 << 5 | 2,
        "on, layer 3, two steps"
    );
    assert_eq!(
        &s[step..step + 4],
        &[0x20, 12, 0x80, 0],
        "a step plays its layer's wave"
    );
    assert_eq!(
        s[LAB_SOUND_LFO as usize],
        LAB_LFO_TRIANGLE as u8 | 2 << 4,
        "a triangle"
    );
    let fx = (LAB_SOUND_SLOTS + 2 * LIVE_SLOT_FIELDS) as usize;
    assert_eq!(
        &s[fx..fx + 5],
        &[3, 72, 48, 80, 20],
        "the reverb's defaults but its mix"
    );
    round_trips(&s, "PATCH");
}

#[test]
fn every_field_takes_its_range_and_refuses_past_it() {
    for f in json::fields() {
        let p = &f.path;
        match f.kind.as_str() {
            "integer" | "number" => {
                let (lo, hi) = bounds(&f.allowed);
                let step = if f.kind == "integer" { 1000 } else { 500 };
                for x in [lo, hi] {
                    heard_or_silent(&read(&with(p, number(x))), p);
                }
                for (x, past) in [(lo - step, "too low"), (hi + step, "too high")] {
                    let x = number(x);
                    let want = format!("{p}: {x} is {past}. Allowed: {}.", f.allowed);
                    assert_eq!(read(&with(p, x)), Err(want.replace("[]", "[0]")));
                }
                if f.kind == "integer" {
                    let half = number(lo + 500);
                    let e = read(&with(p, half)).unwrap_err();
                    assert!(e.contains("must be a whole number"), "{p}: {e}");
                }
            }
            "enum" | "set" => {
                for n in &f.names {
                    heard_or_silent(&read(&with(p, json!(n))), p);
                }
                let want = format!("{p}: \"bogus\" is not allowed. Allowed: {}.", f.allowed);
                assert_eq!(
                    read(&with(p, json!("bogus"))),
                    Err(want.replace("[]", "[0]"))
                );
            }
            "boolean" => {
                for x in [true, false] {
                    heard_or_silent(&read(&with(p, json!(x))), p);
                }
                let e = read(&with(p, json!(1))).unwrap_err();
                assert!(
                    e.ends_with("must be true or false. Allowed: true or false."),
                    "{e}"
                );
            }
            _ => {}
        }
    }
}

#[test]
fn a_list_holds_up_to_its_count() {
    for (path, max) in [("layers", 2), ("table.steps", 16), ("lfos", 2)] {
        let mut v = base();
        set(&mut v, path, json!(vec![json!({}); max]));
        read(&v).unwrap_or_else(|e| panic!("{path}: {e}"));
        set(&mut v, path, json!(vec![json!({}); max + 1]));
        let want = format!("{path}: has too many. Allowed: a list of up to {max}.");
        assert_eq!(read(&v), Err(want));
    }
}

#[test]
fn the_waves_join_with_plus_and_the_notes_take_names() {
    let s = read(&with("wave", json!("saw + triangle"))).unwrap();
    assert_eq!(s[0], 0x30);
    for (note, byte) in [
        (json!(-32), 0x60),
        (json!(95), 0x5f),
        (json!("hold"), 0x80),
        (json!("C#0"), 0x81),
        (json!("A4"), 0x80 + 57),
        (json!("B7"), 0xdf),
    ] {
        let s = read(&with("table.steps", json!([{"note": note}, {"note": 0}]))).unwrap();
        assert_eq!(s[LAB_SOUND_TABLE_STEPS as usize + 1], byte, "{note}");
        round_trips(&s, "PATCH");
    }
    for bad in ["C0", "H4", "C8", "saw"] {
        let e = read(&with("table.steps[].note", json!(bad))).unwrap_err();
        assert!(
            e.starts_with(&format!("table.steps[0].note: \"{bad}\" is not allowed")),
            "{e}"
        );
    }
    let e = read(&with("wave", json!("saw+saw"))).unwrap_err();
    assert!(e.starts_with("wave: \"saw+saw\" is not allowed"), "{e}");
}

#[test]
fn every_register_of_a_converted_field_comes_back() {
    let fields: [(u32, u32, bool); 6] = [
        (LAB_SOUND_CUTOFF, LIVE_CUTOFF_MAX, true),
        (LAB_SOUND_PULSE, LIVE_PULSE_WIDTH_MAX, true),
        (LAB_SOUND_PWM_DEPTH, LIVE_PWM_DEPTH_MAX, true),
        (LAB_SOUND_PWM_RATE, LIVE_PWM_STEP_MAX, true),
        (LAB_SOUND_SYNC_BASE, LAB_SOUND_SYNC_BASE_MAX, true),
        (LAB_SOUND_VIBRATO_RATE, LIVE_EXPRESSION_MAX, false),
    ];
    let mut s = json::read(BASE).unwrap().0;
    s[LAB_SOUND_MONO_SYNC as usize] = 1;
    for (at, max, wide) in fields {
        let keep = s;
        for v in 0..=max {
            s[at as usize] = v as u8;
            if wide {
                s[at as usize + 1] = (v >> 8) as u8;
            }
            let text = json::write(&s, "PATCH");
            match json::read(&text) {
                Ok((back, _)) => assert_eq!(back, s, "offset {at} = {v}: {text}"),
                Err(e) => assert!(
                    e.starts_with("the patch is silent"),
                    "offset {at} = {v}: {e}"
                ),
            }
        }
        s = keep;
    }
    for detent in 0..LAB_LFO_RATES {
        let at = (LAB_SOUND_LFO + 1) as usize;
        s[at - 1] = (LAB_LFO_TRIANGLE | LAB_LFO_CUTOFF << LAB_LFO_DEST_SHIFT) as u8;
        s[at] = detent as u8;
        round_trips(&s, "PATCH");
    }
}

#[test]
fn the_refusals_say_which_field_what_is_wrong_and_what_is_allowed() {
    let cases = [
        (r#"{"format": "sidou-patch/1", "filter": {"cuttoff_hz": 800}}"#,
         "filter.cuttoff_hz: no such field. Allowed here: mode, cutoff_hz, resonance, voices, resonance_link, lfo_depth, envelope."),
        (r#"{"format": "sidou-patch/1", "filter": {"resonance": 20}}"#,
         "filter.resonance: 20 is too high. Allowed: 0 to 15."),
        (r#"{"format": "sidou-patch/1", "wave": "sawtooth"}"#,
         "wave: \"sawtooth\" is not allowed. Allowed: triangle, saw, pulse and noise, one or several joined with + (like \"triangle+saw\"), or none."),
        (r#"{"format": "sidou-patch/1", "envelope": 5}"#,
         "envelope: must be an object. Allowed: an object { }."),
        (r#"{"format": "sidou-patch/1", "name": "A VERY LONG NAME"}"#,
         "name: is too long. Allowed: up to 12 characters: A to Z, 0 to 9, space and !\"#$%&'()*+,-./:;<=>?@[\\]^_."),
        (r#"{"format": "sidou-patch/1", "name": "Café"}"#,
         "name: has a character the FM-1 cannot show. Allowed: up to 12 characters: A to Z, 0 to 9, space and !\"#$%&'()*+,-./:;<=>?@[\\]^_."),
        (r#"{"wave": "saw"}"#, "format: is missing. Allowed: \"sidou-patch/1\"."),
        (r#"{"format": "sidou-patch/2"}"#,
         "format: \"sidou-patch/2\" is not allowed. Allowed: \"sidou-patch/1\"."),
        (r#"{"format": "sidou-patch/1", "layers": [{}], "sync_lead": {"on": true}}"#,
         "layers: cannot be used with sync_lead on: remove one of them."),
        (r#"{"format": "sidou-patch/1", "table": {"on": true}}"#,
         "table.steps: an on table needs at least one step."),
        (r#"{"format": "sidou-patch/1", "table": {"loop": 3, "steps": [{}, {}]}}"#,
         "table.loop: is past the last step: 0 plays once, 1 to the number of steps loops back to that step."),
        (r#"{"format": "sidou-patch/1", "fine_tune": 1e1}"#,
         "fine_tune: write the number plainly, without an exponent."),
        ("{\"format\": \"sidou-patch/1\",\n  \"wave\": \"saw\"\n  \"ring\": true}",
         "the text is not valid JSON at line 3, column 3: expected a , or }."),
        ("no patch here", "the text is not valid JSON at line 1, column 1: expected a { to start the patch."),
    ];
    for (text, want) in cases {
        assert_eq!(json::read(text), Err(want.to_string()), "{text}");
    }
}

#[test]
fn a_patch_around_other_text_is_read() {
    let answer = format!("Here is your sound:\n```json\n{BASE}\n```\nEnjoy!");
    assert_eq!(json::read(&answer), json::read(BASE));
}

#[test]
fn a_valid_but_silent_patch_is_refused() {
    let quiet = [
        with(
            "envelope",
            json!({"attack": 0, "decay": 0, "sustain": 0, "release": 0}),
        ),
        with("wave", json!("none")),
        with(
            "filter",
            json!({"mode": "high", "cutoff_hz": 14000, "resonance": 0, "voices": [1, 2, 3]}),
        ),
    ];
    for v in quiet {
        let e = read(&v).unwrap_err();
        assert!(
            e.starts_with("the patch is silent on both chips: nothing would be heard."),
            "{v}: {e}"
        );
    }
}

#[test]
fn a_patch_silent_on_one_chip_only_is_taken_with_a_warning() {
    let c = json::check(&with("wave", json!("saw+triangle")).to_string()).unwrap();
    assert_eq!(c.sound[0], 0x30);
    assert!(
        c.quiet
            .as_deref()
            .unwrap_or_default()
            .starts_with("the patch is silent on the 6581: nothing would be heard."),
        "{c:?}"
    );
    assert_eq!(json::check(BASE).unwrap().quiet, None);
}

#[test]
fn a_patch_goes_to_the_fm1_as_a_one_instrument_song_that_pulls_back_exactly() {
    for s in sidou_sounds() {
        let sng = json::song(&s.bytes, s.name, SID_MODEL_6581).expect(s.name);
        let mut song = Song::device();
        song.read(&sng, 0).unwrap();
        assert_eq!(
            song.raw().name[..s.name.len()]
                .iter()
                .map(|&c| c as u8)
                .collect::<Vec<_>>(),
            s.name.as_bytes()
        );
        let mut got = [0u8; LAB_SOUND_BYTES as usize];
        let notes = unsafe { lab_song_pull(song.ptr(), 1, SID_MODEL_6581, got.as_mut_ptr()) };
        assert_eq!((notes, got), (LAB_SONG_EXACT, s.bytes), "{}", s.name);
    }
}
