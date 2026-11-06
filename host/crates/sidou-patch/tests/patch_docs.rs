// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

mod common;
use sidou_patch::json::{self, Field};
use sidou_patch::sounds::{patch_file, sidou_sounds, Sound};
use sidou_sys::SID_MODEL_6581;

const UPDATE: &str = "SIDOU_PATCH_DOC_UPDATE";
const SITE: &str = "https://dsablic.github.io/sidou/";
const EXAMPLES: [&str; 6] = [
    "303 BASS",
    "WOBBLE BASS",
    "SWEEP PAD",
    "RING BELL",
    "NOISE KICK",
    "CHIP CHORD",
];
const FIELDS_BEGIN: &str = "<!-- the fields, generated -->\n";
const FIELDS_END: &str = "<!-- end of the fields -->\n";

const HINTS: [(&str, &str); 27] = [
    ("pulse_width", "percent of the cycle, 50 is square"),
    ("transpose", "semitones"),
    ("detune", "cents"),
    ("fine_tune", "cents, the whole sound"),
    ("filter.cutoff_hz", "Hz, as an 8580 plays it"),
    ("filter.voices", "the chip voices through the filter"),
    (
        "filter.resonance_link",
        "the resonance follows the cutoff knob",
    ),
    ("filter.lfo_depth", "a cutoff wobble at the PWM rate, 0 off"),
    (
        "filter.envelope.depth",
        "cutoff steps of 32 at the peak, below 0 closes",
    ),
    ("pwm.depth", "percent of the cycle swept"),
    ("pwm.link", "the PWM depth follows the pulse width knob"),
    (
        "sync_lead.on",
        "oscillator 1 hard-synced to a silent one; one note at a time",
    ),
    (
        "sync_lead.ratio",
        "the synced pitch over the note's, 2.75 is SYNC LEAD's",
    ),
    ("sync_lead.semitones", "added to the ratio"),
    ("vibrato.depth", "127 is about a semitone each way"),
    (
        "vibrato.delay",
        "hundredths of a second after the note starts",
    ),
    ("arp.rate", "50 Hz frames a note, or a MIDI clock division"),
    ("bend_range", "semitones"),
    ("velocity", "what the key velocity moves"),
    ("mod_wheel", "what the wheel moves"),
    ("aftertouch", "what pressure moves"),
    (
        "table.layer",
        "the layer it plays on; a one-oscillator sound plays it on every note",
    ),
    (
        "table.loop",
        "0 plays once and holds the last step, n goes back to step n",
    ),
    ("table.frames", "50 Hz frames a step"),
    (
        "table.steps[].test",
        "the SID's test bit: silences and resets the oscillator",
    ),
    ("lfos[].depth", "below 0 inverts; 63 on pitch is an octave"),
    ("lfos[].rate_hz", "snapped to the nearest of 32 rates"),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn compact(text: &str) -> String {
    let mut out = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in text.chars() {
        if quoted {
            quoted = escaped || c != '"';
            escaped = !escaped && c == '\\';
        } else if c.is_whitespace() {
            continue;
        } else {
            quoted = c == '"';
        }
        out.push(c);
    }
    out
}

fn link(text: &str) -> String {
    let encoded: String = compact(text)
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("{SITE}library.html#patch={encoded}")
}

fn hint(path: &str) -> Option<&'static str> {
    HINTS.iter().find(|(p, _)| *p == path).map(|(_, h)| *h)
}

fn children<'a>(fields: &'a [Field], parent: &str) -> Vec<&'a Field> {
    let prefix = if parent.is_empty() {
        String::new()
    } else {
        format!("{parent}.")
    };
    fields
        .iter()
        .filter(|f| {
            f.path
                .strip_prefix(&prefix)
                .is_some_and(|rest| !rest.is_empty() && !rest.contains('.'))
        })
        .collect()
}

