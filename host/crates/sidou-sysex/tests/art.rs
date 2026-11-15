// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::process::Command;

use sidou_sys::*;
use sidou_sysex::art::{self, Refusal};
use sidou_sysex::send::{file_type_of, sent_name};
use sidou_sysex::{crc32, Packet, DATA, END, FILE_ART, PUT};

fn shipped() -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    std::fs::read(root.join("assets/art/red-moon-rite.sidart")).expect("the shipped art pack")
}

fn resealed(mut pack: Vec<u8>) -> Vec<u8> {
    let n = pack.len() - 4;
    let crc = crc32(&pack[..n]);
    pack[n..].copy_from_slice(&crc.to_le_bytes());
    pack
}

fn with(at: usize, value: u8) -> Vec<u8> {
    let mut pack = shipped();
    pack[at] = value;
    resealed(pack)
}

#[test]
fn the_shipped_pack_is_red_moon_rites_and_each_broken_part_is_refused_for_its_reason() {
    let pack = art::check(shipped()).expect("the shipped pack");
    assert_eq!(pack.name(), "RED MOON RITE");
    let mut flipped = shipped();
    let mid = flipped.len() / 2;
    flipped[mid] ^= 1;
    let mut row = shipped();
    let frames = ART_PACK_TABLES as usize;
    let rows = usize::from(u16::from_le_bytes([row[frames + 10], row[frames + 11]]));
    row[rows..rows + 2].copy_from_slice(&0xfff0u16.to_le_bytes());
    let refused = [
        (shipped()[..64].to_vec(), ART_PACK_SHORT),
        (with(0, b'X'), ART_PACK_MAGIC),
        (with(8, 2), ART_PACK_NEWER),
        (with(9, 2), ART_PACK_SCENE),
        (with(16, 0), ART_PACK_NAME),
        (with(ART_PACK_HEAD as usize + 2, 125), ART_PACK_SLOTS),
        (with(frames + 4, 3), ART_PACK_FRAME_BAD),
        (with(frames + 8, 40), ART_PACK_FRAME_BAD),
        (resealed(row), ART_PACK_ROWS),
        (flipped, ART_PACK_CRC_BAD),
    ];
    for (bytes, code) in refused {
        let why = art::check(bytes).err().expect("refused");
        assert_eq!(why, Refusal(code), "{why}");
        assert_ne!(why.to_string(), "refused", "{code}: words of its own");
    }
}

#[test]
fn a_pack_goes_as_red_moon_rites_art_file_and_send_knows_it() {
    let bytes = shipped();
    assert_eq!(file_type_of(&bytes), Some(FILE_ART));
    assert_eq!(sent_name("my-skin.sidart", &bytes), art::NAME);
    let syx = art::syx(&art::check(bytes.clone()).unwrap()).unwrap();
    let frames: Vec<Packet> = syx
        .split_inclusive(|&b| b == 0xf7)
        .map(|f| Packet::decode(f).unwrap())
        .collect();
    let put = &frames[0];
    assert_eq!(put.command, PUT);
    assert_eq!(&put.data[2..15], art::NAME.as_bytes());
    assert_eq!(put.data[18], FILE_ART);
    let sent: Vec<u8> = frames[1..frames.len() - 1]
        .iter()
        .inspect(|p| assert_eq!(p.command, DATA))
        .flat_map(|p| p.data.clone())
        .collect();
    assert_eq!(sent, bytes);
    assert_eq!(frames.last().unwrap().command, END);
}

fn mounted(seed: &[u8]) -> (Vec<u8>, Option<(u8, Vec<u8>)>) {
    let mut flash = vec![0xffu8; STORE_HI as usize];
    for entry in seed.chunks(4 + STORE_SECTOR as usize) {
        let at = u32::from_le_bytes(entry[..4].try_into().unwrap()) as usize;
        flash[at..at + STORE_SECTOR as usize].copy_from_slice(&entry[4..]);
    }
    unsafe extern "C" fn map(ctx: *mut std::ffi::c_void, off: u32) -> *const u8 {
        unsafe { ctx.cast::<u8>().add(off as usize) }
    }
    unsafe extern "C" fn refuse(
        _: *mut std::ffi::c_void,
        _: u32,
        _: *const std::ffi::c_void,
        _: u32,
    ) -> i32 {
        STORE_ERROR
    }
    unsafe extern "C" fn no_erase(_: *mut std::ffi::c_void, _: u32) -> i32 {
        STORE_ERROR
    }
    let hooks = store_flash {
        map: Some(map),
        prog: Some(refuse),
        erase: Some(no_erase),
        ctx: flash.as_mut_ptr().cast(),
    };
    let mut st = Box::<store>::default();
    let name = std::ffi::CString::new(art::NAME).unwrap();
    let mut f = store_file::default();
    let file = unsafe {
        store_init(&mut *st, &hooks, STORE_WINDOW_MAX);
        store_mount(&mut *st);
        while store_busy(&*st) != 0 {
            store_poll(&mut *st);
        }
        (store_file_open(&*st, name.as_ptr(), &mut f) == STORE_OK).then(|| {
            let mut buf = vec![0u8; f.length as usize];
            store_file_read(&*st, &f, 0, buf.as_mut_ptr().cast(), f.length);
            (f.type_, buf)
        })
    };
    (flash, file)
}

#[test]
fn the_web_demos_seed_is_a_sidou_store_holding_the_pack_alone() {
    let bytes = shipped();
    let seed = art::seed(&art::check(bytes.clone()).unwrap());
    let sectors = seed.len() / (4 + STORE_SECTOR as usize);
    assert_eq!(seed.len() % (4 + STORE_SECTOR as usize), 0);
    assert!(
        sectors <= bytes.len().div_ceil(STORE_FILE_SECTOR_DATA as usize) + 2,
        "{sectors}"
    );
    let (_, file) = mounted(&seed);
    assert_eq!(file, Some((FILE_ART, bytes)));
}

#[test]
fn the_art_command_writes_the_upload_and_refuses_a_broken_pack() {
    let dir = std::env::temp_dir().join(format!("sidou-art-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let good = dir.join("skin.sidart");
    std::fs::write(&good, shipped()).unwrap();
    let run = |file: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_sidou-sysex"))
            .args([
                "art",
                file.to_str().unwrap(),
                "--out",
                dir.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    assert!(run(&good).status.success());
    let syx = std::fs::read(dir.join("skin.syx")).unwrap();
    assert_eq!(syx, art::syx(&art::check(shipped()).unwrap()).unwrap());
    let bad = dir.join("bad.sidart");
    std::fs::write(&bad, with(9, 2)).unwrap();
    let out = run(&bad);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("another scene"));
    assert!(!dir.join("bad.syx").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
