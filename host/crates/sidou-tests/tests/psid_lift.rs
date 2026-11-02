// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use gt2_ref::trace::trace;
use gt2_ref::Options;
use sidou_sys::*;
use sidou_tests::player::{default_options, Player};
use sidou_tests::psid;
use sidou_tests::song::{SngSpec, Song};

const VOICE_REGISTERS: usize = 7;
const CONTROL: usize = 4;
const AD: usize = 5;
const SR: usize = 6;
const CUTOFF_HI: usize = 8;
const RES_FILT: usize = 9;
const MODE_VOL: usize = 10;
const PULSE_WAVE: u8 = 0x40;
const CHIP_REGISTERS: usize = 25;
const GATE: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrameRegs([u8; 11]);

impl FrameRegs {
    fn new(freq: u16, pw: u16, ctrl: u8, ad: u8, sr: u8) -> FrameRegs {
        FrameRegs([
            freq as u8,
            (freq >> 8) as u8,
            pw as u8,
            (pw >> 8) as u8,
            ctrl,
            ad,
            sr,
            0,
            0,
            0,
            0x0f,
        ])
    }
    fn freq(&self) -> u16 {
        u16::from_le_bytes([self.0[0], self.0[1]])
    }
    fn set_freq(mut self, f: u16) -> FrameRegs {
        self.0[0] = f as u8;
        self.0[1] = (f >> 8) as u8;
        self
    }
    fn pw(&self) -> u16 {
        u16::from_le_bytes([self.0[2], self.0[3]]) & 0x0fff
    }
    fn set_pw(mut self, p: u16) -> FrameRegs {
        self.0[2] = p as u8;
        self.0[3] = (p >> 8) as u8;
        self
    }
}

