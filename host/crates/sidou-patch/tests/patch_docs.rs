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
const ACID: (&str, &str) = (
    "a squelchy acid bass",
    r#"{"format": "sidou-patch/1", "name": "SQUELCH 2", "wave": "saw", "envelope": {"attack": 0, "decay": 6, "sustain": 0, "release": 3}, "filter": {"mode": "low", "cutoff_hz": 500, "resonance": 14, "voices": [1, 2, 3], "envelope": {"depth": 50, "attack": 0, "decay": 5, "sustain": 0, "release": 3}}, "fx": {"distortion": {"type": "overdrive", "mix": 70}}}"#,
);
const EXAMPLES: [&str; 1] = ["RING BELL"];
const MORE_EXAMPLES: [&str; 4] = ["WOBBLE BASS", "SWEEP PAD", "NOISE KICK", "CHIP CHORD"];

type Card = (&'static str, &'static str);
pub const LAB: [(&str, [Card; 4], &str); 10] = [
    (
        "OSC",
        [
            (
                "PULSE",
                "the pulse width: toward 50 fuller, toward the ends thinner",
            ),
            ("PWM", "the PWM depth: up, a wider sweep"),
            ("PWM HZ", "the PWM rate: up, faster"),
            ("TUNE", "fine tune"),
        ],
        "the wave",
    ),
    (
        "ENVELOPE",
        [
            ("ATTACK", "up: a slower start"),
            ("DECAY", "up: falls more slowly"),
            ("SUS", "up: a louder held part, longer notes"),
            ("RELEASE", "up: a longer tail after the key"),
        ],
        "the envelope mode, for every sound",
    ),
    (
        "FILTER",
        [
            ("CUTOFF", "up: brighter"),
            ("RESO", "up: a sharper, ringing peak"),
            ("VOICES", "the voices through the filter"),
            (
                "LFO",
                "the filter LFO's depth: up, a wider wobble at PWM HZ",
            ),
        ],
        "LP, BP, HP or NOTCH",
    ),
    (
        "FILTER ENV",
        [
            ("ATTACK", "up: the sweep rises more slowly"),
            ("DECAY", "up: the wow falls more slowly"),
            ("SUS", "up: the filter stays open longer"),
            ("RELEASE", "up: it closes more slowly after the key"),
        ],
        "depth: up toward +63, more squelch on each note; below 0 it sweeps down",
    ),
    (
        "MOD",
        [
            ("VIB HZ", "the vibrato's rate"),
            ("DEPTH", "the vibrato's depth"),
            ("DELAY", "up: the vibrato comes in later"),
            ("RATE", "the arpeggio's speed"),
        ],
        "the arpeggio's order, or off",
    ),
    (
        "LFO",
        [
            ("EDIT", "LFO 1 or 2"),
            (
                "DEST",
                "what it moves: pitch, pulse width, cutoff or volume",
            ),
            ("RATE", "up: faster"),
            ("DEPTH", "up: more; below 0 inverted"),
        ],
        "the shape, or off",
    ),
    (
        "FX",
        [
            (
                "AMT",
                "the selected slot's amount (named for its type, overdrive's DRIVE)",
            ),
            ("A", "its second knob (overdrive's TONE)"),
            ("B", "its third (overdrive's BIAS)"),
            ("MIX", "up: wetter"),
        ],
        "the slot's type; SCL selects the slot",
    ),
    (
        "MACRO",
        [
            ("RES LNK", "the resonance follows the cutoff knob"),
            ("PWM LNK", "the PWM depth follows the pulse width knob"),
            ("", ""),
            ("", ""),
        ],
        "nothing",
    ),
    (
        "LAYERS",
        [
            ("EDIT", "the layer OSC and ENVELOPE edit"),
            ("TRANS", "its transpose"),
            ("DETUNE", "up: wider, fatter"),
            ("FILT", "the layer into the filter or not"),
        ],
        "one, two or three oscillators",
    ),
    (
        "TABLE",
        [
            ("STEP", "the step the others edit"),
            ("WAVE", "its wave"),
            ("NOTE", "its pitch"),
            ("FRAMES", "up: slower steps"),
        ],
        "off, play once, or loop",
    ),
];

