// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The flash timing spike (M2a Task 3b, platform/fm1/flash_spike.c): a test firmware for the
//! user's FM-1, built with `-DSIDOU_FLASH_SPIKE=ON` (docs/hardware-test.md, section 12).
//!
//! - The default image (SIDOU_FM1_BUILD) has none of it: no spike symbol in its ELF, as `nm`
//!   would list them, and not the spike's sector mark in its bytes.
//! - The spike's image (SIDOU_FM1_SPIKE_BUILD, e.g. `$PWD/../build/fm1-spike` from host/) runs
//!   in fm1-emu to its result screen with the TRS jack busy throughout: every time 0 (the
//!   model's flash is instantaneous), every check clean, no TRS byte lost. Its writes are
//!   bounded: with every byte of the flash outside the store's last two sectors set to a
//!   sentinel no erase or program could leave alone, none of them changes, and the guard
//!   band of both sectors stays erased.
//! - It writes nothing when a sector holds data that is neither blank nor its own, or when its
//!   round-trip pattern is damaged; on a second run it finds the pattern intact.
//!
//! The bounds themselves, at every address, are sidou-pack's tests/flash_spike_guard.rs.

mod common;

use common::{build_dir, Firmware, Machine};
use fm1_core::Bus;

/// The spike build's directory, made absolute, or None to skip.
fn spike_dir() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("SIDOU_FM1_SPIKE_BUILD").filter(|d| !d.is_empty())?;
    Some(std::path::absolute(&dir).expect("resolve SIDOU_FM1_SPIKE_BUILD"))
}

fn spike_skip(what: &str) {
    println!(
        "skipping {what}: set SIDOU_FM1_SPIKE_BUILD to the spike's build directory \
         (e.g. $PWD/../build/fm1-spike from host/), built with -DSIDOU_FLASH_SPIKE=ON"
    );
}

const FLASH: usize = 1 << 20;
const PATTERN: usize = 0xDE000;
const MEASURE: usize = 0xDF000;
const SPIKE: std::ops::Range<usize> = PATTERN..0xE0000;
const SECTOR: usize = 0x1000;
/// The update staging area and its record (ota.c OTA_AREA).
const OTA: std::ops::Range<usize> = 0xE0000..0xE5000;
const BAND: usize = 0xF00;
const MARK: &[u8; 16] = b"SIDOU FLASHSPIKE";
const SAVE: u8 = 9;

/// flash_spike_res's words (flash_spike.c).
const WORDS: usize = 55;
const STATE: usize = 0;
const STEP: usize = 1;
const RC: usize = 2;
const REFUSED: usize = 3;
const JEDEC: usize = 4;
const CLASS_PATTERN: usize = 7;
const CLASS_MEASURE: usize = 8;
const PATTERN_AT_START: usize = 9;
const STORE_USED: usize = 10;
/// The five timed stats (n, min, max, sum), erase 4K to the page erase, and the suspend's.
const STATS: [(usize, &str, u32); 6] = [
    (11, "erase 4K", 20),
    (15, "program 16", 20),
    (19, "program 64", 20),
    (23, "program 256", 20 * 14),
    (27, "page erase", 0),
    (32, "suspend", 5),
];
const PAGE_ERASE_WORKS: usize = 31;
const CHECKS: std::ops::Range<usize> = 37..44; // sus_read_bad .. twice_bad
const TRS: usize = 44;
const FINAL_BAD: usize = 54;

const BOOT: u32 = 0;
const WAIT: u32 = 1;
const DONE: u32 = 3;
const REFUSED_STATE: u32 = 4;
/// flash_spike_guard.h's sector classes and pattern states.
const BLANK: u32 = 0;
const OURS: u32 = 1;
const FOREIGN: u32 = 2;
const ABSENT: u32 = 0;
const INTACT: u32 = 1;
const DAMAGED: u32 = 2;
/// fm1-emu ignores the page erase (0x81): it is not in its flash model.
const PE_IGNORED: u32 = 0;

fn results(m: &mut Machine, fw: &Firmware) -> Vec<u32> {
    use object::{Object, ObjectSymbol};
    let file = object::File::parse(&*fw.elf).expect("parse the spike's ELF");
    let sym = file
        .symbols()
        .find(|s| s.name() == Ok("flash_spike_res"))
        .expect("no flash_spike_res in the spike's ELF");
    assert_eq!(
        sym.size(),
        4 * WORDS as u64,
        "flash_spike_res is not laid out as here"
    );
    let at = sym.address() as u32;
    (0..WORDS as u32)
        .map(|i| m.soc.read32(at + 4 * i))
        .collect()
}

