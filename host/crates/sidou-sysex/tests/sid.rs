// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! `sidou-sysex sid` (M3 Task 10): the checks (the device's own parser), the store names, the
//! frames, and the CLI on files in a scratch directory. Files are built here; none is a tune
//! from anywhere.

use std::collections::HashSet;
use std::process::Command;

use sidou_sysex::sid::{self, Refusal};
use sidou_sysex::{crc32, put_frames, Packet, DATA, END, FILE_SID, PUT};

/// A PSID v2 file: `title` in its name field, one song, `data` at $1000 (init $1000, play
/// $1003).
fn psid(title: &[u8], data: &[u8]) -> Vec<u8> {
    let mut b = b"PSID".to_vec();
    for w in [2u16, 0x7C, 0x1000, 0x1000, 0x1003, 1, 1] {
        b.extend(w.to_be_bytes());
    }
    b.extend(0u32.to_be_bytes());
    let mut meta = [0u8; 96];
    meta[..title.len()].copy_from_slice(title);
    meta[32..37].copy_from_slice(b"SIDOU");
    b.extend(meta);
    b.extend([0, 4, 0, 0, 0, 0]); // PAL
    b.extend(data);
    b
}

fn tune(title: &[u8]) -> Vec<u8> {
    psid(title, &[0x60; 600])
}

#[test]
fn the_devices_refusals_and_the_stores_size() {
    let good = tune(b"Fine");
    assert_eq!(sid::title(&sid::check(&good).unwrap()), "Fine");
    let mut rsid = good.clone();
    rsid[..4].copy_from_slice(b"RSID");
    let mut mus = good.clone();
    mus[0x77] |= 1;
    let mut past = good[..0x7C].to_vec();
    past[6..8].copy_from_slice(&0x200u16.to_be_bytes());
    let mut long = good.clone();
    long[8..10].copy_from_slice(&0xFF00u16.to_be_bytes());
    let mut port = good.clone();
    port[8..10].copy_from_slice(&0x0001u16.to_be_bytes());
    let big = psid(b"Big", &vec![0; 65_536 - 0x7C + 1]);
    for (file, why) in [
        (&rsid, Refusal::Rsid),
        (&mus, Refusal::Mus),
        (&past, Refusal::Header),
        (&good[..0x20].to_vec(), Refusal::NotSid),
        (&long, Refusal::TooLong),
        (&port, Refusal::Load),
        (&big, Refusal::TooBig),
    ] {
        assert_eq!(sid::check(file).unwrap_err(), why);
    }
    assert!(Refusal::Rsid.to_string().starts_with("RSID: NOT YET"));
    assert!(Refusal::Mus.to_string().starts_with("MUS: NOT SUPPORTED"));
    assert!(Refusal::Header.to_string().starts_with("BROKEN .SID"));
    assert!(Refusal::TooLong.to_string().starts_with("TOO LONG"));
}

#[test]
fn store_names_are_uppercase_in_sidous_characters_and_never_clash() {
    let mut taken = HashSet::new();
    let mut name = |title: &str, file: &str| sid::store_name(title, file, &mut taken);
    assert_eq!(name("Commando", "x"), "COMMANDO");
    assert_eq!(name("  a{b}~c  ", "x"), "A?B??C");
    assert_eq!(
        name("A very long title for a tune", "x"),
        "A VERY LONG TITL"
    );
    assert_eq!(
        name("A very long title for a tune", "x"),
        "A VERY LONG TI-2"
    );
    assert_eq!(name("a very long title FOR", "x"), "A VERY LONG TI-3");
    assert_eq!(name("Commando", "x"), "COMMANDO-2");
    assert_eq!(name("<?>", "my_tune"), "MY_TUNE");
    assert_eq!(name("", ""), "TUNE");
    assert_eq!(name("midi song", "x"), "MIDI SONG-2");
    assert_eq!(name("MIDI INSTRUMENT", "x"), "MIDI INSTRUMEN-2");
    for n in &taken {
        assert!(!n.is_empty() && n.len() <= 16 && !n.starts_with(' ') && !n.ends_with(' '));
        assert!(n.bytes().all(|b| (0x20..=0x5f).contains(&b)), "{n}");
    }
}

#[test]
fn a_tunes_frames_are_put_data_and_end_with_its_length_crc_and_type() {
    let file = tune(b"Frames");
    let frames = put_frames("FRAMES", FILE_SID, &file).unwrap();
    assert_eq!(frames.len(), file.len().div_ceil(256) + 2);
    let packets: Vec<Packet> = frames.iter().map(|f| Packet::decode(f).unwrap()).collect();
    assert_eq!(packets[0].command, PUT);
    let put = &packets[0].data;
    assert_eq!(&put[..2], &[0, 0]);
    assert_eq!(&put[2..8], b"FRAMES");
    assert!(put[8..18].iter().all(|&b| b == 0));
    assert_eq!(put[18], FILE_SID);
    assert_eq!(
        u32::from_le_bytes(put[19..23].try_into().unwrap()),
        file.len() as u32
    );
    assert_eq!(
        u32::from_le_bytes(put[23..27].try_into().unwrap()),
        crc32(&file)
    );
    let mut data: Vec<u8> = Vec::new();
    for (i, p) in packets.iter().enumerate() {
        assert_eq!(p.sequence, i as u8);
        if i > 0 && i + 1 < packets.len() {
            assert_eq!(p.command, DATA);
            assert!(p.data.len() == 256 || i + 2 == packets.len());
            data.extend(&p.data);
        }
    }
    assert_eq!(data, file);
    assert_eq!(packets.last().unwrap().command, END);
    assert_eq!(sid::syx("FRAMES", &file).unwrap(), frames.concat());
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    // The sequence wraps at 128 (a 64 KiB file is 258 messages).
    let big = vec![7u8; 65_536];
    let frames = put_frames("BIG", 1, &big).unwrap();
    assert_eq!(frames.len(), 258);
    assert_eq!(Packet::decode(&frames[200]).unwrap().sequence, 200 - 128);
    assert!(put_frames("BIG", 1, &[0; 65_537]).is_err());
}

