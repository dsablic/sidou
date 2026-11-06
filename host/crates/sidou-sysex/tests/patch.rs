// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::process::Command;

use sidou_patch::json;
use sidou_patch::sounds::sidou_sounds;
use sidou_sys::{SID_MODEL_6581, SID_MODEL_8580};
use sidou_sysex::import::Model;
use sidou_sysex::patch::patch_file;
use sidou_sysex::{Packet, FILE_SONG, PUT};

#[test]
fn a_patch_file_is_the_site_s_song_and_its_put() {
    for s in sidou_sounds() {
        let text = json::write(&s.bytes, s.name);
        for (model, sid) in [
            (Model::Mos6581, SID_MODEL_6581),
            (Model::Csg8580, SID_MODEL_8580),
        ] {
            let f = patch_file(&text, model).unwrap();
            assert_eq!(
                f.song.sng,
                json::song(&s.bytes, s.name, sid).unwrap(),
                "{}",
                s.name
            );
            let first = f.song.syx.split_inclusive(|&b| b == 0xf7).next().unwrap();
            let put = Packet::decode(first).unwrap();
            assert_eq!((put.command, put.data[18]), (PUT, FILE_SONG));
            assert!(put.data[2..18].starts_with(s.name.as_bytes()), "{}", s.name);
        }
    }
}

#[test]
fn a_refused_patch_says_why_and_a_quiet_one_warns() {
    let bad = patch_file(
        r#"{"format": "sidou-patch/1", "filter": {"resonance": 20}}"#,
        Model::Mos6581,
    );
    assert_eq!(
        bad.err().as_deref(),
        Some("filter.resonance: 20 is too high. Allowed: 0 to 15.")
    );
    let quiet = r#"{"format": "sidou-patch/1", "wave": "saw+triangle", "envelope": {"sustain": 15}, "filter": {"voices": []}}"#;
    let f = patch_file(quiet, Model::Mos6581).unwrap();
    assert!(f
        .quiet
        .unwrap()
        .starts_with("the patch is silent on the 6581"));
}

#[test]
fn the_cli_writes_the_song_and_its_syx_and_refuses_with_the_field() {
    let dir = std::env::temp_dir().join(format!("sidou-patch-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("acid.json"),
        r#"{"format": "sidou-patch/1", "name": "acid", "filter": {"envelope": {"depth": 30}}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("bad.json"),
        r#"{"format": "sidou-patch/1", "wave": "sawtooth"}"#,
    )
    .unwrap();
    let run = |file: &str| {
        Command::new(env!("CARGO_BIN_EXE_sidou-sysex"))
            .args(["patch", file, "--out", "out"])
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    let ok = run("acid.json");
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    for file in ["ACID.sng", "ACID.syx"] {
        assert!(dir.join("out").join(file).is_file(), "{file}");
    }
    let bad = run("bad.json");
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr)
        .contains("wave: \"sawtooth\" is not allowed. Allowed: triangle, saw"));
    std::fs::remove_dir_all(&dir).unwrap();
}
