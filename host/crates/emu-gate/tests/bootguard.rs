// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The boot-loop guard in the firmware (platform/fm1/main.c, app/bootguard.c): boots that die
//! early twice in a row end in UBOOT on the third, and a stable boot clears the count. And the
//! power-on gesture into UBOOT (main.c power_on_uboot): OCT- alone, held a second. And the
//! watchdog: the running firmware feeds it, and a hang resets the chip through it.
//!
//! fm1-emu resets the chip as the FM-1 does (P33: the watchdog, and PR_PWR, which fm1_reboot
//! and fm1_enter_uboot write): the core starts the firmware again from its entry, the chip's
//! registers as the loader left them, RAM (the guard's .noinit words, the boot-state mailbox)
//! kept. A boot here dies as a crash does: the test raises an exception, the firmware's fault
//! handler (fm1_fault_c) records it and resets the chip. A hang, which the watchdog resets, is
//! the checklist's dying build's (SIDOU_FM1_DEATH_BUILD, below). What UBOOT does next is the
//! mask ROM's, which fm1-emu does not have: the test stops at the firmware's reset request and
//! checks the request, the boot-state mailbox, and that it went no further (no audio).

mod common;

use common::{build_dir, Firmware, Machine, RST_SOFT, RST_WDT};
use fm1_core::Bus;
use sidou_sys::{BOOTGUARD_MAGIC, BOOTGUARD_STABLE_MS};

/// fm1_sys.h FM1_BOOT_STATE: fm1_enter_uboot writes "usb_update_mode" there and resets.
const BOOT_STATE: u32 = 0x01C7_FD80;
/// Before the boot is stable: the audio is running by then.
const DIES_AT_MS: u64 = 600;
/// main.c WDT_TIMEOUT 0x0D: 8 s (fm1_sys.h fm1_wdt_arm).
const WDT_MS: u64 = 8_000;

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

/// Boots with the guard's words as another firmware left them; the machine stops at a reset
/// the firmware asks for (`Machine::stop_at_reset`).
fn boot_with(fw: &Firmware, kept: Option<[u32; 4]>) -> Machine {
    let mut m = Machine::boot(&fw.image);
    m.stop_at_reset = true;
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

/// A crash, as the chip takes one: an exception's vector, one of fm1_vec.S's fatal stubs,
/// entered at the top priority. The fault handler records it and resets the chip through
/// PR_PWR (main.c fm1_fault, fm1_reboot). fm1_fatal_common's first instruction, `trigger`
/// (the debug unit's trace stop, nothing else changes), is one fm1-core does not run: the
/// core steps over it.
fn crash(m: &mut Machine, fw: &Firmware) {
    let stubs = fw.symbol("fm1_fatal_stubs");
    let common_entry = fw.symbol("fm1_fatal_common");
    let vec = (0..128u32)
        .find(|&i| m.soc.read32(fm1_soc::VEC_BASE + 4 * i) == stubs + 6 * i)
        .expect("a vector still on its fatal stub");
    m.cpu.interrupt(stubs + 6 * vec, 7);
    while m.cpu.pc != common_entry {
        common::step(&mut m.cpu, &mut m.soc);
    }
    let win =
        u64::from(m.soc.read32(common_entry)) | u64::from(m.soc.read32(common_entry + 4)) << 32;
    let (len, name) = fm1_core::decode_info(win, common_entry).expect("trigger decodes");
    assert_eq!(name, "trigger");
    m.cpu.pc += len as u32;
}

/// Runs until the firmware asks for a reset (at most `limit_ms`), then carries it out: the
/// ms it came at, from the machine's start.
fn run_to_reset(m: &mut Machine, limit_ms: u64) -> u64 {
    let end = m.ms() + limit_ms;
    while !m.reset_due() {
        assert!(m.ms() < end, "no reset in {limit_ms} ms");
        m.run_ms(1);
    }
    let at = m.ms();
    m.reset();
    at
}

/// A boot that dies DIES_AT_MS after it starts: its reset's cause.
fn die_early(m: &mut Machine, fw: &Firmware) -> u8 {
    m.run_ms(DIES_AT_MS);
    assert!(!m.reset_due(), "the boot reset by itself");
    crash(m, fw);
    run_to_reset(m, 100);
    m.resets.last().expect("a reset").1
}

#[test]
fn two_early_deaths_end_in_uboot() {
    let Some(fw) = fixture("the boot-guard test") else {
        return;
    };
    let mut m = boot_with(&fw, None);
    let first_cause = die_early(&mut m, &fw);
    let after_first = guard(&mut m, &fw);
    assert!(
        !m.soc.periph.audio.is_empty(),
        "the first boot did not get to the audio"
    );
    let second_cause = die_early(&mut m, &fw);
    let after_second = guard(&mut m, &fw);
    let audio_before_third = m.soc.periph.audio.len();

    // The third boot asks for UBOOT at once: the machine stops at its request.
    m.run_ms(DIES_AT_MS);
    assert!(m.reset_due(), "the third boot did not ask for a reset");
    let after_third = guard(&mut m, &fw);
    let mb = mailbox(&mut m);
    let at = fw.function_at(m.cpu.pc);
    let wdt_con = m.soc.p33.reg(fm1_soc::p33::P33::WDT_CON, false);
    m.reset();
    let crashes = m.soc.read32(fw.symbol("fm1_crash") + 4);
    println!(
        "guard (magic, pending, failed): {after_first:x?} -> {after_second:x?} -> {after_third:x?}; \
         resets {:?}; the third boot's mailbox {:?} from {at}, WDT_CON {wdt_con:#x}, {} audio \
         frames in it; crash records {crashes}",
        m.resets,
        String::from_utf8_lossy(&mb),
        (m.soc.periph.audio.len() - audio_before_third) / 2
    );
    assert_eq!([first_cause, second_cause], [RST_SOFT, RST_SOFT]);
    assert_eq!(crashes, 2, "the fault handler did not record both crashes");
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
        at.starts_with("fm1_enter_uboot"),
        "the reset came from {at}"
    );
    assert_eq!(wdt_con & 0x10, 0, "the watchdog runs into UBOOT");
    assert_eq!(m.resets.last().unwrap().1, RST_SOFT, "UBOOT's reset");
    assert_eq!(
        m.soc.periph.audio.len(),
        audio_before_third,
        "the third boot went on to start the audio"
    );
}

