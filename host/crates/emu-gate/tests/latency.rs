// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Note-on to sound latency, end to end in fm1-emu: a note-on into the TRS jack at a known
//! cycle, then the cycles until the chip takes it, until its first non-silent sample is in the
//! audio DMA buffer, and until that sample leaves the DAC. Then the note-off, once the note
//! holds its sustain level: the cycles until the chip takes it and until the release's first
//! step is in the buffer.
//!
//! The path: the jack's UART DMA, the 2 kHz UART poll (TIMER5) into the MIDI ring, the main
//! loop's `midi_drain` onto `live`, which writes the voice's registers into the register
//! ring at once (notes are never held for the 10 ms tick, as bend, aftertouch and the wheel
//! are), the commit, then the next audio interrupt applies the ring at the start of its block
//! and renders the block into the free half of the DMA buffer. That half plays once the
//! other one has finished: one more block.
//!
//! Everything is measured from the cycle the note-on's last byte has arrived, the earliest
//! anything can react to it (its three bytes take 0.96 ms on the wire before that):
//! - **taken**: the audio interrupt that applies the note's writes is entered. The chip's
//!   voice is gated from that block's first sample.
//! - **buffer**: that interrupt returns, its block with the note's first non-silent sample
//!   written into the DMA buffer: taken plus the render (about 0.2 ms), because `audio_fill`
//!   writes the block's samples at its end.
//! - **audible**: that sample leaves the DAC (fm1-emu plays a half at 44.1 kHz from the
//!   moment the DMA switches to it): one more block.
//!
//! "Non-silent" is a deviation from the pre-note baseline, not a nonzero sample: the chip's
//! output at rest is a DC level the DC blocker settles, and a voice with its gate off still
//! leaks its waveform as reSIDfp's does. The baseline is the 20 ms before the note-on; the
//! threshold is `SILENCE_MARGIN` LSB past its largest deviation. The first sample past it
//! comes a few dozen samples into the gated block, but it is not yet the note: it is the
//! gated voice's oscillator leaking through the DAC at envelope 0 (about 40 LSB). The note's
//! envelope is measured as well (`note_*`: past `NOTE_LEVEL` of the held note's peak). With the
//! envelope AUTHENTIC it comes up to 33 ms later: the SID's ADSR delay bug, which the core
//! emulates as reSIDfp does. A release rate's period (267 cycles at SAW LEAD's release 6) is
//! longer than attack 0's (9), so a gate that finds the rate LFSR past 8 waits for it to come
//! round, 32767 cycles, before the attack takes its first step (core/sid/sid_env.h). The
//! note-off meets the same delay: decay 9's period (977) is longer than release 6's, so a
//! note-off that finds the LFSR past 266 holds the sustain level up to 33 ms longer. With the
//! envelope INSTANT, the live instrument's default, each gate edge restarts the LFSR: the
//! envelope is up in the block that takes the note, and the release's first step is in the
//! block that takes the note-off. The release is read from the voice's envelope counter in
//! the rendered chip (`release_*`), a block at a time, not from the audio, where its first
//! step is one level in 170. docs/decisions/0005-midi-note-timing.md has the numbers.
//!
//! The bounds, for every trial:
//! - taken <= block + poll (+ the USB attach's stall around it);
//! - buffer <= block + poll (+ the stall) + the render allowance: the longest audio interrupt
//!   of the run, entry to return. The plan's "buffer <= block + poll" left out the render;
//!   0005 has the amendment. The allowance is measured, not a margin, and is itself held to
//!   the audio budget (`common::AUDIO_TARGET` instructions a sample, a block's worth). This
//!   bound follows from the one above: the buffer is the taking interrupt's return, and that
//!   interrupt is one of the run's, so it lasts no longer than the allowance. It is kept as
//!   the plan's statement, and as a check that the first sample is in that block;
//! - audible <= taken + two blocks;
//! - INSTANT: the envelope is up in the block that takes the note (note buffer == buffer);
//!   AUTHENTIC: within the ADSR delay's longest and the attack, a block's granularity on top;
//! - the note-off: taken within the same bound as the note-on; INSTANT: the release's first
//!   step in the block that takes it; AUTHENTIC: within the ADSR delay's longest, a block's
//!   granularity on top.
//!
//! Each trial boots afresh (fm1-emu's state cannot be cloned), past the boot's title card and
//! its wipe (common::run_past_boot), so the instrument is idle as a player finds it, and
//! injects the note-on at a random cycle in a 40 ms window, so its phase against the audio
//! block (2.9 ms), the UART poll (0.5 ms), the 10 ms tick and the screen's 40 ms compose is
//! random. The note-off goes in `NOTE_MS` later, at a random cycle in a block.
//!
//! The busy case keeps the screen redrawing: K1-K3 turning all the time move three bars, so
//! every compose marks strips dirty and the main loop draws one a pass. The USB case puts the
//! note-on around the main loop's longest stall, `usb_retry`'s attach once a second while USB
//! is off (docs/hardware.md), which fm1-emu, with no USB controller, always has.
//!
//! The sweep case plays a stream of the chip's character CCs (M1c Task 3) all through the
//! trial: age/heat (CC 86) and DAC sloppiness (CC 87) sweeping up and down, so the sink's ramp
//! never rests and builds a DAC table a step a tick (sid_character_ramp_tick). The CCs go into
//! the jack only while it is idle before the note-on and the note-off (their bytes never
//! delay the note's). The changing DACs change what the silent voices leak, so this case
//! measures against the same run without the notes, sample for sample (fm1-emu is
//! deterministic), not against a baseline's mean. It also bounds every main-loop pass
//! (midi_drain to midi_service) by the poll allowance, common::POLL_INSTRUCTIONS: a pass that
//! long would hold a note-on back by as much.
//!
//! The profile step case keeps a profile change in flight all through the trial: CC 85
//! between two profiles other than the chip's, faster than either's preparation, so every
//! tick runs a slice of the spare chip's (app/reg_ring.h, reg_ring_prepare_step) and none
//! ever swaps in. Until M2b a profile step prepared the whole chip in one pass (up to 471.5k
//! instructions) and a note arriving then waited about 5 ms, an exception decision record
//! 0006 kept; now the notes keep the normal bounds and every pass the poll allowance.
//!
//! The cases' trial counts keep CI short; `SIDOU_LATENCY_TRIALS` sets every case's (64
//! reproduces decision record 0005's run). Each case writes its report into the fm1 build
//! directory as `latency_<case>_<envelope>.json`.