fn lab_names() -> String {
    LAB.iter()
        .map(|(page, cards, alg)| {
            let names: Vec<&str> = cards
                .iter()
                .map(|c| c.0)
                .filter(|n| !n.is_empty())
                .collect();
            format!("- {page}: {}; ALGORITHM {alg}", names.join(", "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn lab_effects() -> String {
    let rows: Vec<String> = LAB
        .iter()
        .flat_map(|(page, cards, alg)| {
            cards
                .iter()
                .zip(["K1", "K2", "K3", "K4"])
                .filter(|(c, _)| !c.0.is_empty())
                .map(move |(c, k)| format!("| {page} | {k} {} | {} |", c.0, c.1))
                .chain(std::iter::once(format!("| {page} | ALGORITHM | {alg} |")))
        })
        .collect();
    format!(
        "| Page | Control | Turning it |\n|---|---|---|\n{}",
        rows.join("\n")
    )
}
const FIELDS_BEGIN: &str = "<!-- the fields, generated -->\n";
const FIELDS_END: &str = "<!-- end of the fields -->\n";

const HINTS: [(&str, &str); 27] = [
    ("pulse_width", "percent of the cycle, 50 is square"),
    ("transpose", "semitones"),
    ("detune", "cents"),
    ("fine_tune", "cents, the whole sound"),
    ("filter.cutoff_hz", "Hz, as an 8580 plays it"),
    (
        "filter.voices",
        "the chip voices through the filter: all three or none on a one-oscillator sound",
    ),
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

fn acid_text() -> String {
    let (sound, name) = json::read(ACID.1).unwrap();
    json::write(&sound, &name)
}

fn example(name: &str) -> String {
    let s = sound(name);
    format!(
        "\"{}\":\n\n```json\n{}```",
        s.about,
        json::write(&s.bytes, s.name)
    )
}

fn examples() -> String {
    let acid = acid_text();
    let first = format!(
        "\"{}\":\n\n```json\n{acid}```\n\n{}\n\nFILTER ENV, ALGORITHM (depth) up toward +63: more squelch on each note. \
         FILTER, RESO (K2) down: less ringing, rounder. ENVELOPE, DECAY (K2) up: longer notes.",
        ACID.0,
        link(&acid)
    );
    std::iter::once(first)
        .chain(EXAMPLES.iter().map(|n| example(n)))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn more_examples() -> String {
    let all = sidou_sounds();
    let patches: Vec<(json::Sound, &str)> = all.iter().take(3).map(|s| (s.bytes, s.name)).collect();
    format!(
        "# SIDOU: more examples\n\nMore patches (`sidou-patch/1`), a bank and a STEP pattern, as \
         https://dsablic.github.io/sidou/llms.txt describes them.\n\n{}\n\n## A bank (`sidou-bank/1`)\n\n\
         `name` is the song's name on the FM-1 (1 to 16 characters); `patches` holds 1 to 16 patches, each \
         its `format` optional. All sixteen of SIDOU's sounds as a bank: \
         https://dsablic.github.io/sidou/sounds/sidou-sounds-bank.json. Three of them:\n\n```json\n{}```\n\n\
         ## A STEP pattern (`sidou-step/1`)\n\n{}\n\nAn acid line:\n\n```json\n{}```\n",
        MORE_EXAMPLES.iter().map(|n| example(n)).collect::<Vec<_>>().join("\n\n"),
        json::write_bank("THREE", &patches),
        STEP_FIELDS,
        step_text(common::ACID)
    )
}

const STEP_FIELDS: &str = "The FM-1 gets it as a song that opens on STEP (its 16 step keys) and plays. \
`format` \"sidou-step/1\"; `name` the song's name (1 to 16 characters); `voice` 1 to 3, the lane; `role` \
auto, drums, bass, arp, lead, none, bass+drums or acid (what DICE and the SCENE hear it as); `gate` \
staccato (a key off after each note, the default) or legato; `pages` 1 to 4 of 16 steps. `instrument`: a \
patch (its `format` optional), \"pulse\" or \"bass\" (the new song's own), or \"kit\" (its drums). \
`steps`: one entry a step that plays, `step` 1 to 64; `note` \"C0\" to \"G#7\" (default \"C3\"); with the kit, \
`drum` kick, snare, closed_hat, open_hat, clap, low_tom, high_tom, rim, digi_kick, digi_snare, digi_clap or \
digi_hat; `slide` true (the gate held to the next note, which glides from this one, a 303's slide); \
`accent` true (brighter and louder); `chance` 0 to 100 (its chance to play each pass). Steps left out are \
rests.";

fn fields_doc() -> String {
    format!(
        "# SIDOU: every patch field\n\nThe ranges of `sidou-patch/1`, as the FM-1's converter checks them \
         (the exact contract: https://dsablic.github.io/sidou/patch.schema.json). A field left out is LAB's \
         starting sound.\n\n{}\n\nThe effects' knobs, 0 to 127: tape: amount drive. Overdrive and fuzz: amount \
         drive, a tone (fuzz: octave), b bias. Crush: amount bits, a rate. Lofi: amount tone, a hum, b noise. \
         Chorus: amount depth, a rate, b ensemble. Flanger and phaser: depth, rate, feedback. Tremolo: depth, \
         rate, shape. Autopan: width, rate, shape. Wow: wow, flutter. Delay and pingpong: amount feedback, a \
         time, b damping. Reverb and lush: amount decay, a damping, b diffusion (lush: band).\n",
        field_lines(&json::fields())
    )
}

fn lab_doc() -> String {
    format!(
        "# SIDOU: LAB's controls\n\nWhat each of LAB's pages and controls does, turned up (clockwise), \
         for suggesting what to turn on the FM-1. SELECT steps the pages; K1 to K4 turn the four cards; \
         ALGORITHM is the page's choice.\n\n{}\n",
        lab_effects()
    )
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
    template
        .replace("{{LAB_NAMES}}", &lab_names())
        .replace("{{EXAMPLES}}", &examples())
}

fn full() -> String {
    [instructions(), fields_doc(), lab_doc(), more_examples()].join("\n")
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

const TRIALS: [&str; 3] = ["acid", "bell", "pad"];

fn trial(name: &str, ext: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/data/trials/{name}.{ext}")),
    )
    .unwrap()
}

const FIXTURE_CASES: [&str; 5] = [
    r#"{"format": "sidou-patch/1", "filter": {"resonance": 20}}"#,
    r#"{"format": "sidou-patch/1", "name": "Café"}"#,
    r#"{"format": "sidou-patch/1", "wave": "none"}"#,
    r#"{"format": "sidou-patch/1", "name": "quiet", "wave": "saw+triangle", "envelope": {"sustain": 15}, "filter": {"voices": []}}"#,
    "{\"format\": \"sidou-patch/1\",\n \"wave\": \"saw\"\n}}",
];

const STEP_REFUSED: &str = r#"{"format": "sidou-step/1", "steps": [{"step": 1, "drum": "kick"}]}"#;
const SONG_REFUSED: &str = r#"{"format": "sidou-song/1", "bpm": 400}"#;
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
    cases.extend(FIXTURE_CASES.iter().map(|t| fixture(t)));
    cases.extend(TRIALS.iter().map(|t| fixture(&trial(t, "json"))));
    let all = sidou_sounds();
    let patches: Vec<(json::Sound, &str)> = all.iter().map(|s| (s.bytes, s.name)).collect();
    let sixteen = json::write_bank("SIDOU SOUNDS", &patches);
    for (_, src) in common::STEPS {
        let text = step_text(src);
        cases.push(json!({"text": text, "ok": true, "quiet": "", "name": serde_json::from_str::<Value>(&text).unwrap()["name"],
            "sng": hashed(&json::step_song(&text, SID_MODEL_6581).unwrap())}));
    }
    cases.push(json!({"text": STEP_REFUSED, "ok": false, "error": json::step_song(STEP_REFUSED, SID_MODEL_6581).unwrap_err()}));
    for text in [common::GROOVE, common::THREE_PARTS] {
        let made = json::check_song(text, SID_MODEL_6581).unwrap();
        cases.push(json!({"text": text, "ok": true, "quiet": "", "name": serde_json::from_str::<Value>(text).unwrap()["name"],
            "sng": hashed(&made.sng), "bpm10": made.bpm_played10}));
    }
    cases.push(json!({"text": SONG_REFUSED, "ok": false, "error": json::check_song(SONG_REFUSED, SID_MODEL_6581).unwrap_err()}));
    for text in [sixteen.as_str(), BANK_REFUSED] {
        cases.push(match json::check_bank(text, SID_MODEL_6581) {
            Ok((b, sng)) => json!({"text": text, "ok": true, "quiet": b.quiet.unwrap_or_default(), "name": b.name,
                "sng": hashed(&sng)}),
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
        (r.join("docs/prompt-to-patch-fields.md"), fields_doc()),
        (r.join("docs/prompt-to-patch-lab.md"), lab_doc()),
        (r.join("docs/prompt-to-patch-examples.md"), more_examples()),
        (r.join("docs/prompt-to-patch-full.md"), full()),
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

fn json_blocks(text: &str) -> Vec<&str> {
    text.split("\n```json\n")
        .skip(1)
        .map(|b| b.split("```").next().unwrap())
        .collect()
}

#[test]
fn every_example_in_the_instructions_converts_and_its_link_is_the_patch() {
    let text = instructions();
    let blocks = json_blocks(&text);
    assert_eq!(blocks.len(), EXAMPLES.len() + 1);
    assert_eq!(blocks[0], acid_text());
    assert_eq!(
        json::check(blocks[0]).map(|c| c.quiet),
        Ok(None),
        "the acid example is heard"
    );
    for (block, name) in blocks[1..].iter().zip(EXAMPLES) {
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
fn every_example_in_the_examples_file_converts_back_to_itself() {
    let text = more_examples();
    let blocks = json_blocks(&text);
    assert_eq!(blocks.len(), MORE_EXAMPLES.len() + 2);
    for (block, name) in blocks.iter().zip(MORE_EXAMPLES) {
        assert_eq!(
            json::read(block),
            Ok((sound(name).bytes, name.to_string())),
            "{block}"
        );
    }
    let bank = blocks[MORE_EXAMPLES.len()];
    assert_eq!(json::read_bank(bank).map(|b| b.patches.len()), Ok(3));
    let step = blocks[MORE_EXAMPLES.len() + 1];
    let mut song = sidou_sys::song::Song::device();
    song.read(&json::step_song(step, SID_MODEL_6581).unwrap(), 0)
        .unwrap();
    assert_eq!(
        json::write_step(&song, 0).as_deref(),
        Some(step),
        "the STEP example round-trips"
    );
}

#[test]
fn the_full_instructions_are_the_core_and_every_linked_file() {
    let full = full();
    for part in [instructions(), fields_doc(), lab_doc(), more_examples()] {
        assert!(full.contains(&part));
    }
    for linked in [
        "llms-fields.txt",
        "llms-lab.txt",
        "llms-examples.txt",
        "patch.schema.json",
    ] {
        assert!(
            instructions().contains(&format!("{SITE}{linked}")),
            "{linked}"
        );
    }
}

#[test]
fn every_field_is_in_the_instructions() {
    let text = fields_doc();
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
fn answers_written_from_the_instructions_alone_are_heard_and_have_the_answers_shape() {
    for name in TRIALS {
        let patch = trial(name, "json");
        let answer = format!(
            "```json\n{patch}```\n\n{}\n\n{}",
            link(&patch),
            trial(name, "tweaks")
        );
        let c = json::check(&answer).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(c.quiet, None, "{name}");
        assert_eq!(
            json::read(&json::write(&c.sound, &c.name)),
            Ok((c.sound, c.name.clone()))
        );
        let tweaks: Vec<String> = trial(name, "tweaks").lines().map(str::to_string).collect();
        assert!(
            (2..=4).contains(&tweaks.len()),
            "{name}: two to four suggestions"
        );
        for t in &tweaks {
            let (page, rest) = t.split_once(", ").unwrap();
            let (_, cards, _) = LAB
                .iter()
                .find(|(p, ..)| *p == page)
                .unwrap_or_else(|| panic!("{name}: no page {page}"));
            let named = rest.starts_with("ALGORITHM")
                || cards.iter().enumerate().any(|(k, c)| {
                    !c.0.is_empty() && rest.starts_with(&format!("{} (K{})", c.0, k + 1))
                });
            assert!(named && rest.contains(": "), "{name}: {t}");
        }
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
