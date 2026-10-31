// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use gt2_ref::trace::trace;
use sidou_sys::*;
use sidou_tests::residfp_engine::ResidfpEngine;
use sidou_tests::signal_metrics::*;
use sidou_tests::song::*;
use sidou_tests::transport::*;

const DRUMS: usize = GT2_KIT_DRUMS as usize;
const NAMES: [&str; DRUMS] = [
    "KICK", "SNARE", "CL HAT", "OP HAT", "CLAP", "TOM LO", "TOM HI", "RIM",
];
const FS: f64 = SAMPLE_RATE as f64;
const FRAME_SAMPLES: usize = 882;
const VOICE3_AD: u8 = 0x13;
const ENV3: u8 = 0x1c;

fn drum_song(d: usize) -> Vec<u8> {
    let mut s = SngSpec::new();
    s.instr = (0..DRUMS).map(kit_instr).collect();
    s.tables = kit_tables().to_vec();
    let mut p0 = vec![REST; 64];
    p0[0] = [PATTERN_NOTE_C3, d as u8 + 1, 0, 0];
    s.patterns[0].0 = p0;
    s.encode()
}

fn drum_on_chip_a_from_its_row(d: usize, model: sid_model, ms: usize) -> Vec<f32> {
    let mut song = Song::host_from(&drum_song(d));
    let mut t = Transport::new(&mut song);
    unsafe { sid_init(&mut *t.chip, model, SID_CLOCK_PAL, SAMPLE_RATE) };
    assert!(t.play(0, 0));
    assert!(t.run_until(1000, |t| t.calls() > 0));
    let n = ms * SAMPLE_RATE as usize / 1000;
    let mut out = Vec::with_capacity(n + BLOCK);
    let mut b = [0i16; BLOCK];
    while out.len() < n {
        t.pass();
        t.block(&mut b);
        out.extend(b.iter().map(|&v| f32::from(v) / 32768.0));
    }
    out.truncate(n);
    out
}

fn onset(x: &[f32]) -> usize {
    x.iter()
        .position(|v| v.abs() > 0.02)
        .expect("the drum sounds")
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / x.len() as f64).sqrt()
}

fn db(v: f64) -> f64 {
    20.0 * v.max(1e-9).log10()
}

fn two_pole_low_pass(x: &[f32], fc: f64) -> Vec<f32> {
    let a = 1.0 - (-2.0 * std::f64::consts::PI * fc / FS).exp();
    let (mut y1, mut y2) = (0.0f64, 0.0f64);
    x.iter()
        .map(|&v| {
            y1 += a * (f64::from(v) - y1);
            y2 += a * (y1 - y2);
            y2 as f32
        })
        .collect()
}