fn leaf(fields: &[Field], f: &Field) -> String {
    let name = f.path.rsplit('.').next().unwrap();
    let name = name.trim_end_matches("[]");
    let base = format!("`{name}`");
    match f.kind.as_str() {
        "object" => format!("{base} {{{}}}", group(fields, &f.path)),
        "list" => {
            let element = format!("{}[]", f.path);
            let inner = children(fields, &element);
            let same = inner.iter().all(|c| {
                let top = c.path.rsplit('.').next().unwrap();
                fields
                    .iter()
                    .any(|t| t.path == top && t.allowed == c.allowed)
            });
            let names: Vec<String> = inner
                .iter()
                .map(|c| format!("`{}`", c.path.rsplit('.').next().unwrap()))
                .collect();
            let max = f.allowed.rsplit(' ').next().unwrap();
            if same {
                format!(
                    "{base} up to {max} of {{{} as at the top}}",
                    names.join(", ")
                )
            } else {
                format!("{base} up to {max} of {{{}}}", group(fields, &element))
            }
        }
        _ => match hint(&f.path) {
            Some(h) => format!("{base} {} ({h})", f.allowed),
            None => format!("{base} {}", f.allowed),
        },
    }
}

fn group(fields: &[Field], parent: &str) -> String {
    let kids = children(fields, parent);
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < kids.len() {
        let mut j = i + 1;
        let simple =
            |f: &Field| !matches!(f.kind.as_str(), "object" | "list") && hint(&f.path).is_none();
        while j < kids.len()
            && simple(kids[i])
            && simple(kids[j])
            && kids[j].allowed == kids[i].allowed
        {
            j += 1;
        }
        if j - i > 1 {
            let names: Vec<String> = kids[i..j]
                .iter()
                .map(|f| format!("`{}`", f.path.rsplit('.').next().unwrap()))
                .collect();
            parts.push(format!("{} {}", names.join(", "), kids[i].allowed));
        } else {
            parts.push(leaf(fields, kids[i]));
        }
        i = j;
    }
    parts.join("; ")
}

