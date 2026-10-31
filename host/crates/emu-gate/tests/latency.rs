// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::panel::K1;
use common::{
    build_dir, skip_message, Firmware, Machine, AUDIO_TARGET, CPU_HZ, HALF_FRAMES, POLL_HZ,
    POLL_INSTRUCTIONS,
};
use fm1_core::Bus;
use sidou_sys::ilp32;

const C4: u8 = 60;
const PHASE_WINDOW_MS: u64 = 40;
const BASELINE_MS: u64 = 20;
const NOTE_MS: u64 = 140;
const HELD_MS: u64 = 10;
const RELEASE_MS: u64 = 45;
const SILENCE_MARGIN: i32 = 8;
const NOTE_LEVEL: f64 = 1.0 / 31.6;
const KNOBS: [u8; 3] = [K1, K1 + 1, K1 + 2];
const KNOB_TURN_DETENTS: i32 = -50;
const STORE_FM1_FILE: u32 = 3;
const STORE_HOOK_LENGTH: u32 = 8;
const SAVED_FILE_BYTES: u32 = 64 * 1024;
const SAVING_WRITE_WINDOW: u32 = 16;
const ENVELOPE_CC_SETTLE_MS: u64 = 10;
const BYTE_BITS: u64 = 10;
const BAUD: u64 = 31_250;
const FRAME_T4: u64 = 544;
const T4_HZ: u64 = 24_000_000;
const SID_HZ: u64 = 985_248;
const LFSR_PERIOD: u64 = 32_767;
const ATTACK_MS: u64 = 2;
const CC_ENVELOPE_MODE: u8 = 26;
const INSTRUCTIONS_PER_CYCLE: f64 = 1.0;

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

#[derive(Debug, Clone, Copy, PartialEq)]
enum Envelope {
    Instant,
    Authentic,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Scenario {
    Idle,
    Busy,
    UsbRetry,
    CharacterSweep,
    ProfileStep,
    Saving,
}

const SWEEP_MS: u64 = 5;
const SWEEP_STEP: i32 = 24;
const CC_AGE: u8 = 86;
const CC_DAC: u8 = 87;

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

#[derive(Debug, Clone, Copy)]
struct Trial {
    taken: u64,
    buffer: u64,
    audible: u64,
    note_buffer: u64,
    note_audible: u64,
    off_taken: u64,
    off_buffer: u64,
    release_buffer: u64,
    ring_wait_us: f64,
    ui_poll_max: u64,
    isr_max: u64,
    pass_max: u64,
    slices: u32,
    profile: u8,
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % n
    }
}

fn played(m: &Machine) -> &[i16] {
    &m.soc.periph.audio
}

fn env_addresses(m: &mut Machine, fw: &Firmware, field: usize) -> Vec<u32> {
    let base = fw.chips();
    let active = m.soc.read8(base + ilp32::reg_ring_chips::active as u32);
    let chip = base + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32;
    (0..3usize)
        .map(|v| chip + (ilp32::sid::env + v * ilp32::sid_env::SIZE + field) as u32)
        .collect()
}

fn boot(fw: &Firmware, scenario: Scenario, envelope: Envelope) -> Machine {
    let mut m = common::booted(fw);
    if envelope == Envelope::Authentic {
        m.midi(&[0xB0, CC_ENVELOPE_MODE, 127]);
        m.run_ms(ENVELOPE_CC_SETTLE_MS);
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
        m.soc
            .write32(fw.symbol("store_fm1_window"), SAVING_WRITE_WINDOW);
        let hook = fw.symbol("store_fm1_hook");
        m.soc.write32(hook + STORE_HOOK_LENGTH, SAVED_FILE_BYTES);
        m.soc.write32(hook, STORE_FM1_FILE);
        m.run_ms(50);
        let progs = m.soc.read32(fw.symbol("store_fm1_counts"));
        assert!(progs > 0, "the file is not being written");
    }
    if scenario == Scenario::Busy {
        for k in KNOBS {
            m.turn(k, KNOB_TURN_DETENTS);
        }
        m.run_ms(BASELINE_MS);
    }
    m
}

