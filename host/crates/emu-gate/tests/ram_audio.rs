// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The audio path runs from SRAM (`.ram_text`, placed by `platform/fm1/app.ld`): the boot
//! copies it there, the ALNK0 interrupt executes nothing else, and no direct branch crosses
//! between flash and SRAM code (at the edge of the pi32v2's branch reach).
//!
//! What catches what: a function the audio path calls directly but the `.ram_text` rules
//! miss stays in `.text`, at least 0x400158 bytes from SRAM, and the link usually fails with
//! "relocation truncated to fit" (R_PI32V2_LONG_JUMP_23M2). It need not: the relocation
//! test below is the guard, and rejects crossings that happen to be in reach. Only code entered through an absolute
//! address (the ISR vector, a future callback through a pointer) can run from flash with
//! everything linking; the interrupt test catches that.
//!
//! fm1-emu does not model the I-cache, so the timing benefit is left to hardware
//! (docs/hardware.md, open checks).

mod common;

use std::path::PathBuf;

use common::{build_dir, Firmware, Machine, APP_BASE, TAP_PER_SAMPLE};
use fm1_core::Bus;

/// RAMTEXT's size in app.ld: 48 KB, ending at Felucca's RAM-run debug area (0x01C0C000), which
/// code must not reach (on the device, code there misbehaved: app.ld).
const RAMTEXT_BYTES: u32 = 48 * 1024;
/// Where Felucca's RAM-run debug area starts: RAMTEXT ends here (app.ld).
const DEBUG_AREA: u32 = 0x01C0_C000;
/// A4's key id, held so the render has a voice to run.
const A4_KEY: u8 = 30;
/// PLAY (app/panel.h PANEL_BTN_PLAY): the song player runs in the interrupt too.
const PLAY: u8 = sidou_sys::PANEL_BTN_PLAY as u8;

fn fixture(what: &str) -> Option<(PathBuf, Firmware)> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    let fw = Firmware::load(&build);
    Some((build, fw))
}

fn ram_text(fw: &Firmware) -> std::ops::Range<u32> {
    fw.symbol("_rt_start")..fw.symbol("_rt_end")
}

#[test]
fn the_boot_copies_the_audio_path_to_sram() {
    let Some((_, fw)) = fixture("the SRAM audio test") else {
        return;
    };
    let rt = ram_text(&fw);
    let load = fw.symbol("_rt_load");
    let size = rt.end - rt.start;
    let mut m = Machine::boot(&fw.image);
    m.run_ms(50);
    let from = (load - APP_BASE) as usize;
    let image = &fw.image[from..from + size as usize];
    let ram: Vec<u8> = (rt.clone()).map(|a| m.soc.read8(a)).collect();
    println!(
        ".ram_text: {size} bytes of RAMTEXT's {RAMTEXT_BYTES} ({:.1} %), at {:#010x}, \
         loaded from image offset {from:#x}; image {} bytes",
        f64::from(size) * 100.0 / f64::from(RAMTEXT_BYTES),
        rt.start,
        fw.image.len()
    );
    assert!(size > 0, ".ram_text is empty: nothing runs from SRAM");
    assert!(size <= RAMTEXT_BYTES);
    assert!(
        rt.end <= DEBUG_AREA,
        ".ram_text ends at {:#010x}, in Felucca's RAM-run debug area ({DEBUG_AREA:#010x}..): \
         on the device code there misbehaved (STEP's edits were never applied)",
        rt.end
    );
    assert!(
        ram == image,
        "SRAM at .ram_text does not hold the image's copy: fm1_cstart did not copy it"
    );
}

