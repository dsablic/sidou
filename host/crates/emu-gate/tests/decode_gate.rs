// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::{build_dir, Firmware};
use fm1_core::{Bus, CoreError, Cpu};
use fm1_soc::{Soc, RAM_BASE, XIP_BASE};
use object::{Object, ObjectSection, ObjectSymbol, SectionFlags, SymbolKind};

const SHF_EXECINSTR: u64 = 4;

fn exec_status(soc: &mut Soc, cpu: &mut Cpu, pc: u32) -> Result<(), String> {
    cpu.pc = pc;
    cpu.pair_pending = None;
    cpu.stuck_on = None;
    cpu.sp = SCRATCH;
    cpu.regs.fill(SCRATCH);
    let mut res = cpu.step(soc);
    if res.is_ok() && cpu.pair_pending.is_some() {
        res = cpu.step(soc);
    }
    match res {
        Ok(()) => Ok(()),
        Err(CoreError::Unsupported { name, .. }) => {
            Err(format!("`{name}` decodes, has no semantics"))
        }
        Err(CoreError::MissingSlot { .. }) => Err("a slot is missing (no semantics)".into()),
        Err(_) => Ok(()),
    }
}

const SCRATCH: u32 = RAM_BASE + 0x20000;

#[test]
fn every_instruction_of_the_image_decodes() {
    let Some(build) = build_dir() else {
        common::skip_message("the decode gate");
        return;
    };
    let fw = Firmware::load(&build);
    let file = object::File::parse(&*fw.elf).expect("parse the firmware ELF");

    let mut starts: Vec<u64> = Vec::new();
    let mut ends: Vec<u64> = Vec::new();
    let mut funcs: Vec<(u64, u64, String)> = Vec::new();
    let mut labels: Vec<(u64, String)> = Vec::new();
    for s in file.symbols() {
        let name = s.name().unwrap_or("");
        if name.starts_with(".GJTIS") || name.starts_with(".GJTIL") {
            starts.push(s.address());
        } else if name.starts_with(".GJTIE") {
            ends.push(s.address());
        } else if matches!(s.kind(), SymbolKind::Text | SymbolKind::Unknown)
            && s.section_index().is_some()
            && !name.is_empty()
        {
            labels.push((s.address(), name.to_string()));
            if s.kind() == SymbolKind::Text && s.size() > 0 {
                funcs.push((s.address(), s.size(), name.to_string()));
            }
        }
    }
    starts.sort_unstable();
    starts.dedup();
    ends.sort_unstable();
    ends.dedup();
    let tables: Vec<(u64, u64)> = starts
        .iter()
        .map(|&a| {
            let e = ends.iter().copied().find(|&e| e > a);
            (
                a,
                e.unwrap_or_else(|| panic!("jump table at {a:#x} has no end marker")),
            )
        })
        .collect();
    funcs.sort();
    labels.sort();
    let label = |pc: u64| {
        let i = labels.partition_point(|l| l.0 <= pc);
        i.checked_sub(1)
            .map(|i| format!("{}+{:#x}", labels[i].1, pc - labels[i].0))
            .unwrap_or_else(|| "?".into())
    };
    let table_at = |a: u64| {
        tables
            .binary_search_by_key(&a, |t| t.0)
            .ok()
            .map(|i| tables[i].1)
    };

    let mut xip = Vec::new();
    let mut ram = vec![0u8; 0];
    for sec in file.sections() {
        let SectionFlags::Elf { sh_flags } = sec.flags() else {
            continue;
        };
        if sh_flags & SHF_EXECINSTR == 0 {
            continue;
        }
        let (a, d) = (sec.address() as u32, sec.data().expect("section data"));
        let (buf, base) = if a >= XIP_BASE {
            (&mut xip, XIP_BASE)
        } else {
            (&mut ram, RAM_BASE)
        };
        let o = (a - base) as usize;
        if buf.len() < o + d.len() {
            buf.resize(o + d.len(), 0);
        }
        buf[o..o + d.len()].copy_from_slice(d);
    }
    let mut soc = Soc::new(256 * 1024, xip);
    for (i, b) in ram.iter().enumerate() {
        soc.write8(RAM_BASE + i as u32, *b);
    }
    let mut cpu = Cpu::new(XIP_BASE);
    let mut failures: Vec<String> = Vec::new();
    let (mut insns, mut table_bytes, mut code_bytes, mut covered) = (0u64, 0u64, 0u64, 0u64);
    let mut allowed: Vec<String> = Vec::new();
    let mut tails: Vec<String> = Vec::new();

    for sec in file.sections() {
        let SectionFlags::Elf { sh_flags } = sec.flags() else {
            continue;
        };
        if sh_flags & SHF_EXECINSTR == 0 {
            continue;
        }
        let (base, data) = (sec.address(), sec.data().expect("section data"));
        let end = base + data.len() as u64;
        code_bytes += data.len() as u64;
        let mut ranges: Vec<(u64, u64, String)> = Vec::new();
        let mut at = base;
        let mut prev = String::from("the section's start");
        for f in funcs.iter().filter(|f| f.0 >= base && f.0 < end) {
            if f.0 > at {
                ranges.push((at, f.0 - at, "(code between two sized symbols)".to_string()));
            }
            ranges.push(f.clone());
            at = at.max(f.0 + f.1);
            prev = f.2.clone();
        }
        if ranges.is_empty() {
            ranges.push((
                base,
                data.len() as u64,
                sec.name().unwrap_or("?").to_string(),
            ));
        } else {
            tails.push(format!("{:#010x}..{end:#010x} after {prev}", at.min(end)));
        }
        for (start, size, name) in &ranges {
            let fend = (start + size).min(end);
            covered += fend - start;
            let mut pc = *start;
            while pc < fend {
                if let Some(e) = table_at(pc) {
                    table_bytes += e - pc;
                    pc = e;
                    continue;
                }
                let off = (pc - base) as usize;
                let mut win = 0u64;
                let take = (data.len() - off).min(8);
                for (k, b) in data[off..off + take].iter().enumerate() {
                    win |= u64::from(*b) << (8 * k);
                }
                insns += 1;
                match fm1_core::decode_info(win, pc as u32) {
                    None => {
                        failures.push(format!(
                            "{pc:#010x} {:02x?} in {}: not decoded",
                            &data[off..off + take.min(6)],
                            label(pc)
                        ));
                        pc += 2;
                    }
                    Some((len, class)) => {
                        let bytes = &data[off..off + len.min(data.len() - off)];
                        if let Err(why) = exec_status(&mut soc, &mut cpu, pc as u32) {
                            let line =
                                format!("{pc:#010x} {bytes:02x?} in {}: {class}: {why}", label(pc));
                            if class == "trigger" && label(pc).starts_with("fm1_fatal_common+") {
                                allowed.push(line);
                            } else {
                                failures.push(line);
                            }
                        }
                        pc += len as u64;
                    }
                }
            }
            if pc != fend && !failures.iter().any(|f| f.contains(&format!("in {name}+"))) {
                failures.push(format!(
                    "{name}: the walk ends at {pc:#010x}, not at the function's end {fend:#010x}"
                ));
            }
        }
    }
    for a in &allowed {
        println!("  allowed: {a}");
    }
    println!(
        "decode gate: {insns} instructions in {covered} of {code_bytes} code bytes, {table_bytes} bytes of jump tables skipped, not walked (data after the last function): {}",
        tails.join(", ")
    );
    assert!(
        insns > 10_000,
        "only {insns} instructions walked: the symbol table is not what the test expects"
    );
    assert!(
        failures.is_empty(),
        "{} of {insns} instructions the pinned fm1-emu cannot run:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_gate_sees_what_it_should() {
    assert_eq!(fm1_core::decode_info(u64::MAX, XIP_BASE), None);
    let mut xip = vec![0u8; 64];
    xip[..4].copy_from_slice(&[0x70, 0xe8, 0x00, 0x00]);
    let mut soc = Soc::new(256 * 1024, xip);
    let mut cpu = Cpu::new(XIP_BASE);
    let why = exec_status(&mut soc, &mut cpu, XIP_BASE).expect_err("trigger has no semantics");
    assert!(why.contains("trigger"), "{why}");
}
