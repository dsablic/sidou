// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! PAPER LANTERNS: chill and melodic, D major, a swung 94 BPM (funktempo 9/7, sixteenths),
//! the 8580.
//!
//! Plan. Gmaj7 F#m7 Em7 A7 | Gmaj7 F#m7 Bm7 Asus, then Em7 F#m7 Gmaj7 A6 | Bm7 Gmaj7 Em9
//! Asus-A. Voice 1 a round triangle bass whose downbeats open with a pitch-dropped thump (a
//! kick and a bass in one note), multiplexed with a soft rim click on the backbeats; voice 2
//! the melody, a slowly pulse-width modulated lead with delayed gentle vibrato, then a
//! pulse-to-triangle flute; voice 3 seventh-chord arpeggios, two frames a note (a shimmer
//! rather than a buzz), the only filtered voice: a low-pass that opens through the intro
//! and then breathes, a two-bar swell.
//!
//! Bars: intro 4 (the chords and the bass, the filter opening, a pickup) | A 8 (the
//! melody) | B 8 (the second melody, rising) | A' 8 (the melody, its second half higher,
//! on the flute) | ending 4 (Gmaj7 A6 Dmaj9). 32 bars, 82 s; the loop restarts at A.

use super::*;

pub const SONG: Showcase = Showcase {
    name: "paper-lanterns",
    title: "PAPER LANTERNS",
    model: SID_MODEL_8580,
    kit,
};

/// A chord program: the intervals, two frames each, looping.
fn chord(k: &mut Kit, name: &str, notes: &[u8]) {
    let mut rows = vec![(0x41, 0x00), (0x00, 0x00)];
    for &n in &notes[1..] {
        rows.push((0x00, n));
        rows.push((0x00, n));
    }
    rows.push((0x00, 0x00));
    rows.push(jump(2));
    k.wave(name, &rows);
}

/// A bar of voice 3: the chord struck on the downbeat and again on the and of three.
fn chords(root: &str, kind: &str) -> String {
    format!("{root}/{kind} .9 {root}/{kind} .5")
}

