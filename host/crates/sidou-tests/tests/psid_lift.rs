// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The lift (core/psid/psid_lift.h; 1.5 design, section 3): a voice's next note captured from a
//! tune's writes and turned into a GoatTracker 2 instrument. The capture and the conversion on
//! synthetic write streams (a gate-on, a pulse sweep, the envelope, an SR change, a filter routed
//! voice, a digi burst, vibrato), the lifted instrument played by SIDOU's player and by
//! GoatTracker's own (gt2-ref) call for call, and a real tune's voice lifted and played again.

use gt2_ref::trace::trace;
use gt2_ref::Options;
use sidou_sys::*;
use sidou_tests::player::{default_options, Player};
use sidou_tests::psid;
use sidou_tests::song::{SngSpec, Song};

const V: usize = 7;

/// A note's registers for a frame: voice registers 0-6, then $15-$18 (psid_lift's frame).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Regs([u8; 11]);

impl Regs {
    fn new(freq: u16, pw: u16, ctrl: u8, ad: u8, sr: u8) -> Regs {
        Regs([
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
    fn set_freq(mut self, f: u16) -> Regs {
        self.0[0] = f as u8;
        self.0[1] = (f >> 8) as u8;
        self
    }
    fn pw(&self) -> u16 {
        u16::from_le_bytes([self.0[2], self.0[3]]) & 0x0fff
    }
    fn set_pw(mut self, p: u16) -> Regs {
        self.0[2] = p as u8;
        self.0[3] = (p >> 8) as u8;
        self
    }
}

struct Lift(Box<psid_lift>);

impl Lift {
    fn new() -> Lift {
        let mut l = Box::<psid_lift>::default();
        unsafe { psid_lift_init(&mut *l) };
        Lift(l)
    }
    fn arm(&mut self, voice: u32, chips: u32) {
        unsafe { psid_lift_arm(&mut *self.0, voice, chips) };
    }
    fn write(&mut self, chip: u32, reg: u32, value: u8) {
        unsafe { psid_lift_write(&mut *self.0, chip, reg, u32::from(value)) };
    }
    fn call(&mut self) {
        unsafe { psid_lift_call(&mut *self.0) };
    }
    /// A play call that writes voice `v`'s registers as `r` (control last, as GoatTracker's
    /// player does) and the filter's.
    fn frame(&mut self, v: usize, r: Regs) {
        for reg in [5usize, 6, 2, 3, 0, 1, 4] {
            self.write(0, (v * V + reg) as u32, r.0[reg]);
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
        let n =
            unsafe { psid_lift_convert(&mut *self.0, i32::from(ntsc), name.as_ptr(), &mut flags) };
        if n < 0 {
            Err(n)
        } else {
            Ok((self.0.file[..n as usize].to_vec(), flags))
        }
    }
}

/// A GTI5 file's instrument and four tables (left, right).
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

/// The note of GoatTracker's frequency table nearest `f` (C-0 is 0).
fn note_freq(n: u8) -> u16 {
    unsafe { u16::from(gt2_freq_lo[n as usize]) | u16::from(gt2_freq_hi[n as usize]) << 8 }
}

const C4: u8 = 48;

/// A frame's registers through the lift: the gate-on frame first (gate on), then `frames`.
fn lift(frames: &[Regs], v: usize) -> Lift {
    let mut l = Lift::new();
    l.arm(v as u32, 1);
    // a frame before: the gate off, another note's frequency (as a player leaves it)
    l.frame(v, Regs::new(note_freq(C4 - 5), 0, 0x40, 0x0f, 0x00));
    for r in frames {
        l.frame(v, *r);
    }
    l
}

/// The song that plays the instrument: `note` on channel 1 at its first row, held.
fn song_of(g: &Gti, note: u8) -> Vec<u8> {
    let mut s = SngSpec::new();
    s.instr = vec![g.ins];
    s.tables = g.tables.clone();
    let mut p0 = vec![[0xbdu8, 0, 0, 0]; 64];
    p0[0] = [0x60 + note, 1, 0, 0];
    s.patterns[0].0 = p0;
    s.encode()
}

/// SIDOU's player over the song: each call's registers as they stand after it (25), from the
/// call whose voice-1 control turns the gate on, `n` calls; and whether the packer would write the
/// pulse in one byte (GoatTracker's simple pulse: a song whose pulse rows all have a zero low
/// nybble).
fn played(sng: &[u8], o: &gt2_options, n: usize) -> (Vec<[u8; 25]>, bool) {
    let mut song = Song::host();
    song.read(sng, 0).map_err(|i| i.msg).unwrap();
    let mut p = Player::new(&song, 0, o).expect("the player");
    let simple = u32::from(p.shape().flags) & GT2_PF_SIMPLEPULSE != 0;
    let mut regs = [0u8; 25];
    let mut out = Vec::new();
    for _ in 0..2000 {
        let was = regs[4];
        for (r, v) in p.call() {
            regs[r as usize] = v;
        }
        if !out.is_empty() || (was & 1 == 0 && regs[4] & 1 == 1) {
            out.push(regs);
            if out.len() == n {
                return (out, simple);
            }
        }
    }
    panic!("no gate-on");
}

/// A pulse width as GoatTracker's simple pulse writes it: its middle byte to both registers, so
/// the low nybble is the high one's.
fn simple_pulse(pw: u16, simple: bool) -> u16 {
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

/// SIDOU's player and GoatTracker's packed one give the same writes, call for call.
fn same_as_goattracker(sng: &[u8], o: &gt2_options, frames: usize) {
    assert_eq!(o.player_flags, default_options().player_flags);
    let t = trace(sng, 0, frames, &ref_options(o)).expect("trace");
    let mut song = Song::host();
    song.read(sng, 0).map_err(|i| i.msg).unwrap();
    let mut p = Player::new(&song, 0, o).expect("the player");
    for (f, frame) in t.frames.iter().enumerate() {
        for want in &frame.calls {
            let got = p.call();
            // A channel's frequency before its first note: bytes only the packed layout knows
            // (gt2_match.rs, freq_unknown).
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

/// The lifted frames 1.. played back: each frame's control, frequency, pulse width (12 bits),
/// AD and SR as captured, and frame 0's control, AD and SR.
fn plays_back(frames: &[Regs], g: &Gti, base: u8) {
    let (got, simple) = played(&song_of(g, base), &exact(), frames.len());
    for (k, (want, have)) in frames.iter().zip(&got).enumerate() {
        let h = Regs([
            have[0], have[1], have[2], have[3], have[4], have[5], have[6], have[21], have[22],
            have[23], have[24],
        ]);
        assert_eq!(
            (h.0[4], h.0[5], h.0[6]),
            (want.0[4], want.0[5], want.0[6]),
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
            if want.0[4] & 0x40 != 0 {
                assert_eq!(h.pw(), simple_pulse(want.pw(), simple), "frame {k}: pulse");
            }
        }
    }
}

/// The options that play a lift as captured: GoatTracker's pulse and realtime optimisations skip
/// table steps and effects at the song's own row ticks, so they are off.
fn exact() -> gt2_options {
    let mut o = default_options();
    o.optimize_pulse = 0;
    o.optimize_realtime = 0;
    o
}

/// The capture: it starts at the armed voice's gate-on (not before, not on another voice), keeps
/// a frame a call, and ends at the gate-off (that call not kept).
#[test]
fn the_capture_starts_at_the_gate_on_and_ends_at_the_gate_off() {
    let mut l = Lift::new();
    l.arm(1, 1);
    let on = Regs::new(note_freq(C4), 0x800, 0x41, 0x09, 0xa0);
    l.frame(0, on); // voice 1's gate: not the armed voice
    l.frame(1, Regs([0; 11]));
    assert_eq!(l.state(), PSID_LIFT_ARMED);
    for _ in 0..5 {
        l.frame(1, on);
    }
    assert_eq!(l.state(), PSID_LIFT_TAKING);
    assert_eq!(l.0.frames, 5);
    l.frame(1, Regs([0x40; 11])); // the gate off: the note is over
    assert_eq!(l.state(), PSID_LIFT_TAKEN);
    assert_eq!(l.0.frames, 5);
    assert_eq!(l.0.frame[0][4], 0x41);
    assert_eq!(l.0.frame[4][5], 0x09);
}

/// The capture ends at the next gate-on, and after a second of frames at most.
#[test]
fn the_capture_ends_at_the_next_gate_on_or_after_a_second() {
    let on = Regs::new(note_freq(C4), 0x800, 0x41, 0x09, 0xa0);
    let mut l = Lift::new();
    l.arm(2, 1);
    l.frame(2, on);
    l.frame(2, on);
    // a hard restart inside a call: off and on again
    l.write(0, 2 * 7 + 4, 0x40);
    l.write(0, 2 * 7 + 4, 0x41);
    l.call();
    assert_eq!((l.state(), l.0.frames), (PSID_LIFT_TAKEN, 2));
    let mut l = Lift::new();
    l.arm(0, 1);
    for _ in 0..80 {
        l.frame(0, on);
    }
    assert_eq!(
        (l.state(), u32::from(l.0.frames)),
        (PSID_LIFT_TAKEN, PSID_LIFT_FRAMES)
    );
}

/// A 2SID tune: either chip's voice starts it, and the frames are that chip's.
#[test]
fn a_2sid_tune_lifts_from_the_chip_whose_voice_gates_first() {
    let mut l = Lift::new();
    l.arm(0, 2);
    l.write(1, 0, 0x34);
    l.write(1, 4, 0x11);
    l.write(0, 4, 0x21); // chip A's gate after B's: B has it
    l.call();
    assert_eq!(l.0.chip, 1);
    assert_eq!(l.0.frame[0][4], 0x11);
    let mut one = Lift::new();
    one.arm(0, 1);
    one.write(1, 4, 0x11); // a 1SID lift never watches chip B
    one.call();
    assert_eq!(one.state(), PSID_LIFT_ARMED);
}

/// A gate-on frame, a held note with a pulse sweep: the instrument's AD, SR and first wave, a
/// wave row that sets the base note and a delay, a set pulse and one modulation row; SIDOU's
/// player and GoatTracker's play it the same, and as captured.
#[test]
fn a_pulse_sweep_becomes_a_set_row_and_a_modulation_row() {
    let mut frames = vec![Regs::new(note_freq(C4), 0x800, 0x41, 0x09, 0xa0)];
    for k in 1..12u16 {
        frames.push(frames[0].set_pw(0x800 + 0x10 * k));
    }
    let mut l = lift(&frames, 0);
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
    same_as_goattracker(&sng, &default_options(), 200);
    plays_back(&frames, &g, C4);
}

/// Waveform and note changes (an arpeggio, a drum's noise), the first frame's pitch the note
/// before's (as GoatTracker leaves it: frame 1 is the base), an SR change as a $F6 row.
#[test]
fn waveforms_notes_and_an_sr_change_become_wave_rows() {
    let base = note_freq(C4 + 3);
    let f0 = Regs::new(note_freq(C4 - 5), 0x400, 0x81, 0x00, 0xf0); // the note before's pitch
    let mut frames = vec![f0];
    frames.push(f0.set_freq(base)); // 1: the base note
    frames.push(Regs::new(note_freq(C4 + 7), 0x400, 0x41, 0x00, 0xf0)); // 2: pulse, +4
    frames.push(Regs::new(note_freq(C4 + 10), 0x400, 0x41, 0x00, 0xf0)); // 3: +7
    frames.push(Regs::new(note_freq(C4 + 10), 0x400, 0x41, 0x00, 0xf0)); // 4
    frames.push(Regs::new(note_freq(C4 + 10), 0x400, 0x41, 0x00, 0x88)); // 5: SR
    frames.push(Regs::new(note_freq(C4 - 2), 0x400, 0x11, 0x00, 0x88)); // 6: triangle, -5
    frames.push(Regs::new(note_freq(C4 - 2), 0x400, 0x11, 0x00, 0x88)); // 7
    let mut l = lift(&frames, 1);
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
    same_as_goattracker(&song_of(&g, C4 + 3), &default_options(), 200);
    plays_back(&frames, &g, C4 + 3);
}

/// A voice routed to the filter: filter rows (mode, resonance with the voice's own bit, the
/// cutoff's high byte) and a sweep as modulation; a voice not routed has none.
#[test]
fn a_filter_routed_voice_gets_filter_rows() {
    let mut frames = Vec::new();
    for k in 0..10u8 {
        let mut r = Regs::new(note_freq(C4), 0, 0x21, 0x0a, 0x00);
        r.0[8] = 0x20 + 4 * k; // the cutoff's high byte
        r.0[9] = 0x76; // resonance 7, voices 2 and 3
        r.0[10] = 0x1f; // low-pass, full volume
        frames.push(r);
    }
    let mut l = lift(&frames, 2);
    let (f, flags) = l.convert(false).unwrap();
    let g = parse(&f);
    assert_eq!(flags, PSID_LIFT_F_FILTER);
    assert_eq!(g.ins[4], 1);
    assert_eq!(
        g.tables[2],
        vec![(0x90, 0x74), (0x00, 0x24), (8, 4), (0xff, 0)]
    );
    same_as_goattracker(&song_of(&g, C4), &default_options(), 200);
    let mut l = lift(&frames, 0); // voice 1: not routed
    let (f, flags) = l.convert(false).unwrap();
    assert_eq!(flags, 0);
    assert!(parse(&f).tables[2].is_empty());
}

/// A digi (a register written on and on within a call) is refused.
#[test]
fn a_digi_burst_is_refused() {
    let on = Regs::new(note_freq(C4), 0x800, 0x41, 0x09, 0xa0);
    let mut l = Lift::new();
    l.arm(0, 1);
    l.frame(0, on);
    for k in 0..40u8 {
        l.write(0, 0x18, k & 15);
    }
    l.frame(0, on);
    l.frame(0, on);
    assert_eq!(l.convert(false), Err(PSID_LIFT_E_DIGI));
    // eight writes of a register in a call is a player's business, not a digi
    let mut l = Lift::new();
    l.arm(0, 1);
    l.frame(0, on);
    for _ in 0..7 {
        l.write(0, 4, 0x41);
    }
    l.frame(0, on);
    assert!(l.convert(false).is_ok());
}

/// GoatTracker's calculated vibrato from frame `from` on: the frequencies its instrument vibrato
/// gives (speed, the note's step >> shift, after `delay` frames), frame 1 the note.
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

/// A vibrato as GoatTracker's instrument makes it is fitted again (its speed row, delay); a
/// swing no instrument vibrato follows is dropped, the centre held.
#[test]
fn vibrato_is_fitted_or_dropped() {
    let freqs = vibrato(C4, 3, 2, 4, 40);
    let frames: Vec<Regs> = freqs
        .iter()
        .map(|&f| Regs::new(f, 0, 0x11, 0x0a, 0xa9))
        .collect();
    let mut l = lift(&frames, 0);
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
    same_as_goattracker(&song_of(&g, C4), &exact(), 200);
    plays_back(&frames, &g, C4);
    // a wobble of unequal swings: no vibrato follows it
    let wobble = [
        0i32, 9, -30, 4, 25, -12, 40, -3, -36, 18, 2, -27, 31, -8, 14,
    ];
    let frames: Vec<Regs> = wobble
        .iter()
        .map(|&d| Regs::new((i32::from(note_freq(C4)) + d) as u16, 0, 0x11, 0x0a, 0xa9))
        .collect();
    let mut l = lift(&frames, 0);
    let (f, flags) = l.convert(false).unwrap();
    assert_eq!(flags, PSID_LIFT_F_NO_VIBRATO);
    let g = parse(&f);
    assert_eq!(g.ins[5], 0);
    assert_eq!(g.tables[0][0], (0, 0), "the centre, held");
}

/// An NTSC tune's frequencies are the PAL table's notes once scaled.
#[test]
fn ntsc_frequencies_are_scaled_to_the_pal_notes() {
    let ntsc = |n: u8| (f64::from(note_freq(n)) * 985_248.0 / 1_022_727.0).round() as u16;
    let frames = vec![
        Regs::new(ntsc(C4), 0, 0x11, 0x0a, 0xa9),
        Regs::new(ntsc(C4), 0, 0x11, 0x0a, 0xa9),
        Regs::new(ntsc(C4 + 12), 0, 0x11, 0x0a, 0xa9),
    ];
    let mut l = lift(&frames, 0);
    let (f, _) = l.convert(true).unwrap();
    assert_eq!(l.0.base, C4);
    assert_eq!(parse(&f).tables[0], vec![(0, 0), (0, 12), (0xff, 0)]);
}

/// A note too busy for the file (every frame a new pulse jump, filter step and waveform) keeps
/// the frames that fit, and says so.
#[test]
fn a_busy_note_is_cut_to_fit_the_file() {
    let mut frames = Vec::new();
    for k in 0..50u16 {
        let mut r = Regs::new(
            note_freq(C4 + (k % 5) as u8),
            (k * 700) & 0xfff,
            0x41 + ((k as u8 & 1) << 4),
            0x0a,
            0xa9,
        );
        r.0[8] = (k * 37) as u8;
        r.0[9] = 0x11 | ((k as u8 & 3) << 4);
        r.0[10] = 0x1f;
        frames.push(r);
    }
    let mut l = lift(&frames, 0);
    let (f, flags) = l.convert(false).unwrap();
    assert!(f.len() <= PSID_LIFT_FILE_MAX as usize);
    assert_ne!(flags & PSID_LIFT_F_SHORT, 0);
    parse(&f);
}

/// A real tune: GoatTracker's example The Consultant (assets/starter, Cadaver), each voice
/// lifted at its next note from the third second and played again by SIDOU's player at the lift's
/// base note; each frame from the first (the gate-on frame is the note before's pitch in
/// GoatTracker's player) compared with the captured one: control, AD, SR, frequency, and the pulse
/// width while the pulse waveform sounds.
#[test]
fn a_starter_tune_s_voices_lift_and_play_back() {
    let file = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/starter/consultant.sid"),
    )
    .unwrap();
    for (voice, at) in (0..3u32).flat_map(|v| [(v, 1000), (v, 3000)]) {
        let mut p = psid::Player::new(&file);
        let mut l = Box::<psid_lift>::default();
        unsafe { psid_lift_init(&mut *l) };
        p.ctl.lift = &mut *l;
        assert!(p.play(1));
        let mut blocks = 0;
        while l.state != PSID_LIFT_TAKEN as u8 {
            p.pass();
            p.block();
            blocks += 1;
            if blocks == at {
                unsafe { psid_lift_arm(&mut *l, voice, 1) };
            }
            assert!(blocks < at + 20_000, "voice {voice}: no note");
        }
        let frames: Vec<Regs> = (0..l.frames as usize).map(|k| Regs(l.frame[k])).collect();
        let mut flags = 0;
        let n = unsafe { psid_lift_convert(&mut *l, 0, c"CONSULTANT".as_ptr(), &mut flags) };
        assert!(n > 0, "voice {voice}: {n}");
        let g = parse(&l.file[..n as usize]);
        // GoatTracker's pulse and realtime optimisations skip table steps at the song's own
        // rows' ticks (the source's skips are in the frames, as still steps): off.
        let (got, simple) = played(&song_of(&g, l.base), &exact(), frames.len());
        // Every frame's control, AD, SR and pulse width as captured; the frequency exact, but
        // under a fitted vibrato (approximate: GoatTracker's calculated vibrato steps by the
        // note's step >> a shift), where its mean difference is at most half of what holding the
        // centre note would leave.
        let (mut err, mut held) = (0u32, 0u32);
        let centre = u32::from(note_freq(l.base));
        for (k, (want, have)) in frames.iter().zip(&got).enumerate().skip(1) {
            let h = Regs([
                have[0], have[1], have[2], have[3], have[4], have[5], have[6], 0, 0, 0, 0,
            ]);
            let pulse = want.0[4] & 0x40 != 0;
            assert_eq!(
                (
                    h.0[4],
                    h.0[5],
                    h.0[6],
                    pulse && h.pw() != simple_pulse(want.pw(), simple)
                ),
                (want.0[4], want.0[5], want.0[6], false),
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
