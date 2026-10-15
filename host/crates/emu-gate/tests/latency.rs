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
//! The cases' trial counts keep CI short; `SIDOU_LATENCY_TRIALS` sets every case's (64
//! reproduces decision record 0005's run). Each case writes its report into the fm1 build
//! directory as `latency_<case>_<envelope>.json`.

mod common;

use common::{
    build_dir, skip_message, Firmware, Machine, AUDIO_TARGET, CPU_HZ, HALF_FRAMES, POLL_HZ,
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
fn trial(
    fw: &Firmware,
    offset: u64,
    off_offset: u64,
    scenario: Scenario,
    envelope: Envelope,
) -> Trial {
    let mut m = boot(fw, scenario, envelope);
    let c = Clock {
        hz: m.soc.periph.hz,
    };
    m.watch_calls(fw.symbol("ui_poll"));
    m.run_cycles(offset);
    let base: Vec<i32> = played(&m)
        .iter()
        .rev()
        .take(2 * (BASELINE_MS * 44_100 / 1000) as usize)
        .map(|&s| i32::from(s))
        .collect();
    let mean = base.iter().sum::<i32>() / base.len() as i32;
    let threshold = base.iter().map(|s| (s - mean).abs()).max().unwrap() + SILENCE_MARGIN;
    let dev = |s: i16| (i32::from(s) - mean).abs();

    let abuf = fw.symbol("abuf");
    let words = 2 * 2 * HALF_FRAMES as u32;
    let before: Vec<i16> = (0..words)
        .map(|i| (m.soc.read32(abuf + 4 * i) as i32 >> 8) as i16)
        .collect();
    assert!(
        before.iter().all(|&s| dev(s) <= threshold),
        "the buffer sounds before the note-on"
    );
    // The voices' gates, then their envelope counters.
    m.snapshot_bytes = env_addresses(&mut m, fw, ilp32::sid_env::gate);
    let counters = env_addresses(&mut m, fw, ilp32::sid_env::counter);
    m.snapshot_bytes.extend(counters);
    m.snapshot_buffer(Some((abuf, words)));
    let played_before = m.half_played.len();
    let injected = m.soc.periph.cycles;
    m.midi(&[0x90, C4, 100]);
    let arrived = injected + 3 * c.byte();
    m.run_ms(NOTE_MS);
    let held_until = m.buffer_snapshots.len();
    let held_len = played(&m).len();
    m.run_cycles(off_offset);
    let off_injected = m.soc.periph.cycles;
    m.midi(&[0x80, C4, 0]);
    let off_arrived = off_injected + 3 * c.byte();
    m.run_ms(RELEASE_MS);
    m.snapshot_buffer(None);

    assert_eq!(played(&m).len(), m.half_played.len() * HALF_FRAMES * 2);
    // Up to the note-off: the note held.
    let audio = &played(&m)[..held_len];
    let held = (audio.len() - 2 * (HELD_MS * 44_100 / 1000) as usize..audio.len())
        .map(|i| dev(audio[i]))
        .max()
        .unwrap();
    // SAW LEAD peaks at about 2,500 since M1b Task 6 put the 6581's filter at reSIDfp's level
    // (about 4,000 before, 2.4 dB over reSIDfp's): half of that is clearly a note.
    assert!(held > 1250, "the note is silent: {held}");
    let level = (f64::from(held) * NOTE_LEVEL) as i32;
    assert!(level > threshold, "the note level is within the baseline");

    // The block that takes the gate, and the first blocks past the thresholds.
    let snaps = &m.buffer_snapshots;
    let gated = snaps
        .iter()
        .position(|s| s.bytes.iter().any(|&g| g != 0))
        .expect("no voice was gated");
    let first_over = |t: i32| {
        snaps
            .iter()
            .position(|s| s.words.iter().any(|&x| dev(x) > t))
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
            .find(|&i| dev(audio[2 * i]) > t || dev(audio[2 * i + 1]) > t)
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