struct Lift(Box<psid_lift>, #[allow(dead_code)] Box<psid_lift_past>);

impl Lift {
    fn new() -> Lift {
        Lift::of_chips(1)
    }
    fn of_chips(chips: u32) -> Lift {
        let mut past = Box::<psid_lift_past>::default();
        let mut l = Box::<psid_lift>::default();
        unsafe { psid_lift_init(&mut *l, &mut *past, chips) };
        Lift(l, past)
    }
    fn take(&mut self, voice: u32) -> u32 {
        unsafe { psid_lift_take(&mut *self.0, voice) as u32 }
    }
    fn write(&mut self, chip: u32, reg: u32, value: u8) {
        unsafe { psid_lift_write(&mut *self.0, chip, reg, u32::from(value)) };
    }
    fn call(&mut self) {
        unsafe { psid_lift_call(&mut *self.0) };
    }
    fn frame(&mut self, v: usize, r: FrameRegs) {
        for reg in [5usize, 6, 2, 3, 0, 1, 4] {
            self.write(0, (v * VOICE_REGISTERS + reg) as u32, r.0[reg]);
        }
        for k in 0..4 {
            self.write(0, 0x15 + k, r.0[7 + k as usize]);
        }
        self.call();
    }
    fn state(&self) -> u32 {
        u32::from(self.0.state)
    }
    fn convert(&mut self, ntsc: bool) -> Result<(Vec<u8>, u32), i32> {
        let mut flags = 0u32;
        let name = c"TEST V1";
        let mut n = PSID_LIFT_AGAIN as i32;
        while n == PSID_LIFT_AGAIN as i32 {
            n = unsafe {
                psid_lift_convert(&mut *self.0, i32::from(ntsc), name.as_ptr(), &mut flags)
            };
        }
        if n < 0 {
            Err(n)
        } else {
            Ok((self.0.file[..n as usize].to_vec(), flags))
        }
    }
}

struct Gti {
    ins: [u8; 25],
    tables: Vec<Vec<(u8, u8)>>,
}

fn parse(f: &[u8]) -> Gti {
    assert_eq!(&f[..4], b"GTI5");
    let mut ins = [0u8; 25];
    ins.copy_from_slice(&f[4..29]);
    let mut at = 29;
    let mut tables = Vec::new();
    for _ in 0..4 {
        let n = f[at] as usize;
        at += 1;
        tables.push((0..n).map(|i| (f[at + i], f[at + n + i])).collect());
        at += 2 * n;
    }
    assert_eq!(at, f.len(), "the file's length");
    Gti { ins, tables }
}

fn note_freq(n: u8) -> u16 {
    unsafe { u16::from(gt2_freq_lo[n as usize]) | u16::from(gt2_freq_hi[n as usize]) << 8 }
}

const C4: u8 = 48;

fn lifted_after_a_gate_off_frame(frames: &[FrameRegs], v: usize) -> Lift {
    let mut l = Lift::new();
    l.take(v as u32);
    l.frame(v, FrameRegs::new(note_freq(C4 - 5), 0, 0x40, 0x0f, 0x00));
    for r in frames {
        l.frame(v, *r);
    }
    l
}

fn song_of(g: &Gti, note: u8) -> Vec<u8> {
    let mut s = SngSpec::new();
    s.instr = vec![g.ins];
    s.tables = g.tables.clone();
    let mut p0 = vec![[0xbdu8, 0, 0, 0]; 64];
    p0[0] = [0x60 + note, 1, 0, 0];
    s.patterns[0].0 = p0;
    s.encode()
}

fn sidou_player_regs_from_the_gate_on(
    sng: &[u8],
    o: &gt2_options,
    n: usize,
) -> (Vec<[u8; CHIP_REGISTERS]>, bool) {
    let mut song = Song::host();
    song.read(sng, 0).map_err(|i| i.msg).unwrap();
    let mut p = Player::new(&song, 0, o).expect("the player");
    let simple = u32::from(p.shape().flags) & GT2_PF_SIMPLEPULSE != 0;
    let mut regs = [0u8; CHIP_REGISTERS];
    let mut out = Vec::new();
    for _ in 0..2000 {
        let was = regs[CONTROL];
        for (r, v) in p.call() {
            regs[r as usize] = v;
        }
        if !out.is_empty() || (was & GATE == 0 && regs[CONTROL] & GATE == GATE) {
            out.push(regs);
            if out.len() == n {
                return (out, simple);
            }
        }
    }
    panic!("no gate-on");
}

fn as_simple_pulse_writes_it(pw: u16, simple: bool) -> u16 {
    if simple {
        (pw & 0xff0) | (pw >> 8)
    } else {
        pw
    }
}

fn ref_options(o: &gt2_options) -> Options {
    Options {
        hr_adsr: u32::from(o.hr_adsr),
        multiplier: u32::from(o.multispeed),
        ntsc: u32::from(o.ntsc),
        finevibrato: u32::from(o.finevibrato),
        optimize_pulse: u32::from(o.optimize_pulse),
        optimize_realtime: u32::from(o.optimize_realtime),
        player_version: 0,
        sid_address: u32::from(o.sid_address),
        player_address: u32::from(o.player_page) << 8,
        zeropage: u32::from(o.zeropage),
        sid_model: u32::from(o.sid_model),
        ..Options::default()
    }
}

fn assert_same_as_goattracker(sng: &[u8], o: &gt2_options, frames: usize) {
    assert_eq!(o.player_flags, default_options().player_flags);
    let t = trace(sng, 0, frames, &ref_options(o)).expect("trace");
    let mut song = Song::host();
    song.read(sng, 0).map_err(|i| i.msg).unwrap();
    let mut p = Player::new(&song, 0, o).expect("the player");
    for (f, frame) in t.frames.iter().enumerate() {
        for want in &frame.calls {
            let got = p.call();
            let unknown = p.raw().freq_unknown;
            let known = |r: u8| !(r < 21 && r % 7 < 2 && unknown & (1 << (r / 7)) != 0);
            assert_eq!(got.len(), want.len(), "frame {f}: {got:02X?} {want:02X?}");
            for (g, w) in got.iter().zip(want) {
                assert!(
                    g.0 == w.0 && (g.1 == w.1 || !known(w.0)),
                    "frame {f}: {got:02X?} {want:02X?}"
                );
            }
        }
    }
}

fn assert_plays_back_as_captured(frames: &[FrameRegs], g: &Gti, base: u8) {
    let (got, simple) = sidou_player_regs_from_the_gate_on(
        &song_of(g, base),
        &options_playing_as_captured(),
        frames.len(),
    );
    for (k, (want, have)) in frames.iter().zip(&got).enumerate() {
        let h = FrameRegs([
            have[0], have[1], have[2], have[3], have[4], have[5], have[6], have[21], have[22],
            have[23], have[24],
        ]);
        assert_eq!(
            (h.0[CONTROL], h.0[AD], h.0[SR]),
            (want.0[CONTROL], want.0[AD], want.0[SR]),
            "frame {k}: control, AD, SR"
        );
        if k > 0 {
            if h.freq() != want.freq() {
                let all: Vec<(u16, u16)> = frames
                    .iter()
                    .zip(&got)
                    .map(|(w, g)| (w.freq(), u16::from_le_bytes([g[0], g[1]])))
                    .collect();
                println!("{all:?}");
            }
            assert_eq!(h.freq(), want.freq(), "frame {k}: frequency");
            if want.0[CONTROL] & PULSE_WAVE != 0 {
                assert_eq!(
                    h.pw(),
                    as_simple_pulse_writes_it(want.pw(), simple),
                    "frame {k}: pulse"
                );
            }
        }
    }
}

fn options_playing_as_captured() -> gt2_options {
    let mut o = default_options();
    o.optimize_pulse = 0;
    o.optimize_realtime = 0;
    o
}

const ON: FrameRegs = FrameRegs([0x67, 0x11, 0x00, 0x08, 0x41, 0x09, 0xa0, 0, 0, 0, 0x0f]);
const OFF: FrameRegs = FrameRegs([0x67, 0x11, 0x00, 0x08, 0x40, 0x09, 0xa0, 0, 0, 0, 0x0f]);

fn with_ad(r: FrameRegs, ad: u8) -> FrameRegs {
    let mut r = r;
    r.0[AD] = ad;
    r
}

fn calls(l: &mut Lift, v: usize, r: FrameRegs, n: usize) {
    for _ in 0..n {
        l.frame(v, r);
    }
}

#[test]
fn the_capture_starts_at_the_gate_on_and_ends_at_the_gate_off() {
    let mut l = Lift::new();
    assert_eq!(l.take(1), PSID_LIFT_ARMED, "no note yet: it waits");
    l.frame(0, ON);
    l.frame(1, FrameRegs([0; 11]));
    assert_eq!(l.state(), PSID_LIFT_ARMED);
    calls(&mut l, 1, ON, 5);
    assert_eq!(l.state(), PSID_LIFT_TAKING);
    assert_eq!(l.0.frames, 5);
    l.frame(1, FrameRegs([0x40; 11]));
    assert_eq!(l.state(), PSID_LIFT_TAKEN);
    assert_eq!(l.0.frames, 5);
    assert_eq!(l.0.frame[0][4], 0x41);
    assert_eq!(l.0.frame[4][5], 0x09);
}

#[test]
fn the_capture_ends_at_the_next_gate_on_or_after_a_second() {
    let mut l = Lift::new();
    l.take(2);
    calls(&mut l, 2, ON, 2);
    l.write(0, 2 * 7 + 4, 0x40);
    l.write(0, 2 * 7 + 4, 0x41);
    l.call();
    assert_eq!((l.state(), l.0.frames), (PSID_LIFT_TAKEN, 2));
    let mut l = Lift::new();
    l.take(0);
    calls(&mut l, 0, ON, 80);
    assert_eq!(
        (l.state(), u32::from(l.0.frames)),
        (PSID_LIFT_TAKEN, PSID_LIFT_FRAMES)
    );
}

#[test]
fn a_take_lifts_the_voices_most_recent_note_from_the_history() {
    let mut l = Lift::new();
    calls(&mut l, 2, ON, 6);
    l.frame(2, OFF);
    let before = OFF.set_freq(note_freq(C4 - 5));
    l.frame(2, before);
    calls(&mut l, 2, with_ad(ON, 0x3c), 4);
    calls(&mut l, 2, OFF, 3);
    assert_eq!(l.take(2), PSID_LIFT_TAKEN, "ended: taken at once");
    assert_eq!(l.0.frames, 4);
    assert_eq!(l.0.frame[0][AD], 0x3c, "the last note, not the one before");
    assert_eq!(&l.0.before, &[before.0[0], before.0[1]]);
    assert!(l.convert(false).is_ok());
}

#[test]
fn a_sounding_note_is_followed_live_to_its_end() {
    let mut l = Lift::new();
    l.frame(0, OFF);
    calls(&mut l, 0, ON, 3);
    assert_eq!((l.take(0), l.0.frames), (PSID_LIFT_TAKING, 3));
    calls(&mut l, 0, ON, 2);
    l.frame(0, OFF);
    assert_eq!((l.state(), l.0.frames), (PSID_LIFT_TAKEN, 5));
}

#[test]
fn a_voice_silent_in_the_history_is_taken_at_its_next_note() {
    let mut l = Lift::new();
    calls(&mut l, 0, ON, 10);
    assert_eq!(l.take(1), PSID_LIFT_ARMED);
    calls(&mut l, 1, ON, 2);
    assert_eq!((l.state(), l.0.frames), (PSID_LIFT_TAKING, 2));
    assert_eq!(l.take(PSID_LIFT_NO_VOICE), PSID_LIFT_OFF, "cancelled");
}

#[test]
fn the_history_holds_the_last_calls_of_every_chip() {
    for chips in [1u32, 2] {
        let depth = PSID_LIFT_PAST / chips;
        for (held, kept) in [(depth, true), (depth + 1, false)] {
            let mut l = Lift::of_chips(chips);
            l.frame(0, OFF);
            calls(&mut l, 0, ON, held as usize);
            assert_eq!(
                l.take(0) != PSID_LIFT_ARMED,
                kept,
                "{chips} chips, a note {held} calls old"
            );
        }
    }
    let mut l = Lift::new();
    calls(&mut l, 0, ON, 3);
    unsafe { psid_lift_forget(&mut *l.0) };
    assert_eq!(l.take(0), PSID_LIFT_ARMED, "forgotten");
}

#[test]
fn a_2sid_tune_lifts_the_more_recent_note_of_either_chip() {
    let gate = |l: &mut Lift, chip: u32| l.write(chip, 4, 0x11);
    let mut l = Lift::of_chips(2);
    gate(&mut l, 1);
    l.call();
    gate(&mut l, 0);
    l.call();
    assert_eq!(
        (l.take(0), l.0.chip),
        (PSID_LIFT_TAKING, 0),
        "chip A's, later"
    );
    let mut l = Lift::of_chips(2);
    gate(&mut l, 0);
    l.call();
    gate(&mut l, 1);
    l.call();
    assert_eq!(l.take(0), PSID_LIFT_TAKING);
    assert_eq!((l.0.chip, l.0.frames), (1, 1), "chip B's, later");
    let mut l = Lift::of_chips(2);
    gate(&mut l, 1);
    gate(&mut l, 0);
    l.call();
    assert_eq!(
        (l.take(0), l.0.chip),
        (PSID_LIFT_TAKING, 0),
        "one call: chip A's"
    );
    let mut one = Lift::new();
    one.write(1, 4, 0x11);
    one.call();
    assert_eq!(one.take(0), PSID_LIFT_ARMED, "a 1SID tune's chip B");
}

#[test]
fn a_pulse_sweep_becomes_a_set_row_and_a_modulation_row() {
    let mut frames = vec![FrameRegs::new(note_freq(C4), 0x800, 0x41, 0x09, 0xa0)];
    for k in 1..12u16 {
        frames.push(frames[0].set_pw(0x800 + 0x10 * k));
    }
    let mut l = lifted_after_a_gate_off_frame(&frames, 0);
    let (f, flags) = l.convert(false).unwrap();
    let g = parse(&f);
    assert_eq!(flags, 0);
    assert_eq!(l.0.base, C4);
    assert_eq!(&g.ins[..9], &[0x09, 0xa0, 1, 1, 0, 0, 0, 2, 0x41]);
    assert_eq!(&g.ins[9..16], b"TEST V1");
    assert_eq!(
        g.tables[0],
        vec![(0x00, 0x00), (0xff, 0)],
        "the still frames need no row"
    );
    assert_eq!(g.tables[1], vec![(0x88, 0x10), (10, 0x10), (0xff, 0)]);
    assert!(g.tables[2].is_empty() && g.tables[3].is_empty());
    let sng = song_of(&g, C4);
    assert_same_as_goattracker(&sng, &default_options(), 200);
    assert_plays_back_as_captured(&frames, &g, C4);
}

#[test]
fn waveforms_notes_and_an_sr_change_become_wave_rows() {
    let base = note_freq(C4 + 3);
    let f0 = FrameRegs::new(note_freq(C4 - 5), 0x400, 0x81, 0x00, 0xf0);
    let mut frames = vec![f0];
    frames.push(f0.set_freq(base));
    frames.push(FrameRegs::new(note_freq(C4 + 7), 0x400, 0x41, 0x00, 0xf0));
    frames.push(FrameRegs::new(note_freq(C4 + 10), 0x400, 0x41, 0x00, 0xf0));
    frames.push(FrameRegs::new(note_freq(C4 + 10), 0x400, 0x41, 0x00, 0xf0));
    frames.push(FrameRegs::new(note_freq(C4 + 10), 0x400, 0x41, 0x00, 0x88));
    frames.push(FrameRegs::new(note_freq(C4 - 2), 0x400, 0x11, 0x00, 0x88));
    frames.push(FrameRegs::new(note_freq(C4 - 2), 0x400, 0x11, 0x00, 0x88));
    let mut l = lifted_after_a_gate_off_frame(&frames, 1);
    let (f, _) = l.convert(false).unwrap();
    let g = parse(&f);
    assert_eq!(l.0.base, C4 + 3);
    assert_eq!(
        g.tables[0],
        vec![
            (0x00, 0x00),
            (0x41, 0x04),
            (0x00, 0x07),
            (0x00, 0x80),
            (0xf6, 0x88),
            (0x11, 0x7b),
            (0xff, 0)
        ]
    );
    assert_eq!(g.tables[1], vec![(0x84, 0x00), (0xff, 0)]);
    assert_same_as_goattracker(&song_of(&g, C4 + 3), &default_options(), 200);
    assert_plays_back_as_captured(&frames, &g, C4 + 3);
}

#[test]
fn a_filter_routed_voice_gets_filter_rows() {
    let mut frames = Vec::new();
    for k in 0..10u8 {
        let mut r = FrameRegs::new(note_freq(C4), 0, 0x21, 0x0a, 0x00);
        r.0[CUTOFF_HI] = 0x20 + 4 * k;
        r.0[RES_FILT] = 0x76;
        r.0[MODE_VOL] = 0x1f;
        frames.push(r);
    }
    let mut l = lifted_after_a_gate_off_frame(&frames, 2);
    let (f, flags) = l.convert(false).unwrap();
    let g = parse(&f);
    assert_eq!(flags, PSID_LIFT_F_FILTER);
    assert_eq!(g.ins[4], 1);
    assert_eq!(
        g.tables[2],
        vec![(0x90, 0x74), (0x00, 0x24), (8, 4), (0xff, 0)]
    );
    assert_same_as_goattracker(&song_of(&g, C4), &default_options(), 200);
    let mut l = lifted_after_a_gate_off_frame(&frames, 0);
    let (f, flags) = l.convert(false).unwrap();
    assert_eq!(flags, 0);
    assert!(parse(&f).tables[2].is_empty());
}

#[test]
fn a_digi_burst_is_refused_live_or_from_the_history() {
    let on = FrameRegs::new(note_freq(C4), 0x800, 0x41, 0x09, 0xa0);
    let digi = |l: &mut Lift| (0..40u8).for_each(|k| l.write(0, 0x18, k & 15));
    let a_busy_player = |l: &mut Lift| (0..7).for_each(|_| l.write(0, 4, 0x41));
    let bursts: [(&dyn Fn(&mut Lift), bool); 2] = [(&digi, true), (&a_busy_player, false)];
    for taken_after in [false, true] {
        for (burst, refused) in bursts {
            let mut l = Lift::new();
            if !taken_after {
                l.take(0);
            }
            l.frame(0, on);
            burst(&mut l);
            calls(&mut l, 0, on, 2);
            if taken_after {
                l.take(0);
            }
            assert_eq!(
                l.convert(false).err() == Some(PSID_LIFT_E_DIGI),
                refused,
                "taken after the burst: {taken_after}"
            );
        }
    }
}

fn vibrato(note: u8, speed: u8, shift: u32, delay: u32, frames: usize) -> Vec<u16> {
    let step = note_freq(note + 1).wrapping_sub(note_freq(note)) >> shift;
    let (mut f, mut time, mut d) = (note_freq(note), 0u8, delay);
    let mut out = vec![note_freq(note), f];
    for _ in 2..frames {
        if d > 0 {
            d -= 1;
        } else {
            let mut a = time;
            if a & 0x80 == 0 && a > speed {
                a = !a;
            }
            time = a.wrapping_add(2);
            f = if time & 1 != 0 {
                f.wrapping_sub(step)
            } else {
                f.wrapping_add(step)
            };
        }
        out.push(f);
    }
    out
}

#[test]
fn vibrato_is_fitted_or_dropped() {
    let freqs = vibrato(C4, 3, 2, 4, 40);
    let frames: Vec<FrameRegs> = freqs
        .iter()
        .map(|&f| FrameRegs::new(f, 0, 0x11, 0x0a, 0xa9))
        .collect();
    let mut l = lifted_after_a_gate_off_frame(&frames, 0);
    let (f, flags) = l.convert(false).unwrap();
    let g = parse(&f);
    println!("{:02X?} {:02X?}", g.ins, g.tables);
    assert_eq!(flags, PSID_LIFT_F_VIBRATO, "{:?}", g.tables);
    assert_eq!(
        (g.ins[5], g.ins[6]),
        (1, 5),
        "the speed row, the delay (one more)"
    );
    assert_eq!(g.tables[3], vec![(0x83, 2)]);
    assert_same_as_goattracker(&song_of(&g, C4), &options_playing_as_captured(), 200);
    assert_plays_back_as_captured(&frames, &g, C4);
    let wobble = [
        0i32, 9, -30, 4, 25, -12, 40, -3, -36, 18, 2, -27, 31, -8, 14,
    ];
    let frames: Vec<FrameRegs> = wobble
        .iter()
        .map(|&d| FrameRegs::new((i32::from(note_freq(C4)) + d) as u16, 0, 0x11, 0x0a, 0xa9))
        .collect();
    let mut l = lifted_after_a_gate_off_frame(&frames, 0);
    let (f, flags) = l.convert(false).unwrap();
    assert_eq!(flags, PSID_LIFT_F_NO_VIBRATO);
    let g = parse(&f);
    assert_eq!(g.ins[5], 0);
    assert_eq!(g.tables[0][0], (0, 0), "the centre, held");
}

#[test]
fn ntsc_frequencies_are_scaled_to_the_pal_notes() {
    let ntsc = |n: u8| (f64::from(note_freq(n)) * 985_248.0 / 1_022_727.0).round() as u16;
    let frames = vec![
        FrameRegs::new(ntsc(C4), 0, 0x11, 0x0a, 0xa9),
        FrameRegs::new(ntsc(C4), 0, 0x11, 0x0a, 0xa9),
        FrameRegs::new(ntsc(C4 + 12), 0, 0x11, 0x0a, 0xa9),
    ];
    let mut l = lifted_after_a_gate_off_frame(&frames, 0);
    let (f, _) = l.convert(true).unwrap();
    assert_eq!(l.0.base, C4);
    assert_eq!(parse(&f).tables[0], vec![(0, 0), (0, 12), (0xff, 0)]);
}

#[test]
fn a_busy_note_is_cut_to_fit_the_file() {
    let mut frames = Vec::new();
    for k in 0..50u16 {
        let mut r = FrameRegs::new(
            note_freq(C4 + (k % 5) as u8),
            (k * 700) & 0xfff,
            0x41 + ((k as u8 & 1) << 4),
            0x0a,
            0xa9,
        );
        r.0[CUTOFF_HI] = (k * 37) as u8;
        r.0[RES_FILT] = 0x11 | ((k as u8 & 3) << 4);
        r.0[MODE_VOL] = 0x1f;
        frames.push(r);
    }
    let mut l = lifted_after_a_gate_off_frame(&frames, 0);
    let (f, flags) = l.convert(false).unwrap();
    assert!(f.len() <= PSID_LIFT_FILE_MAX as usize);
    assert_ne!(flags & PSID_LIFT_F_SHORT, 0);
    parse(&f);
}

#[test]
fn a_starter_tune_s_voices_lift_and_play_back() {
    let file = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/starter/consultant.sid"),
    )
    .unwrap();
    for (voice, at) in (0..3u32).flat_map(|v| [(v, 1000), (v, 3000)]) {
        let mut p = psid::Player::new(&file);
        let mut lift = Lift::new();
        let l = &mut *lift.0;
        p.ctl.lift = l;
        assert!(p.play(1));
        let mut blocks = 0;
        while l.state != PSID_LIFT_TAKEN as u8 {
            p.pass();
            p.block();
            blocks += 1;
            if blocks == at {
                unsafe { psid_lift_take(l, voice) };
            }
            assert!(blocks < at + 20_000, "voice {voice}: no note");
        }
        let frames: Vec<FrameRegs> = (0..l.frames as usize)
            .map(|k| FrameRegs(l.frame[k]))
            .collect();
        let mut flags = 0;
        let mut n = PSID_LIFT_AGAIN as i32;
        while n == PSID_LIFT_AGAIN as i32 {
            n = unsafe { psid_lift_convert(l, 0, c"CONSULTANT".as_ptr(), &mut flags) };
        }
        assert!(n > 0, "voice {voice}: {n}");
        let g = parse(&l.file[..n as usize]);
        let (got, simple) = sidou_player_regs_from_the_gate_on(
            &song_of(&g, l.base),
            &options_playing_as_captured(),
            frames.len(),
        );
        let (mut err, mut held) = (0u32, 0u32);
        let centre = u32::from(note_freq(l.base));
        for (k, (want, have)) in frames.iter().zip(&got).enumerate().skip(1) {
            let h = FrameRegs([
                have[0], have[1], have[2], have[3], have[4], have[5], have[6], 0, 0, 0, 0,
            ]);
            let pulse = want.0[CONTROL] & PULSE_WAVE != 0;
            assert_eq!(
                (
                    h.0[CONTROL],
                    h.0[AD],
                    h.0[SR],
                    pulse && h.pw() != as_simple_pulse_writes_it(want.pw(), simple)
                ),
                (want.0[CONTROL], want.0[AD], want.0[SR], false),
                "voice {voice} frame {k}: control, AD, SR, pulse"
            );
            err += u32::from(h.freq()).abs_diff(u32::from(want.freq()));
            held += u32::from(want.freq()).abs_diff(centre);
        }
        println!(
            "voice {voice} from block {at}: {} frames, base {}, flags {flags:#x}, frequency off \
             by {err} in all, the held note {held}",
            frames.len(),
            l.base
        );
        if flags & PSID_LIFT_F_VIBRATO != 0 {
            assert!(2 * err <= held, "voice {voice}: the vibrato");
        } else {
            assert_eq!(err, 0, "voice {voice}: the frequency");
        }
    }
}

