// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The starter pack (M3 Task 13, https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-19-m3-sid-files.md): `assets/starter/`'s
//! `.sid` tunes, each GoatTracker's own packer's PSID v2 of a `.sng` that is in the
//! repository, and one `.md` of metadata for each. The list is the user's (signed off
//! 2026-10-24); what is left out, and why, is in LICENSING.md.
//!
//! `check_or_write` is the generator and the check: `SIDOU_STARTER_UPDATE=1` writes the files,
//! otherwise it lists the stale ones.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sidou_sys::*;

use crate::psid_fixtures::{Psid, CLOCK_PAL};
use crate::song::{Song, TAG_CHPA};

/// The licences a starter tune may be under (the plan's allowlist); a `written permission`
/// needs a file in `docs/permissions/`.
pub const LICENCES: [&str; 6] = [
    "GPL-3.0-or-later",
    "GPL-2.0-or-later",
    "BSD-2-Clause",
    "CC0-1.0",
    "CC-BY-4.0",
    "CC-BY-SA-4.0",
];

/// One tune of the pack.
pub struct Tune {
    /// The file's name in `assets/starter`, without `.sid`.
    pub stem: &'static str,
    /// The `.sng` it is packed from, from the repository's root.
    pub source: String,
    pub licence: &'static str,
    /// The composer as the metadata and CREDITS.md name them.
    pub composer: &'static str,
    /// `-S` of gt2reloc: 1 is one call a frame (a VBI tune); more is CIA-timed (the packed
    /// init sets the timer, the header's speed bits say so).
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

/// The pack, in the order of its table in LICENSING.md.
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

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub fn dir() -> PathBuf {
    root().join("assets/starter")
}

/// A `.sng`'s three header strings: title, author, copyright (32 bytes each after `GTS5`).
pub fn sng_strings(sng: &[u8]) -> [String; 3] {
    let text = |i: usize| {
        let field = &sng[4 + 32 * i..4 + 32 * (i + 1)];
        let n = field.iter().position(|&b| b == 0).unwrap_or(32);
        field[..n].iter().map(|&b| b as char).collect::<String>()
    };
    [text(0), text(1), text(2)]
}

/// The chip a song's trailer names (`CHPA`): 6581 or 8580; a song without one is GoatTracker's
/// default, a 6581.
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
    std::fs::read(root().join(&t.source)).unwrap_or_else(|e| panic!("{}: {e}", t.source))
}

/// The tune's `.sid`: PSID v2, PAL, the song's chip, GoatTracker's packer at its default
/// options (and `-S` for a multispeed tune, as GoatTracker's own `.sid` writer makes it: the timer's
/// stub before the player, every song's speed bit set), the name, author and released fields from the
/// song's own header.
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
    // GoatTracker's own `.sid` writer (greloc.c) puts ten bytes before a multispeed tune's player:
    // LDX #lo / STX $DC04 / LDX #hi / STX $DC05, which set CIA 1's timer to a frame's cycles
    // over the multiple (PAL: $4CC7 / n); the tune loads ten bytes lower and inits there, and the
    // stub falls through into the player's init. The packed `.prg` has no such stub.
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

/// The tune's `.md`.
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
    // The generated file's own licence header; reuse must not read it as this file's.
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

/// The pack's files that are not as their sources make them (or, with `update`, that were
/// written): the names, relative to `assets/starter`.
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