fn usb_attach(fw: &Firmware, envelope: Envelope) -> (u64, u64) {
    let mut m = boot(fw, Scenario::UsbRetry, envelope);
    let from = m.soc.periph.cycles;
    m.watch_calls(fw.symbol("usb_probe"));
    m.run_ms(1100);
    assert!(
        !m.watched_calls.is_empty(),
        "no USB retry within a second (fm1-emu has no USB controller)"
    );
    (m.watched_starts[0] - from, m.watched_calls[0])
}

fn host_held_peak() -> i32 {
    use sidou_sys::{
        live, live_init, live_note_on, live_sid_sink, live_sid_target, sid, SID_CLOCK_PAL,
        SID_MODEL_6581,
    };
    const FS: u64 = common::SAMPLE_RATE as u64;
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
    render(1000);
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
    fn audio(&self, i: usize, s: i16) -> i32 {
        match self {
            Baseline::Mean { mean, .. } => (i32::from(s) - mean).abs(),
            Baseline::Run { audio, .. } => (i32::from(s) - i32::from(audio[i])).abs(),
        }
    }
    fn word(&self, k: usize, w: usize, s: i16) -> i32 {
        match self {
            Baseline::Mean { mean, .. } => (i32::from(s) - mean).abs(),
            Baseline::Run { words, .. } => (i32::from(s) - i32::from(words[k][w])).abs(),
        }
    }
}

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
    let audio = &played(&m)[..held_len];
    let held = (audio.len() - 2 * (HELD_MS * 44_100 / 1000) as usize..audio.len())
        .map(|i| baseline.audio(i, audio[i]))
        .max()
        .unwrap();
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
    assert_eq!(c.hz, CPU_HZ, "fm1-emu's clock is not common::CPU_HZ");
    let mut rng = Rng(0x5EED_0000 + scenario as u64);
    let (from, span, attach) = if scenario == Scenario::UsbRetry {
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
        for t in &trials {
            assert!(t.slices >= 15, "only {} slices ran: {t:?}", t.slices);
            assert_eq!(u32::from(t.profile), sidou_sys::SID_CHIP_6581_R4AR, "{t:?}");
        }
    }
    for t in &trials {
        assert!(
            t.taken <= bound,
            "a note-on waited {:.3} ms for its block, over {:.3} ms: {t:?}",
            c.ms(t.taken),
            c.ms(bound)
        );
        assert!(
            t.buffer <= bound + render,
            "a note-on's first sample reached the buffer {:.3} ms after it arrived, over \
             {:.3} ms and a {:.3} ms render: {t:?}",
            c.ms(t.buffer),
            c.ms(bound),
            c.ms(render)
        );
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
            Envelope::Instant => {
                assert_eq!(
                    t.note_buffer,
                    t.buffer,
                    "the note's envelope came {:.3} ms after its first sample: {t:?}",
                    c.ms(t.note_buffer - t.buffer)
                );
                assert_eq!(
                    t.release_buffer,
                    t.off_buffer,
                    "the release came {:.3} ms after the note-off's block: {t:?}",
                    c.ms(t.release_buffer - t.off_buffer)
                );
            }
            Envelope::Authentic => {
                assert!(
                    t.note_buffer - t.buffer <= adsr + c.block(),
                    "the note's envelope came {:.3} ms after its first sample: {t:?}",
                    c.ms(t.note_buffer - t.buffer)
                );
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

#[test]
fn an_authentic_envelope_waits_for_the_adsr_delay_at_most() {
    measure(Scenario::Idle, Envelope::Authentic, 16);
}

#[test]
fn a_note_on_keeps_its_bounds_while_the_character_sweeps() {
    measure(Scenario::CharacterSweep, Envelope::Instant, 24);
}

#[test]
fn a_note_on_keeps_its_bounds_while_a_profile_step_is_prepared() {
    measure(Scenario::ProfileStep, Envelope::Instant, 24);
}

#[test]
fn a_note_on_keeps_its_bounds_while_a_file_is_saved() {
    measure(Scenario::Saving, Envelope::Instant, 24);
}