pub fn kit() -> Kit {
    let mut k = Kit::new("PAPER LANTERNS");

    // the wavetable
    k.wave("tri", &[(0x11, 0x00), jump(0)]);
    // the kick-bass: the triangle dropping two octaves onto the note
    k.wave(
        "kb",
        &[
            (0x11, 0x18),
            (0x11, 0x10),
            (0x11, 0x0a),
            (0x11, 0x05),
            (0x11, 0x00),
            jump(0),
        ],
    );
    k.wave(
        "rim",
        &[(0x81, abs(80)), (0x41, abs(62)), (0x40, abs(62)), jump(0)],
    );
    k.wave("lead", &[(0x41, 0x00), jump(0)]);
    k.wave(
        "flute",
        &[(0x41, 0x00), (0x41, 0x00), (0x11, 0x00), jump(0)],
    );
    chord(&mut k, "maj7", &[0, 4, 7, 11]);
    chord(&mut k, "min7", &[0, 3, 7, 10]);
    chord(&mut k, "dom7", &[0, 4, 7, 10]);
    chord(&mut k, "six", &[0, 4, 7, 9]);
    chord(&mut k, "min9", &[0, 3, 7, 10, 14]);
    chord(&mut k, "sus", &[0, 5, 7, 10]);
    chord(&mut k, "maj9", &[0, 4, 7, 11, 14]);

    // the pulse table
    k.pulse("lead", &[(0x86, 0x00), (0x7f, 0x06), (0x7f, 0xfa), jump(2)]);
    k.pulse(
        "chord",
        &[(0x88, 0x00), (0x60, 0x08), (0x60, 0xf8), jump(2)],
    );
    k.pulse("rim", &[(0x84, 0x00), jump(0)]);

    // the filter table: low-pass on voice 3
    k.filter(
        "dawn",
        &[
            (0x90, 0x54),
            (0x00, 0x08),
            (0x07, 0x00),
            (0x01, 0x01),
            jump(3),
        ],
    );
    k.filter(
        "breath",
        &[
            (0x90, 0x54),
            (0x00, 0x28),
            (0x30, 0x01),
            (0x50, 0x00),
            (0x30, 0xff),
            (0x50, 0x00),
            jump(3),
        ],
    );
    k.filter(
        "dusk",
        &[
            (0x90, 0x54),
            (0x00, 0x48),
            (0x0b, 0x00),
            (0x01, 0xff),
            jump(3),
        ],
    );

    // the speed table
    k.speed("swing", 0x09, 0x07);
    k.speed("vib", 0x06, 0x0c);
    k.speed("glide", 0x01, 0x80);

    k.instrument("kb", 0x09, 0xaa, "kb", "", "", ("", 0), 2, 0x09);
    k.instrument("bass", 0x08, 0xaa, "tri", "", "", ("", 0), 2, 0x09);
    k.instrument("rim", 0x02, 0x00, "rim", "rim", "", ("", 0), 2, 0x09);
    k.instrument("lead", 0x49, 0x8a, "lead", "lead", "", ("vib", 16), 2, 0x09);
    k.instrument(
        "flute",
        0x38,
        0x9a,
        "flute",
        "lead",
        "",
        ("vib", 12),
        2,
        0x09,
    );
    for kind in ["maj7", "min7", "dom7", "six", "min9", "sus", "maj9"] {
        k.instrument(kind, 0x35, 0x6a, kind, "chord", "", ("", 0), 2, 0x09);
    }

    k.alias("r", "C-4/rim");

    // voice 1: the bass and the rim, on D
    let groove = "D-2/kb . . D-2/bass r . A-2/bass . D-2/kb . D-2/bass . r . D-3/bass .";
    k.pattern("groove", 16, groove);
    k.pattern(
        "groove-in",
        16,
        &groove.replacen("D-2/kb", "D-2/kb@E=swing", 1),
    );
    k.pattern(
        "groove-turn",
        16,
        "D-2/kb . . D-2/bass r . A-2/bass . D-2/kb . r r r . C-3/bass .",
    );
    k.pattern("bass-final", 32, "D-2/kb .7 A-2/bass .7 D-2/bass .15");

    // voice 2: the melody
    k.pattern("pickup", 64, ".48 .10 A-4/lead . D-5 . E-5 .");
    k.pattern(
        "a1",
        64,
        "F#5/lead .5 E-5 . D-5 .3 B-4 .3 \
         | A-4 .5 C#5 . E-5 .7 \
         | D-5 .3 E-5 . F#5 . G-5 .5 F#5 . \
         | E-5 .11 C#5 . A-4 .",
    );
    k.pattern(
        "a2",
        64,
        "F#5 .5 E-5 . D-5 .3 A-5 .3 \
         | F#5 .5 E-5 . C#5 .3 E-5 .3 \
         | D-5 .7 C#5 . B-4 . A-4 . F#4 . \
         | A-4 .15",
    );
    k.pattern(
        "b1",
        64,
        "B-4/lead .3 D-5 .3 G-5 .7 \
         | F#5 .5 E-5 . C#5 .7 \
         | D-5 .3 F#5 .3 B-5@3=glide .@3=glide .6 \
         | A-5 .5 F#5 . E-5 .7",
    );
    k.pattern(
        "b2",
        64,
        "F#5 .7 D-5 .3 B-4 .3 \
         | D-5 .5 E-5 . F#5 .7 \
         | G-5 .5 F#5 . E-5 . D-5 . E-5 .3 \
         | D-5 .7 C#5 .3 A-4 . D-5 .",
    );
    k.pattern(
        "a1-flute",
        64,
        "F#5/flute .5 E-5 . D-5 .3 B-4 .3 \
         | A-4 .5 C#5 . E-5 .7 \
         | D-5 .3 E-5 . F#5 . G-5 .5 F#5 . \
         | E-5 .11 C#5 . A-4 .",
    );
    k.pattern(
        "a2-high",
        64,
        "B-5 .5 A-5 . F#5 .3 D-5 .3 \
         | C#6 .5 A-5 . E-5 .7 \
         | D-6 .7 C#6 . B-5 . A-5 . F#5 . \
         | E-5 .7 D-5 .3 E-5 .3",
    );
    k.pattern(
        "ending",
        64,
        "F#5/lead .7 E-5 .3 D-5 .3 | E-5 .11 F#5@3=glide .@3=glide .2 | .32",
    );

    // voice 3: the chords
    let a = [
        ("G-3", "maj7"),
        ("F#3", "min7"),
        ("E-3", "min7"),
        ("A-3", "dom7"),
        ("G-3", "maj7"),
        ("F#3", "min7"),
        ("B-3", "min7"),
        ("A-3", "sus"),
    ];
    let b = [
        ("E-3", "min7"),
        ("F#3", "min7"),
        ("G-3", "maj7"),
        ("A-3", "six"),
        ("B-3", "min7"),
        ("G-3", "maj7"),
        ("E-3", "min9"),
        ("A-3", "sus"),
    ];
    let bars = |list: &[(&str, &str)]| {
        list.iter()
            .map(|(r, c)| chords(r, c))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let intro = bars(&a[..4]).replacen("G-3/maj7", "G-3/maj7@A=dawn", 1);
    k.pattern("c-intro", 64, &intro);
    let main_a = bars(&a).replacen("G-3/maj7", "G-3/maj7@A=breath", 1);
    k.pattern("c-a", 128, &main_a);
    let main_b = bars(&b[..7]) + " A-3/sus .7 A-3/dom7 .7";
    k.pattern("c-b", 128, &main_b);
    k.pattern(
        "c-end",
        64,
        "G-3/maj7@A=dusk .9 G-3/maj7 .5 A-3/six .9 A-3/six .5 D-3/maj9 .31",
    );

    // intro 4 | A 8 | B 8 | A' 8 | ending 4
    let bass_a = "+5 groove +4 groove +2 groove +7 groove +5 groove +4 groove +9 groove +7 groove";
    let bass_b =
        "+2 groove +4 groove +5 groove +7 groove +9 groove +5 groove +2 groove +7 groove-turn";
    k.order(
        0,
        &format!(
            "+5 groove-in +4 groove +2 groove +7 groove | {bass_a} {bass_b} {bass_a} \
             +5 groove +7 groove +0 bass-final"
        ),
    );
    k.order(1, "pickup | a1 a2 b1 b2 a1-flute a2-high ending");
    k.order(2, "c-intro | c-a c-b c-a c-end");
    k
}