fn field_lines(fields: &[Field]) -> String {
    children(fields, "")
        .iter()
        .map(|f| format!("- {}", leaf(fields, f)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn sound(name: &str) -> Sound {
    sidou_sounds()
        .into_iter()
        .find(|s| s.name == name)
        .expect(name)
}

fn examples() -> String {
    EXAMPLES
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let s = sound(name);
            let text = json::write(&s.bytes, s.name);
            let tail = if i == 0 {
                format!("\n\n{}", link(&text))
            } else {
                String::new()
            };
            format!("\"{}\":\n\n```json\n{}```{tail}", s.about, text)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn step_text(src: &str) -> String {
    let voice = serde_json::from_str::<Value>(src).unwrap()["voice"]
        .as_u64()
        .unwrap_or(1) as u32
        - 1;
    let mut song = sidou_sys::song::Song::device();
    song.read(&json::step_song(src, SID_MODEL_6581).unwrap(), 0)
        .unwrap();
    json::write_step(&song, voice).unwrap()
}

fn instructions() -> String {
    let template = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/prompt-to-patch.md"),
    )
    .unwrap();
    let fields = json::fields();
    template
        .replace("{{FIELDS}}", &field_lines(&fields))
        .replace("{{EXAMPLES}}", &examples())
        .replace("{{STEP_EXAMPLE}}", &step_text(common::ACID))
}

fn bounds(allowed: &str) -> (Value, Value) {
    let words: Vec<&str> = allowed.split(' ').collect();
    let at = words.iter().position(|w| *w == "to").unwrap();
    let num = |w: &str| serde_json::from_str::<Value>(w.trim_end_matches(',')).unwrap();
    (num(words[at - 1]), num(words[at + 1]))
}

fn schema_of(fields: &[Field], f: &Field) -> Value {
    let mut v = match f.kind.as_str() {
        "integer" | "number" => {
            let (lo, hi) = bounds(&f.allowed);
            json!({"type": f.kind, "minimum": lo, "maximum": hi})
        }
        "boolean" => json!({"type": "boolean"}),
        "enum" => json!({"enum": f.names}),
        "set" => {
            let any = f.names[1..].join("|");
            json!({"type": "string", "pattern": format!("^({}|({any})( ?\\+ ?({any}))*)$", f.names[0])})
        }
        "voices" => json!({"type": "array", "uniqueItems": true,
            "items": {"type": "integer", "minimum": 1, "maximum": 3}}),
        "note" => json!({"oneOf": [{"type": "integer", "minimum": -32, "maximum": 95},
            {"type": "string", "pattern": "^(hold|[A-G]#?[0-7]|[A-G]-[0-7])$"}]}),
        "name" => json!({"type": "string", "pattern": "^[ -_a-z]{0,12}$"}),
        "format" => json!({"const": json::FORMAT}),
        "object" => object_schema(fields, &f.path),
        _ => {
            let max: u64 = f.allowed.rsplit(' ').next().unwrap().parse().unwrap();
            json!({"type": "array", "maxItems": max,
                "items": object_schema(fields, &format!("{}[]", f.path))})
        }
    };
    let text = hint(&f.path).map_or(f.allowed.clone(), |h| format!("{} ({h})", f.allowed));
    v["description"] = json!(text);
    v
}

fn object_schema(fields: &[Field], parent: &str) -> Value {
    let mut props = Map::new();
    for f in children(fields, parent) {
        let name = f.path.rsplit('.').next().unwrap().trim_end_matches("[]");
        props.insert(name.to_string(), schema_of(fields, f));
    }
    json!({"type": "object", "additionalProperties": false, "properties": props})
}

fn schema() -> String {
    let mut s = object_schema(&json::fields(), "");
    let o = s.as_object_mut().unwrap();
    o.insert("required".into(), json!(["format"]));
    let mut head = Map::new();
    head.insert(
        "$schema".into(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    head.insert("$id".into(), json!(format!("{SITE}patch.schema.json")));
    head.insert("title".into(), json!("A SIDOU LAB sound (sidou-patch/1)"));
    for (k, v) in o.iter() {
        head.insert(k.clone(), v.clone());
    }
    serde_json::to_string_pretty(&Value::Object(head)).unwrap() + "\n"
}

fn bank_schema() -> String {
    let v = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": format!("{SITE}bank.schema.json"),
        "title": "Up to sixteen SIDOU LAB sounds, sent as one song (sidou-bank/1)",
        "type": "object",
        "additionalProperties": false,
        "required": ["format", "patches"],
        "properties": {
            "format": {"const": "sidou-bank/1"},
            "name": {"type": "string", "pattern": "^[!-_a-z][ -_a-z]{0,15}$", "description": "the song's name on the FM-1"},
            "patches": {"type": "array", "minItems": 1, "maxItems": 16,
                "items": {"$ref": format!("{SITE}patch.schema.json")},
                "description": "each a sidou-patch/1 patch, its format optional"}
        }
    });
    serde_json::to_string_pretty(&v).unwrap() + "\n"
}

fn reference_doc(current: &str) -> String {
    let fields = json::fields();
    let rows: Vec<String> = fields
        .iter()
        .filter(|f| !matches!(f.kind.as_str(), "object"))
        .map(|f| {
            let h = hint(&f.path).map_or(String::new(), |h| format!(" ({h})"));
            format!("| `{}` | {}{h} |", f.path, f.allowed.replace('|', "\\|"))
        })
        .collect();
    let table = format!("| Field | Allowed |\n|---|---|\n{}\n", rows.join("\n"));
    let (head, rest) = current
        .split_once(FIELDS_BEGIN)
        .expect("the fields' start mark");
    let (_, tail) = rest.split_once(FIELDS_END).expect("the fields' end mark");
    format!("{head}{FIELDS_BEGIN}{table}{FIELDS_END}{tail}")
}

fn committed(path: PathBuf, want: &str) -> bool {
    if std::env::var(UPDATE).as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, want).unwrap();
        return true;
    }
    std::fs::read_to_string(&path).ok().as_deref() == Some(want)
}

const TRIALS: [&str; 3] = [
    r#"{"format": "sidou-patch/1", "name": "SQUELCH", "wave": "saw", "envelope": {"attack": 0, "decay": 7, "sustain": 8, "release": 4}, "filter": {"mode": "low", "cutoff_hz": 2200, "resonance": 13, "voices": [1], "envelope": {"depth": 45, "attack": 0, "decay": 6, "sustain": 0, "release": 4}}, "bend_range": 12, "fx": {"distortion": {"type": "overdrive", "mix": 90}}}"#,
    r#"{"format": "sidou-patch/1", "name": "GLASS BELL", "wave": "triangle", "ring": true, "envelope": {"attack": 0, "decay": 10, "sustain": 0, "release": 9}, "filter": {"voices": []}, "layers": [{"ring": false, "detune": 4}, {"ring": false, "transpose": 19}], "fx": {"effect": {"type": "reverb", "mix": 35}}}"#,
    r#"{"format": "sidou-patch/1", "name": "DUSK PAD", "wave": "pulse", "pulse_width": 40, "envelope": {"attack": 9, "decay": 10, "sustain": 12, "release": 10}, "filter": {"mode": "low", "cutoff_hz": 3000, "resonance": 4, "voices": [1, 2, 3]}, "pwm": {"depth": 20, "rate_hz": 0.4}, "vibrato": {"rate_hz": 5, "depth": 8, "delay": 60}, "lfos": [{"to": "cutoff", "rate_hz": 0.2, "depth": 15}], "fx": {"modulation": {"type": "chorus"}, "effect": {"type": "lush", "mix": 40}}}"#,
];

const FIXTURE_CASES: [&str; 5] = [
    r#"{"format": "sidou-patch/1", "filter": {"resonance": 20}}"#,
    r#"{"format": "sidou-patch/1", "name": "Café"}"#,
    r#"{"format": "sidou-patch/1", "wave": "none"}"#,
    r#"{"format": "sidou-patch/1", "name": "quiet", "wave": "saw+triangle", "envelope": {"sustain": 15}, "filter": {"voices": []}}"#,
    "{\"format\": \"sidou-patch/1\",\n \"wave\": \"saw\"\n}}",
];

const STEP_REFUSED: &str = r#"{"format": "sidou-step/1", "steps": [{"step": 1, "drum": "kick"}]}"#;
const BANK_REFUSED: &str =
    r#"{"format": "sidou-bank/1", "name": "TWO", "patches": [{}, {"wave": "sawtooth"}]}"#;

fn hashed(b: &[u8]) -> Value {
    let h: String = Sha256::digest(b)[..8]
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect();
    json!([b.len(), h])
}

fn fixture(text: &str) -> Value {
    match json::check(text) {
        Ok(c) => {
            json!({"text": text, "ok": true, "quiet": c.quiet.unwrap_or_default(), "name": c.name,
            "sng": hashed(&json::song(&c.sound, &c.name, SID_MODEL_6581).unwrap())})
        }
        Err(e) => json!({"text": text, "ok": false, "error": e}),
    }
}

fn fixtures() -> String {
    let mut cases: Vec<Value> = sidou_sounds()
        .iter()
        .map(|s| fixture(&json::write(&s.bytes, s.name)))
        .collect();
    cases.extend(FIXTURE_CASES.iter().chain(&TRIALS).map(|t| fixture(t)));
    let all = sidou_sounds();
    let patches: Vec<(json::Sound, &str)> = all.iter().map(|s| (s.bytes, s.name)).collect();
    let sixteen = json::write_bank("SIDOU SOUNDS", &patches);
    for (_, src) in common::STEPS {
        let text = step_text(src);
        cases.push(json!({"text": text, "ok": true, "quiet": "", "name": serde_json::from_str::<Value>(&text).unwrap()["name"],
            "sng": hashed(&json::step_song(&text, SID_MODEL_6581).unwrap())}));
    }
    cases.push(json!({"text": STEP_REFUSED, "ok": false, "error": json::step_song(STEP_REFUSED, SID_MODEL_6581).unwrap_err()}));
    for text in [sixteen.as_str(), BANK_REFUSED] {
        cases.push(match json::read_bank(text) {
            Ok(b) => json!({"text": text, "ok": true, "quiet": b.quiet.unwrap_or_default(), "name": b.name,
                "sng": hashed(&json::bank_song(text, SID_MODEL_6581).unwrap())}),
            Err(e) => json!({"text": text, "ok": false, "error": e}),
        });
    }
    let rows: Vec<String> = cases.iter().map(|c| format!("  {c}")).collect();
    format!("{{\"patches\": [\n{}\n]}}\n", rows.join(",\n"))
}

#[test]
fn the_instructions_schema_reference_and_examples_are_current() {
    let r = root();
    let reference = r.join("docs/reference/patch-json.md");
    let doc = reference_doc(&std::fs::read_to_string(&reference).unwrap());
    let mut stale = Vec::new();
    let mut files = vec![
        (r.join("docs/prompt-to-patch.md"), instructions()),
        (r.join("docs/reference/patch.schema.json"), schema()),
        (reference, doc),
        (r.join("web/patch-fixtures.json"), fixtures()),
    ];
    let all = sidou_sounds();
    let patches: Vec<(json::Sound, &str)> = all.iter().map(|s| (s.bytes, s.name)).collect();
    files.push((
        r.join("assets/sounds/sidou-sounds-bank.json"),
        json::write_bank("SIDOU SOUNDS", &patches),
    ));
    files.push((r.join("docs/reference/bank.schema.json"), bank_schema()));
    for (name, src) in common::STEPS {
        files.push((r.join(format!("assets/steps/{name}.json")), step_text(src)));
    }
    for s in sidou_sounds() {
        files.push((
            r.join("assets/sounds").join(patch_file(s.name)),
            json::write(&s.bytes, s.name),
        ));
    }
    for (path, want) in files {
        if !committed(path.clone(), &want) {
            stale.push(path.display().to_string());
        }
    }
    assert!(stale.is_empty(), "stale, rerun with {UPDATE}=1: {stale:?}");
}

#[test]
fn every_example_in_the_instructions_converts_and_its_link_is_the_patch() {
    let text = instructions();
    let all: Vec<&str> = text
        .split("\n```json\n")
        .skip(1)
        .map(|b| b.split("```").next().unwrap())
        .collect();
    let (steps, blocks): (Vec<&str>, Vec<&str>) =
        all.iter().partition(|b| b.contains("\"sidou-step/1\""));
    assert_eq!((steps.len(), blocks.len()), (1, EXAMPLES.len()));
    let mut song = sidou_sys::song::Song::device();
    song.read(&json::step_song(steps[0], SID_MODEL_6581).unwrap(), 0)
        .unwrap();
    assert_eq!(
        json::write_step(&song, 0).as_deref(),
        Some(steps[0]),
        "the STEP example round-trips"
    );
    for (block, name) in blocks.iter().zip(EXAMPLES) {
        assert_eq!(
            json::read(block),
            Ok((sound(name).bytes, name.to_string())),
            "{block}"
        );
    }
    let first = text
        .lines()
        .find(|l| l.starts_with(&format!("{SITE}library.html#patch=")))
        .unwrap();
    assert_eq!(first, link(blocks[0]));
}

#[test]
fn every_field_is_in_the_instructions() {
    let text = instructions();
    for f in json::fields() {
        let name = f.path.rsplit('.').next().unwrap().trim_end_matches("[]");
        assert!(text.contains(&format!("`{name}`")), "{}", f.path);
    }
    for (path, _) in HINTS {
        assert!(
            json::fields().iter().any(|f| f.path == path),
            "a hint for no field: {path}"
        );
    }
}

#[test]
fn patches_written_from_the_instructions_alone_are_sounds_on_both_chips() {
    for t in TRIALS {
        let c = json::check(t).unwrap_or_else(|e| panic!("{e}: {t}"));
        assert_eq!(c.quiet, None, "{t}");
        assert_eq!(
            json::read(&json::write(&c.sound, &c.name)),
            Ok((c.sound, c.name))
        );
    }
}

#[test]
fn a_link_is_short_enough_for_any_sound() {
    let longest = sidou_sounds()
        .iter()
        .map(|s| link(&json::write(&s.bytes, s.name)).len())
        .max()
        .unwrap();
    println!("the longest of the sixteen links: {longest} characters");
    assert!(longest < 2048, "{longest}");
}