mod common;

use common::{
    build_dir, skip_message, Firmware, Machine, AUDIO_TARGET, CPU_HZ, HALF_FRAMES, POLL_HZ,
    POLL_INSTRUCTIONS,
};
use fm1_core::Bus;
use sidou_sys::ilp32;

const C4: u8 = 60;
/// Where in the window after boot the note-on goes: the screen's compose period.
const PHASE_WINDOW_MS: u64 = 40;
/// The pre-note baseline.
const BASELINE_MS: u64 = 20;
/// The note-off this long after the note-on: the ADSR delay (33 ms), the attack, SAW LEAD's
/// decay 9 to its sustain level (85 steps of 977 cycles, 84 ms) and some of it held. The
/// held note's peak is taken over the last `HELD_MS`.
const NOTE_MS: u64 = 140;
const HELD_MS: u64 = 10;
/// Run this long after the note-off: the ADSR delay (33 ms) and two blocks.
const RELEASE_MS: u64 = 45;
/// LSB beyond the baseline's largest deviation that count as sound.
const SILENCE_MARGIN: i32 = 8;
/// The note's envelope has come up: a deviation past this share of the held note's peak
/// (-30 dB).
const NOTE_LEVEL: f64 = 1.0 / 31.6;
/// Felucca's ids: K1..K3 (cutoff, resonance, pulse width).
const KNOBS: [u8; 3] = [2, 3, 4];
/// 31250 baud, 10 bits a byte.
const BYTE_BITS: u64 = 10;
const BAUD: u64 = 31_250;
/// TIMER4 ticks per I2S frame (24 MHz / 544 = 44.1 kHz).
const FRAME_T4: u64 = 544;
const T4_HZ: u64 = 24_000_000;
/// The SID's clock (PAL) and its envelope's rate LFSR period: the longest ADSR delay.
const SID_HZ: u64 = 985_248;
const LFSR_PERIOD: u64 = 32_767;
/// Attack 0 from 0 to the note level: 2 ms to the top.
const ATTACK_MS: u64 = 2;
/// The map's envelope mode (docs/reference/midi.md): CC 26, steps, on from 64.
const CC_ENVELOPE_MODE: u8 = 26;
/// fm1-emu runs one instruction a cycle; the audio budget is in instructions.
const INSTRUCTIONS_PER_CYCLE: f64 = 1.0;

/// Cycles of one audio block (a DMA half), one frame, one poll period and one MIDI byte.
struct Clock {
    hz: u64,
}

impl Clock {
    fn frame(&self) -> u64 {
        FRAME_T4 * self.hz / T4_HZ
    }
    fn block(&self) -> u64 {
        HALF_FRAMES as u64 * self.frame()
    }
    fn poll(&self) -> u64 {
        self.hz / POLL_HZ
    }
    fn byte(&self) -> u64 {
        self.hz * BYTE_BITS / BAUD
    }
    fn of_ms(&self, ms: u64) -> u64 {
        self.hz * ms / 1000
    }
    fn ms(&self, cycles: u64) -> f64 {
        cycles as f64 * 1000.0 / self.hz as f64
    }
}

/// The live instrument's envelope mode (LIVE_PARAM_ENVELOPE_MODE).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Envelope {
    /// The default: each gate edge restarts the envelope's rate counter.
    Instant,
    /// The chip's ADSR delay; set over TRS (CC 26 = 127) after the boot.
    Authentic,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Scenario {
    /// The status screen at rest.
    Idle,
    /// K1-K3 turning: the screen redraws all the time.
    Busy,
    /// The note-on lands around `usb_retry`'s attach.
    UsbRetry,
    /// The character's CCs sweeping all the while (the module's doc).
    CharacterSweep,
    /// A profile step in flight all the while: CC 85 between two profiles other than the
    /// chip's, faster than either's preparation, so the spare is prepared a slice every tick
    /// and never swaps in (the module's doc).
    ProfileStep,
    /// A file being written all the while (M2a Task 4): the store's programs in 16-byte write
    /// windows right after the audio blocks (platform/fm1/store_fm1.c; the emulator test hook
    /// starts a 64 KiB file, store_fm1_window set to 16 as the spike's numbers would).
    Saving,
}

