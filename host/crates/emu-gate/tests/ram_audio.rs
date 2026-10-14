// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The audio path runs from SRAM (`.ram_text`, placed by `platform/fm1/app.ld`): the boot
//! copies it there, the ALNK0 interrupt executes nothing else, and no direct branch crosses
//! between flash and SRAM code (at the edge of the pi32v2's branch reach).
//!
//! What catches what: a function the audio path calls directly but the `.ram_text` rules
//! miss stays in `.text`, at least 0x400158 bytes from SRAM, and the link fails with
//! "relocation truncated to fit" (R_PI32V2_LONG_JUMP_23M2). The relocation test below also
//! rejects crossings that happen to be in reach. Only code entered through an absolute
//! address (the ISR vector, a future callback through a pointer) can run from flash with
//! everything linking; the interrupt test catches that.
//!
//! fm1-emu does not model the I-cache, so the timing benefit is left to hardware
//! (docs/hardware.md, open checks).

mod common;

use std::path::PathBuf;

use common::{build_dir, Firmware, Machine, APP_BASE};
use fm1_core::Bus;

/// RAMTEXT's size in app.ld.
const RAMTEXT_BYTES: u32 = 24 * 1024;
/// A4's key id, held so the render has a voice to run.
const A4_KEY: u8 = 30;

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
    m.watch_alnk_pcs(ram_text(&fw));
    m.run_ms(400);
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