const LAB_BYTES: usize = LAB_SOUND_BYTES as usize;
const NOISE: u8 = 0x80;

fn lab(sound: &[u8; LAB_BYTES], field: u32) -> u8 {
    sound[field as usize]
}

fn lab16(sound: &[u8; LAB_BYTES], field: u32) -> u32 {
    u32::from(u16::from_le_bytes([
        sound[field as usize],
        sound[field as usize + 1],
    ]))
}

fn pulled(frames: &[FrameRegs], voice: usize) -> ([u8; LAB_BYTES], u32) {
    let mut l = lifted_after_a_gate_off_frame(frames, voice);
    let (f, _) = l.convert(false).unwrap();
    let mut sound = [0u8; LAB_BYTES];
    sound[LAB_SOUND_CONTROL as usize] = NOISE;
    let mut notes = 0;
    let ok = unsafe {
        lab_song_pull_gti(
            f.as_ptr(),
            f.len() as u32,
            0,
            sound.as_mut_ptr(),
            &mut notes,
        )
    };
    assert_eq!(ok, 1, "the lifted file reads");
    (sound, notes)
}

fn triangle(lo: u16, step: u16, half: u16, frames: u16) -> Vec<FrameRegs> {
    (0..frames)
        .map(|k| {
            let phase = k % (2 * half);
            let up = if phase < half {
                phase
            } else {
                2 * half - phase
            };
            FrameRegs::new(note_freq(C4), lo + step * up, 0x41, 0x09, 0xa0)
        })
        .collect()
}