/// The sweep: a CC every SWEEP_MS, CC 86 and CC 87 in turn, each value SWEEP_STEP from its
/// last, up to 127 and back down to 0.
const SWEEP_MS: u64 = 5;
const SWEEP_STEP: i32 = 24;
const CC_AGE: u8 = 86;
const CC_DAC: u8 = 87;

/// The profile step: CC 85 every STEP_MS, R2 (0) and WORN (43: CC 85 spans the five profiles and
/// the eight USER slots, 0..12 in steps of 128/13) in turn, the chip R4AR. Each request restarts
/// the spare's preparation, which takes at least 7 ticks (70 ms).
const STEP_MS: u64 = 40;
const CC_PROFILE: u8 = 85;
const CC_PROFILE_WORN: u8 = 43;

struct Sweep {
    next: u64,
    sent: u32,
    period_ms: u64,
    profile: bool,
}

impl Sweep {
    fn new(scenario: Scenario) -> Self {
        let profile = scenario == Scenario::ProfileStep;
        Sweep {
            next: 0,
            sent: 0,
            period_ms: if profile { STEP_MS } else { SWEEP_MS },
            profile,
        }
    }

    /// The next CC: a triangle 0..127 in SWEEP_STEPs, per controller; or the profile step's.
    fn message(&mut self) -> [u8; 3] {
        if self.profile {
            self.sent += 1;
            return [
                0xB0,
                CC_PROFILE,
                if self.sent & 1 == 1 {
                    0
                } else {
                    CC_PROFILE_WORN
                },
            ];
        }
        let cc = if self.sent & 1 == 0 { CC_AGE } else { CC_DAC };
        let step = (self.sent / 2) as i32 * SWEEP_STEP;
        let period = 2 * 127;
        let phase = step % period;
        let value = if phase <= 127 { phase } else { period - phase };
        self.sent += 1;
        [0xB0, cc, value as u8]
    }

    /// Runs `cycles`, a CC every SWEEP_MS, none whose bytes would still be on the wire at the
    /// end (where a note goes in).
    fn run(&mut self, m: &mut Machine, c: &Clock, cycles: u64) {
        let end = m.soc.periph.cycles + cycles;
        let on_the_wire = 3 * c.byte() + c.byte() / 2;
        while m.soc.periph.cycles < end {
            let now = m.soc.periph.cycles;
            if now >= self.next {
                if end - now > on_the_wire {
                    let msg = self.message();
                    m.midi(&msg);
                }
                self.next = now + c.of_ms(self.period_ms);
            }
            m.run_cycles(self.next.min(end) - now);
        }
    }
}

/// One trial, in cycles from the note-on's last byte (the note-off's for `off_*` and
/// `release_*`), and what the main loop did.
#[derive(Debug, Clone, Copy)]
struct Trial {
    taken: u64,
    buffer: u64,
    audible: u64,
    /// The note's envelope past `NOTE_LEVEL`: in the buffer, and out of the DAC.
    note_buffer: u64,
    note_audible: u64,
    /// The note-off: the audio interrupt that clears the gate is entered, and returns.
    off_taken: u64,
    off_buffer: u64,
    /// The block with the release's first step returns.
    release_buffer: u64,
    /// midi_stats.max_wait_ticks: the longest an entry waited in the MIDI ring (us).
    ring_wait_us: f64,
    /// The longest `ui_poll` call, instructions.
    ui_poll_max: u64,
    /// The longest audio interrupt of the trial, entry to return, cycles.
    isr_max: u64,
    /// The longest main-loop pass (midi_drain to midi_service), instructions.
    pass_max: u64,
    /// Slices of the spare's preparation run during the trial (reg_ring_out.slices), and the
    /// chip's profile at its end (live.profile).
    slices: u32,
    profile: u8,
}

/// xorshift64*: reproducible phases.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % n
    }
}

/// Interleaved L/R as played so far.
fn played(m: &Machine) -> &[i16] {
    &m.soc.periph.audio
}

/// A field of each voice's envelope on the rendered chip (platform/fm1/audio.c `chips`),
/// e.g. `ilp32::sid_env::gate`.
fn env_addresses(m: &mut Machine, fw: &Firmware, field: usize) -> Vec<u32> {
    let base = fw.chips();
    let active = m.soc.read8(base + ilp32::reg_ring_chips::active as u32);
    let chip = base + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32;
    (0..3usize)
        .map(|v| chip + (ilp32::sid::env + v * ilp32::sid_env::SIZE + field) as u32)
        .collect()
}

