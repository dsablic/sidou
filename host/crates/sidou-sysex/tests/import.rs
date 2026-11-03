// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sysex::import::{bank_files, Model};
use sidou_sysex::{Packet, FILE_SONG, PUT};

const NAME_BYTES: usize = 10;
const DECODED_BYTES: usize = 146;
const PATCH_MODE: usize = 22;
const OSC1_WAVE: usize = 36 + 15;
const OSC1_SUSTAIN: usize = 36 + 8;
const SAW: u8 = 0x20;

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
