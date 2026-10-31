// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

pub const SONG: Showcase = Showcase {
    name: "neon-highway",
    title: "NEON HIGHWAY",
    model: SID_MODEL_8580,
    kit,
};

const CHORDS: [(&str, &str); 8] = [
    ("E-3", "min"),
    ("C-3", "maj"),
    ("G-3", "maj"),
    ("D-3", "maj"),
    ("E-3", "min"),
    ("C-3", "maj"),
    ("A-3", "min"),
    ("B-2", "maj"),
];

pub fn kit() -> Kit {
    let mut k = Kit::new("NEON HIGHWAY");

    k.wave("saw", &[(0x21, 0x00), jump(0)]);
    k.wave("pulse", &[(0x41, 0x00), jump(0)]);
    k.wave("lead", &[(0x21, 0x0c), (0x41, 0x00), jump(0)]);
    k.wave(
        "kick",
        &[
            (0x81, abs(68)),
            (0x41, abs(36)),
            (0x41, abs(30)),
            (0x41, abs(25)),
            (0x41, abs(21)),
            (0x40, abs(18)),
            jump(0),
        ],
    );
    k.wave(
        "snare",
        &[
            (0x81, abs(80)),
            (0x41, abs(43)),
            (0x41, abs(37)),
            (0x81, abs(78)),
            (0x81, abs(74)),
            jump(0),
        ],
    );
    k.wave("hat", &[(0x81, abs(95)), jump(0)]);
    k.wave("crash", &[(0x81, abs(84)), (0x81, abs(76)), jump(0)]);
    k.wave(
        "maj",
        &[
            (0x41, 0x00),
            (0x00, 0x04),
            (0x00, 0x07),
            (0x00, 0x00),
            jump(2),
        ],
    );
    k.wave(
        "min",
        &[
            (0x41, 0x00),
            (0x00, 0x03),
            (0x00, 0x07),
            (0x00, 0x00),
            jump(2),
        ],
    );
    k.wave(
        "maj7",
        &[
            (0x41, 0x00),
            (0x00, 0x04),
            (0x00, 0x07),
            (0x00, 0x0b),
            (0x00, 0x00),
            jump(2),
        ],
    );
    k.wave(
        "min7",
        &[
            (0x41, 0x00),
            (0x00, 0x03),
            (0x00, 0x07),
            (0x00, 0x0a),
            (0x00, 0x00),
            jump(2),
        ],
    );
    k.wave(
        "add9",
        &[
            (0x41, 0x00),
            (0x00, 0x03),
            (0x00, 0x07),
            (0x00, 0x0e),
            (0x00, 0x00),
            jump(2),
        ],
    );

    k.pulse("pwm", &[(0x83, 0x00), (0x60, 0x10), (0x60, 0xf0), jump(2)]);
    k.pulse(
        "pluck",
        &[(0x82, 0x00), (0x30, 0x18), (0x30, 0xe8), jump(2)],
    );
    k.pulse("mid", &[(0x88, 0x00), jump(0)]);
    k.pulse(
        "chord",
        &[(0x84, 0x00), (0x40, 0x10), (0x40, 0xf0), jump(2)],
    );

    k.filter(
        "intro",
        &[
            (0x90, 0xc1),
            (0x00, 0x10),
            (0x01, 0x01),
            (0x02, 0x00),
            jump(3),
        ],
    );
    k.filter(
        "wah",
        &[
            (0x90, 0xc1),
            (0x00, 0x28),
            (0x60, 0x01),
            (0x60, 0xff),
            jump(3),
        ],
    );
    k.filter(
        "close",
        &[
            (0x90, 0xe1),
            (0x00, 0x78),
            (0x03, 0x00),
            (0x01, 0xff),
            jump(3),
        ],
    );
    k.filter("open", &[(0x90, 0x81), (0x00, 0xa0), jump(0)]);
    k.filter(
        "fade",
        &[
            (0x90, 0xc1),
            (0x00, 0x80),
            (0x02, 0x00),
            (0x01, 0xff),
            jump(3),
        ],
    );

    k.speed("vib", 0x05, 0x20);
    k.speed("slide", 0x01, 0x80);
    k.speed("wide", 0x04, 0x30);

    k.instrument("bass", 0x08, 0xc9, "saw", "", "", ("", 0), 2, 0x09);
    k.instrument("arp", 0x05, 0x60, "pulse", "pluck", "", ("", 0), 2, 0x09);
    k.instrument("lead", 0x29, 0xa8, "lead", "pwm", "", ("vib", 10), 2, 0x09);
    k.instrument(
        "bright",
        0x18,
        0xb8,
        "lead",
        "pluck",
        "",
        ("wide", 8),
        2,
        0x09,
    );
    k.instrument("pad", 0x88, 0xab, "pulse", "pwm", "", ("vib", 16), 2, 0x09);
    k.instrument("kick", 0x08, 0x00, "kick", "mid", "", ("", 0), 2, 0x09);
    k.instrument("snare", 0x09, 0x00, "snare", "mid", "", ("", 0), 2, 0x09);
    k.instrument("hat", 0x02, 0x00, "hat", "", "", ("", 0), 2, 0x09);
    k.instrument("ohat", 0x07, 0x00, "hat", "", "", ("", 0), 2, 0x09);
    k.instrument("crash", 0x0b, 0x00, "crash", "", "", ("", 0), 2, 0x09);
    for kind in ["maj", "min", "maj7", "min7", "add9"] {
        k.instrument(kind, 0x04, 0x86, kind, "chord", "", ("", 0), 2, 0x09);
    }

    k.alias("k", "C-4/kick");
    k.alias("s", "C-4/snare");
    k.alias("h", "C-4/hat");
    k.alias("o", "C-4/ohat");
    k.alias("x", "C-4/crash");

    let bounce = "E-1 . E-2 . E-1 . E-2 . E-1 . E-2 . E-1 . E-2 .";
    k.pattern(
        "bass-in",
        16,
        "E-1/bass@A=intro . E-2 . E-1 . E-2 . E-1 . E-2 . E-1 . E-2 .",
    );
    k.pattern("bass", 16, &format!("E-1/bass {}", &bounce[4..]));
    k.pattern(
        "bass-wah",
        16,
        "E-1/bass@A=wah . E-2 . E-1 . E-2 . E-1 . E-2 . E-1 . E-2 .",
    );
    k.pattern(
        "bass-push",
        16,
        "E-1/bass . E-2 . E-1 . E-2 . E-1 . E-2 . E-1 E-1 E-2 E-1",
    );
    k.pattern("bass-close", 16, "C-2/bass@A=close .15");
    k.pattern("bass-long", 16, "D-2/bass .15");
    k.pattern("bass-bm", 16, "B-1/bass .15");
    k.pattern("bass-em", 16, "E-2/bass .7 E-1 . E-2 . E-1 E-2 E-1 E-2");
    k.pattern(
        "bass-open",
        16,
        "E-1/bass@A=open . E-2 . E-1 . E-2 . E-1 . E-2 . E-1 . E-2 .",
    );
    k.pattern("bass-end", 32, "E-1/bass@A=fade .31");

    k.pattern(
        "riff-min",
        16,
        "E-4/arp B-4 E-5 B-4 | G-5 E-5 B-4 E-5 | E-4 B-4 E-5 B-4 | G-5 B-5 G-5 E-5",
    );
    k.pattern(
        "riff-maj",
        16,
        "C-4/arp G-4 C-5 G-4 | E-5 C-5 G-4 C-5 | C-4 G-4 C-5 G-4 | E-5 G-5 E-5 C-5",
    );
    k.pattern(
        "hook-a",
        64,
        "B-4/lead .5 D-5@3=slide . E-5 .7 \
         | G-5 .3 F#5 . E-5 .5 D-5 . E-5 . \
         | D-5 .5 B-4 . D-5 .3 G-5 .3 \
         | F#5 .9 E-5 . D-5 . A-4 .",
    );
    k.pattern(
        "hook-a2",
        64,
        "B-4/bright .5 D-5@3=slide . E-5 .7 \
         | G-5 .3 F#5 . E-5 .5 D-5 . E-5 . \
         | D-5 .5 B-4 . D-5 .3 G-5 .3 \
         | F#5 .9 E-5 . D-5 . A-4 .",
    );
    k.pattern(
        "hook-b",
        64,
        "B-4 .5 D-5@3=slide . E-5 .5 G-5 . \
         | A-5 .5 G-5 . E-5 .7 \
         | C-6 .3 B-5 . A-5 . G-5 .3 E-5 .3 \
         | D#5@3=slide .7 F#5 .3 B-5 .3",
    );
    k.pattern(
        "hook-b2",
        64,
        "B-4 .5 D-5@3=slide . E-5 .5 G-5 . \
         | A-5 .5 G-5 . E-5 .7 \
         | C-6 .3 B-5 . A-5 . G-5 .3 E-5 .3 \
         | D#5@3=slide .3 F#5 .3 A-5 .3 B-5 .3",
    );
    k.pattern(
        "counter",
        64,
        "G-4/pad .7 B-4 .7 | A-4 .7 F#4 .7 | D-5 .7 A-4 .7 | B-4 .15",
    );
    k.pattern("lead-end", 32, "E-5/lead .31");

    k.pattern("silent", 16, ".16");
    k.pattern("hats", 16, "h . h . h . h . h . o . h . h h");
    k.pattern("roll", 16, "s . s . s . s . s s s s s s s s");
    k.pattern("beat", 16, "k h h h s h h h k h k h s h h o");
    k.pattern("beat-x", 16, "x h h h s h h h k h k h s h h o");
    k.pattern("beat-fill", 16, "k h h h s h h h k h k h s s s s");
    for (i, (root, kind)) in CHORDS.iter().enumerate() {
        let c = format!("{root}/{kind}");
        let text = format!("k {c} . . s {c} . . k {c} k {c} s {c} . .");
        k.pattern(&format!("chords{i}"), 16, &text);
    }
    k.pattern(
        "chords-x",
        16,
        "x E-3/min . . s E-3/min . . k E-3/min k E-3/min s E-3/min . .",
    );
    k.pattern(
        "chords-fill",
        16,
        "k B-2/maj . . s B-2/maj . . k B-2/maj k s s s s s",
    );
    k.pattern("pad0", 16, "C-3/maj7 .15");
    k.pattern("pad1", 16, "D-3/maj .15");
    k.pattern("pad2", 16, "B-2/min7 .15");
    k.pattern("pad3", 16, "E-3/add9 . . . s . s . s s s s s s s s");
    k.pattern("end", 32, "x .31");

    let a = "bass-wah -4 bass +3 bass -2 bass +0 bass -4 bass +5 bass +7 bass-push";
    let b = "+0 bass-wah -4 bass +3 bass -2 bass +0 bass -4 bass +5 bass +7 bass-push";
    k.order(
        0,
        &format!(
            "bass-in x2 bass bass-push | {a} {b} +0 bass-close bass-long bass-bm bass-em \
             +0 bass-open -4 bass +3 bass -2 bass +0 bass -4 bass +5 bass +7 bass-push \
             +0 bass-wah -4 bass +3 bass -2 bass +0 bass-end"
        ),
    );
    let riff = "+0 riff-min riff-maj -5 riff-maj +2 riff-maj";
    k.order(
        1,
        &format!(
            "{riff} | {riff} +0 riff-min riff-maj -7 riff-min -1 riff-maj \
             +0 hook-a hook-b counter hook-a2 hook-b2 {riff} +0 lead-end"
        ),
    );
    k.order(
        2,
        "x2 silent hats roll | beat-x beat beat beat-fill beat beat beat beat-fill \
         chords-x chords1 chords2 chords3 chords4 chords5 chords6 chords-fill \
         pad0 pad1 pad2 pad3 \
         chords-x chords1 chords2 chords3 chords4 chords5 chords6 chords-fill \
         beat-x beat beat beat-fill end",
    );
    k
}