const PWM_PERIOD_FRAMES: u32 = 32;

fn lab_pwm_period_frames(rate: u32) -> u32 {
    32_768 * 5 / 6 / rate
}

#[test]
fn a_lifted_note_pulls_into_lab_with_its_wave_envelope_and_pwm() {
    let saw = vec![FrameRegs::new(note_freq(C4), 0, 0x21, 0x2a, 0xb9); 10];
    let (s, notes) = pulled(&saw, 0);
    assert_eq!(
        (
            lab(&s, LAB_SOUND_CONTROL),
            lab(&s, LAB_SOUND_AD),
            lab(&s, LAB_SOUND_SR)
        ),
        (0x20, 0x2a, 0xb9),
        "the note's own wave, not the sound's before"
    );
    assert_eq!(notes & LAB_SONG_APPROX, 0, "nothing approximated");

    let (s, notes) = pulled(&triangle(0x600, 0x40, 16, 50), 0);
    assert_eq!(lab(&s, LAB_SOUND_CONTROL), 0x40);
    assert_eq!(notes & LAB_SONG_SWEEP, 0, "a modulation, not a sweep");
    assert!(
        lab16(&s, LAB_SOUND_PULSE).abs_diff(0x800) <= 0x40,
        "the centre width"
    );
    assert!(
        lab16(&s, LAB_SOUND_PWM_DEPTH).abs_diff(0x200) <= 0x40,
        "half the swing"
    );
    let period = lab_pwm_period_frames(lab16(&s, LAB_SOUND_PWM_RATE));
    assert!(
        period.abs_diff(PWM_PERIOD_FRAMES) <= 2,
        "the period: {period} frames"
    );

    let (s, notes) = pulled(&triangle(0x600, 0x10, 40, 30), 0);
    assert_ne!(notes & LAB_SONG_SWEEP, 0, "a sweep one way is named");
    assert_eq!(lab16(&s, LAB_SOUND_PWM_DEPTH), 0);
}