#[test]
fn the_cli_writes_one_syx_a_tune_and_refuses_the_rest_by_name() {
    let dir = std::env::temp_dir().join(format!("sidou-sysex-cli-{}", std::process::id()));
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let a = tune(b"Same Title");
    let b = tune(b"same title");
    let mut rsid = tune(b"Real");
    rsid[..4].copy_from_slice(b"RSID");
    std::fs::write(dir.join("a.sid"), &a).unwrap();
    std::fs::write(dir.join("b.sid"), &b).unwrap();
    std::fs::write(dir.join("real.sid"), &rsid).unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_sidou-sysex"))
        .arg("sid")
        .args(["a.sid", "b.sid", "real.sid", "--out", "out"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success(), "a refusal fails the run");
    assert!(
        stderr.contains("real.sid: refused: RSID: NOT YET"),
        "{stderr}"
    );
    assert!(
        stdout.contains("as SAME TITLE:") && stdout.contains("as SAME TITLE-2:"),
        "{stdout}"
    );
    assert_eq!(
        std::fs::read(out.join("a.syx")).unwrap(),
        sid::syx("SAME TITLE", &a).unwrap()
    );
    assert_eq!(
        std::fs::read(out.join("b.syx")).unwrap(),
        sid::syx("SAME TITLE-2", &b).unwrap()
    );
    assert!(!out.join("real.syx").exists());
    let ok = Command::new(env!("CARGO_BIN_EXE_sidou-sysex"))
        .args(["sid", "a.sid", "--out", "out", "--trs"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(ok.status.success());
    assert!(String::from_utf8_lossy(&ok.stdout).contains("TRS: pause one second"));
    let usage = Command::new(env!("CARGO_BIN_EXE_sidou-sysex"))
        .output()
        .unwrap();
    assert_eq!(usage.status.code(), Some(2));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `--pack`: one `pack.syx` of the tunes' uploads end to end under one set of store names (a
/// repeated title gets `-2`), a refused tune named on stderr and left out, a directory standing
/// for its `.sid` files by name; and the starter pack (`assets/starter`) as the real case.
#[test]
fn pack_writes_one_syx_with_one_set_of_names() {
    let dir = std::env::temp_dir().join(format!("sidou-sysex-pack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let tunes = dir.join("tunes");
    let out = dir.join("out");
    std::fs::create_dir_all(&tunes).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let a = tune(b"Same Title");
    let b = tune(b"same title");
    let c = psid(b"Other", &[0x60; 300]);
    let mut rsid = tune(b"Real");
    rsid[..4].copy_from_slice(b"RSID");
    for (n, f) in [("a", &a), ("b", &b), ("c", &c), ("d", &rsid)] {
        std::fs::write(tunes.join(format!("{n}.sid")), f).unwrap();
    }
    std::fs::write(tunes.join("notes.txt"), b"not a tune").unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_sidou-sysex"))
        .args(["sid", "--pack", "--out"])
        .arg(&out)
        .arg(&tunes)
        .output()
        .unwrap();
    assert!(!run.status.success(), "a refusal fails the run");
    assert!(String::from_utf8_lossy(&run.stderr).contains("d.sid: refused: RSID: NOT YET"));
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
    let mut want = sid::syx("SAME TITLE", &a).unwrap();
    want.extend(sid::syx("SAME TITLE-2", &b).unwrap());
    want.extend(sid::syx("OTHER", &c).unwrap());
    assert_eq!(std::fs::read(out.join("pack.syx")).unwrap(), want);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pack_makes_the_starter_pack_one_syx() {
    let starter = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/starter");
    let dir = std::env::temp_dir().join(format!("sidou-sysex-starter-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_sidou-sysex"))
        .args(["sid", "--pack", "--out"])
        .arg(&dir)
        .arg(&starter)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let pack = std::fs::read(dir.join("pack.syx")).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let mut sids: Vec<_> = std::fs::read_dir(&starter)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "sid"))
        .collect();
    sids.sort();
    assert_eq!(sids.len(), 11);
    let mut taken = HashSet::new();
    let mut want = Vec::new();
    for p in &sids {
        let file = std::fs::read(p).unwrap();
        let h = sid::check(&file).unwrap();
        let stem = p.file_stem().unwrap().to_string_lossy();
        let name = sid::store_name(&sid::title(&h), &stem, &mut taken);
        want.extend(sid::syx(&name, &file).unwrap());
    }
    assert_eq!(taken.len(), 11, "eleven store names");
    assert_eq!(pack, want);
}
