// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The reference harness: GoatTracker 2.77's packer and its packed player in the host 6502.

use gt2_ref::cpu::{Cpu6502, Mos6502, Ram, Regs};
use gt2_ref::trace::{run, trace, Setup, Trace, TraceError, Writes};
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
    ram.load(0x1000, &[0x4c, 0x06, 0x10, 0x4c, 0x10, 0x10, 0x60])
        .unwrap();
    let mut play = vec![0xa9, 0x11, 0x8d, 0x04, 0xd4]; // LDA #$11, STA $D404
    play.extend_from_slice(op);
    play.push(0x60);
    ram.load(0x1010, &play).unwrap();
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

// The review's regressions (M2c Tasks 1-3).

#[test]
fn a_song_goattracker_cannot_load_is_refused() {
    let (_, sng) = &examples()[2];
    assert!(pack(sng, &Options::default()).is_ok());
    assert!(canonical(sng).is_ok());
    // A bad ident: loadsong reads nothing and leaves the cleared song, which would pack.
    let mut bad = sng.clone();
    bad[..4].copy_from_slice(b"GTX5");
    assert!(pack(&bad, &Options::default()).is_err(), "bad ident packed");
    assert!(canonical(&bad).is_err(), "bad ident canonicalised");
    assert!(pack(&[], &Options::default()).is_err(), "empty file packed");
    // A short file: loadsong's reads run past the end and read zeros.
    for cut in [3, 50, 101, sng.len() / 2, sng.len() - 1] {
        let short = &sng[..cut];
        assert!(
            pack(short, &Options::default()).is_err(),
            "cut {cut} packed"
        );
        assert!(canonical(short).is_err(), "cut {cut} canonicalised");
    }
    // Every ident loadsong takes still loads, and so does a file with bytes after the song
    // (a SIDOU trailer).
    let mut longer = sng.clone();
    longer.extend_from_slice(b"SIDOU...trailer");
    assert!(pack(&longer, &Options::default()).is_ok());
    for (name, sng) in examples() {
        assert!(canonical(&sng).is_ok(), "{name}");
    }
}

#[test]
fn code_over_the_sid_the_stack_slot_or_the_trampoline_is_refused() {
    let (_, sng) = &examples()[2];
    let len = pack(sng, &Options::default()).unwrap().code.len() as u32;
    let at = |page: u32| Options {
        player_address: page << 8,
        ..Options::default()
    };
    // -Wd0: the code runs into $D400.
    for page in [0xd0u32, 0xd4, 0xd7] {
        let r = trace(sng, 0, 2, &at(page));
        assert!(
            matches!(r, Err(TraceError::Placement(_))),
            "-W{page:02x}: {r:?}"
        );
    }
    // The return address the harness pushes, at $01FE-$01FF.
    let r = trace(sng, 0, 2, &at(0x01));
    assert!(matches!(r, Err(TraceError::Placement(_))), "-W01: {r:?}");
    // Up to the trampoline at $FFF0, or past $FFFF.
    let top = (0xfff0 - len) >> 8;
    for page in [top + 1, 0xff] {
        let r = trace(sng, 0, 2, &at(page));
        assert!(
            matches!(r, Err(TraceError::Placement(_)) | Err(TraceError::Pack(_))),
            "-W{page:02x}: {r:?}"
        );
    }
    // Clear of them all, it traces: below the SID, above it, and just below the trampoline.
    for page in [0x02u32, 0xd4 - len.div_ceil(256) - 1, 0xd8, top] {
        trace(sng, 0, 2, &at(page)).unwrap_or_else(|e| panic!("-W{page:02x}: {e}"));
    }
    // A SID moved with -L: code over its registers is refused too.
    let o = Options {
        sid_address: 0x1020,
        ..Options::default()
    };
    assert!(matches!(
        trace(sng, 0, 2, &o),
        Err(TraceError::Placement(_))
    ));
}

#[test]
fn ram_refuses_a_load_past_ffff() {
    let mut ram = Ram::new(0xd400);
    assert!(ram.load(0xfff0, &[1; 16]).is_ok());
    assert_eq!(ram.bytes[0xffff], 1);
    let mut ram = Ram::new(0xd400);
    assert!(ram.load(0xfff0, &[1; 17]).is_err());
    assert!(ram.bytes.iter().all(|&b| b == 0), "a refused load wrote");
    assert!(ram.load(0, &vec![2; 0x10000]).is_ok());
    assert!(ram.load(1, &vec![2; 0x10000]).is_err());
}

/// The alternative player's copy order (readme 5.1): each channel's wave, frequency, pulse
/// width, then ADSR; the filter's registers, which the player computes before the
/// channels, first.
const ALT_ORDER: [u8; 25] = [
    0x15, 0x16, 0x17, 0x18, 0x04, 0x00, 0x01, 0x02, 0x03, 0x05, 0x06, 0x0b, 0x07, 0x08, 0x09, 0x0a,
    0x0c, 0x0d, 0x12, 0x0e, 0x0f, 0x10, 0x11, 0x13, 0x14,
];

#[test]
fn the_alternative_players_ghost_registers_are_copied_channel_by_channel() {
    let mut sorted = ALT_ORDER;
    sorted.sort();
    assert_eq!(sorted.to_vec(), (0..=0x18u8).collect::<Vec<_>>());
    let (_, sng) = &examples()[2];
    for hr in [0xf000u32, 0xf0ff, 0xff00] {
        let o = Options {
            hr_adsr: hr,
            player_version: PLAYER_ZPGHOSTREGS | PLAYER_BUFFERED,
            zeropage: 0xe0,
            ..Options::default()
        };
        let t = trace(sng, 0, 50, &o).unwrap();
        for f in &t.frames {
            let regs: Vec<u8> = f.calls[0].iter().map(|w| w.0).collect();
            assert_eq!(regs, ALT_ORDER, "-A{hr:04X}");
        }
    }
    // Below $F000, the reverse loop stands.
    let o = Options {
        hr_adsr: 0xef00,
        player_version: PLAYER_ZPGHOSTREGS | PLAYER_BUFFERED,
        zeropage: 0xe0,
        ..Options::default()
    };
    let t = trace(sng, 0, 5, &o).unwrap();
    let regs: Vec<u8> = t.frames[0].calls[0].iter().map(|w| w.0).collect();
    assert_eq!(regs, (0..=0x18u8).rev().collect::<Vec<_>>());
}

