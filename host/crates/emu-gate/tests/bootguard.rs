// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The boot-loop guard in the firmware (platform/fm1/main.c, app/bootguard.c): boots that die
//! early twice in a row end in UBOOT on the third, and a stable boot clears the count.
//!
//! fm1-emu does not model a reset (no P33, so neither the watchdog nor `fm1_reboot` resets
//! the chip; docs/TODO.md). Each reset is played here by booting a fresh machine that keeps
//! the guard's .noinit words, as the chip's RAM keeps them; a boot "dies" by being cut off
//! before it is stable. What UBOOT does next is the mask ROM's: the test checks the firmware's
//! request, the boot-state mailbox, and that it went no further (no audio).

mod common;

use common::{build_dir, Firmware, Machine};
use fm1_core::Bus;
use sidou_sys::{BOOTGUARD_MAGIC, BOOTGUARD_STABLE_MS};

/// fm1_sys.h FM1_BOOT_STATE: fm1_enter_uboot writes "usb_update_mode" there and resets.
const BOOT_STATE: u32 = 0x01C7_FD80;
/// Before the boot is stable: the audio is running by then.
const DIES_AT_MS: u64 = 600;

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

/// platform/fm1/main.c `boot_guard` (a `bootguard`: magic, pending, failed).
fn guard(m: &mut Machine, fw: &Firmware) -> [u32; 3] {
    let at = fw.symbol("boot_guard");
    [0, 4, 8].map(|o| m.soc.read32(at + o))
}

/// Boots with the guard's words as a reset left them.
fn boot_with(fw: &Firmware, kept: Option<[u32; 3]>) -> Machine {
    let mut m = Machine::boot(&fw.image);
    if let Some(words) = kept {
        let at = fw.symbol("boot_guard");
        for (i, w) in words.iter().enumerate() {
            m.soc.write32(at + 4 * i as u32, *w);
        }
    }
    m
}

fn mailbox(m: &mut Machine) -> Vec<u8> {
    (0..16).map(|i| m.soc.read8(BOOT_STATE + i)).collect()
}

#[test]
fn two_early_deaths_end_in_uboot() {
    let Some(fw) = fixture("the boot-guard test") else {
        return;
    };
    let mut first = boot_with(&fw, None);
    first.run_ms(DIES_AT_MS);
    let after_first = guard(&mut first, &fw);
    assert!(
        !first.soc.periph.audio.is_empty(),
        "the first boot did not get to the audio"
    );

    let mut second = boot_with(&fw, Some(after_first));
    second.run_ms(DIES_AT_MS);
    let after_second = guard(&mut second, &fw);

    let mut third = boot_with(&fw, Some(after_second));
    third.run_ms(DIES_AT_MS);
    let after_third = guard(&mut third, &fw);
    let mb = mailbox(&mut third);
    println!(
        "guard (magic, pending, failed): {after_first:x?} -> {after_second:x?} -> {after_third:x?}; \
         third boot's mailbox {:?}, {} audio frames",
        String::from_utf8_lossy(&mb),
        third.soc.periph.audio.len() / 2
    );
    assert_eq!(after_first, [BOOTGUARD_MAGIC, 1, 0]);
    assert_eq!(after_second, [BOOTGUARD_MAGIC, 1, 1]);
    assert_eq!(
        after_third,
        [BOOTGUARD_MAGIC, 0, 0],
        "cleared for the boot after UBOOT"
    );
    assert_eq!(
        &mb[..15],
        b"usb_update_mode",
        "the third boot did not ask for UBOOT"
    );
    assert!(
        third.soc.periph.audio.is_empty(),
        "the third boot went on to start the audio"
    );
}

#[test]
fn a_stable_boot_clears_an_earlier_death() {
    let Some(fw) = fixture("the boot-guard test") else {
        return;
    };
    let mut first = boot_with(&fw, None);
    first.run_ms(DIES_AT_MS);
    let kept = guard(&mut first, &fw);
    let mut second = boot_with(&fw, Some(kept));
    second.run_ms(DIES_AT_MS);
    let dying = guard(&mut second, &fw);
    second.run_ms(u64::from(BOOTGUARD_STABLE_MS));
    let stable = guard(&mut second, &fw);
    println!("guard after one death: {dying:x?}, once stable: {stable:x?}");
    assert_eq!(dying, [BOOTGUARD_MAGIC, 1, 1]);
    assert_eq!(stable, [BOOTGUARD_MAGIC, 0, 0]);
    assert_ne!(
        &mailbox(&mut second)[..15],
        b"usb_update_mode",
        "a single death entered UBOOT"
    );
}
