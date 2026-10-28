// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The boot-loop guard in the firmware (platform/fm1/main.c, app/bootguard.c): boots that die
//! early twice in a row end in UBOOT on the third, and a stable boot clears the count. And the
//! power-on gesture into UBOOT (main.c power_on_uboot): OCT- alone, held a second.
//!
//! fm1-emu does not model a reset (no P33, so neither the watchdog nor `fm1_reboot` resets
//! the chip; TESTING.md, "fm1-emu's limits"). Each reset is played here by booting a fresh machine that keeps
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

/// platform/fm1/main.c `boot_guard` (a `bootguard`: magic, pending, failed, check).
fn words(m: &mut Machine, fw: &Firmware) -> [u32; 4] {
    let at = fw.symbol("boot_guard");
    [0, 4, 8, 12].map(|o| m.soc.read32(at + o))
}

/// The guard's magic, pending and failed.
fn guard(m: &mut Machine, fw: &Firmware) -> [u32; 3] {
    let w = words(m, fw);
    [w[0], w[1], w[2]]
}

/// Boots with the guard's words as a reset left them.
fn boot_with(fw: &Firmware, kept: Option<[u32; 4]>) -> Machine {
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
    let kept_first = words(&mut first, &fw);
    assert!(
        !first.soc.periph.audio.is_empty(),
        "the first boot did not get to the audio"
    );

    let mut second = boot_with(&fw, Some(kept_first));
    second.run_ms(DIES_AT_MS);
    let after_second = guard(&mut second, &fw);
    let kept_second = words(&mut second, &fw);

    let mut third = boot_with(&fw, Some(kept_second));
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
    let kept = words(&mut first, &fw);
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

#[test]
fn another_firmwares_words_over_the_guard_do_not_enter_uboot() {
    let Some(fw) = fixture("the boot-guard test") else {
        return;
    };
    // After Felucca 1.6.1: SIDOU's magic survives (Felucca's 68-byte crash record only writes
    // that word on a second-core crash), the words after it are Felucca's.
    let mut m = boot_with(&fw, Some([BOOTGUARD_MAGIC, 0x4247_5244, 3, 1]));
    m.run_ms(DIES_AT_MS);
    let after = guard(&mut m, &fw);
    println!("guard after a boot over foreign words: {after:x?}");
    assert_ne!(
        &mailbox(&mut m)[..15],
        b"usb_update_mode",
        "foreign words entered UBOOT"
    );
    assert!(
        !m.soc.periph.audio.is_empty(),
        "the boot did not get to the audio"
    );
    assert_eq!(after, [BOOTGUARD_MAGIC, 1, 0]);
}

/// Felucca's ids: OCT- and OCT+ (buttons 0 and 1), and a note key.
const OCT_DOWN: u8 = 0;
const OCT_UP: u8 = 1;
const NOTE_C: u8 = 14;
/// main.c uboot_gesture: 21 scans 50 ms apart.
const GESTURE_MS: u64 = 1_000;

/// A boot with `keys` held from power-on, let go of at `release` ms (if any), run to `ms`.
fn boot_holding(fw: &Firmware, keys: &[u8], release: Option<u64>, ms: u64) -> Machine {
    let mut m = boot_with(fw, None);
    for &k in keys {
        m.key(k, true);
    }
    if let Some(at) = release {
        m.run_ms(at);
        for &k in keys {
            m.key(k, false);
        }
    }
    let now = m.ms();
    m.run_ms(ms - now);
    m
}

#[test]
fn oct_down_alone_held_a_second_at_power_on_enters_uboot() {
    let Some(fw) = fixture("the power-on UBOOT test") else {
        return;
    };
    // Not before the second is up.
    let mut early = boot_holding(&fw, &[OCT_DOWN], None, GESTURE_MS - 50);
    assert_ne!(
        &mailbox(&mut early)[..15],
        b"usb_update_mode",
        "UBOOT before OCT- was held a second"
    );
    let mut m = boot_holding(&fw, &[OCT_DOWN], None, GESTURE_MS + 200);
    let mb = mailbox(&mut m);
    let pc = m.cpu.pc;
    let at = fw.function_at(pc);
    // PA2 (port A's OUT bit 2) high: the backlight dark, the gesture's confirmation.
    let pa_out = m.soc.read32(0x5_0000);
    println!(
        "mailbox {:?}, the core at {pc:#x} ({at}), PA2 {}, {} audio frames",
        String::from_utf8_lossy(&mb),
        (pa_out >> 2) & 1,
        m.soc.periph.audio.len() / 2
    );
    assert_eq!(&mb[..15], b"usb_update_mode", "no UBOOT request");
    // fm1_enter_uboot's last step asks P33 for the chip reset (which fm1-emu does not model):
    // the core waits for it in that loop, with interrupts off, and never got to anything else.
    assert!(
        at.starts_with("fm1_enter_uboot"),
        "the core is not waiting for the reset: {at}"
    );
    m.run_ms(50);
    assert_eq!(m.cpu.pc, pc, "the core left the reset's wait");
    assert_eq!((pa_out >> 2) & 1, 1, "the backlight is not dark");
    assert!(
        m.soc.periph.audio.is_empty(),
        "the boot went on to the audio"
    );
    assert_eq!(guard(&mut m, &fw)[0], 0, "the boot guard ran first");
}

#[test]
fn oct_down_let_go_within_the_second_boots_on() {
    let Some(fw) = fixture("the power-on UBOOT test") else {
        return;
    };
    for release in [5, 300, GESTURE_MS - 100] {
        let mut m = boot_holding(&fw, &[OCT_DOWN], Some(release), 2_500);
        assert_ne!(
            &mailbox(&mut m)[..15],
            b"usb_update_mode",
            "UBOOT although OCT- was let go at {release} ms"
        );
        assert!(
            !m.soc.periph.audio.is_empty(),
            "no audio after OCT- let go at {release} ms"
        );
    }
}

#[test]
fn oct_down_with_another_key_or_none_does_nothing() {
    let Some(fw) = fixture("the power-on UBOOT test") else {
        return;
    };
    for keys in [&[OCT_DOWN, OCT_UP][..], &[OCT_DOWN, NOTE_C], &[OCT_UP], &[]] {
        let mut m = boot_holding(&fw, keys, None, 2_000);
        assert_ne!(
            &mailbox(&mut m)[..15],
            b"usb_update_mode",
            "UBOOT with {keys:?} held"
        );
        assert!(
            !m.soc.periph.audio.is_empty(),
            "no audio with {keys:?} held"
        );
    }
}

#[test]
fn a_second_key_pressed_during_the_hold_cancels_it() {
    let Some(fw) = fixture("the power-on UBOOT test") else {
        return;
    };
    let mut m = boot_with(&fw, None);
    m.key(OCT_DOWN, true);
    m.run_ms(500);
    m.key(OCT_UP, true);
    m.run_ms(100);
    m.key(OCT_UP, false);
    m.run_ms(1_500);
    assert_ne!(&mailbox(&mut m)[..15], b"usb_update_mode");
    assert!(!m.soc.periph.audio.is_empty());
}
