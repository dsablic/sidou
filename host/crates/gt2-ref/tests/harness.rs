// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The reference harness: GoatTracker 2.77's packer and its packed player in the host 6502.

use gt2_ref::cpu::{Mos6502, Ram};
use gt2_ref::trace::{run, trace, Setup, TraceError};
use gt2_ref::*;

const FRAMES: usize = 3000;

#[test]
fn every_example_traces_without_a_trap() {
    let o = Options::default();
    let mut report = Vec::new();
    for (name, sng) in examples() {
        for sub in 0..subtunes(&sng) {
            let t = trace(&sng, sub as u8, FRAMES, &o)
                .unwrap_or_else(|e| panic!("{name} subtune {sub}: {e}"));
            assert_eq!(t.frames.len(), FRAMES);
            let writes: usize = t.frames.iter().map(|f| f.calls[0].len()).sum();
            assert!(writes > 0, "{name} {sub}: the player wrote nothing");
            report.push(format!("{name}/{sub}: {writes} writes"));
        }
    }
    eprintln!(
        "{} subtunes, {FRAMES} frames each:\n{}",
        report.len(),
        report.join("\n")
    );
}

#[test]
fn traces_are_stable_across_runs() {
    let o = Options::default();
    for (name, sng) in examples().into_iter().take(4) {
        let a = trace(&sng, 0, 200, &o).unwrap();
        let b = trace(&sng, 0, 200, &o).unwrap();
        assert_eq!(a, b, "{name}");
    }
}

#[test]
fn init_and_play_are_where_gt2reloc_says() {
    let (_, sng) = &examples()[2];
    for page in [0x10u32, 0x20, 0xc0] {
        let o = Options {
            player_address: page << 8,
            ..Options::default()
        };
        let p = pack(sng, &o).unwrap();
        // gt2reloc: "Player address: $1000"; init at the address, play at +3 (readme 5),
        // each a JMP into the player.
        assert_eq!(p.load as u32, page << 8);
        assert_eq!((p.init(), p.play()), (p.load, p.load + 3));
        assert_eq!(p.code[0], 0x4c);
        assert_eq!(p.code[3], 0x4c);
        let target = u16::from_le_bytes([p.code[1], p.code[2]]);
        assert!(target > p.load && (target as usize) < p.load as usize + p.code.len());
    }
}

#[test]
fn options_change_the_packed_player_as_they_should() {
    let (_, sng) = &examples()[2];
    let base = pack(sng, &Options::default()).unwrap();
    let with = |f: &dyn Fn(&mut Options)| {
        let mut o = Options::default();
        f(&mut o);
        pack(sng, &o).unwrap()
    };
    // Multispeed: the default tempo (and so the code) changes.
    assert_ne!(with(&|o| o.multiplier = 2).code, base.code);
    // Pulse and realtime optimisation, buffered writes, the hard restart ADSR: other code.
    assert_ne!(with(&|o| o.optimize_pulse = 0).code, base.code);
    assert_ne!(with(&|o| o.optimize_realtime = 0).code, base.code);
    assert_ne!(
        with(&|o| o.player_version = PLAYER_BUFFERED).code,
        base.code
    );
    assert_ne!(with(&|o| o.hr_adsr = 0x0a00).code, base.code);
    // A-4's pitch: another frequency table, the same length.
    let tuned = with(&|o| o.a4_hz = 432.0);
    assert_ne!(tuned.code, base.code);
    assert_eq!(tuned.code.len(), base.code.len());
    // NTSC: a packed .prg is the same; gt2reloc's NTSC only changes a .sid's header and
    // speed code, and the player has one frequency table (the editor's PAL one).
    assert_eq!(with(&|o| o.ntsc = 1), base);
}

#[test]
fn multispeed_calls_the_player_n_times_a_frame() {
    let (_, sng) = &examples()[0]; // 2xtest.sng, a 2x-speed song
    let o = Options {
        multiplier: 2,
        ..Options::default()
    };
    let t = trace(sng, 0, 100, &o).unwrap();
    assert!(t.frames.iter().all(|f| f.calls.len() == 2));
}

#[test]
fn zero_page_ghost_registers_are_copied_by_the_harness() {
    let (_, sng) = &examples()[2];
    let o = Options {
        player_version: PLAYER_ZPGHOSTREGS | PLAYER_BUFFERED,
        zeropage: 0xe0,
        ..Options::default()
    };
    let t = trace(sng, 0, 50, &o).unwrap();
    for f in &t.frames {
        let regs: Vec<u8> = f.calls[0].iter().map(|w| w.0).collect();
        assert_eq!(regs, (0..=0x18u8).rev().collect::<Vec<_>>());
    }
}

/// A hand-made player: init is RTS; play runs `op` at `$1010`.
fn hand_made(op: &[u8]) -> (Mos6502, Setup) {
    let mut ram = Ram::new(0xd400);
    ram.load(0x1000, &[0x4c, 0x06, 0x10, 0x4c, 0x10, 0x10, 0x60]);
    let mut play = vec![0xa9, 0x11, 0x8d, 0x04, 0xd4]; // LDA #$11, STA $D404
    play.extend_from_slice(op);
    play.push(0x60);
    ram.load(0x1010, &play);
    let setup = Setup {
        load: 0x1000,
        init: 0x1000,
        play: 0x1003,
        calls_per_frame: 1,
        ghost: None,
        sid: 0xd400,
        trap_sed: false,
    };
    (Mos6502::new(ram), setup)
}

#[test]
fn an_undocumented_opcode_fails_the_trace_with_its_address() {
    let (mut cpu, setup) = hand_made(&[0xea]);
    let t = run(&mut cpu, &setup, 0, 3).unwrap();
    assert_eq!(t.frames[2].calls[0], vec![(4, 0x11)]);
    let (mut cpu, setup) = hand_made(&[0xa7, 0x10]); // LAX $10
    assert_eq!(
        run(&mut cpu, &setup, 0, 3),
        Err(TraceError::Undocumented {
            opcode: 0xa7,
            address: 0x1015,
            frame: Some(0)
        })
    );
    // SED, when the harness is told to trap it.
    let (mut cpu, mut setup) = hand_made(&[0xf8, 0xd8]);
    assert!(run(&mut cpu, &setup, 0, 1).is_ok());
    setup.trap_sed = true;
    assert_eq!(
        run(&mut cpu, &setup, 0, 1),
        Err(TraceError::Decimal {
            address: 0x1015,
            frame: Some(0)
        })
    );
    // A call that never returns.
    let (mut cpu, setup) = hand_made(&[0x4c, 0x15, 0x10]);
    assert!(matches!(
        run(&mut cpu, &setup, 0, 1),
        Err(TraceError::Hang { frame: Some(0), .. })
    ));
}
