// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The audio path runs from SRAM (`.ram_text`, placed by `platform/fm1/app.ld`): the boot
//! copies it there, the ALNK0 interrupt executes nothing else, and no direct call crosses
//! between flash and SRAM code (out of the pi32v2's call range).
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

/// A direct `call` or `goto` between flash (XIP, 0x02000000..) and SRAM code (0x01C00000..)
/// may be out of reach (R_PI32V2_LONG_JUMP_23M2): the link fails, or links only while the
/// code happens to lie close enough, and breaks with an unrelated change. So there are none:
/// such calls go through a pointer (audio.c `far_drain`). Scans the build's disassembly.
#[test]
fn no_direct_call_crosses_between_flash_and_sram() {
    let Some((build, _)) = fixture("the SRAM audio test") else {
        return;
    };
    let dis = common::read(
        &build.join("platform/fm1/sidou_app.dis"),
        "firmware disassembly",
    );
    let text = String::from_utf8_lossy(&dis);
    let lines: Vec<&str> = text.lines().collect();
    let in_sram = |a: u32| a < 0x0200_0000;
    let (mut function, mut scanned, mut crossing) = ("", 0, Vec::new());
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with(' ') && line.ends_with(':') && !line.contains(' ') {
            function = &line[..line.len() - 1];
            continue;
        }
        let Some((addr, rest)) = line.trim_start().split_once(':') else {
            continue;
        };
        let Ok(src) = u32::from_str_radix(addr, 16) else {
            continue;
        };
        // Direct ones only, `call 2712 <...>` or `if (...) goto -40 <...>`; `call r3` goes
        // through a register.
        let words: Vec<&str> = rest.split_whitespace().collect();
        let direct = words
            .windows(2)
            .any(|w| (w[0] == "call" || w[0] == "goto") && w[1].parse::<i64>().is_ok());
        if !direct {
            continue;
        }
        // The target, `<name : address >`, is on this line or, inside a predicated block,
        // on one of the next few.
        let target = lines[i..lines.len().min(i + 4)].iter().find_map(|l| {
            let t = &l[l.find('<')? + 1..];
            let (name, addr) = t[..t.find('>')?].split_once(':')?;
            Some((
                name.trim().to_owned(),
                u32::from_str_radix(addr.trim(), 16).ok()?,
            ))
        });
        let (name, dst) =
            target.unwrap_or_else(|| panic!("no target for {function}: {}", line.trim()));
        scanned += 1;
        if in_sram(src) != in_sram(dst) {
            crossing.push(format!("{function} -> {name}"));
        }
    }
    println!(
        "{scanned} direct calls and jumps, {} crossing",
        crossing.len()
    );
    assert!(
        scanned > 500,
        "only {scanned} calls found: the disassembly format changed?"
    );
    assert!(
        crossing.is_empty(),
        "direct calls between flash and SRAM code: {crossing:?}"
    );
}
