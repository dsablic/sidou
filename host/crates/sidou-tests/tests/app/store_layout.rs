// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The store's place in flash (`app/store_layout.h`) against everything else that owns flash:
//! the update loader's ranges (`platform/fm1/loader/ldr_core.c`) and Felucca's
//! (`platform/fm1/hal/fm1_flash.h`), read from their text so a change there fails here; the
//! update protocol's staging area (`platform/fm1/src/ota.c`); the app slot (`app.ld`); and
//! the package's VM descriptor, read from a package `sidou-pack` builds.
//! docs/hardware.md, "Flash map".

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

/// `#define NAME <number>` in `text`.
fn define(text: &str, name: &str) -> u32 {
    let found = text.lines().find_map(|l| {
        let mut w = l.split_whitespace();
        (w.next() == Some("#define") && w.next() == Some(name)).then(|| w.next().unwrap())
    });
    number(found.unwrap_or_else(|| panic!("no #define {name}")))
}

const MIB: u32 = 0x10_0000;

#[test]
fn the_store_is_whole_sectors_of_the_range_the_loader_never_touches() {
    assert_eq!(STORE_SECTOR, 0x1000);
    assert_eq!(STORE_LO % STORE_SECTOR, 0);
    assert_eq!(STORE_HI % STORE_SECTOR, 0);
    assert_eq!((STORE_HI - STORE_LO) / STORE_SECTOR, STORE_SECTORS);
    assert_eq!(STORE_SECTORS, 73);
    assert_eq!(STORE_HI - STORE_LO, 292 * 1024);

    let ldr = repo("platform/fm1/loader/ldr_core.c");
    // The loader writes the app area only, and erases what looks like an update record in
    // [LDR_REC_LO, LDR_REC_HI) except in [LDR_DATA_LO, LDR_DATA_HI): exactly the store.
    assert_eq!(STORE_LO, define(&ldr, "LDR_DATA_LO"));
    assert_eq!(STORE_HI, define(&ldr, "LDR_DATA_HI"));
    assert!(
        define(&ldr, "LDR_APP_HI") <= STORE_LO,
        "the app area reaches the store"
    );
    assert!(define(&ldr, "LDR_REC_LO") <= STORE_LO && STORE_HI <= define(&ldr, "LDR_REC_HI"));
    // Felucca's main store, the same range (fm1_flash.h).
    let hal = repo("platform/fm1/hal/fm1_flash.h");
    assert_eq!(STORE_LO, define(&hal, "FL_DATA_LO"));
    assert_eq!(STORE_HI, define(&hal, "FL_DATA_HI"));
}

#[test]
fn the_guard_band_is_the_last_page_of_every_sector_and_can_never_hold_a_record() {
    assert_eq!(STORE_SECTOR_USE, 0xF00);
    assert_eq!(STORE_PAGE, 0x100);
    // A whole page, so a page program (which wraps inside its page) cannot reach it.
    assert_eq!(STORE_SECTOR_USE % STORE_PAGE, 0);
    assert_eq!(STORE_SECTOR - STORE_SECTOR_USE, STORE_PAGE);
    // An update record is 80 bytes at "4K boundary - 256" (ota.c OTA_RES), its magic 0x5441
    // at +6: all inside the band, which stays erased (0xFF), so the magic never matches.
    let ota = repo("platform/fm1/src/ota.c");
    let res = define(&ota, "OTA_AREA") + define(&ota, "OTA_AREA_LEN") - 0x100;
    assert_eq!(
        res % STORE_SECTOR,
        STORE_SECTOR_USE,
        "records sit at a sector's 0xF00"
    );
    const { assert!(STORE_SECTOR_USE + 80 <= STORE_SECTOR) };
    let erased = [0xFFu8; 0x100];
    assert_ne!(u16::from_le_bytes([erased[6], erased[7]]), 0x5441);
}

#[test]
fn the_store_ends_where_the_update_protocols_staging_area_begins() {
    let ota = repo("platform/fm1/src/ota.c");
    let (area, len) = (define(&ota, "OTA_AREA"), define(&ota, "OTA_AREA_LEN"));
    assert_eq!((area, len), (0xE0000, 0x5000));
    assert_eq!(
        STORE_HI, area,
        "the store and the staging area touch, never overlap"
    );
    let hal = repo("platform/fm1/hal/fm1_flash.h");
    assert_eq!(define(&hal, "FL_OTA_LO"), area);
    assert_eq!(define(&hal, "FL_OTA_HI"), area + len);
}

#[test]
fn the_app_slot_ends_below_the_store() {
    // app.ld: `XIP (rx) : ORIGIN = 0x02000120, LENGTH = 0x8DFBC`. The SPL maps flash 0x4000
    // at XIP 0x02000000, app.bin at 0x120 of it.
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
    assert_eq!(origin, 0x0200_0120);
    assert_eq!(length as usize, sidou_pack::APP_SLOT);
    let end = 0x4000 + 0x120 + length;
    let ldr = repo("platform/fm1/loader/ldr_core.c");
    assert!(
        end <= define(&ldr, "LDR_APP_HI") && end <= 0x93000,
        "the app slot ends at {end:#x}"
    );
    assert_eq!(sidou_pack::FLASH_SIZE, 0x93000);
    assert!(end <= STORE_LO);
}

/// A package with stand-in SDK inputs: the region descriptors are sidou-pack's own.
fn package() -> Vec<u8> {
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
    let pkg = package();
    let u = fm1_soc::fwsc::unpack(&pkg).expect("fm1-emu reads the package");
    let vm = u.entry("VM").expect("the package's VM descriptor");
    let (lo, hi) = (vm.offset as u32, (vm.offset + vm.size) as u32);
    assert_eq!((lo, hi - lo), (0x93000, 352_256), "the VM descriptor moved");
    assert!(lo <= STORE_LO && STORE_HI <= hi);
    let ota = repo("platform/fm1/src/ota.c");
    let (area, len) = (define(&ota, "OTA_AREA"), define(&ota, "OTA_AREA_LEN"));
    assert!(lo <= area && area + len <= hi);
    assert!(hi <= MIB);
    // Nothing else the package declares overlaps the store.
    for h in &u.dir {
        if h.name == "VM" || h.name == "PRCT" || h.name == "app.bin" || h.name == "cfg_tool.bin" {
            continue; // VM holds it; PRCT is [0, 0x93000); the others are app-area offsets
        }
        let (a, b) = (h.offset as u32, (h.offset + h.size) as u32);
        assert!(
            b <= STORE_LO || a >= STORE_HI,
            "{} [{a:#x}, {b:#x}) overlaps the store",
            h.name
        );
    }
}

/// The update loader's write gate (`platform/fm1/loader/loader.c` `FL_RANGE_OK`): the app
/// area up to the store, and from the store's end on; never the store.
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
    assert_eq!(app.1, STORE_LO, "the app area's range ends where the store begins");
    assert_eq!(rest.0, STORE_HI, "the next range begins where the store ends");
    for (lo, hi) in ranges {
        assert!(hi <= STORE_LO || lo >= STORE_HI, "[{lo:#x}, {hi:#x}) reaches the store");
    }
}