fn models() -> [(sid_model, &'static str); 2] {
    [(SID_MODEL_6581, "6581"), (SID_MODEL_8580, "8580")]
}

#[test]
fn every_drum_sounds_on_both_chips_and_fades() {
    for (d, name) in NAMES.iter().enumerate() {
        for (model, m) in models() {
            let x = drum_on_chip_a_from_its_row(d, model, 1500);
            let on = onset(&x);
            let start = db(rms(&x[on..on + FRAME_SAMPLES / 2]));
            let end = db(rms(&x[x.len() - FRAME_SAMPLES * 5..]));
            println!(
                "{} {m}: onset at {on}, {start:.1} dB, the last 100 ms {end:.1} dB",
                name
            );
            assert!(start > -32.0, "{} {m}: {start:.1} dB at its start", name);
            assert!(
                end < start - 30.0,
                "{} {m}: still {end:.1} dB after 1.5 s",
                name
            );
        }
    }
}

#[test]
fn the_kick_and_the_toms_fall_in_pitch() {
    for (model, m) in models() {
        let f0 = |d: usize, frame: usize| {
            let x = drum_on_chip_a_from_its_row(d, model, 400);
            let at = onset(&x) + frame * FRAME_SAMPLES;
            estimate_f0(&x[at..at + FRAME_SAMPLES], FS, 30.0, 1000.0)
        };
        let (kick_early, kick_body) = (f0(GT2_KIT_KICK as usize, 0), f0(GT2_KIT_KICK as usize, 6));
        println!("{m}: kick {kick_early:.0} Hz, then {kick_body:.0} Hz");
        assert!(kick_early > 1.4 * kick_body, "{m}: the kick does not fall");
        assert!(
            (50.0..68.0).contains(&kick_body),
            "{m}: the kick's body at {kick_body:.0} Hz"
        );
        for tom in [GT2_KIT_LOW_TOM as usize, GT2_KIT_HIGH_TOM as usize] {
            let (a, b) = (f0(tom, 0), f0(tom, 6));
            println!("{m}: {} {a:.0} Hz, then {b:.0} Hz", NAMES[tom]);
            assert!(a > 1.15 * b, "{m}: {} does not fall", NAMES[tom]);
        }
        let ratio = f0(GT2_KIT_HIGH_TOM as usize, 6) / f0(GT2_KIT_LOW_TOM as usize, 6);
        assert!(
            (1.4..1.6).contains(&ratio),
            "{m}: the toms are {ratio:.2} apart, not a fifth"
        );
    }
}

#[test]
fn the_kick_has_weight_on_both_models() {
    let mut share = Vec::new();
    for (model, m) in models() {
        let x = drum_on_chip_a_from_its_row(GT2_KIT_KICK as usize, model, 400);
        let x = &x[onset(&x)..];
        let low = db(rms(&two_pole_low_pass(x, 120.0))) - db(rms(x));
        println!("{m}: the kick's band below 120 Hz is {low:.1} dB of it");
        assert!(low > -4.0, "{m}: the kick's low band is {low:.1} dB of it");
        share.push(low);
    }
    assert!(
        (share[0] - share[1]).abs() < 2.0,
        "the models' kicks differ: {share:?}"
    );
}

#[test]
fn the_hats_are_bright_and_the_closed_one_short() {
    for (model, m) in models() {
        let fall = |d: usize| {
            let x = drum_on_chip_a_from_its_row(d, model, 800);
            let x = &x[onset(&x)..];
            let crossings = x[..FRAME_SAMPLES]
                .windows(2)
                .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                .count();
            let env = rms_envelope_db(x, FRAME_SAMPLES / 2);
            let below = env.iter().position(|&e| e < env[0] - 24.0).unwrap();
            (
                crossings as f64 * FS / FRAME_SAMPLES as f64 / 2.0,
                below * FRAME_SAMPLES / 2,
            )
        };
        let (closed_hz, closed) = fall(GT2_KIT_CLOSED_HAT as usize);
        let (open_hz, open) = fall(GT2_KIT_OPEN_HAT as usize);
        println!(
            "{m}: hats at {closed_hz:.0} and {open_hz:.0} Hz, -24 dB after {closed} and {open}"
        );
        assert!(
            closed_hz > 4000.0 && open_hz > 4000.0,
            "{m}: the hats are dull"
        );
        assert!(
            closed * 3 < open,
            "{m}: the closed hat is not short against the open one"
        );
        assert!(
            closed < FRAME_SAMPLES * 5,
            "{m}: the closed hat rings {closed} samples"
        );
    }
}

#[test]
fn the_clap_is_three_bursts() {
    for (model, m) in models() {
        let x = drum_on_chip_a_from_its_row(GT2_KIT_CLAP as usize, model, 400);
        let x = &x[onset(&x)..];
        let env: Vec<f64> = rms_envelope_db(x, FRAME_SAMPLES / 4);
        let dips = (1..24)
            .filter(|&i| env[i] < env[i - 1] - 3.0 && env[i + 2] > env[i] + 3.0)
            .count();
        println!(
            "{m}: clap {:?}",
            env[..24].iter().map(|e| e.round()).collect::<Vec<_>>()
        );
        assert!(dips >= 2, "{m}: the clap has {dips} dips");
    }
}

fn new_song_instruments_on_voice3(
    before: Option<u8>,
    instr: u8,
    staccato: bool,
    tempo: u8,
) -> Vec<u8> {
    let new = Song::host_from(&new_song_template());
    let raw = new.raw();
    let rows =
        |t: usize, n: u32| (0..n as usize).map(move |r| (raw.ltable[t][r], raw.rtable[t][r]));
    let mut s = SngSpec::new();
    s.instr = raw.instr[..SONG_IO_KIT as usize - 1 + DRUMS].to_vec();
    s.tables = vec![
        rows(0, GT2_KIT_WAVE_ROWS + 4).collect(),
        rows(1, GT2_KIT_PULSE_ROWS + 2).collect(),
        vec![],
        vec![],
    ];
    let mut quiet = vec![REST; 16];
    quiet[0] = [REST[0], 0, 0x0f, tempo];
    let mut v3 = vec![REST; 16];
    if let Some(i) = before {
        v3[0] = [PATTERN_NOTE_C3, i, 0, 0];
    }
    v3[4] = [PATTERN_NOTE_C3, instr, 0, 0];
    if staccato {
        v3[5] = [KEY_OFF, 0, 0, 0];
    }
    s.orders = vec![vec![(vec![0], 0), (vec![0], 0), (vec![1], 0)]];
    s.patterns = vec![(quiet, [0xff, 0, 0, 0]), (v3, [0xff, 0, 0, 0])];
    s.encode()
}

fn new_song_drum(d: usize) -> u8 {
    (SONG_IO_KIT as usize + d) as u8
}

const FRAME_CYCLES: u32 = SID_CLOCK_PAL / 50;
const ENV3_READ_CYCLES: u32 = 100;

fn env3_from_the_row(sng: &[u8], ad: u8, model: sid_model, frame: u32) -> [Vec<u8>; 2] {
    let t = trace(sng, 0, 60, &gt2_ref::Options::default()).expect("GoatTracker plays it");
    let row = t
        .frames
        .iter()
        .rposition(|f| f.calls.iter().any(|c| c.contains(&(VOICE3_AD, ad))))
        .expect("the note's first frame");
    let mut reference = ResidfpEngine::new(model, FS, true);
    let mut chip = Box::new(sid::default());
    unsafe { sid_init(&mut *chip, model, SID_CLOCK_PAL, SAMPLE_RATE) };
    let mut env = [Vec::new(), Vec::new()];
    for (f, calls) in t.frames.iter().enumerate() {
        for &(reg, v) in calls.calls.iter().flatten() {
            reference.write(reg, v);
            unsafe { sid_write(&mut *chip, reg, v) };
        }
        for _ in 0..frame / ENV3_READ_CYCLES {
            reference.clock_silent(ENV3_READ_CYCLES);
            unsafe { sid_clock(&mut *chip, ENV3_READ_CYCLES) };
            if f >= row {
                env[0].push(reference.read(ENV3));
                env[1].push(unsafe { sid_read(&mut *chip, ENV3) });
            }
        }
        reference.clock_silent(frame % ENV3_READ_CYCLES);
        unsafe { sid_clock(&mut *chip, frame % ENV3_READ_CYCLES) };
    }
    env
}

#[test]
fn every_new_instrument_starts_on_time() {
    let step_ms = f64::from(ENV3_READ_CYCLES) * 1000.0 / f64::from(SID_CLOCK_PAL);
    let new = Song::host_from(&new_song_template());
    let ad = |i: u8| new.raw().instr[i as usize - 1][GT2_INS_AD as usize];
    let played: Vec<(u8, &str, f64)> = [(1, "PULSE", 0.0), (2, "BASS", 0.0)]
        .into_iter()
        .chain(
            NAMES
                .iter()
                .enumerate()
                .map(|(d, &name)| (new_song_drum(d), name, 1.0)),
        )
        .collect();
    for (model, m) in models() {
        for frame in FRAME_CYCLES - 4..=FRAME_CYCLES + 4 {
            let frame_ms = f64::from(frame) * 1000.0 / f64::from(SID_CLOCK_PAL);
            for before in [
                None,
                Some(1),
                Some(new_song_drum(GT2_KIT_OPEN_HAT as usize)),
            ] {
                for &(instr, name, frames) in &played {
                    let sng = new_song_instruments_on_voice3(before, instr, false, 6);
                    let at = frames * frame_ms;
                    for (env, chip) in env3_from_the_row(&sng, ad(instr), model, frame)
                        .iter()
                        .zip(["reSIDfp", "SIDOU"])
                    {
                        let mut low = u8::MAX;
                        let on = env.iter().position(|&e| {
                            low = low.min(e);
                            e >= low.saturating_add(0x10)
                        });
                        let on_ms = on.map(|i| (i + 1) as f64 * step_ms);
                        assert!(
                            on_ms.is_some_and(|t| t > at && t < at + 1.5),
                            "{name} {m} on {chip} after {before:?}, {frame}-cycle frames: \
                             its envelope rises {on_ms:?} ms after its row, not {at:.1}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_staccato_drum_sounds_as_a_held_one() {
    for tempo in [6, 5] {
        for (d, name) in NAMES
            .iter()
            .enumerate()
            .filter(|&(d, _)| d != GT2_KIT_RIM as usize)
        {
            let song =
                |staccato| new_song_instruments_on_voice3(None, new_song_drum(d), staccato, tempo);
            let ad = kit_instr(d)[GT2_INS_AD as usize];
            let held = env3_from_the_row(&song(false), ad, SID_MODEL_6581, FRAME_CYCLES);
            let staccato = env3_from_the_row(&song(true), ad, SID_MODEL_6581, FRAME_CYCLES);
            for chip in 0..2 {
                assert_eq!(
                    held[chip], staccato[chip],
                    "{name} at tempo {tempo}: the key off changes its envelope"
                );
            }
        }
    }
}

fn new_song_template() -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        let n = unsafe {
            song_io_template(
                std::ptr::null_mut(),
                out.len() as u32,
                buf.as_mut_ptr(),
                256,
            )
        };
        if n == 0 {
            return out;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
}

#[test]
fn the_new_song_holds_the_kit_and_goattracker_keeps_it() {
    let sng = new_song_template();
    let song = Song::host_from(&sng);
    let raw = song.raw();
    for (d, name) in NAMES.iter().enumerate() {
        assert_eq!(
            raw.instr[SONG_IO_KIT as usize - 1 + d],
            kit_instr(d),
            "{name}"
        );
    }
    assert_eq!(
        gt2_ref::canonical(&sng).expect("GoatTracker loads it"),
        sng,
        "GoatTracker saves NEW's song differently"
    );
    assert_eq!(song.write(false), sng, "SIDOU saves NEW's song differently");
}

#[test]
fn the_new_bass_is_an_octave_down_with_a_thinner_pulse() {
    let sng = new_song_template();
    let song = Song::host_from(&sng);
    let raw = song.raw();
    let (lead, bass) = (&raw.instr[0], &raw.instr[1]);
    let pulse_row = |i: &[u8; 25]| usize::from(i[GT2_INS_PULSE as usize]) - 1;
    assert_eq!(
        raw.ltable[1][pulse_row(lead)],
        0x88,
        "the lead's width $800"
    );
    assert_eq!(
        raw.ltable[1][pulse_row(bass)],
        0x84,
        "the bass's width $400"
    );
    assert_eq!(raw.ltable[1][pulse_row(bass) + 1], 0xff, "the pulse stops");
    let wave_row = |i: &[u8; 25]| usize::from(i[GT2_INS_WAVE as usize]) - 1;
    assert_eq!(raw.rtable[0][wave_row(lead)], 0x00, "the lead at the note");
    assert_eq!(raw.rtable[0][wave_row(bass)], 0x74, "the bass at -12");
    assert_eq!(raw.ltable[0][wave_row(bass) + 1], 0xff, "the wave stops");
}