/// A core that cannot decode anything: the harness reports it as such, not as a hang.
struct Undecoding(Mos6502);

impl Cpu6502 for Undecoding {
    fn regs(&self) -> Regs {
        self.0.regs()
    }
    fn set_regs(&mut self, r: Regs) {
        self.0.set_regs(r)
    }
    fn step(&mut self) -> bool {
        false
    }
    fn ram(&self) -> &Ram {
        self.0.ram()
    }
    fn ram_mut(&mut self) -> &mut Ram {
        self.0.ram_mut()
    }
    fn cycles(&self) -> Option<u64> {
        None
    }
}

#[test]
fn an_opcode_the_core_cannot_decode_is_not_a_hang() {
    let (cpu, setup) = hand_made(&[0xea]);
    let mut cpu = Undecoding(cpu);
    let e = run(&mut cpu, &setup, 0, 1).unwrap_err();
    assert!(!matches!(e, TraceError::Hang { .. }), "{e:?}");
    assert!(e.to_string().contains("cannot decode"), "{e}");
}

/// FNV-1a over a trace, for pinning one.
fn digest(t: &Trace) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut eat = |b: u8| {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    };
    let mut writes = |w: &Writes| {
        for &(r, v) in w {
            eat(r);
            eat(v);
        }
        eat(0xff);
    };
    writes(&t.init);
    for f in &t.frames {
        for c in &f.calls {
            writes(c);
        }
    }
    h
}

#[test]
fn multispeed_calls_differ_from_1x_and_each_writes() {
    let (_, sng) = &examples()[0]; // 2xtest.sng
    let one = trace(sng, 0, 100, &Options::default()).unwrap();
    for m in [2u32, 4] {
        let o = Options {
            multiplier: m,
            ..Options::default()
        };
        let t = trace(sng, 0, 100, &o).unwrap();
        assert!(t.frames.iter().all(|f| f.calls.len() == m as usize));
        for k in 0..m as usize {
            let writes: usize = t.frames.iter().map(|f| f.calls[k].len()).sum();
            assert!(writes > 0, "{m}x: call {k} never writes");
        }
        assert_ne!(digest(&t), digest(&one), "{m}x traces as 1x");
        let calls: Vec<&Writes> = t.frames.iter().flat_map(|f| &f.calls).collect();
        let once: Vec<&Writes> = one.frames.iter().flat_map(|f| &f.calls).collect();
        assert_ne!(
            calls[..100],
            once[..],
            "{m}x's first 100 calls are 1x's frames"
        );
    }
}

/// The option sets the review ran by hand, as gt2reloc's flags, each traced over every
/// example's first subtune (300 frames) without a trap, and pinned: a digest of all the
/// traces per set. A change to the packer, the assembler, the core or the harness that
/// changes any frame changes a digest.
#[test]
fn option_matrix_traces_are_pinned() {
    let d = Options::default;
    let sets: Vec<(&str, Options, u64)> = vec![
        ("", d(), 0x38fb_2025_af83_4802),
        (
            "-AF000",
            Options {
                hr_adsr: 0xf000,
                ..d()
            },
            0x82c9_09fe_61fb_ce3b,
        ),
        (
            "-B1",
            Options {
                player_version: PLAYER_BUFFERED,
                ..d()
            },
            0x768f_c80b_5034_1c38,
        ),
        (
            "-J1",
            Options {
                player_version: PLAYER_FULLBUFFERED,
                ..d()
            },
            0x6152_857c_0d7d_64d2,
        ),
        (
            "-D1 -E1",
            Options {
                player_version: PLAYER_SOUNDEFFECTS | PLAYER_VOLUME,
                ..d()
            },
            0x3086_c0ca_ab14_c62d,
        ),
        (
            "-I0 -O0 -R0",
            Options {
                player_version: PLAYER_NOOPTIMIZATION,
                optimize_pulse: 0,
                optimize_realtime: 0,
                ..d()
            },
            0xc2f9_9f30_30ee_c3cc,
        ),
        (
            "-S2",
            Options {
                multiplier: 2,
                ..d()
            },
            0x8bda_301e_bfc9_0418,
        ),
        (
            "-S4",
            Options {
                multiplier: 4,
                ..d()
            },
            0xe2ae_c743_ce76_fa2f,
        ),
    ];
    let mut got = Vec::new();
    let mut digests = std::collections::HashSet::new();
    for (flags, o, want) in &sets {
        let mut h = 0u64;
        for (name, sng) in examples() {
            let t = trace(&sng, 0, 300, o).unwrap_or_else(|e| panic!("{flags} {name}: {e}"));
            let writes: usize = t.frames.iter().flat_map(|f| &f.calls).map(Vec::len).sum();
            assert!(writes > 0, "{flags} {name}: no writes");
            h = h.rotate_left(5) ^ digest(&t);
        }
        got.push(format!("{flags:?}: {h:#018x}"));
        assert!(digests.insert(h), "{flags}: traces as another set does");
        assert_eq!(h, *want, "{flags}: the traces changed ({h:#018x})");
    }
    eprintln!("{}", got.join("\n"));
}
