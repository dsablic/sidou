// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The boot-loop guard (`app/bootguard.c`): resets are played by calling `bootguard_boot`
//! again on the same state, as .noinit RAM keeps it.

use sidou_sys::*;

fn boot(g: &mut bootguard) -> bool {
    unsafe { bootguard_boot(g) != 0 }
}

fn poll(g: &mut bootguard, ms: u32) {
    unsafe { bootguard_poll(g, ms) }
}

#[test]
fn power_on_noise_starts_a_fresh_guard() {
    for noise in [0u32, 0xFFFF_FFFF, 0x1234_5678] {
        let mut g = bootguard {
            magic: noise,
            pending: noise,
            failed: noise,
        };
        assert!(!boot(&mut g), "noise {noise:#x} entered UBOOT");
        assert_eq!((g.magic, g.pending, g.failed), (BOOTGUARD_MAGIC, 1, 0));
    }
}

#[test]
fn two_failed_boots_enter_uboot_on_the_third() {
    let mut g = bootguard::default();
    assert!(!boot(&mut g), "first boot");
    // It crashes before it is stable (no poll past the window), and so does the next.
    poll(&mut g, BOOTGUARD_STABLE_MS - 1);
    assert!(!boot(&mut g), "after one failed boot");
    assert_eq!(g.failed, 1);
    assert!(boot(&mut g), "after two failed boots");
    assert_eq!(
        (g.pending, g.failed),
        (0, 0),
        "cleared for the boot after UBOOT"
    );
    assert!(!boot(&mut g), "the boot after UBOOT starts afresh");
}

#[test]
fn a_stable_boot_clears_the_count() {
    let mut g = bootguard::default();
    assert!(!boot(&mut g));
    assert!(!boot(&mut g)); // one failed boot
    poll(&mut g, BOOTGUARD_STABLE_MS);
    assert_eq!((g.pending, g.failed), (0, 0));
    // A later crash counts from zero again.
    assert!(!boot(&mut g));
    assert!(!boot(&mut g));
    assert_eq!(g.failed, 1);
}

#[test]
fn an_intentional_reset_is_not_a_failure() {
    let mut g = bootguard::default();
    for _ in 0..5 {
        assert!(!boot(&mut g));
        unsafe { bootguard_leave(&mut g) };
    }
    assert_eq!(g.failed, 0);
}
