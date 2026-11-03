// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::mbsid::{patch_dump, PATCH_BYTES};
use sidou_sysex::import::{bank_files, default_name, Model};
use sidou_sysex::{Packet, FILE_SONG, PUT};

const NAME_BYTES: usize = 10;
const DECODED_BYTES: usize = 146;
const PATCH_MODE: usize = 22;
const OSC1_WAVE: usize = 36 + 15;
const OSC1_SUSTAIN: usize = 36 + 8;
const SAW: u8 = 0x20;
const MB_ENGINE: usize = 0x10;
const MB_DRUM: u8 = 2;
const MB_VOICE1: usize = 0x60;
const MB_SAW: u8 = 0x02;
const MB_TRANSPOSE_ZERO: u8 = 0x40;
const MB_CENTRE: u8 = 0x80;

fn mbsid_dump(name: &str, engine: u8, number: u8) -> Vec<u8> {
    let mut patch = [0u8; PATCH_BYTES];
    patch[..name.len()].copy_from_slice(name.as_bytes());
    patch[MB_ENGINE] = engine;
    patch[MB_VOICE1 + 1] = MB_SAW;
    patch[MB_VOICE1 + 3] = 0xF0;
    patch[MB_VOICE1 + 8] = MB_TRANSPOSE_ZERO;
    patch[MB_VOICE1 + 9] = MB_CENTRE;
    patch_dump(&patch, 0, number)
}

fn dump(name: &str) -> Vec<u8> {
    let mut decoded = vec![b' '; NAME_BYTES];
    decoded[..name.len()].copy_from_slice(name.as_bytes());
    decoded.resize(DECODED_BYTES - 3, 0);
    decoded.extend([0xFF; 3]);
    decoded[PATCH_MODE] = 1;
    decoded[OSC1_WAVE] = SAW;
    decoded[OSC1_SUSTAIN] = 15;
    sidou_patch::patch_dump(&decoded, 0)
}

#[test]
fn a_bank_becomes_named_song_uploads_and_a_report() {
    let bank: Vec<u8> = (0..18).flat_map(|n| dump(&format!("Patch {n}"))).collect();
    let files = bank_files(&bank, Model::Mos6581, "SIDSTN").unwrap();
    let names: Vec<&str> = files.songs.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["SIDSTN 1", "SIDSTN 2"]);
    for song in &files.songs {
        let first =
            Packet::decode(&song.syx[..song.syx.iter().position(|&b| b == 0xF7).unwrap() + 1])
                .unwrap();
        assert_eq!(first.command, PUT);
        assert_eq!(
            first.data[2..2 + song.name.len()],
            *song.name.as_bytes(),
            "the store name"
        );
        assert_eq!(first.data[18], FILE_SONG);
        assert!(song.sng.starts_with(b"GTS5"));
    }
    assert!(
        files.report.contains("SIDSTN 1") && files.report.contains("I01 PATCH 0"),
        "{}",
        files.report
    );
}

#[test]
fn a_file_that_is_not_a_bank_is_refused_with_the_reason() {
    let err = bank_files(b"not a bank", Model::Mos6581, "SIDSTN")
        .err()
        .unwrap();
    assert!(err.contains("SysEx"), "{err}");
}

#[test]
fn a_midibox_sid_bank_is_told_by_its_header_and_imports_its_lead_patches() {
    let mut bank = mbsid_dump("Lead One", 0, 0);
    bank.extend(mbsid_dump("Drum Kit", MB_DRUM, 1));
    bank.extend(mbsid_dump("Lead Two", 0, 2));
    assert_eq!(default_name(&bank), "MBSID");
    assert_eq!(default_name(&dump("Patch")), "SIDSTN");
    let files = bank_files(&bank, Model::Mos6581, "MBSID").unwrap();
    assert_eq!(files.songs.len(), 1);
    assert!(files.songs[0].sng.starts_with(b"GTS5"));
    for line in [
        "refused A002 Drum Kit: a Drum engine patch, only Lead patches import",
        "I01 LEAD ONE",
        "I02 LEAD TWO",
    ] {
        assert!(files.report.contains(line), "{line}: {}", files.report);
    }
}

#[test]
fn a_midibox_sid_bank_with_no_lead_patch_is_refused_with_each_reason() {
    let err = bank_files(&mbsid_dump("Drum Kit", MB_DRUM, 0), Model::Mos6581, "MBSID")
        .err()
        .unwrap();
    assert!(err.contains("A001 Drum Kit: a Drum engine patch"), "{err}");
}