/// The firmware booted past the boot's title card and its wipe, with the envelope mode
/// (checked on the rendered chip), the screen busy for the busy case.
fn boot(fw: &Firmware, scenario: Scenario, envelope: Envelope) -> Machine {
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, fw);
    if envelope == Envelope::Authentic {
        // 1 ms on the wire, then a main-loop pass and an audio block; the window starts well
        // after.
        m.midi(&[0xB0, CC_ENVELOPE_MODE, 127]);
        m.run_ms(10);
    }
    let want = u8::from(envelope == Envelope::Instant);
    for a in env_addresses(&mut m, fw, ilp32::sid_env::gate_reset) {
        assert_eq!(
            m.soc.read8(a),
            want,
            "the rendered chip's gate reset is not {envelope:?}'s"
        );
    }
    if scenario == Scenario::Saving {
        m.soc.write32(fw.symbol("store_fm1_window"), 16);
        let hook = fw.symbol("store_fm1_hook");
        m.soc.write32(hook + 8, 65_536); // len
        m.soc.write32(hook, 3); // STORE_FM1_FILE
        m.run_ms(50);
        let progs = m.soc.read32(fw.symbol("store_fm1_counts"));
        assert!(progs > 0, "the file is not being written");
    }
    if scenario == Scenario::Busy {
        // 400 ms of turning (a detent per 8 ms), past the window and the measurement.
        for k in KNOBS {
            m.turn(k, -50);
        }
        m.run_ms(BASELINE_MS); // the first compose with the bars moving
    }
    m
}

/// The first `usb_attach_now` after `boot`: its start, in cycles after `boot`, and its
/// instructions.
fn usb_attach(fw: &Firmware, envelope: Envelope) -> (u64, u64) {
    let mut m = boot(fw, Scenario::UsbRetry, envelope);
    let from = m.soc.periph.cycles;
    m.watch_calls(fw.symbol("usb_attach_now"));
    m.run_ms(1100);
    assert!(
        !m.watched_calls.is_empty(),
        "no USB retry within a second (fm1-emu has no USB controller)"
    );
    (m.watched_starts[0] - from, m.watched_calls[0])
}

/// One trial: the note-on's bytes start `offset` cycles after `boot`, the note-off's
/// `NOTE_MS` and `off_offset` cycles after the note-on's.
/// The held note's peak on the host core: the live instrument as it boots (SAW LEAD, the
/// 6581) into a host chip, C4 at velocity 100 for `NOTE_MS`, its largest deviation in the
/// last `HELD_MS` from the mean of the `BASELINE_MS` before the note, as trial measures it.
fn host_held_peak() -> i32 {
    use sidou_sys::{
        live, live_init, live_note_on, live_sid_sink, live_sid_target, sid, SID_CLOCK_PAL,
        SID_MODEL_6581,
    };
    const FS: u64 = 44_118; // platform/fm1/audio.h, AUDIO_SAMPLE_RATE
    let mut chip = Box::<sid>::default();
    let mut target = Box::new(live_sid_target {
        chip: &mut *chip,
        clock_hz: SID_CLOCK_PAL,
        sample_rate: FS as u32,
        ..Default::default()
    });
    let mut l = Box::<live>::default();
    let ctx: *mut live_sid_target = &mut *target;
    let mut render = |ms: u64| {
        let mut out = vec![0i16; (ms * FS / 1000) as usize];
        unsafe { sidou_sys::sid_render(&mut *chip, out.as_mut_ptr(), out.len() as u32) };
        out
    };
    unsafe { live_init(&mut *l, Some(live_sid_sink), ctx.cast(), SID_MODEL_6581) };
    render(1000); // the chip's power-on release and the DC blocker settle
    let base = render(BASELINE_MS);
    let mean = base.iter().map(|&s| i32::from(s)).sum::<i32>() / base.len() as i32;
    unsafe { live_note_on(&mut *l, C4, 100) };
    let note = render(NOTE_MS);
    note[note.len() - (HELD_MS * FS / 1000) as usize..]
        .iter()
        .map(|&s| (i32::from(s) - mean).abs())
        .max()
        .unwrap()
}

/// What a trial's samples are measured against: the 20 ms before the note-on (its mean, and
/// the threshold SILENCE_MARGIN past its largest deviation), or the same run without the notes
/// (the sweep case), sample for sample and snapshot for snapshot, SILENCE_MARGIN past it.
enum Baseline {
    Mean {
        mean: i32,
        threshold: i32,
    },
    Run {
        audio: Vec<i16>,
        words: Vec<Vec<i16>>,
    },
}

impl Baseline {
    fn threshold(&self) -> i32 {
        match self {
            Baseline::Mean { threshold, .. } => *threshold,
            Baseline::Run { .. } => SILENCE_MARGIN,
        }
    }
    /// The deviation of played sample `i`.
    fn audio(&self, i: usize, s: i16) -> i32 {
        match self {
            Baseline::Mean { mean, .. } => (i32::from(s) - mean).abs(),
            Baseline::Run { audio, .. } => (i32::from(s) - i32::from(audio[i])).abs(),
        }
    }
    /// The deviation of word `w` of buffer snapshot `k`.
    fn word(&self, k: usize, w: usize, s: i16) -> i32 {
        match self {
            Baseline::Mean { mean, .. } => (i32::from(s) - mean).abs(),
            Baseline::Run { words, .. } => (i32::from(s) - i32::from(words[k][w])).abs(),
        }
    }
}