#[test]
fn a_stable_boot_clears_an_earlier_death() {
    let Some(fw) = fixture("the boot-guard test") else {
        return;
    };
    let mut m = boot_with(&fw, None);
    die_early(&mut m, &fw);
    m.run_ms(DIES_AT_MS);
    let dying = guard(&mut m, &fw);
    m.run_ms(u64::from(BOOTGUARD_STABLE_MS));
    let stable = guard(&mut m, &fw);
    println!("guard after one death: {dying:x?}, once stable: {stable:x?}");
    assert_eq!(dying, [BOOTGUARD_MAGIC, 1, 1]);
    assert_eq!(stable, [BOOTGUARD_MAGIC, 0, 0]);
    assert!(!m.reset_due(), "a single death reset the chip again");
    assert_ne!(
        &mailbox(&mut m)[..15],
        b"usb_update_mode",
        "a single death entered UBOOT"
    );
}

/// The main loop feeds the watchdog (main.c sidou_main): well past its 8 s, playing, nothing
/// resets. And it is armed: reset mode, WDT_TIMEOUT's 8 s.
#[test]
fn the_running_firmware_feeds_the_watchdog() {
    let Some(fw) = fixture("the watchdog test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.run_ms_jit(3 * WDT_MS);
    let con = m.soc.p33.reg(fm1_soc::p33::P33::WDT_CON, false);
    println!("WDT_CON {con:#x}, resets {:?}", m.resets);
    assert_eq!(con & 0x1F, 0x10 | 0x0D, "the watchdog is not armed for 8 s");
    assert_eq!(fm1_soc::p33::P33::wdt_ms(con), WDT_MS);
    assert!(m.resets.is_empty(), "the running firmware reset");
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
    // fm1_enter_uboot's last step asks P33 for the chip reset (PR_PWR), where the machine
    // stopped: the core never got to anything else, and the watchdog is off for UBOOT.
    assert!(m.reset_due(), "no reset request");
    assert!(
        at.starts_with("fm1_enter_uboot"),
        "the reset came from {at}"
    );
    let wdt_con = m.soc.p33.reg(fm1_soc::p33::P33::WDT_CON, false);
    assert_eq!(wdt_con & 0x10, 0, "the watchdog runs into UBOOT");
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

// ---- the checklist's dying build (docs/hardware-test.md, section 10) ----

/// The hang build's directory (`-DSIDOU_TEST_DEATH=hang`, e.g. `$PWD/../build/fm1-death` from
/// host/), made absolute, or None to skip.
fn death_dir() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("SIDOU_FM1_DEATH_BUILD").filter(|d| !d.is_empty())?;
    Some(std::path::absolute(&dir).expect("resolve SIDOU_FM1_DEATH_BUILD"))
}

/// The hang build with OCT- held from power-on hangs SIDOU_TEST_DEATH_MS (1 s) after each boot
/// with the interrupts off (main.c test_death): the watchdog resets the chip 8 s after the
/// main loop's last feed, twice, and the third boot goes to UBOOT through PR_PWR. Hardware
/// checklist section 10's first half, in the emulator.
#[test]
fn a_hang_resets_through_the_watchdog_and_the_third_boot_enters_uboot() {
    let Some(dir) = death_dir() else {
        println!(
            "skipping the watchdog hang test: set SIDOU_FM1_DEATH_BUILD to a build made with \
             -DSIDOU_TEST_DEATH=hang (e.g. $PWD/../build/fm1-death from host/)"
        );
        return;
    };
    let fw = Firmware::load(&dir);
    fw.symbol("test_death_armed"); // (a dying build's: panics on another)
    let mut m = Machine::boot(&fw.image);
    m.stop_at_reset = true;
    m.key(OCT_DOWN, true);
    // Two hangs, each run to its reset (the JIT skips the hang's spin); after each, the guard
    // as the next boot's first 5 ms leave it. The third boot asks for UBOOT within them, and
    // the machine stops at its request, which leaves the mailbox (the next boot clears it).
    let mut guards = Vec::new();
    while m.resets.len() < 2 {
        assert!(m.ms() < 30_000, "resets by 30 s: {:?}", m.resets);
        let n = m.resets.len();
        m.run_ms_jit(1_000);
        if m.resets.len() != n {
            m.run_ms(5);
            guards.push(guard(&mut m, &fw));
        }
    }
    assert!(m.reset_due(), "the third boot did not ask for a reset");
    let at = fw.function_at(m.cpu.pc);
    let mb = mailbox(&mut m);
    m.reset();
    println!(
        "resets (ms, cause) {:?}; the guard in boots 2 and 3 {guards:x?}; the third's request \
         from {at}, mailbox {:?}",
        m.resets,
        String::from_utf8_lossy(&mb)
    );
    let causes: Vec<u8> = m.resets.iter().map(|r| r.1).collect();
    assert_eq!(causes, [RST_WDT, RST_WDT, RST_SOFT]);
    // Each hang: 1 s into the boot, then the watchdog's 8 s.
    let first = m.resets[0].0;
    assert!(
        (1_000 + WDT_MS..1_000 + WDT_MS + 500).contains(&first),
        "the first watchdog reset at {first} ms"
    );
    let second = m.resets[1].0 - first;
    assert!(
        (1_000 + WDT_MS..1_000 + WDT_MS + 500).contains(&second),
        "the second watchdog reset {second} ms after the first"
    );
    assert_eq!(
        guards[0],
        [BOOTGUARD_MAGIC, 1, 1],
        "boot 2 counts boot 1's death"
    );
    assert_eq!(
        guards[1],
        [BOOTGUARD_MAGIC, 0, 0],
        "boot 3 cleared the guard for UBOOT"
    );
    // (fm1_enter_uboot, inlined into fm1_cstart in this build, which has no UBOOT gesture)
    assert!(
        at.starts_with("fm1_enter_uboot") || at.starts_with("fm1_cstart"),
        "the reset came from {at}"
    );
    assert_eq!(
        &mb[..15],
        b"usb_update_mode",
        "the third boot did not ask for UBOOT"
    );
}
