// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use sidou_sys::*;

fn repo(path: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(path);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn number(text: &str) -> u32 {
    let t = text.trim().trim_end_matches(['u', 'U']);
    match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => t.parse(),
    }
    .unwrap_or_else(|e| panic!("{text:?}: {e}"))
}

fn define_value(text: &str, name: &str) -> u32 {
    let found = text.lines().find_map(|l| {
        let mut w = l.split_whitespace();
        (w.next() == Some("#define") && w.next() == Some(name)).then(|| w.next().unwrap())
    });
    number(found.unwrap_or_else(|| panic!("no #define {name}")))
}

const MIB: u32 = 0x10_0000;
const OTA_RECORD_BELOW_BOUNDARY: u32 = 0x100;
const OTA_RECORD_LEN: u32 = 80;
const OTA_MAGIC: u16 = 0x5441;
const OTA_MAGIC_AT: usize = 6;
const XIP_BASE: u32 = 0x0200_0000;
const SPL_XIP_FLASH_BASE: u32 = 0x4000;
const APP_BIN_AT: u32 = 0x120;
const CONTAINERS_AND_APP_RELATIVE_ENTRIES: [&str; 4] = ["VM", "PRCT", "app.bin", "cfg_tool.bin"];

#[test]
fn the_store_is_whole_sectors_of_the_range_the_loader_never_touches() {
    assert_eq!(STORE_SECTOR, 0x1000);
    assert_eq!(STORE_LO % STORE_SECTOR, 0);
    assert_eq!(STORE_HI % STORE_SECTOR, 0);
    assert_eq!((STORE_HI - STORE_LO) / STORE_SECTOR, STORE_SECTORS);
    assert_eq!(STORE_SECTORS, 73);
    assert_eq!(STORE_HI - STORE_LO, 292 * 1024);

    let ldr = repo("platform/fm1/loader/ldr_core.c");
    assert_eq!(STORE_LO, define_value(&ldr, "LDR_DATA_LO"));
    assert_eq!(STORE_HI, define_value(&ldr, "LDR_DATA_HI"));
    assert!(
        define_value(&ldr, "LDR_APP_HI") <= STORE_LO,
        "the app area reaches the store"
    );
    assert!(
        define_value(&ldr, "LDR_REC_LO") <= STORE_LO
            && STORE_HI <= define_value(&ldr, "LDR_REC_HI")
    );
    let hal = repo("platform/fm1/hal/fm1_flash.h");
    assert_eq!(STORE_LO, define_value(&hal, "FL_DATA_LO"));
    assert_eq!(STORE_HI, define_value(&hal, "FL_DATA_HI"));
}

#[test]
fn the_guard_band_is_the_last_page_of_every_sector_and_can_never_hold_a_record() {
    assert_eq!(STORE_SECTOR_USE, 0xF00);
    assert_eq!(STORE_PAGE, 0x100);
    assert_eq!(STORE_SECTOR_USE % STORE_PAGE, 0);
    assert_eq!(STORE_SECTOR - STORE_SECTOR_USE, STORE_PAGE);
    let ota = repo("platform/fm1/src/ota.c");
    let res = define_value(&ota, "OTA_AREA") + define_value(&ota, "OTA_AREA_LEN")
        - OTA_RECORD_BELOW_BOUNDARY;
    assert_eq!(
        res % STORE_SECTOR,
        STORE_SECTOR_USE,
        "records sit at a sector's 0xF00"
    );
    const { assert!(STORE_SECTOR_USE + OTA_RECORD_LEN <= STORE_SECTOR) };
    let erased = [0xFFu8; 0x100];
    assert_ne!(
        u16::from_le_bytes([erased[OTA_MAGIC_AT], erased[OTA_MAGIC_AT + 1]]),
        OTA_MAGIC
    );
}

#[test]
fn the_store_ends_where_the_update_protocols_staging_area_begins() {
    let ota = repo("platform/fm1/src/ota.c");
    let (area, len) = (
        define_value(&ota, "OTA_AREA"),
        define_value(&ota, "OTA_AREA_LEN"),
    );
    assert_eq!((area, len), (0xE0000, 0x5000));
    assert_eq!(
        STORE_HI, area,
        "the store and the staging area touch, never overlap"
    );
    let hal = repo("platform/fm1/hal/fm1_flash.h");
    assert_eq!(define_value(&hal, "FL_OTA_LO"), area);
    assert_eq!(define_value(&hal, "FL_OTA_HI"), area + len);
}