/// A trial's run: after `boot`, `offset` cycles, the note-on (if `notes`), NOTE_MS, `off_offset`
/// cycles, the note-off (if `notes`), RELEASE_MS. With the sweep case, its CCs throughout.
/// Returns the machine and, from the note-on's injection: the cycle it went in, the buffer
/// snapshots' count at the note-off's run and the played samples' count then, and the cycle
/// the note-off went in.
fn run_trial(
    fw: &Firmware,
    offset: u64,
    off_offset: u64,
    scenario: Scenario,
    envelope: Envelope,
    notes: bool,
    before: impl FnOnce(&mut Machine),
) -> (Machine, u64, usize, usize, u64) {
    let mut m = boot(fw, scenario, envelope);
    let c = Clock {
        hz: m.soc.periph.hz,
    };
    let swept = matches!(scenario, Scenario::CharacterSweep | Scenario::ProfileStep);
    let mut sweep = swept.then(|| Sweep::new(scenario));
    let mut run = |m: &mut Machine, cycles: u64| match &mut sweep {
        Some(s) => s.run(m, &c, cycles),
        None => m.run_cycles(cycles),
    };
    m.watch_calls(fw.symbol("ui_poll"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    run(&mut m, offset);
    before(&mut m);
    let injected = m.soc.periph.cycles;
    if notes {
        m.midi(&[0x90, C4, 100]);
    }
    run(&mut m, c.of_ms(NOTE_MS));
    let held_until = m.buffer_snapshots.len();
    let held_len = played(&m).len();
    run(&mut m, off_offset);
    let off_injected = m.soc.periph.cycles;
    if notes {
        m.midi(&[0x80, C4, 0]);
    }
    run(&mut m, c.of_ms(RELEASE_MS));
    m.snapshot_buffer(None);
    (m, injected, held_until, held_len, off_injected)
}

fn trial(
    fw: &Firmware,
    offset: u64,
    off_offset: u64,
    scenario: Scenario,
    envelope: Envelope,
) -> Trial {
    let abuf = fw.symbol("abuf");
    let words = 2 * 2 * HALF_FRAMES as u32;
    // Before the note-on: the baseline, the buffer silent, then the snapshots start.
    let mut base = None;
    let mut played_before = 0;
    let mut start = |m: &mut Machine| {
        let samples: Vec<i32> = played(m)
            .iter()
            .rev()
            .take(2 * (BASELINE_MS * 44_100 / 1000) as usize)
            .map(|&s| i32::from(s))
            .collect();
        let mean = samples.iter().sum::<i32>() / samples.len() as i32;
        let threshold = samples.iter().map(|s| (s - mean).abs()).max().unwrap() + SILENCE_MARGIN;
        let buffer: Vec<i16> = (0..words)
            .map(|i| (m.soc.read32(abuf + 4 * i) as i32 >> 8) as i16)
            .collect();
        base = Some((mean, threshold, buffer));
        // The voices' gates, then their envelope counters.
        m.snapshot_bytes = env_addresses(m, fw, ilp32::sid_env::gate);
        let counters = env_addresses(m, fw, ilp32::sid_env::counter);
        m.snapshot_bytes.extend(counters);
        m.snapshot_buffer(Some((abuf, words)));
        played_before = m.half_played.len();
    };
    let slices_at = |m: &mut Machine| {
        m.soc
            .read32(fw.symbol("out") + ilp32::reg_ring_out::slices as u32)
    };
    let mut slices_before = 0;
    let (mut m, injected, held_until, held_len, off_injected) =
        run_trial(fw, offset, off_offset, scenario, envelope, true, |m| {
            slices_before = slices_at(m);
            start(m);
        });
    let slices = slices_at(&mut m) - slices_before;
    let l = m.soc.read32(fw.symbol("ui") + ilp32::views::l as u32);
    let profile = m.soc.read8(l + ilp32::live::profile as u32);
    let (mean, threshold, buffer) = base.unwrap();
    let baseline = if scenario == Scenario::CharacterSweep {
        let (r, ..) = run_trial(fw, offset, off_offset, scenario, envelope, false, |m| {
            m.snapshot_bytes.clear();
            m.snapshot_buffer(Some((abuf, words)));
        });
        Baseline::Run {
            audio: played(&r).to_vec(),
            words: r.buffer_snapshots.iter().map(|s| s.words.clone()).collect(),
        }
    } else {
        assert!(
            buffer
                .iter()
                .all(|&s| (i32::from(s) - mean).abs() <= threshold),
            "the buffer sounds before the note-on"
        );
        Baseline::Mean { mean, threshold }
    };
    let threshold = baseline.threshold();
    let c = Clock {
        hz: m.soc.periph.hz,
    };
    let arrived = injected + 3 * c.byte();
    let off_arrived = off_injected + 3 * c.byte();

    assert_eq!(played(&m).len(), m.half_played.len() * HALF_FRAMES * 2);
    // Up to the note-off: the note held.
    let audio = &played(&m)[..held_len];
    let held = (audio.len() - 2 * (HELD_MS * 44_100 / 1000) as usize..audio.len())
        .map(|i| baseline.audio(i, audio[i]))
        .max()
        .unwrap();
    // At least half the held note's peak on the host core: clearly a note, whatever level
    // the preset and the filter give it, at the MASTER pot's gain (the audio taper's at the
    // pot's power-on reading: the host core's peak is the chip's own).
    let gain =
        unsafe { sidou_sys::master_gain_of(u32::from(common::POT), sidou_sys::LIVE_MASTER_MAX) };
    let reference =
        (i64::from(host_held_peak()) * i64::from(gain) / i64::from(sidou_sys::MASTER_UNITY)) as i32;
    assert!(
        2 * held > reference,
        "the note is silent: {held}, against {reference} on the host"
    );
    let level = (f64::from(held) * NOTE_LEVEL) as i32;
    assert!(level > threshold, "the note level is within the baseline");

    // The block that takes the gate, and the first blocks past the thresholds.
    let snaps = &m.buffer_snapshots;
    let gated = snaps
        .iter()
        .position(|s| s.bytes.iter().take(3).any(|&g| g != 0))
        .expect("no voice was gated");
    let first_over = |t: i32| {
        snaps
            .iter()
            .enumerate()
            .position(|(k, s)| {
                s.words
                    .iter()
                    .enumerate()
                    .any(|(w, &x)| baseline.word(k, w, x) > t)
            })
            .expect("the note never reached the buffer")
    };
    let (sounds, note) = (first_over(threshold), first_over(level));
    assert!(
        snaps[gated].entry > arrived,
        "a voice was gated before the note-on arrived"
    );
    assert_eq!(
        sounds, gated,
        "the first non-silent sample is not in the block that took the note"
    );

    // When the first sample past a threshold left the DAC.
    let plays = |t: i32| {
        let first = (played_before * HALF_FRAMES..audio.len() / 2)
            .find(|&i| {
                baseline.audio(2 * i, audio[2 * i]) > t
                    || baseline.audio(2 * i + 1, audio[2 * i + 1]) > t
            })
            .expect("the note never played");
        m.half_played[first / HALF_FRAMES] - c.block() + (first % HALF_FRAMES) as u64 * c.frame()
    };
    let (audible, note_audible) = (plays(threshold), plays(level));
    assert!(audible > snaps[sounds].exit && note_audible > snaps[note].exit);

    // The note-off: the block that clears the voice's gate, and the first with its envelope
    // counter below the level it held.
    let v = (0..3).find(|&v| snaps[gated].bytes[v] != 0).unwrap();
    let sustain = snaps[held_until - 1].bytes[3 + v];
    assert!(
        snaps[held_until - 4..held_until]
            .iter()
            .all(|s| s.bytes[v] != 0 && s.bytes[3 + v] == sustain),
        "the note was not holding its sustain level at the note-off"
    );
    let off = held_until
        + snaps[held_until..]
            .iter()
            .position(|s| s.bytes[v] == 0)
            .expect("the note-off never cleared the gate");
    let release = off
        + snaps[off..]
            .iter()
            .position(|s| s.bytes[3 + v] < sustain)
            .expect("the release never started");
    assert!(
        snaps[off].entry > off_arrived,
        "the gate was cleared before the note-off arrived"
    );

    let wait_ticks = m.soc.read32(fw.symbol("midi_stats") + 12);
    Trial {
        taken: snaps[gated].entry - arrived,
        buffer: snaps[sounds].exit - arrived,
        audible: audible - arrived,
        note_buffer: snaps[note].exit - arrived,
        note_audible: note_audible - arrived,
        off_taken: snaps[off].entry - off_arrived,
        off_buffer: snaps[off].exit - off_arrived,
        release_buffer: snaps[release].exit - off_arrived,
        ring_wait_us: f64::from(wait_ticks) / 24.0,
        ui_poll_max: m.watched_calls.iter().copied().max().unwrap_or(0),
        isr_max: snaps.iter().map(|s| s.exit - s.entry).max().unwrap(),
        pass_max: m.passes.iter().copied().max().unwrap_or(0),
        slices,
        profile,
    }
}

struct Summary {
    min: f64,
    median: f64,
    mean: f64,
    max: f64,
}

fn summary(mut ms: Vec<f64>) -> Summary {
    ms.sort_by(f64::total_cmp);
    let n = ms.len();
    Summary {
        min: ms[0],
        median: if n % 2 == 1 {
            ms[n / 2]
        } else {
            (ms[n / 2 - 1] + ms[n / 2]) / 2.0
        },
        mean: ms.iter().sum::<f64>() / n as f64,
        max: ms[n - 1],
    }
}

fn json(s: &Summary) -> serde_json::Value {
    serde_json::json!({
        "min_ms": s.min, "median_ms": s.median, "mean_ms": s.mean, "max_ms": s.max,
    })
}

/// `SIDOU_LATENCY_TRIALS`, or the case's own count.
fn trials(default: usize) -> usize {
    match std::env::var("SIDOU_LATENCY_TRIALS") {
        Ok(v) => v
            .parse()
            .ok()
            .filter(|&n| n > 0)
            .expect("SIDOU_LATENCY_TRIALS is a count of trials, 1 or more"),
        Err(_) => default,
    }
}

fn measure(scenario: Scenario, envelope: Envelope, default_trials: usize) {
    let Some(build) = build_dir() else {
        skip_message("the latency test");
        return;
    };
    let fw = Firmware::load(&build);
    let c = Clock {
        hz: Machine::boot(&fw.image).soc.periph.hz,
    };
    // common::POLL_INSTRUCTIONS, which screen.rs holds the views' worst ui_poll to, is this
    // gate's poll at this clock.
    assert_eq!(c.hz, CPU_HZ, "fm1-emu's clock is not common::CPU_HZ");
    let mut rng = Rng(0x5EED_0000 + scenario as u64);
    // The note-on's last byte lands in the window [from, from + span) after `boot`.
    let (from, span, attach) = if scenario == Scenario::UsbRetry {
        // From a millisecond and a poll before the attach to its end.
        let (start, instructions) = usb_attach(&fw, envelope);
        let lead = c.poll() + c.of_ms(1);
        (start - lead, lead + instructions, Some(instructions))
    } else {
        (3 * c.byte(), c.of_ms(PHASE_WINDOW_MS), None)
    };
    let n = trials(default_trials);
    let trials: Vec<Trial> = (0..n)
        .map(|_| {
            let arrive = from + rng.below(span);
            let off = rng.below(c.block());
            trial(&fw, arrive - 3 * c.byte(), off, scenario, envelope)
        })
        .collect();
    let ms = |f: fn(&Trial) -> u64| summary(trials.iter().map(|t| c.ms(f(t))).collect());
    let block_plus_poll = c.block() + c.poll();
    // The render allowance: the run's longest audio interrupt, within a block of the budget.
    let render = trials.iter().map(|t| t.isr_max).max().unwrap();
    let budget = (AUDIO_TARGET * HALF_FRAMES as f64 / INSTRUCTIONS_PER_CYCLE) as u64;
    assert!(
        render <= budget,
        "an audio interrupt took {render} cycles, over the audio budget's block ({budget})"
    );
    let report = serde_json::json!({
        "scenario": format!("{scenario:?}"),
        "envelope": format!("{envelope:?}"),
        "trials": n,
        "from": "the note-on's last byte in the UART (its 3 bytes take 0.96 ms before)",
        "taken_by_the_audio_interrupt": json(&ms(|t| t.taken)),
        "first_sample_in_the_dma_buffer": json(&ms(|t| t.buffer)),
        "first_sample_audible": json(&ms(|t| t.audible)),
        "render": json(&ms(|t| t.buffer - t.taken)),
        "note_envelope_in_the_dma_buffer": json(&ms(|t| t.note_buffer)),
        "note_envelope_audible": json(&ms(|t| t.note_audible)),
        "adsr_delay": json(&ms(|t| t.note_buffer - t.buffer)),
        "note_off_taken_by_the_audio_interrupt": json(&ms(|t| t.off_taken)),
        "release_first_step_in_the_dma_buffer": json(&ms(|t| t.release_buffer)),
        "release_adsr_delay": json(&ms(|t| t.release_buffer - t.off_buffer)),
        "attack_in_the_taking_block": trials.iter().filter(|t| t.note_buffer == t.buffer).count(),
        "release_in_the_taking_block": trials
            .iter()
            .filter(|t| t.release_buffer == t.off_buffer)
            .count(),
        "block_ms": c.ms(c.block()),
        "poll_ms": c.ms(c.poll()),
        "block_plus_poll_ms": c.ms(block_plus_poll),
        "render_allowance_ms": c.ms(render),
        "render_allowance_cycles": render,
        "longest_audio_interrupt_per_trial": json(&ms(|t| t.isr_max)),
        "usb_attach_instructions": attach,
        "max_midi_ring_wait_us": trials.iter().map(|t| t.ring_wait_us).fold(0.0, f64::max),
        "max_ui_poll_instructions": trials.iter().map(|t| t.ui_poll_max).max(),
        "max_main_loop_pass_instructions": trials.iter().map(|t| t.pass_max).max(),
        "poll_instructions": POLL_INSTRUCTIONS,
        "min_preparation_slices": trials.iter().map(|t| t.slices).min(),
    });
    println!("{report}");
    let name = format!(
        "latency_{}_{}.json",
        format!("{scenario:?}").to_lowercase(),
        format!("{envelope:?}").to_lowercase()
    );
    std::fs::write(build.join(&name), report.to_string())
        .unwrap_or_else(|e| panic!("writing {name}: {e}"));
    // A stall of the main loop comes on top of the plan's bound, all of it at worst.
    let bound = block_plus_poll + attach.unwrap_or(0);
    let lfsr = (LFSR_PERIOD + 2) * c.hz / SID_HZ;
    let adsr = lfsr + c.of_ms(ATTACK_MS);
    if matches!(scenario, Scenario::CharacterSweep | Scenario::ProfileStep) {
        let pass = trials.iter().map(|t| t.pass_max).max().unwrap();
        assert!(
            pass <= POLL_INSTRUCTIONS,
            "a main-loop pass took {pass} instructions ({scenario:?}), over the poll allowance \
             ({POLL_INSTRUCTIONS})"
        );
    }
    if scenario == Scenario::ProfileStep {
        // A slice every tick from the first CC on (the trial runs over 200 ms), and the chip
        // never swapped: every note played on the R4AR it booted with.
        for t in &trials {
            assert!(t.slices >= 15, "only {} slices ran: {t:?}", t.slices);
            assert_eq!(u32::from(t.profile), sidou_sys::SID_CHIP_6581_R4AR, "{t:?}");
        }
    }
    for t in &trials {
        // The note is in the block that starts within a block and a poll of its arrival...
        assert!(
            t.taken <= bound,
            "a note-on waited {:.3} ms for its block, over {:.3} ms: {t:?}",
            c.ms(t.taken),
            c.ms(bound)
        );
        // ...and its first sample is in the buffer once that block is rendered...
        assert!(
            t.buffer <= bound + render,
            "a note-on's first sample reached the buffer {:.3} ms after it arrived, over \
             {:.3} ms and a {:.3} ms render: {t:?}",
            c.ms(t.buffer),
            c.ms(bound),
            c.ms(render)
        );
        // ...which plays one block later.
        assert!(
            t.audible - t.taken <= 2 * c.block(),
            "a note-on's first sample played {:.3} ms after its block was taken: {t:?}",
            c.ms(t.audible - t.taken)
        );
        assert!(
            t.off_taken <= bound,
            "a note-off waited {:.3} ms for its block, over {:.3} ms: {t:?}",
            c.ms(t.off_taken),
            c.ms(bound)
        );
        match envelope {
            // The envelope is up in the block that takes the note.
            Envelope::Instant => {
                assert_eq!(
                    t.note_buffer,
                    t.buffer,
                    "the note's envelope came {:.3} ms after its first sample: {t:?}",
                    c.ms(t.note_buffer - t.buffer)
                );
                // The release steps in the block that takes the note-off.
                assert_eq!(
                    t.release_buffer,
                    t.off_buffer,
                    "the release came {:.3} ms after the note-off's block: {t:?}",
                    c.ms(t.release_buffer - t.off_buffer)
                );
            }
            // The envelope rises within the ADSR delay's longest and the attack, a block's
            // granularity on top.
            Envelope::Authentic => {
                assert!(
                    t.note_buffer - t.buffer <= adsr + c.block(),
                    "the note's envelope came {:.3} ms after its first sample: {t:?}",
                    c.ms(t.note_buffer - t.buffer)
                );
                // The release's first step within the ADSR delay's longest.
                assert!(
                    t.release_buffer - t.off_buffer <= lfsr + c.block(),
                    "the release came {:.3} ms after the note-off's block: {t:?}",
                    c.ms(t.release_buffer - t.off_buffer)
                );
            }
        }
    }
}

#[test]
fn a_note_on_reaches_the_buffer_within_a_block_and_a_poll() {
    measure(Scenario::Idle, Envelope::Instant, 64);
}

#[test]
fn a_note_on_reaches_the_buffer_within_a_block_and_a_poll_while_the_screen_redraws() {
    measure(Scenario::Busy, Envelope::Instant, 64);
}

#[test]
fn a_usb_retry_stalls_a_note_on_by_its_attach_at_most() {
    measure(Scenario::UsbRetry, Envelope::Instant, 32);
}

/// The chip's timing, CC 26 = 127 after the boot (checked on the rendered chip): the attack
/// and the release wait for the ADSR delay at most. The screen and USB cases are the same
/// path as with INSTANT, so this one case covers the mode.
#[test]
fn an_authentic_envelope_waits_for_the_adsr_delay_at_most() {
    measure(Scenario::Idle, Envelope::Authentic, 16);
}

/// A note-on while the chip's character CCs sweep (the module's doc): the ramp's table
/// building in the main loop holds no note back past the bounds, and no pass exceeds the poll
/// allowance.
#[test]
fn a_note_on_keeps_its_bounds_while_the_character_sweeps() {
    measure(Scenario::CharacterSweep, Envelope::Instant, 24);
}

/// A note-on while a profile step is in flight (the module's doc): the spare chip's
/// preparation, a slice every tick, holds no note back past the bounds (decision record 0006:
/// M1's 5 ms exception, a reset in one pass, is gone), and no pass exceeds the poll
/// allowance.
#[test]
fn a_note_on_keeps_its_bounds_while_a_profile_step_is_prepared() {
    measure(Scenario::ProfileStep, Envelope::Instant, 24);
}

/// A note-on while a file is saved (M2a Task 4): the store's programs in write windows, with
/// interrupts masked, hold no note back past the bounds, and no pass exceeds the poll
/// allowance.
#[test]
fn a_note_on_keeps_its_bounds_while_a_file_is_saved() {
    measure(Scenario::Saving, Envelope::Instant, 24);
}
