// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::process::Command;

use sidou_patch::json;
use sidou_patch::sounds::sidou_sounds;
use sidou_sys::{SID_MODEL_6581, SID_MODEL_8580};
use sidou_sysex::import::Model;
use sidou_sysex::patch::{bank_file, patch_file};
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

#[test]
fn a_bank_file_is_the_sixteen_s_song_and_its_put() {
    let text = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/sounds/sidou-sounds-bank.json"),
    )
    .unwrap();
    let f = bank_file(&text, Model::Mos6581).unwrap();
    let site = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/sounds/sidou-sounds.sng"),
    )
    .unwrap();
    assert_eq!((f.song.name.as_str(), f.song.sng), ("SIDOU SOUNDS", site));
    let refused = bank_file(
        r#"{"format": "sidou-bank/1", "patches": [{"wave": "sawtooth"}]}"#,
        Model::Mos6581,
    );
    assert!(refused
        .err()
        .unwrap()
        .starts_with("patches[0].wave: \"sawtooth\" is not allowed"));
    let dir = std::env::temp_dir().join(format!("sidou-bank-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("kit.json"),
        r#"{"format": "sidou-bank/1", "name": "kit", "patches": [{}, {"wave": "noise"}]}"#,
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_sidou-sysex"))
        .args(["bank", "kit.json", "--out", "out"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(dir.join("out/KIT.sng").is_file() && dir.join("out/KIT.syx").is_file());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_cli_makes_a_step_song_and_send_refuses_what_is_not_a_store_file() {
    let dir = std::env::temp_dir().join(format!("sidou-step-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("beat.json"), r#"{"format": "sidou-step/1", "name": "beat", "instrument": "kit", "steps": [{"step": 1, "drum": "kick"}]}"#).unwrap();
    std::fs::write(dir.join("sidou.fwsc"), b"FWSC firmware").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sidou-sysex"))
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    let step = run(&["step", "beat.json", "--out", "out"]);
    assert!(
        step.status.success(),
        "{}",
        String::from_utf8_lossy(&step.stderr)
    );
    assert!(dir.join("out/BEAT.sng").is_file() && dir.join("out/BEAT.syx").is_file());
    let send = run(&["send", "sidou.fwsc", "--port", "NO SUCH PORT"]);
    assert!(!send.status.success());
    assert!(String::from_utf8_lossy(&send.stderr).contains("never a firmware package or update"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_cli_makes_a_song_from_text_and_says_the_bpm_it_plays() {
    let text = r#"{"format": "sidou-song/1", "name": "night", "bpm": 120,
      "sections": {"A": {"rows": 4, "voices": [{"instrument": "bass", "notes": "C2! - . G1"}]}},
      "order": ["A", "A"]}"#;
    let dir = std::env::temp_dir().join(format!("sidou-song-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("night.json"), text).unwrap();
    std::fs::write(dir.join("bad.json"), text.replace("G1", "H1")).unwrap();
    let run = |file: &str| {
        Command::new(env!("CARGO_BIN_EXE_sidou-sysex"))
            .args(["song", file, "--out", "out"])
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    let ok = run("night.json");
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert!(String::from_utf8_lossy(&ok.stdout).contains("asked 120, plays 115.7"));
    assert_eq!(
        std::fs::read(dir.join("out/NIGHT.sng")).unwrap(),
        json::song_song(text, SID_MODEL_6581).unwrap()
    );
    assert!(dir.join("out/NIGHT.syx").is_file());
    let bad = run("bad.json");
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr)
        .contains("sections.A.voices[0].notes: token 4 \"H1\" is not a note."));
    std::fs::remove_dir_all(&dir).unwrap();
}
