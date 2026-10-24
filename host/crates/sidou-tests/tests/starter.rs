// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The starter pack (M3 Task 13, `assets/starter`): every tune has its `.md` with an allowlisted
//! licence and the file's SHA-256, parses, is not RSID, plays 10 s in the host render on its
//! chip with no overrun, no late write and no clipping; the files are in step with their
//! sources. (`sidou-sysex sid --pack` on the pack is sidou-sysex's tests; the pack loaded into an
//! emulated store over TRS is emu-gate's `psid_store`.)
//!
//! SIDOU_STARTER_UPDATE=1 rewrites the files from their sources.

use std::collections::{BTreeMap, BTreeSet};

use sidou_sys::*;
use sidou_tests::psid::{header, Player, BLOCK, SAMPLE_RATE};
use sidou_tests::starter::{self, sha256, Tune, LICENCES};

fn files(ext: &str) -> BTreeSet<String> {
    std::fs::read_dir(starter::dir())
        .expect("assets/starter")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(ext))
        .collect()
}

/// The `| Key | value |` rows of a tune's `.md`.
fn fields(md: &str) -> BTreeMap<String, String> {
    md.lines()
        .filter_map(|l| l.strip_prefix("| "))
        .filter_map(|l| l.strip_suffix(" |"))
        .filter_map(|l| l.split_once(" | "))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn read(name: &str) -> Vec<u8> {
    std::fs::read(starter::dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn the_files_are_up_to_date() {
    let update = std::env::var("SIDOU_STARTER_UPDATE").as_deref() == Ok("1");
    let stale = starter::check_or_write(update);
    assert!(
        stale.is_empty(),
        "{stale:?} is stale; rerun with SIDOU_STARTER_UPDATE=1"
    );
}

#[test]
fn the_directory_holds_the_signed_off_list_and_nothing_else() {
    let want: BTreeSet<String> = starter::tunes()
        .iter()
        .map(|t| format!("{}.sid", t.stem))
        .collect();
    assert_eq!(want.len(), 11);
    assert_eq!(files(".sid"), want);
    let mds: BTreeSet<String> = want.iter().map(|n| n.replace(".sid", ".md")).collect();
    assert_eq!(files(".md"), mds, "every tune has its .md");
    let all = std::fs::read_dir(starter::dir()).unwrap().count();
    assert_eq!(all, 22, "nothing else is in assets/starter");
}

#[test]
fn every_tune_has_its_metadata_an_allowlisted_licence_and_its_hash() {
    for name in files(".sid") {
        let stem = name.trim_end_matches(".sid");
        let md = String::from_utf8(read(&format!("{stem}.md"))).unwrap();
        let f = fields(&md);
        for key in [
            "File",
            "Title",
            "Composer",
            "Licence",
            "Source",
            "SHA-256",
            "Chip",
            "Clock",
            "Songs",
            "In the firmware",
        ] {
            assert!(
                f.get(key).is_some_and(|v| !v.is_empty()),
                "{stem}.md: no {key}"
            );
        }
        assert_eq!(f["In the firmware"], "no (the store)", "{stem}");
        let licence = &f["Licence"];
        assert!(
            LICENCES.contains(&licence.as_str()) || licence == "written permission",
            "{stem}: licence {licence} is not on the allowlist"
        );
        if licence == "written permission" {
            let perm = starter::root().join("docs/permissions");
            let has = std::fs::read_dir(&perm)
                .map(|d| {
                    d.filter_map(|e| e.ok())
                        .any(|e| e.file_name().to_string_lossy().starts_with(stem))
                })
                .unwrap_or(false);
            assert!(
                has,
                "{stem}: written permission needs a file in docs/permissions"
            );
        }
        assert!(
            !licence.to_lowercase().contains("non-commercial")
                && !licence.to_lowercase().contains("noncommercial"),
            "{stem}: non-commercial"
        );
        let hash = f["SHA-256"].trim_matches('`');
        assert_eq!(hash, sha256(&read(&name)), "{stem}: the SHA-256");
        assert_eq!(f["File"], format!("`{name}`"));
    }
}

#[test]
fn every_tune_parses_is_not_rsid_and_the_device_accepts_it() {
    for name in files(".sid") {
        let file = read(&name);
        assert_eq!(&file[..4], b"PSID", "{name}: not RSID");
        let h = header(&file);
        assert!(h.songs >= 1 && h.songs <= 32, "{name}");
        sidou_sysex::sid::check(&file).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(file.len() <= 65_536, "{name}");
        let md = String::from_utf8(read(&name.replace(".sid", ".md"))).unwrap();
        let f = fields(&md);
        assert_eq!(f["Songs"], h.songs.to_string(), "{name}");
        let chip = if h.model1 == 2 { "8580" } else { "6581" };
        assert_eq!(f["Chip"], chip, "{name}");
        assert_eq!(h.clock, 1, "{name}: PAL");
        assert!(f["Clock"].starts_with("PAL"), "{name}");
    }
}

/// Every song of every tune, 10 s in the host render on its chip: the 6502 in time (no play call
/// past its period), no write applied late, no sample at full scale, and not silent.
#[test]
fn every_tune_plays_10_s_on_its_chip_without_overrun_late_writes_or_clipping() {
    let seconds = 10;
    let names: Vec<String> = files(".sid").into_iter().collect();
    let results: Vec<Vec<String>> = std::thread::scope(|sc| {
        let handles: Vec<_> = names
            .iter()
            .map(|name| {
                sc.spawn(move || {
                    let file = read(name);
                    let h = header(&file);
                    let mut report = Vec::new();
                    for song in 1..=u32::from(h.songs) {
                        let mut p = Player::new(&file);
                        assert!(p.play(song), "{name} song {song}");
                        let (a, b) = p.run((seconds * SAMPLE_RATE) as usize / BLOCK, 8);
                        assert_eq!(p.late(), 0, "{name} song {song}: late writes");
                        assert_eq!(p.work.env.overruns, 0, "{name} song {song}: overruns");
                        assert_eq!(p.ctl.dropped, 0, "{name} song {song}");
                        assert!(
                            b.iter().all(|&x| x == 0),
                            "{name} song {song}: chip B sounds"
                        );
                        let peak = a.iter().map(|&x| i32::from(x).abs()).max().unwrap();
                        assert!(peak < 32_767, "{name} song {song}: clipped");
                        // the chip is the tune's: its model
                        assert_eq!(
                            p.chips[0].model,
                            if h.model1 == 2 {
                                SID_MODEL_8580
                            } else {
                                SID_MODEL_6581
                            },
                            "{name}"
                        );
                        if song == 1 {
                            assert!(peak > 500, "{name}: silent ({peak})");
                        }
                        report.push(format!(
                            "{name} song {song}: peak {:.1} dBFS, {} writes, {} play calls",
                            20.0 * (f64::from(peak) / 32768.0).log10(),
                            p.log.len(),
                            p.work.env.calls
                        ));
                    }
                    report
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for line in results.concat() {
        println!("{line}");
    }
}

#[test]
fn the_pack_lists_the_users_signed_off_tunes() {
    let tunes: Vec<Tune> = starter::tunes();
    let mut stems: Vec<&str> = tunes.iter().map(|t| t.stem).collect();
    stems.sort();
    assert_eq!(
        stems,
        [
            "2xtest",
            "chrome-comet",
            "consultant",
            "dojo",
            "iron-rite",
            "neon-highway",
            "paper-lanterns",
            "sanction",
            "sidou-loop",
            "transylvanian",
            "unleash"
        ]
    );
}
