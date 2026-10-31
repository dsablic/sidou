// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::panel::{note_key, press, A4_KEY, HOME, PLAY};
use common::{booted, build_and_firmware, Firmware, Machine, APP_BASE, TAP_PER_SAMPLE};
use fm1_core::Bus;

const RAMTEXT_BYTES: u32 = 48 * 1024;
const DEBUG_AREA: u32 = 0x01C0_C000;

fn ram_text(fw: &Firmware) -> std::ops::Range<u32> {
    fw.symbol("_rt_start")..fw.symbol("_rt_end")
}

#[test]
fn the_boot_copies_the_audio_path_to_sram() {
    let Some((_, fw)) = build_and_firmware("the SRAM audio test") else {
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
         code stays out of it until a device check shows it is the app's (app.ld)",
        rt.end
    );
    assert!(
        ram == image,
        "SRAM at .ram_text does not hold the image's copy: fm1_cstart did not copy it"
    );
}

#[test]
fn the_audio_interrupt_runs_only_from_sram() {
    let Some((_, fw)) = build_and_firmware("the SRAM audio test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.key(A4_KEY, true);
    m.run_ms(500);
    common::run_past_boot(&mut m, &fw);
    press(&mut m, PLAY);
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

#[test]
fn the_audio_interrupt_runs_only_from_sram_while_a_tune_plays() {
    let Some((_, fw)) = build_and_firmware("the SRAM audio test") else {
        return;
    };
    let example = gt2_ref::examples()
        .into_iter()
        .find(|(n, _)| n == "dojo.sng")
        .unwrap()
        .1;
    for sid in [0xD400, 0xD420] {
        let file = sidou_tests::psid::packed(&example, sid);
        let mut flash = common::settled_flash(&fw.image);
        common::put_file(&mut flash, "TUNE", sidou_sys::STORE_FILE_SID, &file);
        let mut m = Machine::boot_blank(&fw.image);
        m.soc.flash.mem.copy_from_slice(&flash);
        common::run_past_boot(&mut m, &fw);
        common::ui_play_tune(&mut m, &fw, 0);
        common::ui_back_to(&mut m, &fw, "live_view_view");
        m.run_ms(700);
        m.watch_alnk_pcs(ram_text(&fw));
        m.run_ms(400);
        let ctl = fw.symbol("player_isr");
        let writes = m
            .soc
            .read32(ctl + sidou_sys::ilp32::psid_ctl::writes as u32);
        let blocks = m
            .soc
            .read32(ctl + sidou_sys::ilp32::psid_ctl::blocks as u32);
        assert!(
            writes > 100 && blocks > 100,
            "the tune did not play ({writes} writes)"
        );
        let outside: Vec<String> = m
            .alnk_pcs_outside
            .iter()
            .map(|&pc| fw.function_at(pc))
            .collect();
        println!(
            "a tune at ${sid:04X}: {blocks} blocks, {} instructions at the ALNK0 level from SRAM, \
             {} distinct PCs elsewhere",
            m.alnk_pcs_inside,
            outside.len()
        );
        assert!(
            outside.is_empty(),
            "the audio interrupt runs code outside .ram_text while a tune plays: {outside:?}"
        );
    }
}

const R_LONG_JUMP_23M2: u32 = 2;
const R_MOV_ABS32: u32 = 5;
const R_ABS32: u32 = 8;

#[test]
fn no_direct_branch_crosses_between_flash_and_sram() {
    use object::{
        Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget, SectionIndex,
    };
    let Some((_, fw)) = build_and_firmware("the SRAM audio test") else {
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

const XIP: std::ops::Range<u64> = 0x0200_0000..0x0300_0000;

#[test]
fn the_audio_path_reads_no_data_from_flash() {
    use object::{Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget};
    let Some((_, fw)) = build_and_firmware("the SRAM audio test") else {
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
            continue;
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

const CHORD: [u8; 3] = [note_key(57), note_key(64), note_key(76)];

fn per_sample(m: &mut Machine, ms: u64) -> (f64, usize) {
    let from = m.alnk_blocks.len();
    m.run_ms(ms);
    let b = &m.alnk_blocks[from..];
    (
        b.iter().sum::<u64>() as f64 / b.len() as f64 / 128.0,
        b.len(),
    )
}

#[test]
fn the_live_view_tap_costs_the_audio_interrupt_little() {
    let Some((_, fw)) = build_and_firmware("the tap cost test") else {
        return;
    };
    let mut m = booted(&fw);
    for k in CHORD {
        m.key(k, true);
    }
    m.run_ms(1500);
    let (mut on, mut off) = (Vec::new(), Vec::new());
    for _ in 0..3 {
        assert!(common::tap_on(&mut m, &fw), "the tap is off under LIVE");
        on.push(per_sample(&mut m, 400));
        press(&mut m, HOME);
        m.run_ms(400);
        assert!(!common::tap_on(&mut m, &fw), "the tap runs under the menu");
        off.push(per_sample(&mut m, 400));
        press(&mut m, HOME);
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
