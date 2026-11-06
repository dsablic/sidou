// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;
mod flash_plan;

use common::{Firmware, Machine, REGION_FREE_MIN};
use flash_plan::IMAGE_CEILING;
use fm1_core::Bus;
use sidou_sys::{ilp32, LIVE_VOICES};

const C4: u8 = 60;
const NOINIT: std::ops::Range<u32> = 0x01C7_C000..0x01C7_FD50;
const RAM_START: u32 = 0x01C0_E000;
const POOL_START: u32 = 0x01C1_E000;
const BOOT_MS: u64 = 500;
const ROLE_PERIPHERAL: u8 = 1;
const ROLE_NONE: u8 = 0;

fn ble_build() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("SIDOU_FM1_BLE_BUILD").filter(|d| !d.is_empty());
    if dir.is_none() {
        common::skip_message("the Bluetooth build (SIDOU_FM1_BLE_BUILD)");
    }
    dir.map(std::path::PathBuf::from)
}

fn section(fw: &Firmware, name: &str) -> (u32, u64) {
    use object::{Object, ObjectSection};
    let file = object::File::parse(&*fw.elf).expect("the firmware ELF");
    file.section_by_name(name)
        .map_or((0, 0), |s| (s.address() as u32, s.size()))
}

fn held(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let voices = fw.symbol("instrument") + ilp32::live::voice as u32;
    let mut notes = Vec::new();
    for i in 0..LIVE_VOICES {
        let v = voices + i * ilp32::live_voice::SIZE as u32;
        if m.soc.read8(v + ilp32::live_voice::gate as u32) != 0 {
            notes.push(m.soc.read8(v + ilp32::live_voice::note as u32));
        }
    }
    notes
}

fn from_the_air(m: &mut Machine, fw: &Firmware, pkt: u32) {
    let (ring, w) = (fw.symbol("ble_radio_in"), fw.symbol("ble_radio_in_w"));
    let n = m.soc.read32(w);
    let slots = fw.function_range("ble_radio_in").len() as u32 / 4;
    m.soc.write32(ring + 4 * (n % slots), pkt);
    m.soc.write32(w, n + 1);
}

fn note_on(note: u8) -> u32 {
    u32::from_le_bytes([0x09, 0x90, note, 100])
}

#[test]
fn the_bluetooth_build_fits_the_ceiling() {
    let Some(build) = ble_build() else { return };
    let size = std::fs::metadata(build.join("platform/fm1/sidou_app.bin"))
        .expect("the Bluetooth build's image")
        .len() as usize;
    println!("Bluetooth image {size} bytes, the ceiling {IMAGE_CEILING}");
    assert!(
        size <= IMAGE_CEILING,
        "the Bluetooth image is {size} bytes, {} over the ceiling",
        size - IMAGE_CEILING
    );
}

#[test]
fn the_stack_lives_in_noinit_and_the_ram_region_keeps_its_floor() {
    let Some(build) = ble_build() else { return };
    let fw = Firmware::load(&build);
    let (lo, hi) = (fw.symbol("_ble_noinit_start"), fw.symbol("_ble_noinit_end"));
    assert!(NOINIT.contains(&lo) && hi <= NOINIT.end && hi > lo);
    for ring in ["ble_radio_in", "ble_radio_out", "in_ring"] {
        let at = fw.symbol(ring);
        assert!(NOINIT.contains(&at), "{ring} at {at:#x}, outside NOINIT");
    }
    let used = section(&fw, ".data").1 + section(&fw, ".bss").1 + section(&fw, ".song_ram").1;
    let free = u64::from(POOL_START - RAM_START) - used;
    println!(
        "the RAM region: {free} bytes free with Bluetooth, NOINIT {} bytes",
        section(&fw, ".noinit").1
    );
    assert!(
        free >= REGION_FREE_MIN,
        "the RAM region has {free} bytes free"
    );
}

#[test]
fn notes_from_the_air_play_and_a_dropped_link_releases_them() {
    let Some(build) = ble_build() else { return };
    let fw = Firmware::load(&build);
    let mut m = Machine::boot(&fw.image);
    m.run_ms(BOOT_MS);
    let role = fw.symbol("ble_radio_role");
    m.soc.write8(role, ROLE_PERIPHERAL);
    from_the_air(&mut m, &fw, note_on(C4));
    m.run_ms(50);
    assert_eq!(held(&mut m, &fw), [C4], "the note from Bluetooth");
    m.soc.write8(role, ROLE_NONE);
    m.run_ms(50);
    assert!(
        held(&mut m, &fw).is_empty(),
        "the link dropped with the note held"
    );
}