/// What the spike's screen shows (its grid's shown_ch), row by row.
fn screen_text(m: &mut Machine, fw: &Firmware) -> Vec<String> {
    // The grid sits in the song's region; flash_spike_screen_at holds its address.
    let shown = m.soc.read32(fw.symbol("flash_spike_screen_at")) + 3 * 900;
    (0..30u32)
        .map(|r| {
            (0..30u32)
                .map(|c| char::from(m.soc.read8(shown + 30 * r + c)))
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// Boots the spike on `flash`, waits for its start screen, presses SAVE and runs to the end
/// with the TRS jack busy (active sensing, one byte a message) throughout.
fn run(fw: &Firmware, flash: &[u8]) -> (Machine, Vec<u32>) {
    // A blank-flash boot: the spike stores no settings, and the test sets the flash itself.
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    let mut r = results(&mut m, fw);
    for _ in 0..30 {
        if r[STATE] != BOOT {
            break;
        }
        m.run_ms(100);
        r = results(&mut m, fw);
    }
    if r[STATE] != WAIT {
        return (m, r);
    }
    m.midi_paced(200, |_| vec![0xFE]);
    m.key(SAVE, true);
    m.midi_paced(100, |_| vec![0xFE]);
    m.key(SAVE, false);
    for _ in 0..120 {
        m.midi_paced(100, |_| vec![0xFE]);
        let r = results(&mut m, fw);
        if r[STATE] >= DONE {
            return (m, r);
        }
    }
    let r = results(&mut m, fw);
    panic!(
        "the spike did not finish in 12 s: step {}, state {}",
        r[STEP], r[STATE]
    );
}

/// Every byte outside the spike's sectors a sentinel (alternately 0x5A and 0xA5: an erase
/// sets it to 0xFF, and the spike programs no data that keeps both); the two sectors blank,
/// and the update staging area blank too, as on a unit after a completed install: the boot's
/// `update_boot` (midi.c, SIDOU's as in every build) erases a staging area it finds in use,
/// which is the update protocol's cleanup, not the spike's.
fn sentinel_flash() -> Vec<u8> {
    let mut f: Vec<u8> = (0..FLASH)
        .map(|i| if i % 2 == 0 { 0x5A } else { 0xA5 })
        .collect();
    f[SPIKE].fill(0xFF);
    f[OTA].fill(0xFF);
    f
}

fn assert_outside_unchanged(before: &[u8], after: &[u8]) {
    let moved: Vec<usize> = (0..FLASH)
        .filter(|a| !SPIKE.contains(a) && before[*a] != after[*a])
        .take(8)
        .collect();
    assert!(
        moved.is_empty(),
        "the spike changed flash outside its sectors at {moved:#x?}"
    );
}

#[test]
fn the_default_image_has_no_spike() {
    let Some(build) = build_dir() else {
        common::skip_message("the spike's absence from the default image");
        return;
    };
    use object::{Object, ObjectSymbol, SymbolKind};
    let fw = Firmware::load(&build);
    let file = object::File::parse(&*fw.elf).expect("parse the firmware ELF");
    // nm's view: every named symbol but the file names (the build path may hold "spike").
    let spike: Vec<String> = file
        .symbols()
        .filter(|s| s.kind() != SymbolKind::File)
        .filter_map(|s| s.name().ok().map(str::to_string))
        .filter(|n| n.contains("flash_spike") || n.contains("spike_"))
        .collect();
    assert!(
        spike.is_empty(),
        "spike symbols in the default image: {spike:?}"
    );
    assert!(
        !fw.image.windows(MARK.len()).any(|w| w == MARK),
        "the spike's sector mark is in the default image"
    );
}

#[test]
fn the_spike_runs_to_its_result_screen_and_writes_only_its_two_sectors() {
    let Some(build) = spike_dir() else {
        spike_skip("the flash spike's run");
        return;
    };
    let fw = Firmware::load(&build);
    let before = sentinel_flash();
    let (mut m, r) = run(&fw, &before);
    let report = serde_json::json!({ "flash_spike_res": r, "state": r[STATE] });
    println!("{report}");
    let out = build.join("flash_spike.json");
    std::fs::write(&out, report.to_string())
        .unwrap_or_else(|e| panic!("write report {}: {e}", out.display()));
    assert_eq!(r[STATE], DONE, "step {}, rc {}", r[STEP], r[RC] as i32);
    assert_eq!(
        r[REFUSED], 0,
        "the guard refused a call: a bug in the spike"
    );
    assert_eq!(r[JEDEC], 0x85_6014);
    assert_eq!((r[CLASS_PATTERN], r[CLASS_MEASURE]), (BLANK, BLANK));
    assert_eq!(r[PATTERN_AT_START], ABSENT);
    assert_eq!(
        r[STORE_USED], 71,
        "the store's sectors but the spike's two hold the sentinel"
    );
    for (at, name, runs) in STATS {
        let (n, min, max, sum) = (r[at], r[at + 1], r[at + 2], r[at + 3]);
        assert_eq!(n, runs, "{name}: runs");
        assert_eq!(
            (min, max, sum),
            (0, 0, 0),
            "{name}: fm1-emu's flash is instantaneous"
        );
    }
    assert_eq!(r[PAGE_ERASE_WORKS], PE_IGNORED);
    // The suspend's SUS1 count (36) is 0 here: fm1-emu has no suspend. Every read-back clean.
    for i in CHECKS {
        assert_eq!(r[i], 0, "check word {i}");
    }
    assert_eq!(r[FINAL_BAD], 0);
    // The TRS jack was busy: about 3125 bytes in the open second, some in each masked run,
    // and nothing lost, not even across a 35 ms mask (109 bytes, under the 128-byte ring).
    let trs = &r[TRS..TRS + 6];
    assert!(
        (3000..=3200).contains(&trs[0]),
        "TRS bytes in 1 s: {}",
        trs[0]
    );
    assert!(
        trs[2] > 1000 && trs[4] > 1000,
        "TRS bytes while masking: {trs:?}"
    );
    assert_eq!(
        (trs[1], trs[3], trs[5]),
        (0, 0, 0),
        "TRS bytes lost: {trs:?}"
    );

    let after = m.soc.flash.mem.clone();
    assert_outside_unchanged(&before, &after);
    assert!(
        after[MEASURE..MEASURE + SECTOR].iter().all(|&b| b == 0xFF),
        "0xDF000 erased"
    );
    assert_eq!(
        &after[PATTERN..PATTERN + 16],
        MARK,
        "0xDE000 keeps the mark"
    );
    for s in [PATTERN, MEASURE] {
        assert!(
            after[s + BAND..s + SECTOR].iter().all(|&b| b == 0xFF),
            "{s:#x}'s guard band"
        );
    }

    let text = screen_text(&mut m, &fw);
    println!("{}", text.join("\n"));
    assert!(m.soc.lcd.display_on, "the display is on");
    assert!(text[0].starts_with("SIDOU FLASH SPIKE"), "{:?}", text[0]);
    assert!(text[11].starts_with("ERASE 4K"), "{:?}", text[11]);
    assert!(text[27].starts_with("DONE."), "{:?}", text[27]);

    // A second run on what the first left: its own sectors are taken, the pattern is intact.
    let (_, r2) = run(&fw, &after);
    assert_eq!(r2[STATE], DONE);
    assert_eq!((r2[CLASS_PATTERN], r2[CLASS_MEASURE]), (OURS, BLANK));
    assert_eq!(r2[PATTERN_AT_START], INTACT);

    // The pattern damaged since (by the stock firmware, say), the mark intact: refused, and
    // nothing written, so the finding stays for the user to record.
    let mut damaged = after.clone();
    damaged[PATTERN + 0x800] ^= 0x10;
    let (m3, r3) = run(&fw, &damaged);
    assert_eq!(r3[STATE], REFUSED_STATE);
    assert_eq!((r3[CLASS_PATTERN], r3[PATTERN_AT_START]), (OURS, DAMAGED));
    assert!(
        m3.soc.flash.mem == damaged,
        "the spike wrote while refusing"
    );
}

#[test]
fn the_spike_refuses_a_sector_that_is_not_its_own_and_writes_nothing() {
    let Some(build) = spike_dir() else {
        spike_skip("the flash spike's refusal");
        return;
    };
    let fw = Firmware::load(&build);
    for at in [MEASURE + 0x123, PATTERN, PATTERN + BAND + 6] {
        let mut before = sentinel_flash();
        before[at] = 0x41;
        let (mut m, r) = run(&fw, &before);
        assert_eq!(r[STATE], REFUSED_STATE, "a foreign byte at {at:#x}");
        let class = if at >= MEASURE {
            r[CLASS_MEASURE]
        } else {
            r[CLASS_PATTERN]
        };
        assert_eq!(class, FOREIGN);
        assert!(m.soc.flash.mem == before, "the spike wrote while refusing");
        let text = screen_text(&mut m, &fw);
        assert!(text[27].starts_with("REFUSED"), "{:?}", text[27]);
    }
}
