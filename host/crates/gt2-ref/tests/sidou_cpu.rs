// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's 6502 (core/cpu6502) under GoatTracker's packed player (M3 Task 3): every example
//! song, subtune and option set the harness runs, on `mos6502` and on SIDOU's core in each
//! of its three builds; the ordered SID writes of every call, the cycles of every call, the
//! registers it returns with and the whole memory at the end are equal.

use gt2_ref::cpu::{Access, Cpu6502, Mos6502, Ram, Regs, Sidou6502};
use gt2_ref::trace::cross_check;
use gt2_ref::*;

const ALL: [Access; 3] = [Access::HookAll, Access::Device, Access::HookInline];

fn sets() -> Vec<(&'static str, Options)> {
    let d = Options::default;
    vec![
        ("", d()),
        (
            "-AF000",
            Options {
                hr_adsr: 0xf000,
                ..d()
            },
        ),
        (
            "-B1",
            Options {
                player_version: PLAYER_BUFFERED,
                ..d()
            },
        ),
        (
            "-J1",
            Options {
                player_version: PLAYER_FULLBUFFERED,
                ..d()
            },
        ),
        (
            "-D1 -E1",
            Options {
                player_version: PLAYER_SOUNDEFFECTS | PLAYER_VOLUME,
                ..d()
            },
        ),
        (
            "-I0 -O0 -R0",
            Options {
                player_version: PLAYER_NOOPTIMIZATION,
                optimize_pulse: 0,
                optimize_realtime: 0,
                ..d()
            },
        ),
        (
            "-S2",
            Options {
                multiplier: 2,
                ..d()
            },
        ),
        (
            "-S4",
            Options {
                multiplier: 4,
                ..d()
            },
        ),
        (
            "zp ghost",
            Options {
                player_version: PLAYER_ZPGHOSTREGS | PLAYER_BUFFERED,
                zeropage: 0xe0,
                ..d()
            },
        ),
        (
            "zp ghost -AF000",
            Options {
                hr_adsr: 0xf000,
                player_version: PLAYER_ZPGHOSTREGS | PLAYER_BUFFERED,
                zeropage: 0xe0,
                ..d()
            },
        ),
        (
            "-G432",
            Options {
                a4_hz: 432.0,
                ..d()
            },
        ),
        (
            "-W20",
            Options {
                player_address: 0x2000,
                ..d()
            },
        ),
        (
            "-LD420",
            Options {
                sid_address: 0xd420,
                ..d()
            },
        ),
    ]
}

#[test]
fn every_example_matches_on_sidou_s_6502_in_every_option_set() {
    let mut songs = 0;
    let mut calls = 0;
    let mut timed = 0;
    let mut writes = 0;
    for (flags, o) in sets() {
        let accesses: &[Access] = if (0xd000..=0xd7ff).contains(&o.sid_address) {
            &ALL
        } else {
            &[Access::HookAll, Access::HookInline]
        };
        for (name, sng) in examples() {
            let subs = if flags.is_empty() { subtunes(&sng) } else { 1 };
            for sub in 0..subs {
                let c = cross_check(&sng, sub as u8, 3000, &o, accesses)
                    .unwrap_or_else(|e| panic!("{flags:?} {name}/{sub}: {e}"));
                songs += 1;
                calls += c.calls;
                timed += c.timed_calls;
                writes += c.writes;
            }
        }
    }
    eprintln!("{songs} song runs, {calls} calls ({timed} with cycles compared), {writes} writes");
    assert!(writes > 0 && timed == calls);
}

/// `mos6502` takes 8 cycles for INC abs,X across a page; the NMOS 6502 takes 7 (Tom Harte's
/// tests, and SIDOU's core). The wrapper takes the extra cycle back; JAM is not decodable.
#[test]
fn rmw_absolute_x_is_seven_cycles_on_both() {
    // LDX #$FF / INC $10F0,X (crosses) / INC $1000,X (does not cross for X=0 after INX)
    let code = [0xa2, 0xff, 0xfe, 0xf1, 0x10, 0xe8, 0xfe, 0x00, 0x20, 0x02];
    let mk = || {
        let mut ram = Ram::new(0xd400);
        ram.load(0x0200, &code).unwrap();
        ram
    };
    let regs = Regs {
        sp: 0xfd,
        p: 0x24,
        pc: 0x0200,
        ..Regs::default()
    };
    let mut cpus: Vec<Box<dyn Cpu6502>> = vec![
        Box::new(Mos6502::new(mk())),
        Box::new(Sidou6502::new(mk(), Access::HookAll)),
        Box::new(Sidou6502::new(mk(), Access::Device)),
    ];
    for c in &mut cpus {
        c.set_regs(regs);
        let mut got = Vec::new();
        for _ in 0..4 {
            let before = c.cycles().unwrap();
            if !c.step() {
                got.push(0);
                break;
            }
            got.push(c.cycles().unwrap() - before);
        }
        assert_eq!(got, vec![2, 7, 2, 7]);
    }
    let mut jam = Sidou6502::new(mk(), Access::HookAll);
    jam.set_regs(Regs { pc: 0x0209, ..regs });
    assert!(!jam.step(), "JAM is not an instruction the harness runs");
}
