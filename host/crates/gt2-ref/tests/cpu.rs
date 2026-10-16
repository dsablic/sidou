// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The host 6502: Klaus Dormann's functional test (third_party/6502-tests), decimal mode
//! included, and the documented-opcode table the harness traps with.

use gt2_ref::cpu::{documented, Cpu6502, Mos6502, Ram, Regs};

/// The test's "jmp *" on success, from its listing (6502_functional_test.lst, line 13377).
const SUCCESS: u16 = 0x3469;

#[test]
fn documented_opcodes_are_151() {
    assert_eq!((0..=255u8).filter(|&o| documented(o)).count(), 151);
    // A few undocumented ones (LAX, SAX, the KIL/JAM opcodes, the extra NOPs).
    for op in [0x02, 0xa7, 0x87, 0x1a, 0x80, 0xeb, 0xcb] {
        assert!(!documented(op), "{op:02X}");
    }
}

#[test]
fn dormanns_functional_test_passes_decimal_mode_included() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../third_party/6502-tests/6502_functional_test.bin"
    );
    let bin = std::fs::read(path).expect("6502_functional_test.bin");
    assert_eq!(bin.len(), 0x10000);
    // The listing: assembled with disable_decimal = 0, so ADC and SBC are tested in decimal
    // mode too.
    let lst = std::fs::read_to_string(path.replace(".bin", ".lst")).unwrap();
    assert!(lst.contains("0000 =                  disable_decimal = 0"));
    let mut ram = Ram::new(0xd400);
    ram.load(0, &bin);
    let mut cpu = Mos6502::new(ram);
    cpu.set_regs(Regs {
        sp: 0xff,
        p: 0x24,
        pc: 0x0400,
        ..Regs::default()
    });
    let mut last = 0xffff;
    let mut steps = 0u64;
    loop {
        let pc = cpu.regs().pc;
        if pc == SUCCESS {
            break;
        }
        assert_ne!(
            pc, last,
            "trapped at ${pc:04X} (see the listing) after {steps}"
        );
        last = pc;
        assert!(documented(cpu.ram().bytes[pc as usize]), "at ${pc:04X}");
        assert!(cpu.step(), "undecoded opcode at ${pc:04X}");
        steps += 1;
        assert!(steps < 200_000_000, "no end");
    }
    eprintln!("Dormann's 6502 functional test passed in {steps} instructions");
}
