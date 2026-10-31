// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

pub const SONG: Showcase = Showcase {
    name: "iron-rite",
    title: "IRON RITE",
    model: SID_MODEL_6581,
    kit,
};

pub fn kit() -> Kit {
    let mut k = Kit::new("IRON RITE");

    k.wave("pulse", &[(0x41, 0x00), jump(0)]);
    k.wave("saw", &[(0x21, 0x00), jump(0)]);
    k.wave("chug", &[(0x81, 0x24), (0x41, 0x00), jump(0)]);
    k.wave("bell", &[(0x15, 0x00), jump(0)]);
    k.wave("sync", &[(0x23, 0x00), jump(0)]);
    k.wave("lead", &[(0x21, 0x0c), (0x41, 0x00), jump(0)]);
    k.wave(
        "kick",
        &[
            (0x81, abs(60)),
            (0x41, abs(31)),
            (0x41, abs(26)),
            (0x41, abs(22)),
            (0x41, abs(19)),
            (0x11, abs(16)),
            (0x10, abs(14)),
            jump(0),
        ],
    );
    k.wave(
        "snare",
        &[
            (0x81, abs(78)),
            (0x41, abs(40)),
            (0x41, abs(36)),
            (0x81, abs(74)),
            (0x81, abs(70)),
            (0x81, abs(66)),
            jump(0),
        ],
    );
    k.wave(
        "tom",
        &[
            (0x81, 0x18),
            (0x41, 0x00),
            (0x41, down(1)),
            (0x41, down(2)),
            (0x41, down(3)),
            (0x41, down(4)),
            (0x41, down(5)),
            (0x41, down(6)),
            (0x40, down(7)),
            jump(0),
        ],
    );
    k.wave("anvil", &[(0x81, abs(90)), (0x15, abs(76)), jump(0)]);
    k.wave("crash", &[(0x81, abs(84)), (0x81, abs(78)), jump(0)]);

    k.pulse("chug", &[(0x83, 0x00), (0x40, 0x08), (0x40, 0xf8), jump(2)]);
    k.pulse("lead", &[(0x84, 0x00), (0x50, 0x10), (0x50, 0xf0), jump(2)]);
    k.pulse("mid", &[(0x88, 0x00), jump(0)]);
    k.pulse(
        "drone",
        &[(0x81, 0x00), (0x7f, 0x04), (0x7f, 0xfc), jump(2)],
    );

    k.filter(
        "rite",
        &[
            (0x90, 0xf1),
            (0x00, 0x08),
            (0x01, 0x01),
            (0x03, 0x00),
            jump(3),
        ],
    );
    k.filter("bite", &[(0x90, 0xd1), (0x00, 0xa8), (0x06, 0xf4), jump(0)]);
    k.filter("duel", &[(0x90, 0xf1), (0x00, 0x20), jump(0)]);

    k.speed("vib", 0x05, 0x18);
    k.speed("wobble", 0x08, 0x60);
    k.speed("up", 0x00, 0xc0);
    k.speed("down", 0x00, 0xc0);
    k.speed("slide", 0x02, 0x00);

    k.instrument("drone", 0x8c, 0xcc, "pulse", "drone", "", ("", 0), 2, 0x09);
    k.instrument("chug", 0x06, 0xa9, "chug", "chug", "bite", ("", 0), 2, 0x09);
    k.instrument("grind", 0x08, 0xcb, "saw", "", "", ("", 0), 2, 0x09);
    k.instrument("bell", 0x0b, 0x00, "bell", "", "", ("", 0), 2, 0x09);
    k.instrument("growl", 0x48, 0x99, "sync", "", "", ("wobble", 1), 2, 0x09);
    k.instrument("lead", 0x2a, 0x8a, "lead", "lead", "", ("vib", 12), 2, 0x09);
    k.instrument("scream", 0x04, 0xc8, "sync", "", "", ("", 0), 2, 0x09);
    k.instrument("kick", 0x09, 0x00, "kick", "mid", "", ("", 0), 2, 0x09);
    k.instrument("snare", 0x0a, 0x00, "snare", "mid", "", ("", 0), 2, 0x09);
    k.instrument("tom", 0x0a, 0x00, "tom", "mid", "", ("", 0), 2, 0x09);
    k.instrument("anvil", 0x07, 0x00, "anvil", "", "", ("", 0), 2, 0x09);
    k.instrument("crash", 0x0c, 0x00, "crash", "", "", ("", 0), 2, 0x09);

    k.alias("k", "C-4/kick");
    k.alias("s", "C-4/snare");
    k.alias("a", "C-4/anvil");
    k.alias("x", "C-4/crash");
    k.alias("t1", "A-2/tom");
    k.alias("t2", "E-2/tom");
    k.alias("t3", "B-1/tom");

    k.pattern("drone", 64, "E-1/drone@A=rite .63");
    k.pattern(
        "r1",
        16,
        "E-1/chug . E-1 E-1 . E-1 E-1 . F-1 . E-1 . Bb1 . A-1 .",
    );
    k.pattern(
        "r2",
        16,
        "E-1/chug . E-1 E-1 . E-1 E-1 . G-1 . F-1 . E-2 D-2 C-2 B-1",
    );
    k.pattern(
        "rc",
        16,
        "C-2/chug . C-2 C-2 . C-2 C-2 . D-2 . C-2 . G-1 . F-1 .",
    );
    let cut = |vals: [u8; 15]| {
        vals.iter()
            .map(|v| format!(".@C{v:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let bite_a = cut([
        0x20, 0x90, 0x30, 0x20, 0x20, 0xb0, 0x40, 0x20, 0x90, 0x20, 0xb0, 0x30, 0x20, 0xe0, 0x60,
    ]);
    let bite_b = cut([
        0x20, 0xa0, 0x20, 0x20, 0xa0, 0x20, 0x20, 0xc0, 0x20, 0x90, 0x20, 0xe0, 0x80, 0x40, 0x20,
    ]);
    k.pattern("duel-in", 16, &format!("E-1/grind@A=duel {bite_a}"));
    k.pattern("duel-e", 16, &format!("E-1/grind@C20 {bite_b}"));
    k.pattern("duel-f", 16, &format!("F-1/grind@C20 {bite_a}"));
    k.pattern("duel-bb", 16, &format!("Bb1/grind@C20 {bite_b}"));
    k.pattern("duel-a", 16, &format!("A-1/grind@C20 {bite_a}"));
    k.pattern("end1", 32, "E-1/drone@A=rite .31");

    k.pattern("bells", 64, "E-5/bell .15 Bb4 .15 E-5 .15 Bb4 .7 F-5 .7");
    k.pattern("rest", 64, ".64");
    k.pattern("growl", 64, "B-3/growl .31 C-4 .15 D-4 .15");
    k.pattern(
        "theme1",
        64,
        "E-4/lead .7 F-4 .3 G-4 .3 \
         | A-4 .5 G-4 . F-4 .3 E-4 .3 \
         | B-4 .7 C-5 .3 B-4 . A-4 . \
         | G-4 .11 F-4 .3",
    );
    k.pattern(
        "theme2",
        64,
        "E-5 .7 D-5 . C-5 . B-4 .3 \
         | C-5 .5 B-4 . A-4 . Bb4@3=slide .@3=slide .4 \
         | G-4 .7 A-4 .3 B-4 .3 \
         | A-4 .3 C-5 .3 D-5 .7",
    );
    let up = ".@1=up";
    let down = ".@2=down";
    k.pattern(
        "scream",
        16,
        &format!(
            "B-3/scream@1=up {} G-5@2=down {}",
            [up; 7].join(" "),
            [down; 7].join(" ")
        ),
    );
    k.pattern(
        "scream2",
        16,
        &format!(
            "E-4/scream@1=up {} B-5@2=down {} ^ .",
            [up; 7].join(" "),
            [down; 5].join(" ")
        ),
    );
    k.pattern("silence", 16, ".16");
    k.pattern("end2", 32, "E-5/bell .15 Bb4 .15");

    k.pattern(
        "war",
        64,
        "t3@F07 .7 t3 .7 t3 .3 t3 .3 t2 . t2 . t1 t1 t2 t3 \
         | t3 .7 t3 .7 t3 .3 t3 .3 t2 t2 t1 t1 t1 t2 t3 t3",
    );
    k.pattern("chug1", 16, "k . k k . k k . s . . . k . a .");
    k.pattern("chug2", 16, "k . k k . k k . s . . . k k s s");
    k.pattern("rock", 16, "k . a . s . a . k k a . s . a .");
    k.pattern("rock-fill", 16, "k . a . s . a . k k a . s t1 t2 t3");
    k.pattern("duel1", 16, "k .7 s .7");
    k.pattern("duel2", 16, "k . a . a a . a s . a . t1 t1 t2 t3");
    k.pattern("build", 16, "k . s . k . s . k s k s s s s s");
    k.pattern("end3", 32, "x .31");

    let riff = "r1 r2 r1 r2 r1 r2 rc +2 rc +0";
    k.order(
        0,
        &format!(
            "drone | {riff} {riff} duel-in duel-e duel-f duel-e duel-e duel-e duel-bb duel-a \
             {riff} end1"
        ),
    );
    k.order(
        1,
        "bells | rest growl theme1 theme2 scream silence scream silence scream scream2 scream \
         scream2 +12 theme1 theme2 +0 end2",
    );
    k.order(
        2,
        "war | chug1 chug2 chug1 chug2 chug1 chug2 chug1 chug2 \
         rock rock rock rock-fill rock rock rock rock-fill \
         duel1 duel2 duel1 duel2 duel1 duel2 build build \
         chug1 chug2 chug1 chug2 rock rock rock rock-fill end3",
    );
    k
}
