// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sidou_sys::*;

use crate::font::repo_root;
use crate::psid_fixtures::{Psid, CLOCK_PAL};
use crate::song::{Song, TAG_CHPA};

pub const LICENCES: [&str; 6] = [
    "GPL-3.0-or-later",
    "GPL-2.0-or-later",
    "BSD-2-Clause",
    "CC0-1.0",
    "CC-BY-4.0",
    "CC-BY-SA-4.0",
];

pub struct Tune {
    pub stem: &'static str,
    pub source: String,
    pub licence: &'static str,
    pub composer: &'static str,
    pub multiplier: u32,
}

const EX: &str = "third_party/goattracker2/examples";
const CADAVER: &str = "Cadaver (Lasse Öörni)";
const SIDOU: &str = "SIDOU contributors";

fn own(stem: &'static str) -> Tune {
    Tune {
        stem,
        source: format!("assets/songs/{stem}.sng"),
        licence: "GPL-3.0-or-later",
        composer: SIDOU,
        multiplier: 1,
    }
}

fn cadaver(stem: &'static str, multiplier: u32) -> Tune {
    Tune {
        stem,
        source: format!("{EX}/{stem}.sng"),
        licence: "GPL-2.0-or-later",
        composer: CADAVER,
        multiplier,
    }
}

pub fn tunes() -> Vec<Tune> {
    vec![
        own("chrome-comet"),
        own("iron-rite"),
        own("neon-highway"),
        own("paper-lanterns"),
        own("sidou-loop"),
        cadaver("sanction", 1),
        cadaver("consultant", 1),
        cadaver("dojo", 1),
        cadaver("transylvanian", 1),
        cadaver("unleash", 1),
        cadaver("2xtest", 2),
    ]
}

pub fn dir() -> PathBuf {
    repo_root().join("assets/starter")
}

pub fn sng_strings(sng: &[u8]) -> [String; 3] {
    let text = |i: usize| {
        let field = &sng[4 + 32 * i..4 + 32 * (i + 1)];
        let n = field.iter().position(|&b| b == 0).unwrap_or(32);
        field[..n].iter().map(|&b| b as char).collect::<String>()
    };
    [text(0), text(1), text(2)]
}

pub fn sng_model(sng: &[u8]) -> sid_model {
    let mut song = Song::device();
    if song.read(sng, 4).is_err() {
        return SID_MODEL_6581;
    }
    let mut chip = song_chip_setup::default();
    let found = unsafe { song_ext_get_chip(song.ptr(), TAG_CHPA, &mut chip) };
    if found == 1 && chip.model == 1 {
        SID_MODEL_8580
    } else {
        SID_MODEL_6581
    }
}

pub fn source_bytes(t: &Tune) -> Vec<u8> {
    std::fs::read(repo_root().join(&t.source)).unwrap_or_else(|e| panic!("{}: {e}", t.source))
}

pub fn sid_file(t: &Tune) -> Vec<u8> {
    let sng = source_bytes(t);
    let o = gt2_ref::Options {
        multiplier: t.multiplier,
        ..gt2_ref::Options::default()
    };
    let packed = gt2_ref::pack(&sng, &o).unwrap_or_else(|e| panic!("{}: {e}", t.source));
    let songs = gt2_ref::subtunes(&sng).max(1);
    let model: u16 = if sng_model(&sng) == SID_MODEL_8580 {
        2
    } else {
        1
    };
    let (load, init, code) = if t.multiplier == 1 {
        (packed.load, packed.init(), packed.code.clone())
    } else {
        let timer = 0x4CC7 / t.multiplier;
        let mut code = vec![
            0xA2,
            timer as u8,
            0x8E,
            0x04,
            0xDC,
            0xA2,
            (timer >> 8) as u8,
            0x8E,
            0x05,
            0xDC,
        ];
        code.extend_from_slice(&packed.code);
        (packed.load - 10, packed.load - 10, code)
    };
    let psid = Psid {
        version: 2,
        load,
        init,
        play: packed.play(),
        songs: songs as u16,
        start: 1,
        speed: if t.multiplier == 1 { 0 } else { u32::MAX },
        flags: CLOCK_PAL | (model << 4),
        sid2: 0,
        sid3: 0,
        data: code,
    };
    let mut b = psid.bytes();
    let [title, author, released] = sng_strings(&sng);
    for (at, s) in [(22, title), (54, author), (86, released)] {
        b[at..at + 32].fill(0);
        let n = s.len().min(31);
        b[at..at + n].copy_from_slice(&s.as_bytes()[..n]);
    }
    b
}

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

pub fn md(t: &Tune, sid: &[u8]) -> String {
    let sng = source_bytes(t);
    let [title, _, _] = sng_strings(&sng);
    let songs = gt2_ref::subtunes(&sng).max(1);
    let chip = if sng_model(&sng) == SID_MODEL_8580 {
        "8580"
    } else {
        "6581"
    };
    let speed = if t.multiplier == 1 {
        "VBI (one play call a frame)".to_string()
    } else {
        format!(
            "CIA 1 timer A, {}x speed (a stub before the player sets the timer, as GoatTracker's own `.sid` writer does)",
            t.multiplier
        )
    };
    let how = if t.source.starts_with(EX) {
        "GoatTracker 2.77's example song, packed"
    } else {
        "the repository's song, packed"
    };
    // REUSE-IgnoreStart
    format!(
        "<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->\n\
         <!-- SPDX-License-Identifier: GPL-3.0-or-later -->\n\
         # {title}\n\
         \n\
         | | |\n\
         |---|---|\n\
         | File | `{stem}.sid` |\n\
         | Title | {title} |\n\
         | Composer | {composer} |\n\
         | Licence | {licence} |\n\
         | Source | `{source}`: {how} by `gt2-ref` (GoatTracker 2.77's packer, default options, \
         PAL) into PSID v2; `SIDOU_STARTER_UPDATE=1 cargo test -p sidou-tests --test starter` \
         writes it |\n\
         | SHA-256 | `{sha}` |\n\
         | Chip | {chip} |\n\
         | Clock | PAL, {speed} |\n\
         | Songs | {songs} |\n\
         | In the firmware | no (the store) |\n\
         \n\
         The `.sid` is the song's own data with GoatTracker's playroutine, which carries its own \
         permissive notice (`LicenseRef-GoatTracker-Playroutine`); the `.sng` it is built from is \
         in the repository, as the GPL asks. [LICENSING.md](../../LICENSING.md#the-starter-pack) \
         has the pack's tiers.\n",
        stem = t.stem,
        composer = t.composer,
        licence = t.licence,
        source = t.source,
        sha = sha256(sid),
    )
    // REUSE-IgnoreEnd
}

pub fn check_or_write(update: bool) -> Vec<String> {
    let out = dir();
    if update {
        std::fs::create_dir_all(&out).unwrap();
    }
    let mut stale = Vec::new();
    for t in tunes() {
        let sid = sid_file(&t);
        for (name, want) in [
            (format!("{}.sid", t.stem), sid.clone()),
            (format!("{}.md", t.stem), md(&t, &sid).into_bytes()),
        ] {
            let path: &Path = &out.join(&name);
            if std::fs::read(path).ok().as_deref() == Some(&want) {
                continue;
            }
            if update {
                std::fs::write(path, &want).unwrap();
            }
            stale.push(name);
        }
    }
    if update {
        stale.clear();
    }
    stale
}