#[test]
fn the_audio_interrupt_runs_only_from_sram() {
    let Some((_, fw)) = fixture("the SRAM audio test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.key(A4_KEY, true);
    m.run_ms(500); // past the LCD's start-up delays: the audio starts after them
    common::run_past_boot(&mut m, &fw);
    // and the song player (M2c Task 6): PLAY plays SIDOU's first song in the interrupt
    m.key(PLAY, true);
    m.run_ms(30);
    m.key(PLAY, false);
    m.run_ms(100);
    m.watch_alnk_pcs(ram_text(&fw));
    m.run_ms(400);
    let calls = m
        .soc
        .read32(fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::calls as u32);
    assert!(calls > 10, "the song did not play ({calls} calls)");
    let outside: Vec<String> = m
        .alnk_pcs_outside
        .iter()
        .map(|&pc| fw.function_at(pc))
        .collect();
    println!(
        "{} audio blocks watched; {} instructions at the ALNK0 level from SRAM, {} distinct PCs \
         elsewhere",
        m.alnk_blocks.len(),
        m.alnk_pcs_inside,
        outside.len()
    );
    let blocks = m.alnk_blocks.len();
    assert!(
        blocks > 100,
        "the audio interrupt hardly ran ({blocks} blocks)"
    );
    assert!(m.alnk_pcs_inside > 1_000_000);
    assert!(
        outside.is_empty(),
        "the audio interrupt runs code outside .ram_text: add its sections to app.ld's \
         .ram_text rules: {outside:?}"
    );
}

/// pi32v2 ELF relocation types (JieLi's binutils; the numbers checked against
/// `objdump -r` names on the build's objects).
const R_LONG_JUMP_23M2: u32 = 2; // call and goto: PC-relative, about +-4 MB
const R_MOV_ABS32: u32 = 5; // a 32-bit address loaded into a register
const R_ABS32: u32 = 8; // a 32-bit address in data (tables, vectors)

/// No direct `call` or `goto` crosses between flash (XIP, 0x02000000..) and SRAM
/// (`.ram_text`) code: SRAM is about 4 MB below XIP, the edge of a branch's reach, so such a
/// branch either fails the link ("relocation truncated to fit") or links only while the code
/// happens to lie close enough, and breaks with an unrelated change. The linker is the
/// authority: the firmware links with --emit-relocs, and every branch relocation's site and
/// target must lie in the same region. Address loads (`MOV_ABS32`, `ABS32`) may cross: that
/// is how flash code calls SRAM code through a pointer (audio.c `far_drain`) and how the
/// vector table reaches `isr_alnk0`. Any other relocation type fails the test until it is
/// classified here.
#[test]
fn no_direct_branch_crosses_between_flash_and_sram() {
    use object::{
        Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget, SectionIndex,
    };
    let Some((_, fw)) = fixture("the SRAM audio test") else {
        return;
    };
    let file = object::File::parse(&*fw.elf).expect("parse the firmware ELF");
    let ram_text = file
        .section_by_name(".ram_text")
        .expect("no .ram_text section")
        .index();
    let in_sram = |i: SectionIndex| i == ram_text;
    let (mut branches, mut addresses, mut crossing) = (0, 0, Vec::new());
    for section in file.sections() {
        let site_in_sram = in_sram(section.index());
        for (offset, rel) in section.relocations() {
            let RelocationFlags::Elf { r_type } = rel.flags() else {
                panic!("not an ELF relocation");
            };
            match r_type {
                R_MOV_ABS32 | R_ABS32 => {
                    addresses += 1;
                    continue;
                }
                R_LONG_JUMP_23M2 => branches += 1,
                t => panic!(
                    "relocation type {t} at {:#x} in {}: classify it in this test",
                    offset,
                    section.name().unwrap_or("?")
                ),
            }
            let RelocationTarget::Symbol(sym) = rel.target() else {
                panic!("a branch at {offset:#x} without a symbol");
            };
            let sym = file.symbol_by_index(sym).expect("relocation symbol");
            let Some(target) = sym.section_index() else {
                panic!("a branch at {offset:#x} to {:?}, in no section", sym.name());
            };
            if site_in_sram != in_sram(target) {
                crossing.push(format!(
                    "{} -> {}",
                    fw.function_at(offset as u32),
                    sym.name().unwrap_or("?")
                ));
            }
        }
    }
    println!(
        "{branches} branch relocations, {addresses} address relocations, {} crossing",
        crossing.len()
    );
    assert!(
        branches > 100,
        "only {branches} branch relocations: is the firmware linked with --emit-relocs?"
    );
    assert!(
        crossing.is_empty(),
        "direct branches between flash and SRAM code: {crossing:?}"
    );
}

/// The flash region the core executes and reads in place (XIP), app.ld's `XIP`.
const XIP: std::ops::Range<u64> = 0x0200_0000..0x0300_0000;

/// The audio path reads no data from flash: every address the `.ram_text` code loads
/// (`MOV_ABS32`, `ABS32`) lies in SRAM (RAM, POOL), not in XIP, where a read in the audio
/// interrupt can miss the cache and wait on the flash. A constant table that code inlined into
/// the audio path reads (the compiler places `static const` in `.rodata`, in flash) goes to
/// `.data` on the device (`__attribute__((section(".data.<name>")))` under `__PI32V2__`, as
/// `SID_NOISE_WB`). This sees only addresses the code itself names: data reached through a
/// pointer set up outside the audio path (the combined waveforms' pulldown rows,
/// `sid_osc.pulldown`, in flash by design; the DAC tables, in POOL) is not checked here.
#[test]
fn the_audio_path_reads_no_data_from_flash() {
    use object::{Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget};
    let Some((_, fw)) = fixture("the SRAM audio test") else {
        return;
    };
    let file = object::File::parse(&*fw.elf).expect("parse the firmware ELF");
    let ram_text = file
        .section_by_name(".ram_text")
        .expect("no .ram_text section");
    let (mut loads, mut in_flash) = (0, Vec::new());
    for (offset, rel) in ram_text.relocations() {
        let RelocationFlags::Elf { r_type } = rel.flags() else {
            panic!("not an ELF relocation");
        };
        if r_type == R_LONG_JUMP_23M2 {
            continue; // no_direct_branch_crosses_between_flash_and_sram
        }
        assert!(
            matches!(r_type, R_MOV_ABS32 | R_ABS32),
            "relocation type {r_type} at {offset:#x} in .ram_text: classify it in this test"
        );
        loads += 1;
        let RelocationTarget::Symbol(sym) = rel.target() else {
            panic!("an address at {offset:#x} without a symbol");
        };
        let sym = file.symbol_by_index(sym).expect("relocation symbol");
        let target = sym.address().wrapping_add_signed(rel.addend());
        if XIP.contains(&target) {
            let name = file
                .symbols()
                .filter(|s| s.size() > 0)
                .find(|s| (s.address()..s.address() + s.size()).contains(&target))
                .and_then(|s| s.name().ok())
                .unwrap_or("?");
            in_flash.push(format!(
                "{} -> {name} ({target:#010x})",
                fw.function_at(offset as u32)
            ));
        }
    }
    println!(
        "{loads} address loads in .ram_text, {} into flash",
        in_flash.len()
    );
    assert!(
        loads > 5,
        "only {loads} address loads in .ram_text: --emit-relocs?"
    );
    assert!(
        in_flash.is_empty(),
        "the audio path reads flash (XIP) data: put these in .data on the device: {in_flash:?}"
    );
}

/// A chord's note keys (14 + note - 53): A3, E4, E5.
const CHORD: [u8; 3] = [18, 25, 37];
const HOME: u8 = 8;

/// The audio interrupt's mean instructions a sample over `ms`.
fn per_sample(m: &mut Machine, ms: u64) -> (f64, usize) {
    let from = m.alnk_blocks.len();
    m.run_ms(ms);
    let b = &m.alnk_blocks[from..];
    (
        b.iter().sum::<u64>() as f64 / b.len() as f64 / 128.0,
        b.len(),
    )
}

/// The tap runs while LIVE shows and costs the audio interrupt at most TAP_PER_SAMPLE
/// instructions a sample: the same build with the menu on top (the tap off), against LIVE on
/// top (after its title card), a chord held throughout, by turns.
#[test]
fn the_live_view_tap_costs_the_audio_interrupt_little() {
    let Some((_, fw)) = fixture("the tap cost test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, &fw);
    for k in CHORD {
        m.key(k, true);
    }
    m.run_ms(1500); // to the sustain level: the chip's cost steady
    let (mut on, mut off) = (Vec::new(), Vec::new());
    for _ in 0..3 {
        assert!(common::tap_on(&mut m, &fw), "the tap is off under LIVE");
        on.push(per_sample(&mut m, 400));
        m.key(HOME, true);
        m.run_ms(30);
        m.key(HOME, false);
        m.run_ms(400);
        assert!(!common::tap_on(&mut m, &fw), "the tap runs under the menu");
        off.push(per_sample(&mut m, 400));
        m.key(HOME, true);
        m.run_ms(30);
        m.key(HOME, false);
        // Back to LIVE behind THE OSCILLATOR's title card (app/title_card.h), which the held
        // chord does not end: the tap is off under it.
        let back = m.ms();
        while common::top_view(&mut m, &fw) != fw.symbol("live_view_view")
            || common::screen_animating(&mut m, &fw)
        {
            assert!(m.ms() < back + 4000, "the card did not end");
            m.run_ms(40);
        }
        m.run_ms(400);
    }
    let mean = |v: &[(f64, usize)]| v.iter().map(|x| x.0).sum::<f64>() / v.len() as f64;
    let added = mean(&on) - mean(&off);
    println!(
        "audio interrupt, a chord held: {:.2} instructions a sample with the tap (LIVE), {:.2} \
         without (the menu): the tap adds {added:.2} ({on:?} / {off:?})",
        mean(&on),
        mean(&off)
    );
    assert!(on.iter().chain(&off).all(|x| x.1 > 100));
    assert!(added > 0.0, "the tap costs nothing: is it running?");
    assert!(
        added <= TAP_PER_SAMPLE,
        "the tap adds {added:.2} instructions a sample"
    );
}
