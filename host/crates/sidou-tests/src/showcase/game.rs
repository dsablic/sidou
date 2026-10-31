// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

pub const SONG: Showcase = Showcase {
    name: "chrome-comet",
    title: "CHROME COMET",
    model: SID_MODEL_6581,
    kit,
};

const A: [(&str, &str); 8] = [
    ("D-4", "min"),
    ("D-4", "min"),
    ("Bb3", "maj"),
    ("C-4", "maj"),
    ("D-4", "min"),
    ("D-4", "min"),
    ("G-3", "min"),
    ("A-3", "maj"),
];
const B: [(&str, &str); 8] = [
    ("Bb3", "maj"),
    ("C-4", "maj"),
    ("A-3", "min"),
    ("D-4", "min"),
    ("G-3", "min"),
    ("C-4", "maj"),
    ("F-3", "maj"),
    ("A-3", "maj"),
];
const SOLO: [(&str, &str); 4] = [
    ("D-4", "min"),
    ("C-4", "maj"),
    ("Bb3", "maj"),
    ("A-3", "maj"),
];

fn drums_with(chord: &str) -> String {
    format!("k {chord} . . s {chord} . . k {chord} k {chord} s {chord} h {chord}")
}

pub fn kit() -> Kit {
    let mut k = Kit::new("CHROME COMET");

    k.wave("bass", &[(0x41, 0x00), jump(0)]);
    k.wave("lead", &[(0x41, 0x0c), (0x41, 0x00), jump(0)]);
    k.wave("run", &[(0x41, 0x00), jump(0)]);
    k.wave("sync", &[(0x43, 0x00), jump(0)]);
    k.wave(
        "kick",
        &[
            (0x81, abs(70)),
            (0x41, abs(34)),
            (0x41, abs(28)),
            (0x41, abs(23)),
            (0x11, abs(19)),
            (0x10, abs(16)),
            jump(0),
        ],
    );
    k.wave(
        "snare",
        &[
            (0x81, abs(82)),
            (0x41, abs(45)),
            (0x81, abs(80)),
            (0x41, abs(40)),
            (0x81, abs(76)),
            (0x81, abs(72)),
            jump(0),
        ],
    );
    k.wave("hat", &[(0x81, abs(95)), (0x15, abs(79)), jump(0)]);
    k.wave(
        "tom",
        &[
            (0x11, 0x00),
            (0x11, down(2)),
            (0x11, down(4)),
            (0x11, down(6)),
            (0x11, down(8)),
            (0x11, down(10)),
            (0x10, down(12)),
            jump(0),
        ],
    );
    k.wave("crash", &[(0x81, abs(86)), (0x81, abs(80)), jump(0)]);
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

    k.pulse("bass", &[(0x84, 0x00), (0x20, 0x40), (0x20, 0xc0), jump(2)]);
    k.pulse("lead", &[(0x82, 0x00), (0x20, 0x30), (0x20, 0xd0), jump(2)]);
    k.pulse("mid", &[(0x88, 0x00), jump(0)]);
    k.pulse(
        "chord",
        &[(0x83, 0x00), (0x30, 0x20), (0x30, 0xe0), jump(2)],
    );

    k.filter("warm", &[(0x90, 0x61), (0x00, 0x60), jump(0)]);
    k.filter(
        "sweep",
        &[
            (0x90, 0xf1),
            (0x00, 0x20),
            (0x50, 0x01),
            (0x50, 0xff),
            jump(3),
        ],
    );

    k.speed("vib", 0x03, 0x60);
    k.speed("slide", 0x03, 0x00);
    k.speed("rise", 0x00, 0x50);
    k.speed("zap", 0x0c, 0x00);
    k.speed("wide", 0x02, 0xa0);

    k.instrument("bass", 0x05, 0xa8, "bass", "bass", "", ("", 0), 2, 0x09);
    k.instrument("lead", 0x06, 0xb9, "lead", "lead", "", ("vib", 8), 2, 0x09);
    k.instrument("run", 0x03, 0x88, "run", "lead", "", ("", 0), 2, 0x09);
    k.instrument("sync", 0x02, 0xb8, "sync", "lead", "", ("", 0), 2, 0x09);
    k.instrument("kick", 0x08, 0x00, "kick", "mid", "", ("", 0), 2, 0x09);
    k.instrument("snare", 0x08, 0x00, "snare", "mid", "", ("", 0), 2, 0x09);
    k.instrument("hat", 0x03, 0x00, "hat", "", "", ("", 0), 2, 0x09);
    k.instrument("tom", 0x08, 0x00, "tom", "mid", "", ("", 0), 2, 0x09);
    k.instrument("crash", 0x0b, 0x00, "crash", "", "", ("", 0), 2, 0x09);
    k.instrument("maj", 0x03, 0x96, "maj", "chord", "", ("", 0), 2, 0x09);
    k.instrument("min", 0x03, 0x96, "min", "chord", "", ("", 0), 2, 0x09);
    k.instrument("wide", 0x06, 0xb9, "lead", "lead", "", ("wide", 4), 2, 0x09);

    k.alias("k", "C-4/kick");
    k.alias("s", "C-4/snare");
    k.alias("h", "C-4/hat");
    k.alias("x", "C-4/crash");
    k.alias("t1", "G-4/tom");
    k.alias("t2", "D-4/tom");
    k.alias("t3", "A-3/tom");

    let minor = "D-2 . D-3 D-2 . D-2 D-3 . D-2 . D-3 D-2 C-3 D-3 A-2 C-3";
    let major = "C-2 . C-3 C-2 . C-2 C-3 . C-2 . C-3 C-2 A-2 C-3 G-2 A-2";
    k.pattern("bm-in", 16, &format!("D-2/bass@A=warm {}", &minor[4..]));
    k.pattern("bm", 16, &format!("D-2/bass {}", &minor[4..]));
    k.pattern("bM", 16, &format!("C-2/bass {}", &major[4..]));
    k.pattern("bm-sweep", 16, &format!("D-2/bass@A=sweep {}", &minor[4..]));
    k.pattern("bm-warm", 16, &format!("D-2/bass@A=warm {}", &minor[4..]));
    k.pattern("b-end", 32, "D-2/bass .31");

    let r = |n: usize| ".@1=rise ".repeat(n);
    k.pattern("quiet-in", 16, ".@F05 .15");
    k.pattern("quiet", 16, ".16");
    k.pattern(
        "rise",
        32,
        &format!("D-3/sync@1=rise {}", r(27) + "A-4/run E-5 A-5 C#6"),
    );
    k.pattern(
        "theme-a",
        64,
        "A-4/lead . D-5 . F-5 . A-5 .3 G-5 . F-5 . E-5 . \
         | D-5 .5 A-4 . D-5 . E-5 . F-5 . G-5 . \
         | F-5 .3 D-5 . F-5 . Bb5 .5 A-5 . \
         | G-5 .3 E-5 . C-5 . E-5 . G-5 . C-6 .3",
    );
    k.pattern(
        "theme-b",
        64,
        "D-6 .5 C-6 . A-5 . F-5 . A-5 .3 \
         | D-6 . C-6 . A-5 . F-5 . D-5 . F-5 . A-5 . C-6 . \
         | Bb5 .3 A-5 . G-5 . D-5 .3 G-5 .3 \
         | E-5 .3 A-5 . C#6@3=slide .@3=slide .4 E-6 .3",
    );
    k.pattern(
        "theme-b2",
        64,
        "A-5 . D-6 . A-5 . F-5 . D-5 . F-5 . A-5 . D-6 . \
         | C#6 . D-6 . E-6 . F-6 .5 E-6 . D-6 . \
         | D-6 .3 Bb5 . G-5 . Bb5 . D-6 . G-6 .3 \
         | E-6/wide .7 C#6 .3 A-5 .3",
    );
    k.pattern(
        "song-a",
        64,
        "D-6/wide .11 C-6 . Bb5 . \
         | C-6 .7 G-5 .3 E-5 .3 \
         | E-5 .5 A-5 .5 C-6 .3 \
         | D-6@3=slide .@3=slide .10 F-6 .3",
    );
    k.pattern(
        "song-b",
        64,
        "G-6 .7 F-6 . E-6 . D-6 . Bb5 . \
         | C-6 .7 E-6@3=slide .@3=slide .6 \
         | F-6 .5 E-6 . F-6 . A-6 .5 \
         | E-6 .3 C#6 .3 A-5 .3 E-5 .3",
    );
    let z = ".@2=zap";
    k.pattern(
        "zaps",
        16,
        &format!("A-6/sync@2=zap {z} {z} E-6@2=zap {z} {z} A-6@2=zap {z} . . D-7@2=zap {z} A-6@2=zap {z} . ."),
    );
    k.pattern(
        "zaps2",
        16,
        &format!("A-6/sync@2=zap {z} {z} E-6@2=zap {z} {z} A-6@2=zap {z} C-7@2=zap {z} D-7@2=zap {z} E-7@2=zap {z} G-7@2=zap {z}"),
    );
    let run = |root: &str, notes: &str| format!("{root}/run {notes}");
    k.pattern(
        "run-min",
        16,
        &run(
            "D-5",
            "F-5 A-5 D-6 F-6 D-6 A-5 F-5 D-5 F-5 A-5 D-6 A-5 F-5 D-5 A-4",
        ),
    );
    k.pattern(
        "run-maj",
        16,
        &run(
            "C-5",
            "E-5 G-5 C-6 E-6 C-6 G-5 E-5 C-5 E-5 G-5 C-6 G-5 E-5 C-5 G-4",
        ),
    );
    k.pattern(
        "run-min2",
        16,
        &run(
            "D-5",
            "F-5 A-5 D-6 . D-6 C-6 A-5 F-5 A-5 D-6 F-6 . F-6 E-6 D-6",
        ),
    );
    k.pattern(
        "run-maj2",
        16,
        &run(
            "C-5",
            "E-5 G-5 C-6 . C-6 B-5 G-5 E-5 G-5 C-6 E-6 . E-6 D-6 C-6",
        ),
    );
    k.pattern("lead-end", 32, "D-6/wide .31");

    k.pattern("intro1", 16, "k . . . s . . . k . k . s . . .");
    k.pattern("intro2", 16, "k . h . s . h . k . k h s t1 t2 t3");
    k.pattern("intro3", 16, "k . h . s . h . k . k h s . h .");
    k.pattern("intro4", 16, "k . h . s . h . k s k s t1 t1 t2 t3");
    for (name, chords) in [("a", &A[..]), ("b", &B[..]), ("s", &SOLO[..])] {
        for (i, (root, kind)) in chords.iter().enumerate() {
            let text = drums_with(&format!("{root}/{kind}"));
            k.pattern(&format!("{name}{i}"), 16, &text);
        }
    }
    k.pattern(
        "a-x",
        16,
        "x D-4/min . . s D-4/min . . k D-4/min k D-4/min s D-4/min h D-4/min",
    );
    k.pattern(
        "a-fill",
        16,
        "k A-3/maj . . s A-3/maj . . k A-3/maj k s s t1 t2 t3",
    );
    k.pattern(
        "s-fill",
        16,
        "k A-3/maj . . s A-3/maj . . k A-3/maj k A-3/maj s s s s",
    );
    k.pattern("brk", 16, "k . h . s . h k . k h . s . h h");
    k.pattern("brk-fill", 16, "k . h . s . h k . k h . t1 t1 t2 t3");
    k.pattern("end", 32, "x D-4/min .30");

    let bass_a = "+0 bm bm -2 bM +0 bM +0 bm bm +5 bm +7 bm";
    let bass_b = "-2 bM +0 bM +7 bm +0 bm +5 bm +0 bM +5 bM +7 bm";
    let bass_s = "+0 bm bM -2 bM +7 bm";
    k.order(
        0,
        &format!(
            "+0 bm-in bm bm bm | {bass_a} {bass_a} {bass_b} \
             +0 bm-sweep bm bm bm +0 bm-warm bM -2 bM +7 bm {bass_s} {bass_a} +0 b-end"
        ),
    );
    let solo = "+0 run-min run-maj -2 run-maj +7 run-maj";
    let solo2 = "+0 run-min2 run-maj2 -2 run-maj2 +7 run-maj2";
    k.order(
        1,
        &format!(
            "quiet-in quiet rise | +0 theme-a theme-b theme-a theme-b2 song-a song-b \
             zaps zaps zaps zaps2 {solo} {solo2} +0 theme-a theme-b lead-end"
        ),
    );
    let a = "a-x a1 a2 a3 a4 a5 a6 a-fill";
    k.order(
        2,
        &format!(
            "intro1 intro2 intro3 intro4 | {a} {a} b0 b1 b2 b3 b4 b5 b6 b7 \
             brk brk brk brk-fill s0 s1 s2 s3 s0 s1 s2 s-fill {a} end"
        ),
    );
    k
}