#[test]
fn the_app_slot_ends_below_the_store() {
    let ld = repo("platform/fm1/app.ld");
    let line = ld
        .lines()
        .find(|l| l.trim_start().starts_with("XIP"))
        .expect("app.ld's XIP region");
    let field = |name: &str| {
        let at = line
            .find(name)
            .unwrap_or_else(|| panic!("{name} in {line:?}"))
            + name.len();
        let rest = line[at..].trim_start().trim_start_matches('=').trim_start();
        number(rest.split([',', ' ']).next().unwrap())
    };
    let (origin, length) = (field("ORIGIN"), field("LENGTH"));
    assert_eq!(origin, XIP_BASE + APP_BIN_AT);
    assert_eq!(length as usize, sidou_pack::APP_SLOT);
    let end = SPL_XIP_FLASH_BASE + APP_BIN_AT + length;
    let ldr = repo("platform/fm1/loader/ldr_core.c");
    assert!(
        end <= define_value(&ldr, "LDR_APP_HI") && end <= 0x93000,
        "the app slot ends at {end:#x}"
    );
    assert_eq!(sidou_pack::FLASH_SIZE, 0x93000);
    assert!(end <= STORE_LO);
}

fn package_with_stand_in_sdk_inputs() -> Vec<u8> {
    let inputs = sidou_pack::Inputs {
        uboot: vec![0x5A; 256],
        cfg_tool: vec![0xA5; 256],
        eq_cfg_hw: vec![0x3C; 64],
    };
    let mut app = b"SIDOU layout app FM-1_900\0".to_vec();
    app.resize(4096, 0x11);
    let mut loader = b"\x04\x88".to_vec();
    loader.extend_from_slice(sidou_pack::LOADER_MARK);
    loader.resize(1024, 0x22);
    sidou_pack::package(&inputs, &app, &loader, sidou_pack::DEFAULT_PRODUCT).unwrap()
}

#[test]
fn the_store_and_the_staging_area_lie_inside_the_packages_vm_descriptor() {
    let pkg = package_with_stand_in_sdk_inputs();
    let u = fm1_soc::fwsc::unpack(&pkg).expect("fm1-emu reads the package");
    let vm = u.entry("VM").expect("the package's VM descriptor");
    let (lo, hi) = (vm.offset as u32, (vm.offset + vm.size) as u32);
    assert_eq!((lo, hi - lo), (0x93000, 352_256), "the VM descriptor moved");
    assert!(lo <= STORE_LO && STORE_HI <= hi);
    let ota = repo("platform/fm1/src/ota.c");
    let (area, len) = (
        define_value(&ota, "OTA_AREA"),
        define_value(&ota, "OTA_AREA_LEN"),
    );
    assert!(lo <= area && area + len <= hi);
    assert!(hi <= MIB);
    for h in &u.dir {
        if CONTAINERS_AND_APP_RELATIVE_ENTRIES.contains(&h.name.as_str()) {
            continue;
        }
        let (a, b) = (h.offset as u32, (h.offset + h.size) as u32);
        assert!(
            b <= STORE_LO || a >= STORE_HI,
            "{} [{a:#x}, {b:#x}) overlaps the store",
            h.name
        );
    }
}

#[test]
fn the_loaders_write_gate_stops_exactly_at_the_store() {
    let loader = repo("platform/fm1/loader/loader.c");
    let line = loader
        .lines()
        .find(|l| l.starts_with("#define FL_RANGE_OK"))
        .expect("loader.c's FL_RANGE_OK");
    let ranges: Vec<(u32, u32)> = line
        .split("FL_IN(")
        .skip(1)
        .map(|call| {
            let args: Vec<&str> = call.split(')').next().unwrap().split(',').collect();
            assert_eq!(args.len(), 4, "{call:?}");
            (number(args[2]), number(args[3]))
        })
        .collect();
    assert_eq!(ranges.len(), 2, "{line}");
    let (app, rest) = (ranges[0], ranges[1]);
    assert_eq!(
        app.1, STORE_LO,
        "the app area's range ends where the store begins"
    );
    assert_eq!(
        rest.0, STORE_HI,
        "the next range begins where the store ends"
    );
    for (lo, hi) in ranges {
        assert!(
            hi <= STORE_LO || lo >= STORE_HI,
            "[{lo:#x}, {hi:#x}) reaches the store"
        );
    }
}