#[test]
fn a_lifted_filter_and_vibrato_pull_into_lab() {
    let mut frames = Vec::new();
    for k in 0..10u8 {
        let mut r = FrameRegs::new(note_freq(C4), 0, 0x21, 0x0a, 0x00);
        r.0[CUTOFF_HI] = 0x20 + 4 * k;
        r.0[RES_FILT] = 0x76;
        r.0[MODE_VOL] = 0x1f;
        frames.push(r);
    }
    let (s, notes) = pulled(&frames, 2);
    assert_eq!(
        (
            lab(&s, LAB_SOUND_FILTER_VOICES),
            lab(&s, LAB_SOUND_RESONANCE),
            lab(&s, LAB_SOUND_FILTER_MODE)
        ),
        (7, 7, 0x10)
    );
    let swept = (0x20u32 << 3)..=(0x44 << 3);
    assert!(
        swept.contains(&lab16(&s, LAB_SOUND_CUTOFF)),
        "the cutoff where it starts"
    );
    assert_ne!(notes & LAB_SONG_SWEEP, 0, "its sweep named");
    let wobble: Vec<FrameRegs> = vibrato(C4, 3, 2, 4, 40)
        .iter()
        .map(|&f| FrameRegs::new(f, 0, 0x11, 0x0a, 0xa9))
        .collect();
    let (s, _) = pulled(&wobble, 0);
    assert!(lab(&s, LAB_SOUND_VIBRATO_DEPTH) > 0, "the vibrato kept");
    assert!(lab(&s, LAB_SOUND_VIBRATO_DELAY) > 0, "after its delay");
}

#[test]
fn a_file_that_is_no_gti_is_refused() {
    let mut sound = [0u8; LAB_BYTES];
    let mut notes = 0;
    for f in [&b"GTI4"[..], &b"GTI5\0\0"[..]] {
        let ok = unsafe {
            lab_song_pull_gti(
                f.as_ptr(),
                f.len() as u32,
                0,
                sound.as_mut_ptr(),
                &mut notes,
            )
        };
        assert_eq!(ok, 0);
    }
}
